use super::{Part, Shape};
use glam::Vec3;

/// Triangles in a furniture item's local space, shared by the 3D view and the library thumbnails.
#[derive(Default)]
pub struct LocalMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub colors: Vec<[f32; 3]>,
    pub emissive: Vec<f32>,
    pub indices: Vec<u32>,
}

impl LocalMesh {
    fn vertex(&mut self, p: Vec3, n: Vec3, color: [f32; 3], emissive: f32) -> u32 {
        self.positions.push(p);
        self.normals.push(n.normalize_or_zero());
        self.colors.push(color);
        self.emissive.push(emissive);
        self.positions.len() as u32 - 1
    }

    /// Adds a part; `segments` sets how round cylinders and spheres are.
    pub fn add_part(&mut self, part: &Part, segments: usize) {
        let color = part.color.map(|c| c as f32 / 255.0);
        let e = part.emissive;
        let ([x0, y0, z0], [x1, y1, z1]) = (part.min, part.max);
        let (cx, cz) = ((x0 + x1) / 2.0, (z0 + z1) / 2.0);
        let (rx, rz) = ((x1 - x0) / 2.0, (z1 - z0) / 2.0);
        let p = Vec3::new;

        match part.shape {
            Shape::Box => {
                let faces = [
                    (
                        [p(x0, y1, z0), p(x1, y1, z0), p(x1, y1, z1), p(x0, y1, z1)],
                        Vec3::Y,
                    ),
                    (
                        [p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)],
                        -Vec3::Y,
                    ),
                    (
                        [p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)],
                        Vec3::Z,
                    ),
                    (
                        [p(x0, y0, z0), p(x1, y0, z0), p(x1, y1, z0), p(x0, y1, z0)],
                        -Vec3::Z,
                    ),
                    (
                        [p(x1, y0, z0), p(x1, y0, z1), p(x1, y1, z1), p(x1, y1, z0)],
                        Vec3::X,
                    ),
                    (
                        [p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1), p(x0, y1, z0)],
                        -Vec3::X,
                    ),
                ];
                for (corners, normal) in faces {
                    let i = corners.map(|c| self.vertex(c, normal, color, e));
                    self.indices
                        .extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
                }
            }
            Shape::Cylinder | Shape::Cone { .. } => {
                let top_scale = match part.shape {
                    Shape::Cone { top_scale } => top_scale,
                    _ => 1.0,
                };
                let slope = (1.0 - top_scale) * (rx + rz) / 2.0 / (y1 - y0).max(1e-3);
                let ring = |i: usize, y: f32, scale: f32| {
                    let a = i as f32 / segments as f32 * std::f32::consts::TAU;
                    (
                        p(cx + rx * scale * a.cos(), y, cz + rz * scale * a.sin()),
                        a,
                    )
                };
                let radial = |a: f32| {
                    let (x, z) = (a.cos() / rx.max(1e-4), a.sin() / rz.max(1e-4));
                    let len = (x * x + z * z).sqrt().max(1e-6);
                    p(x / len, slope, z / len)
                };
                let bottom_center = self.vertex(p(cx, y0, cz), -Vec3::Y, color, e);
                let top_center = self.vertex(p(cx, y1, cz), Vec3::Y, color, e);
                for i in 0..segments {
                    let (b0, a0) = ring(i, y0, 1.0);
                    let (b1, a1) = ring(i + 1, y0, 1.0);
                    let (t0, _) = ring(i, y1, top_scale);
                    let (t1, _) = ring(i + 1, y1, top_scale);
                    let (n0, n1) = (radial(a0), radial(a1));
                    let i0 = self.vertex(b0, n0, color, e);
                    let i1 = self.vertex(b1, n1, color, e);
                    let i2 = self.vertex(t1, n1, color, e);
                    let i3 = self.vertex(t0, n0, color, e);
                    self.indices.extend_from_slice(&[i0, i1, i2, i0, i2, i3]);

                    let cb0 = self.vertex(b0, -Vec3::Y, color, e);
                    let cb1 = self.vertex(b1, -Vec3::Y, color, e);
                    self.indices.extend_from_slice(&[bottom_center, cb0, cb1]);
                    let ct0 = self.vertex(t0, Vec3::Y, color, e);
                    let ct1 = self.vertex(t1, Vec3::Y, color, e);
                    self.indices.extend_from_slice(&[top_center, ct0, ct1]);
                }
            }
            Shape::Sphere => {
                let rings = (segments / 2).max(4);
                let ry = (y1 - y0) / 2.0;
                let cy = (y0 + y1) / 2.0;
                let base = self.positions.len() as u32;
                for r in 0..=rings {
                    let theta = r as f32 / rings as f32 * std::f32::consts::PI;
                    for s in 0..=segments {
                        let phi = s as f32 / segments as f32 * std::f32::consts::TAU;
                        let unit = p(
                            theta.sin() * phi.cos(),
                            theta.cos(),
                            theta.sin() * phi.sin(),
                        );
                        let pos = p(cx + rx * unit.x, cy + ry * unit.y, cz + rz * unit.z);
                        let normal = p(
                            unit.x / rx.max(1e-4),
                            unit.y / ry.max(1e-4),
                            unit.z / rz.max(1e-4),
                        );
                        self.vertex(pos, normal, color, e);
                    }
                }
                let stride = segments as u32 + 1;
                for r in 0..rings as u32 {
                    for s in 0..segments as u32 {
                        let a = base + r * stride + s;
                        let b = a + stride;
                        self.indices
                            .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
                    }
                }
            }
        }
    }
}
