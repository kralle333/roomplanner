use crate::{
    furniture::{mesh::LocalMesh, Part},
    models::{Furniture, FurnitureKind},
};
use eframe::egui::{self, epaint::Vertex, Color32, Mesh, Painter, Pos2, Rect};
use glam::Vec3;

/// Draws a small three-quarter view of a piece of furniture, like a catalog picture.
/// It is built from the same parts as the 3D view, flat-shaded and painted back to front.
pub fn paint(painter: &Painter, rect: Rect, item: &Furniture) {
    let mut parts = item.parts();
    if parts.is_empty() {
        // Imported models: a neutral box of the right proportions.
        parts.push(Part::cuboid(
            [-item.width / 2.0, 0.0, -item.depth / 2.0],
            [item.width / 2.0, item.height, item.depth / 2.0],
            [200, 200, 205],
        ));
    }
    let mut local = LocalMesh::default();
    for part in &parts {
        local.add_part(part, 16);
    }
    if local.indices.is_empty() {
        return;
    }

    // Looking at the front-right corner from slightly above.
    let yaw = 35.0_f32.to_radians();
    let pitch = 28.0_f32.to_radians();
    let view = Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        yaw.cos() * pitch.cos(),
    );
    let right = Vec3::Y.cross(view).normalize();
    let up = view.cross(right);
    let light = Vec3::new(-0.3, 0.9, 0.5).normalize();

    let projected: Vec<(f32, f32)> = local
        .positions
        .iter()
        .map(|p| (p.dot(right), -p.dot(up)))
        .collect();
    let (mut min, mut max) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for &(x, y) in &projected {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    let size = ((max.0 - min.0).max(1e-4), (max.1 - min.1).max(1e-4));
    let target = rect.shrink(rect.width().min(rect.height()) * 0.1);
    let scale = (target.width() / size.0).min(target.height() / size.1);
    let offset = target.center() - egui::vec2((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0) * scale;
    let to_screen = |i: u32| {
        let (x, y) = projected[i as usize];
        Pos2::new(x * scale, y * scale) + offset.to_vec2()
    };

    // Painter's algorithm: farthest triangles first.
    let mut triangles: Vec<([u32; 3], f32)> = local
        .indices
        .chunks_exact(3)
        .map(|t| {
            let tri = [t[0], t[1], t[2]];
            let depth = tri
                .iter()
                .map(|&i| local.positions[i as usize].dot(view))
                .sum::<f32>();
            (tri, depth)
        })
        .collect();
    triangles.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut mesh = Mesh::default();
    for (tri, _) in triangles {
        let [a, b, c] = tri.map(|i| local.positions[i as usize]);
        let mut normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.dot(view) < 0.0 {
            normal = -normal;
        }
        let i = tri[0] as usize;
        let shade = 0.62 + 0.38 * normal.dot(light).max(0.0) + local.emissive[i] * 0.25;
        let [r, g, b] = local.colors[i].map(|c| ((c * shade).min(1.0) * 255.0) as u8);
        let color = Color32::from_rgb(r, g, b);
        let base = mesh.vertices.len() as u32;
        for &index in &tri {
            mesh.vertices.push(Vertex {
                pos: to_screen(index),
                uv: egui::epaint::WHITE_UV,
                color,
            });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    painter.add(egui::Shape::mesh(mesh));

    if matches!(item.kind, FurnitureKind::Model { .. }) {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "3D",
            egui::FontId::proportional(11.0),
            Color32::from_gray(90),
        );
    }
}

/// Catalog picture for a window or door, drawn front-on.
pub fn paint_opening(painter: &Painter, rect: Rect, opening: &crate::models::Opening) {
    use crate::models::OpeningKind;
    let frame = Color32::from_rgb(120, 125, 135);
    let glass = Color32::from_rgb(190, 220, 240);
    let wall = Color32::from_rgb(236, 236, 232);

    // A piece of wall with the opening at its real proportions (wall 2.5 m high).
    let wall_rect = rect.shrink(rect.height() * 0.06);
    let scale = wall_rect.height() / 2.5;
    let w = (opening.width * scale).min(wall_rect.width() * 0.8);
    let h = opening.height * scale;
    let bottom = wall_rect.bottom() - opening.sill * scale;
    let hole = Rect::from_min_max(
        Pos2::new(wall_rect.center().x - w / 2.0, bottom - h),
        Pos2::new(wall_rect.center().x + w / 2.0, bottom),
    );
    painter.rect_filled(wall_rect, 3.0, wall);
    let stroke = egui::Stroke::new(1.5, frame);
    match opening.kind {
        OpeningKind::Window => {
            painter.rect_filled(hole, 1.0, glass);
            painter.rect_stroke(hole, 1.0, stroke, egui::StrokeKind::Middle);
            painter.line_segment([hole.center_top(), hole.center_bottom()], stroke);
            painter.line_segment(
                [hole.left_center(), hole.right_center()],
                egui::Stroke::new(1.0, frame),
            );
        }
        OpeningKind::Door => {
            painter.rect_filled(hole, 1.0, Color32::from_rgb(245, 244, 240));
            painter.rect_stroke(hole, 1.0, stroke, egui::StrokeKind::Middle);
            let knob = Pos2::new(hole.right() - w * 0.15, hole.center().y + h * 0.05);
            painter.circle_filled(knob, (w * 0.06).clamp(1.5, 3.0), frame);
            if w > h * 0.5 {
                painter.line_segment(
                    [hole.center_top(), hole.center_bottom()],
                    egui::Stroke::new(1.0, frame),
                );
            }
        }
        OpeningKind::GlassDoor => {
            painter.rect_filled(hole, 1.0, glass);
            painter.rect_stroke(hole, 1.0, stroke, egui::StrokeKind::Middle);
            let inner = hole.shrink(w * 0.12);
            painter.rect_stroke(
                inner,
                0.0,
                egui::Stroke::new(1.0, frame),
                egui::StrokeKind::Middle,
            );
        }
    }
    // Floor line.
    painter.line_segment(
        [wall_rect.left_bottom(), wall_rect.right_bottom()],
        egui::Stroke::new(1.5, Color32::from_rgb(170, 140, 105)),
    );
}
