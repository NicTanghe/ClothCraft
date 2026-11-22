//! Screen-space camera widget drawn into the native USD viewport.
use clothcraft::render::{Camera, PresetView};
use cssimpler::renderer::gpu::{GpuContext, GpuViewportFrame};
use glam::{Vec2, Vec3};

const GRID: [f32; 4] = [0.0, 0.16, 0.84, 1.0];
const GRID_DIRECTIONS: [f32; 3] = [-1.0, 0.0, 1.0];
const QUAD_VERTICES: usize = 6;
const MAX_VERTICES: usize = 3 * (1 + 9 + 15) * QUAD_VERTICES;

struct Face {
    direction: Vec3,
    points: [Vec2; 4],
    tangent: Vec3,
    bitangent: Vec3,
    colour: [f32; 3],
    mark: [u8; 5],
}

impl Face {
    fn project(&self, u: f32, v: f32) -> Vec2 {
        self.points[0]
            + u * (self.points[1] - self.points[0])
            + v * (self.points[3] - self.points[0])
    }

    fn snap_direction(&self, column: usize, row: usize) -> Vec3 {
        (self.direction
            + GRID_DIRECTIONS[column] * self.tangent
            + GRID_DIRECTIONS[row] * self.bitangent)
            .normalize()
    }
}

fn faces(camera: &Camera, size: Vec2) -> impl Iterator<Item = Face> {
    let extent = (size.min_element() * 0.24).min(29.0);
    let centre = Vec2::new(size.x - extent * 1.75 - 12.0, extent * 1.75 + 12.0);
    PresetView::ALL.into_iter().filter_map(move |view| {
        let direction = view.direction();
        if view == PresetView::Isometric
            || extent < 9.0
            || direction.dot(camera.direction()) <= 1e-5
        {
            return None;
        }
        let tangent = if direction.x.abs() > 0.5 {
            Vec3::Z
        } else {
            Vec3::X
        };
        let bitangent = direction.cross(tangent);
        let points = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)].map(|(u, v)| {
            let point = direction + u * tangent + v * bitangent;
            centre + extent * Vec2::new(camera.right().dot(point), -camera.up().dot(point))
        });
        let (colour, mark) = match view {
            PresetView::Front => ([0.22, 0.48, 0.75], [7, 4, 6, 4, 4]),
            PresetView::Back => ([0.11, 0.26, 0.48], [6, 5, 6, 5, 6]),
            PresetView::Left => ([0.34, 0.19, 0.14], [4, 4, 4, 4, 7]),
            PresetView::Right => ([0.70, 0.33, 0.22], [6, 5, 6, 5, 5]),
            PresetView::Top => ([0.30, 0.57, 0.40], [5, 5, 5, 5, 7]),
            PresetView::Bottom => ([0.16, 0.34, 0.25], [6, 5, 5, 5, 6]),
            PresetView::Isometric => unreachable!(),
        };
        Some(Face {
            direction,
            points,
            tangent,
            bitangent,
            colour,
            mark,
        })
    })
}

pub fn pick(camera: &Camera, size: Vec2, pointer: Vec2) -> Option<Vec3> {
    let cell =
        |coordinate: f32| usize::from(coordinate >= GRID[1]) + usize::from(coordinate >= GRID[2]);
    for face in faces(camera, size) {
        let horizontal = face.points[1] - face.points[0];
        let vertical = face.points[3] - face.points[0];
        let offset = pointer - face.points[0];
        let area = horizontal.perp_dot(vertical);
        let u = offset.perp_dot(vertical) / area;
        let v = horizontal.perp_dot(offset) / area;
        // Include the shared seams despite round-off during projection.
        let face_bounds = -1e-5..=1.0 + 1e-5;
        if face_bounds.contains(&u) && face_bounds.contains(&v) {
            return Some(face.snap_direction(cell(u), cell(v)));
        }
    }
    None
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    colour: [f32; 3],
}

fn quad(vertices: &mut Vec<Vertex>, points: [Vec2; 4], size: Vec2, colour: [f32; 3]) {
    vertices.extend([0, 1, 2, 0, 2, 3].map(|index| {
        let position = points[index] / size;
        Vertex {
            position: [position.x * 2.0 - 1.0, 1.0 - position.y * 2.0],
            colour,
        }
    }));
}

pub struct ViewCube {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
}

impl ViewCube {
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("View cube"),
            source: wgpu::ShaderSource::Wgsl(include_str!("view_cube.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("View cube"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("View cube vertices"),
            size: (MAX_VERTICES * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, vertices }
    }

    pub fn draw(&self, gpu: &GpuContext, frame: GpuViewportFrame<'_>, camera: &Camera) {
        let size = Vec2::new(frame.info.bounds.width, frame.info.bounds.height);
        let mut vertices = Vec::with_capacity(MAX_VERTICES);
        for face in faces(camera, size) {
            let centre = face.points.iter().copied().sum::<Vec2>() / 4.0;
            quad(&mut vertices, face.points, size, [0.035, 0.044, 0.055]);
            for (row, v) in GRID.windows(2).enumerate() {
                for (column, u) in GRID.windows(2).enumerate() {
                    let inset = 0.012;
                    let points = [
                        face.project(u[0] + inset, v[0] + inset),
                        face.project(u[1] - inset, v[0] + inset),
                        face.project(u[1] - inset, v[1] - inset),
                        face.project(u[0] + inset, v[1] - inset),
                    ];
                    let colour = if row == 1 && column == 1 {
                        face.colour
                    } else {
                        face.colour.map(|channel| channel * 0.74)
                    };
                    quad(&mut vertices, points, size, colour);
                }
            }
            // A small screen-aligned initial stays upright as the cube turns.
            // Skip it when the projected face is too narrow to contain it.
            let clearance = (0..4)
                .map(|index| {
                    let a = face.points[index];
                    let edge = face.points[(index + 1) % 4] - a;
                    edge.perp_dot(centre - a).abs() / edge.length()
                })
                .fold(f32::INFINITY, f32::min);
            if clearance * (GRID[2] - GRID[1]) >= 7.0 {
                for (row, bits) in face.mark.into_iter().enumerate() {
                    for column in 0..3 {
                        if bits & (4 >> column) != 0 {
                            let origin = centre
                                + Vec2::new(column as f32 * 2.0 - 3.0, row as f32 * 2.0 - 5.0);
                            let points = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]
                                .map(|offset| origin + offset * 2.0);
                            quad(&mut vertices, points, size, [0.95, 0.95, 0.95]);
                        }
                    }
                }
            }
        }
        if vertices.is_empty() {
            return;
        }
        gpu.queue
            .write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("View cube"),
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
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_each_face_edge_and_corner() {
        let size = Vec2::new(640., 480.);
        let mut camera = Camera::default();
        let mut directions = std::collections::HashSet::new();
        for preset in PresetView::ALL {
            if preset == PresetView::Isometric {
                continue;
            }
            camera.apply_preset(preset);
            let face = faces(&camera, size).next().unwrap();

            for (row, v) in [0.08, 0.5, 0.92].into_iter().enumerate() {
                for (column, u) in [0.08, 0.5, 0.92].into_iter().enumerate() {
                    let pointer = face.project(u, v);
                    let expected = preset.direction()
                        + GRID_DIRECTIONS[column] * face.tangent
                        + GRID_DIRECTIONS[row] * face.bitangent;
                    let picked = pick(&camera, size, pointer).unwrap();
                    assert!(picked.abs_diff_eq(expected.normalize(), 1e-5));
                    directions.insert(expected.to_array().map(|component| component as i32));
                }
            }
            assert!(pick(&camera, size, face.project(1.04, 0.5)).is_none());
        }
        assert_eq!(directions.len(), 6 + 12 + 8);
    }
}
