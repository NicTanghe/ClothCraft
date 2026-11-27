//! Screen-space camera widget drawn into the native USD viewport.
use clothcraft::render::{Camera, PresetView};
use cssimpler::renderer::gpu::{GpuContext, GpuViewportFrame};
use glam::{Vec2, Vec3};

const GRID: [f32; 4] = [0.0, 0.16, 0.84, 1.0];
const GRID_DIRECTIONS: [f32; 3] = [-1.0, 0.0, 1.0];
const MAX_FACES: usize = 3;

struct Face {
    direction: Vec3,
    points: [Vec2; 4],
    tangent: Vec3,
    bitangent: Vec3,
    colour: [f32; 3],
    mark: [u8; 5],
}

impl Face {
    fn glyph_right(&self, camera: &Camera) -> Vec2 {
        if self.direction.y != 0.0 {
            // Camera right is horizontal, so it lies in both pole faces.
            let right = camera.right();
            Vec2::new(self.tangent.dot(right), self.bitangent.dot(right))
        } else {
            // Keep the vertical faces' letters aligned with world up.
            Vec2::new(self.bitangent.y, 0.0)
        }
    }

    #[cfg(test)]
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
struct FacePaint {
    origin_and_u: [f32; 4],
    v_and_glyph_right: [f32; 4],
    colour_and_hover: [f32; 4],
    glyph_and_silhouette: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CubePaint {
    bounds: [f32; 4],
    viewport: [f32; 4],
    grid: [f32; 4],
    faces: [FacePaint; MAX_FACES],
}

pub struct ViewCube {
    pipeline: wgpu::RenderPipeline,
    paint: wgpu::Buffer,
    binding: wgpu::BindGroup,
}

impl ViewCube {
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat, shader: &wgpu::ShaderModule) -> Self {
        let device = &gpu.device;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("View cube"),
            layout: None,
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("cube_vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("cube_fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let paint = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("View cube paint"),
            size: std::mem::size_of::<CubePaint>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("View cube paint"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: paint.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            paint,
            binding,
        }
    }

    pub fn draw(
        &self,
        gpu: &GpuContext,
        frame: GpuViewportFrame<'_>,
        camera: &Camera,
        hovered: Option<Vec3>,
    ) {
        use bytemuck::Zeroable;
        let size = Vec2::new(frame.info.bounds.width, frame.info.bounds.height);
        let [width, height] = frame.info.physical_size;
        let physical = Vec2::new(width as f32, height as f32);
        let scale = physical / size.max(Vec2::ONE);
        let mut paint = CubePaint::zeroed();
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        paint.viewport = [physical.x, physical.y, frame.info.scale_factor as f32, 0.0];
        paint.grid = [GRID[1], GRID[2], 0.0, 0.0];

        for face in faces(camera, size) {
            let points = face.points.map(|point| point * scale);
            for point in points {
                min = min.min(point);
                max = max.max(point);
            }
            let origin = points[0];
            let horizontal = points[1] - origin;
            let vertical = points[3] - origin;
            let area = horizontal.perp_dot(vertical);
            if area == 0.0 {
                continue;
            }
            let u = -vertical.perp() / area;
            let v = horizontal.perp() / area;
            let glyph_right = face.glyph_right(camera);
            let hover_index = (0..9)
                .find(|cell| hovered == Some(face.snap_direction(cell % 3, cell / 3)))
                .map_or(-1.0, |cell| cell as f32);
            let glyph = face
                .mark
                .into_iter()
                .fold(0u32, |bits, row| (bits << 3) | u32::from(row));
            let silhouette = [-face.tangent, face.tangent, -face.bitangent, face.bitangent]
                .into_iter()
                .enumerate()
                .fold(0u32, |mask, (edge, normal)| {
                    mask | (u32::from(normal.dot(camera.direction()) <= 1e-5) << edge)
                });
            // Pack only nondegenerate faces so the shader can scan a contiguous array.
            let slot = paint.viewport[3] as usize;
            paint.faces[slot] = FacePaint {
                origin_and_u: [origin.x, origin.y, u.x, u.y],
                v_and_glyph_right: [v.x, v.y, glyph_right.x, glyph_right.y],
                colour_and_hover: [face.colour[0], face.colour[1], face.colour[2], hover_index],
                glyph_and_silhouette: [glyph as f32, silhouette as f32, 0.0, 0.0],
            };
            paint.viewport[3] += 1.0;
        }
        if paint.viewport[3] == 0.0 {
            return;
        }
        paint.bounds = [
            min.x.floor() - 1.0,
            min.y.floor() - 1.0,
            max.x.ceil() + 1.0,
            max.y.ceil() + 1.0,
        ];
        gpu.queue
            .write_buffer(&self.paint, 0, bytemuck::bytes_of(&paint));
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
        pass.set_bind_group(0, &self.binding, &[]);
        pass.draw(0..6, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_shader_provides_both_overlay_pipelines() {
        let module = naga::front::wgsl::parse_str(super::super::VIEWPORT_OVERLAY_SHADER).unwrap();
        for (name, stage) in [
            ("cube_vertex", naga::ShaderStage::Vertex),
            ("cube_fragment", naga::ShaderStage::Fragment),
            ("gizmo_vertex", naga::ShaderStage::Vertex),
            ("gizmo_fragment", naga::ShaderStage::Fragment),
        ] {
            assert!(
                module
                    .entry_points
                    .iter()
                    .any(|entry| entry.name == name && entry.stage == stage)
            );
        }
    }

    #[test]
    fn pole_labels_stay_upright_through_an_orbit() {
        let size = Vec2::splat(640.0);
        let mut camera = Camera::default();
        for pitch in [-1.4, -0.3, 0.3, 1.4] {
            camera.pitch = pitch;
            for step in 0..16 {
                camera.yaw = step as f32 * std::f32::consts::TAU / 16.0;
                let face = faces(&camera, size)
                    .find(|face| face.direction.y != 0.0)
                    .unwrap();
                let right = face.glyph_right(&camera);
                let down = -right.perp();
                let centre = face.project(0.5, 0.5);
                let screen_right = face.project(0.5 + right.x * 0.1, 0.5 + right.y * 0.1) - centre;
                let screen_down = face.project(0.5 + down.x * 0.1, 0.5 + down.y * 0.1) - centre;
                assert!(screen_right.x > 0.0 && screen_right.y.abs() < 1e-4);
                assert!(screen_down.y > 0.0 && screen_down.x.abs() < 1e-4);
            }
        }
    }

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
