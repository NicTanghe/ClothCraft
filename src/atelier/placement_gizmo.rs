//! Native viewport overlay for rigid panel placement. Drawing and picking use
//! the same projected handle segments in logical viewport pixels.

use clothcraft::render::Camera;
use clothcraft::usd_scene::VIEWPORT_CAMERA_HALF_TAN;
use cssimpler::renderer::gpu::{GpuContext, GpuViewportFrame, GpuViewportInfo};
use glam::{Vec2, Vec3};
use std::{cell::Cell, rc::Rc};

const HANDLE_PX: f32 = 55.0;
const RING_STEPS: usize = 48;
const TOGGLE_ORIGIN: Vec2 = Vec2::splat(12.0);
const TOGGLE_SIZE: Vec2 = Vec2::splat(36.0);
const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const AXIS_COLOURS: [[f32; 4]; 3] = [
    [0.94, 0.19, 0.17, 1.0],
    [0.20, 0.81, 0.31, 1.0],
    [0.18, 0.49, 1.00, 1.0],
];
const INK: [f32; 4] = [0.016, 0.020, 0.030, 0.85];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Move,
    Rotate,
}

impl Mode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Move => Self::Rotate,
            Self::Rotate => Self::Move,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rotate => "Rotate",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Overlay {
    pub visible: bool,
    pub mode: Mode,
    pub pivot: Option<Vec3>,
    pub axis_highlight: Option<Vec3>,
    pub toggle_hovered: bool,
}

#[derive(Clone, Copy)]
pub struct Projection {
    pub eye: Vec3,
    pub forward: Vec3,
    right: Vec3,
    up: Vec3,
    size: Vec2,
    focal_pixels: Vec2,
}

impl Projection {
    pub fn new(camera: &Camera, size: Vec2, aspect: f32) -> Self {
        let eye = camera.eye();
        let forward = (camera.target - eye).normalize_or_zero();
        let right = forward.cross(camera.up()).normalize_or_zero();
        let up = right.cross(forward);
        let focal_pixels = Vec2::new(size.x / aspect, size.y) * (0.5 / VIEWPORT_CAMERA_HALF_TAN);
        Self {
            eye,
            forward,
            right,
            up,
            size,
            focal_pixels,
        }
    }

    pub fn point(self, world: Vec3) -> Option<Vec2> {
        let delta = world - self.eye;
        let depth = delta.dot(self.forward);
        (depth > 0.0001).then(|| {
            self.size * 0.5
                + Vec2::new(delta.dot(self.right), -delta.dot(self.up)) * self.focal_pixels / depth
        })
    }

    pub fn ray(self, pointer: Vec2) -> Vec3 {
        let screen = (pointer - self.size * 0.5) / self.focal_pixels;
        (self.forward + self.right * screen.x - self.up * screen.y).normalize_or_zero()
    }
}

pub fn toggle_contains(size: Vec2, pointer: Vec2) -> bool {
    size.cmpge(TOGGLE_ORIGIN + TOGGLE_SIZE).all()
        && pointer.cmpge(TOGGLE_ORIGIN).all()
        && pointer.cmple(TOGGLE_ORIGIN + TOGGLE_SIZE).all()
}

struct Segment {
    axis: usize,
    start: Vec2,
    end: Vec2,
    amount: f32,
    arrow: bool,
    back: bool,
}

fn segments(projection: Projection, mode: Mode, pivot: Vec3, mut add: impl FnMut(Segment)) {
    let Some(center) = projection.point(pivot) else {
        return;
    };
    let radius =
        HANDLE_PX * (pivot - projection.eye).dot(projection.forward) / projection.focal_pixels.y;
    for (index, axis) in AXES.into_iter().enumerate() {
        if mode == Mode::Move {
            if let Some(tip) = projection.point(pivot + axis * radius) {
                let length = center.distance(tip);
                if length >= 14.0 {
                    add(Segment {
                        axis: index,
                        start: center.lerp(tip, 7.0 / length),
                        end: tip,
                        amount: radius * (1.0 - 7.0 / length),
                        arrow: true,
                        back: false,
                    });
                }
            }
            continue;
        }

        let u = AXES[(index + 1) % AXES.len()];
        let v = axis.cross(u);
        let step = std::f32::consts::TAU / RING_STEPS as f32;
        let point = |angle: f32| pivot + radius * (u * angle.cos() + v * angle.sin());
        for i in 0..RING_STEPS {
            let a = point(i as f32 * step);
            let b = point((i + 1) as f32 * step);
            if let (Some(start), Some(end)) = (projection.point(a), projection.point(b)) {
                add(Segment {
                    axis: index,
                    start,
                    end,
                    amount: step,
                    arrow: false,
                    back: ((a + b) * 0.5 - pivot).dot(projection.forward) > 0.0,
                });
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Handle {
    pub axis: Vec3,
    pub tangent: Vec2,
    pub units_per_pixel: f32,
}

pub fn pick(projection: Projection, mode: Mode, pivot: Vec3, pointer: Vec2) -> Option<Handle> {
    let mut best = None;
    let mut best_score = f32::INFINITY;
    segments(projection, mode, pivot, |segment| {
        let delta = segment.end - segment.start;
        let length = delta.length();
        if length < 0.5 {
            return;
        }
        let tangent = delta / length;
        let along = (pointer - segment.start).dot(tangent).clamp(0.0, length);
        let distance = pointer.distance(segment.start + tangent * along);
        let score = distance + if segment.back { 0.5 } else { 0.0 };
        if distance <= 8.0 && score < best_score {
            best_score = score;
            best = Some(Handle {
                axis: AXES[segment.axis],
                tangent,
                units_per_pixel: segment.amount / length,
            });
        }
    });
    best
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    colour: [f32; 4],
}

struct Paint {
    size: Vec2,
    vertices: Vec<Vertex>,
}

impl Paint {
    fn triangle(&mut self, points: [Vec2; 3], colour: [f32; 4]) {
        self.vertices.extend(points.map(|point| Vertex {
            position: ((point / self.size * 2.0 - Vec2::ONE) * Vec2::new(1.0, -1.0)).to_array(),
            colour,
        }));
    }

    fn quad(&mut self, [a, b, c, d]: [Vec2; 4], colour: [f32; 4]) {
        self.triangle([a, b, c], colour);
        self.triangle([a, c, d], colour);
    }

    fn line(&mut self, a: Vec2, b: Vec2, width: f32, colour: [f32; 4]) {
        let offset = (b - a).normalize_or_zero().perp() * (width * 0.5);
        self.quad([a + offset, a - offset, b - offset, b + offset], colour);
    }

    fn arrow(&mut self, tip: Vec2, direction: Vec2, size: f32, colour: [f32; 4]) {
        let base = tip - direction * size;
        let wing = direction.perp() * size * 0.48;
        self.triangle([tip, base + wing, base - wing], colour);
    }

    fn toggle(&mut self, mode: Mode, hovered: bool) {
        if !toggle_contains(self.size, TOGGLE_ORIGIN) {
            return;
        }
        let min = TOGGLE_ORIGIN;
        let max = min + TOGGLE_SIZE;
        self.quad(
            [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
            INK,
        );
        let edge = if hovered {
            [0.28, 0.64, 0.98, 1.0]
        } else {
            [0.29, 0.33, 0.40, 1.0]
        };
        let corner = Vec2::new(max.x, min.y);
        let other = Vec2::new(min.x, max.y);
        for (a, b) in [(min, corner), (corner, max), (max, other), (other, min)] {
            self.line(a, b, 1.0, edge);
        }
        let centre = (min + max) * 0.5;
        let colour = [0.86, 0.92, 1.0, 1.0];
        match mode {
            Mode::Move => {
                for direction in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                    let tip = centre + direction * 10.0;
                    self.line(centre, tip - direction * 2.0, 1.7, colour);
                    self.arrow(tip, direction, 5.0, colour);
                }
            }
            Mode::Rotate => {
                let point = |i: usize| {
                    let angle = i as f32 * 0.29;
                    centre + Vec2::new(angle.cos(), -angle.sin()) * 9.0
                };
                for i in 0..17 {
                    self.line(point(i), point(i + 1), 1.8, colour);
                }
                self.arrow(point(18), (point(18) - point(17)).normalize(), 6.0, colour);
            }
        }
    }
}

const MAX_VERTICES: usize = 3 * RING_STEPS * 12 + 200;
const SHADER: &str = include_str!("placement_gizmo.wgsl");

pub struct GizmoRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    paint: Paint,
    overlay: Rc<Cell<Overlay>>,
    viewport: Rc<Cell<Option<GpuViewportInfo>>>,
}

impl GizmoRenderer {
    pub fn new(
        gpu: &GpuContext,
        info: GpuViewportInfo,
        overlay: Rc<Cell<Overlay>>,
        viewport: Rc<Cell<Option<GpuViewportInfo>>>,
    ) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Placement gizmo shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Placement gizmo"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: info.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Placement gizmo vertices"),
            size: (MAX_VERTICES * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        viewport.set(Some(info));
        Self {
            pipeline,
            buffer,
            paint: Paint {
                size: Vec2::ZERO,
                vertices: Vec::with_capacity(MAX_VERTICES),
            },
            overlay,
            viewport,
        }
    }

    pub fn resize(&self, info: GpuViewportInfo) {
        self.viewport.set(Some(info));
    }

    pub fn draw(&mut self, gpu: &GpuContext, frame: GpuViewportFrame<'_>, camera: &Camera) {
        let overlay = self.overlay.get();
        if !overlay.visible {
            return;
        }
        let size = Vec2::new(frame.info.bounds.width, frame.info.bounds.height);
        let [width, height] = frame.info.physical_size;
        if size.min_element() <= 0.0 || width == 0 || height == 0 {
            return;
        }
        self.paint.size = size;
        self.paint.vertices.clear();
        if let Some(pivot) = overlay.pivot {
            let projection = Projection::new(camera, size, width as f32 / height as f32);
            // Paint the far halves of rotation rings first so crossing axes
            // stay legible and the highlighted axis can be identified.
            for back in [true, false] {
                segments(projection, overlay.mode, pivot, |segment| {
                    if segment.back != back {
                        return;
                    }
                    let active = overlay.axis_highlight == Some(AXES[segment.axis]);
                    let width = if active { 3.5 } else { 2.4 };
                    let mut colour = AXIS_COLOURS[segment.axis];
                    if back && !active {
                        colour[3] = 0.46;
                    }
                    self.paint
                        .line(segment.start, segment.end, width + 2.0, INK);
                    self.paint.line(segment.start, segment.end, width, colour);
                    if segment.arrow {
                        let direction = (segment.end - segment.start).normalize_or_zero();
                        self.paint
                            .arrow(segment.end + direction, direction, 12.0, INK);
                        self.paint.arrow(segment.end, direction, 10.0, colour);
                    }
                });
            }
        }
        self.paint.toggle(overlay.mode, overlay.toggle_hovered);
        if self.paint.vertices.is_empty() {
            return;
        }
        gpu.queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.paint.vertices));
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Placement gizmo"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.paint.vertices.len() as u32, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_round_trips_through_pointer_ray() {
        let camera = Camera::new(0.4, 0.3, 2.0, Vec3::Y);
        let projection = Projection::new(&camera, Vec2::new(800.0, 600.0), 800.0 / 600.0);
        let target = camera.target + Vec3::new(0.2, -0.3, 0.1);
        let pixel = projection.point(target).unwrap();
        assert!(
            projection
                .ray(pixel)
                .abs_diff_eq((target - camera.eye()).normalize(), 1e-6)
        );
    }

    #[test]
    fn picks_drawn_axes_and_toggles() {
        let mut camera = Camera::default();
        camera.look_from(Vec3::Z);
        let size = Vec2::new(800.0, 600.0);
        let projection = Projection::new(&camera, size, size.x / size.y);
        let centre = projection.point(camera.target).unwrap();
        let right = pick(
            projection,
            Mode::Move,
            camera.target,
            centre + Vec2::X * 40.0,
        )
        .unwrap();
        assert_eq!(right.axis, Vec3::X);
        let ring = pick(
            projection,
            Mode::Rotate,
            camera.target,
            centre + Vec2::new(1.0, -1.0).normalize() * HANDLE_PX,
        )
        .unwrap();
        assert_eq!(ring.axis, Vec3::Z);
        assert!(toggle_contains(size, Vec2::splat(24.0)));
        assert!(!toggle_contains(size, centre));
    }

    #[test]
    fn placement_shader_validates() {
        let module = naga::front::wgsl::parse_str(SHADER).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
