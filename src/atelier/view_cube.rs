//! Screen-space camera widget drawn into the native USD viewport.
use clothcraft::render::{Camera, PresetView};
use cssimpler::renderer::gpu::{GpuContext, GpuViewportFrame};
use glam::{Vec2, Vec3};

const EDGE_MARGIN: f32 = 5.0;

struct Face {
    direction: Vec3,
    points: [Vec2; 4],
    colour: [f32; 3],
    mark: [u8; 5],
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
            PresetView::Top => ([0.30, 0.57, 0.40], [7, 2, 2, 2, 2]),
            PresetView::Bottom => ([0.16, 0.34, 0.25], [2, 2, 2, 7, 2]),
            PresetView::Isometric => unreachable!(),
        };
        Some(Face {
            direction,
            points,
            colour,
            mark,
        })
    })
}

pub fn pick(camera: &Camera, size: Vec2, pointer: Vec2) -> Option<Vec3> {
    let mut nearest_edge = None;
    let mut nearest_distance = EDGE_MARGIN;
    let mut face_direction = None;
    for face in faces(camera, size) {
        let centre = face.points.iter().copied().sum::<Vec2>() / 4.0;
        let mut inside = true;
        for index in 0..4 {
            let a = face.points[index];
            let b = face.points[(index + 1) % 4];
            let edge = b - a;
            inside &= edge.perp_dot(pointer - a) * edge.perp_dot(centre - a) >= 0.0;

            let fraction = ((pointer - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
            let distance = pointer.distance(a + fraction * edge);
            if distance < nearest_distance {
                nearest_distance = distance;
                // The two outward normals sum to the direction facing this edge.
                let axis = if face.direction.x.abs() > 0.5 {
                    Vec3::Z
                } else {
                    Vec3::X
                };
                let other = face.direction.cross(axis);
                let adjacent = [-other, axis, other, -axis][index];
                nearest_edge = Some((face.direction + adjacent).normalize());
            }
        }
        if inside {
            face_direction = Some(face.direction);
        }
    }
    nearest_edge.or(face_direction)
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
            size: (3 * (2 + 15) * 6 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, vertices }
    }

    pub fn draw(&self, gpu: &GpuContext, frame: GpuViewportFrame<'_>, camera: &Camera) {
        let size = Vec2::new(frame.info.bounds.width, frame.info.bounds.height);
        let mut vertices = Vec::new();
        for face in faces(camera, size) {
            let centre = face.points.iter().copied().sum::<Vec2>() / 4.0;
            quad(&mut vertices, face.points, size, [0.035, 0.044, 0.055]);
            quad(
                &mut vertices,
                face.points.map(|point| point.lerp(centre, 0.11)),
                size,
                face.colour,
            );
            // A small screen-aligned initial stays upright as the cube turns.
            // Skip it when the projected face is too narrow to contain it.
            let clearance = (0..4)
                .map(|index| {
                    let a = face.points[index];
                    let edge = face.points[(index + 1) % 4] - a;
                    edge.perp_dot(centre - a).abs() / edge.length()
                })
                .fold(f32::INFINITY, f32::min);
            if clearance >= 7.0 {
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
    fn face_centres_snap_to_axes_and_edge_margins_snap_to_bisectors() {
        let size = Vec2::new(640., 480.);
        let mut camera = Camera::default();
        for preset in PresetView::ALL {
            if preset == PresetView::Isometric {
                continue;
            }
            camera.apply_preset(preset);
            let face = faces(&camera, size).next().unwrap();
            let centre = face.points.iter().copied().sum::<Vec2>() / 4.0;
            assert_eq!(pick(&camera, size, centre), Some(preset.direction()));

            for index in 0..4 {
                let edge = (face.points[index] + face.points[(index + 1) % 4]) / 2.0;
                let inward = (centre - edge).normalize();
                let adjacent = inward.x * -camera.right() + inward.y * camera.up();
                let expected = (preset.direction() + adjacent).normalize();
                for offset in [-3.0, 3.0] {
                    let picked = pick(&camera, size, edge + inward * offset).unwrap();
                    assert!(picked.abs_diff_eq(expected, 1e-5));
                }
                assert!(pick(&camera, size, edge - inward * 8.0).is_none());
            }
        }
    }
}
