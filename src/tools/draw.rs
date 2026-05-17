use crate::{
    app::{RoomPlannerApp, PIXELS_PER_METER},
    geometry::{math::closest_point_on_segment, rooms::triangulate},
    models::Tool,
};
use eframe::egui::{self, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

pub fn draw_scene(
    app: &RoomPlannerApp,
    painter: &Painter,
    pointer: Option<Pos2>,
    hovered_endpoints: &[(usize, bool)],
    hovered_wall_idx: Option<usize>,
    active_alignments: &[Pos2],
    snapped_preview: Option<Pos2>,
    snapped_wall_idx: Option<usize>,
) {
    for room_polygon in &app.rooms {
        let floor_color = Color32::from_rgba_unmultiplied(100, 200, 150, 60);
        let mut mesh = egui::epaint::Mesh::default();
        let triangles = triangulate(room_polygon);

        for tri in triangles {
            let idx = mesh.vertices.len() as u32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: app.world_to_screen(tri[0]),
                uv: Pos2::ZERO,
                color: floor_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: app.world_to_screen(tri[1]),
                uv: Pos2::ZERO,
                color: floor_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: app.world_to_screen(tri[2]),
                uv: Pos2::ZERO,
                color: floor_color,
            });
            mesh.indices.extend([idx, idx + 1, idx + 2]);
        }
        painter.add(Shape::mesh(mesh));
    }

    if let Some(snapped_pos) = snapped_preview {
        let guide_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(200, 50, 50, 120));
        for &align_target in active_alignments {
            painter.line_segment(
                [
                    app.world_to_screen(snapped_pos),
                    app.world_to_screen(align_target),
                ],
                guide_stroke,
            );
            painter.circle_stroke(app.world_to_screen(align_target), 4.0, guide_stroke);
        }
    }

    if app.current_tool == Tool::DrawWall {
        if let Some(snap_pos) = snapped_preview {
            if let Some(wall_idx) = snapped_wall_idx {
                if let Some(wall) = app.walls.get(wall_idx) {
                    let dir = (wall.end - wall.start).normalized();
                    let normal = vec2(-dir.y, dir.x);

                    let mut offset_dir = normal;
                    if let Some(mouse_pos) = pointer {
                        if (mouse_pos - snap_pos).dot(normal) < 0.0 {
                            offset_dir = -normal;
                        }
                    }
                    let offset = offset_dir * (25.0 / app.zoom_factor);

                    let p_start = wall.start + offset;
                    let p_end = wall.end + offset;
                    let p_cursor = snap_pos + offset;

                    let dist_start = snap_pos.distance(wall.start) / PIXELS_PER_METER;
                    let dist_end = snap_pos.distance(wall.end) / PIXELS_PER_METER;

                    let dim_color = Color32::from_rgb(0, 150, 200);
                    let stroke = Stroke::new(1.0, dim_color);
                    let faint = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 150, 200, 100));

                    painter.line_segment(
                        [app.world_to_screen(p_start), app.world_to_screen(p_cursor)],
                        stroke,
                    );
                    painter.line_segment(
                        [app.world_to_screen(p_cursor), app.world_to_screen(p_end)],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            app.world_to_screen(wall.start),
                            app.world_to_screen(p_start),
                        ],
                        faint,
                    );
                    painter.line_segment(
                        [app.world_to_screen(snap_pos), app.world_to_screen(p_cursor)],
                        faint,
                    );
                    painter.line_segment(
                        [app.world_to_screen(wall.end), app.world_to_screen(p_end)],
                        faint,
                    );

                    painter.circle_filled(app.world_to_screen(p_start), 2.0, dim_color);
                    painter.circle_filled(app.world_to_screen(p_cursor), 3.0, dim_color);
                    painter.circle_filled(app.world_to_screen(p_end), 2.0, dim_color);

                    if dist_start > 0.1 {
                        painter.text(
                            app.world_to_screen(
                                p_start.lerp(p_cursor, 0.5) + offset_dir * (8.0 / app.zoom_factor),
                            ),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.2} m", dist_start),
                            egui::FontId::proportional(12.0),
                            dim_color,
                        );
                    }
                    if dist_end > 0.1 {
                        painter.text(
                            app.world_to_screen(
                                p_cursor.lerp(p_end, 0.5) + offset_dir * (8.0 / app.zoom_factor),
                            ),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.2} m", dist_end),
                            egui::FontId::proportional(12.0),
                            dim_color,
                        );
                    }
                }
            }
        }

        if let Some(start) = app.wall_start_point {
            if let Some(end) = snapped_preview {
                painter.line_segment(
                    [app.world_to_screen(start), app.world_to_screen(end)],
                    Stroke::new(4.0, Color32::from_gray(150)),
                );

                if start.distance(end) > (5.0 / app.zoom_factor) {
                    let length_m = start.distance(end) / PIXELS_PER_METER;
                    painter.text(
                        app.world_to_screen(
                            start.lerp(end, 0.5) + vec2(0.0, -15.0 / app.zoom_factor),
                        ),
                        egui::Align2::CENTER_BOTTOM,
                        format!("{:.2} m\n[ESC] to cancel", length_m),
                        egui::FontId::proportional(13.0),
                        Color32::BLACK,
                    );

                    let mut prev_dir = None;
                    for wall in app.walls.iter().rev() {
                        if wall.end.distance(start) < 2.0 {
                            prev_dir = Some((start - wall.start).normalized());
                            break;
                        } else if wall.start.distance(start) < 2.0 {
                            prev_dir = Some((start - wall.end).normalized());
                            break;
                        }
                    }

                    if prev_dir.is_none() {
                        for wall in app.walls.iter().rev() {
                            let proj = closest_point_on_segment(start, wall.start, wall.end);
                            if proj.distance(start) < 2.0 {
                                let dir1 = (start - wall.start).normalized();
                                let dir2 = (start - wall.end).normalized();
                                let new_dir = (end - start).normalized();
                                if dir1.dot(new_dir) > dir2.dot(new_dir) {
                                    prev_dir = Some(dir1);
                                } else {
                                    prev_dir = Some(dir2);
                                }
                                break;
                            }
                        }
                    }

                    if let Some(p_dir) = prev_dir {
                        let dir_back = -p_dir;
                        let dir_new = (end - start).normalized();
                        let angle1 = dir_back.angle();
                        let angle2 = dir_new.angle();

                        let mut diff = angle2 - angle1;
                        while diff > std::f32::consts::PI {
                            diff -= std::f32::consts::TAU;
                        }
                        while diff <= -std::f32::consts::PI {
                            diff += std::f32::consts::TAU;
                        }

                        let display_angle = diff.abs().to_degrees();
                        let radius = 30.0 / app.zoom_factor;
                        let steps = 32;
                        let mut arc_points = Vec::new();

                        for i in 0..=steps {
                            let a = angle1 + (diff * (i as f32 / steps as f32));
                            arc_points
                                .push(app.world_to_screen(start + vec2(a.cos(), a.sin()) * radius));
                        }

                        painter.add(Shape::line(
                            arc_points,
                            Stroke::new(2.0, Color32::from_rgb(0, 150, 255)),
                        ));
                        let mid_angle = angle1 + diff / 2.0;
                        let text_pos = start
                            + vec2(mid_angle.cos(), mid_angle.sin())
                                * (radius + (15.0 / app.zoom_factor));

                        painter.text(
                            app.world_to_screen(text_pos),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.1}°", display_angle),
                            egui::FontId::proportional(12.0),
                            Color32::from_rgb(0, 120, 210),
                        );
                    } else {
                        let mut abs_angle = (end - start).angle().to_degrees();
                        if abs_angle < 0.0 {
                            abs_angle += 360.0;
                        }
                        painter.text(
                            app.world_to_screen(
                                start.lerp(end, 0.5) + vec2(0.0, 15.0 / app.zoom_factor),
                            ),
                            egui::Align2::CENTER_TOP,
                            format!("{:.1}°", abs_angle),
                            egui::FontId::proportional(12.0),
                            Color32::DARK_GRAY,
                        );
                    }
                }
            }
        }
    }

    for (idx, wall) in app.walls.iter().enumerate() {
        let mut is_selected = app.selected_walls.contains(&idx);
        if let Some(rect) = app.selection_rect {
            if rect.contains(wall.start) || rect.contains(wall.end) {
                is_selected = true;
            }
        }

        let is_hovered = hovered_wall_idx == Some(idx) && app.current_tool == Tool::Select;
        let wall_color = if is_selected {
            Color32::from_rgb(50, 100, 255)
        } else if is_hovered {
            Color32::from_rgb(120, 160, 255)
        } else {
            Color32::BLACK
        };
        let stroke_width = if is_selected || is_hovered { 8.0 } else { 6.0 };

        painter.line_segment(
            [
                app.world_to_screen(wall.start),
                app.world_to_screen(wall.end),
            ],
            Stroke::new(stroke_width, wall_color),
        );

        let node_radius = if is_selected {
            stroke_width
        } else {
            stroke_width / 2.0
        };
        painter.circle_filled(app.world_to_screen(wall.start), node_radius, wall_color);
        painter.circle_filled(app.world_to_screen(wall.end), node_radius, wall_color);

        if is_selected && app.current_tool == Tool::Select {
            let length_m = wall.start.distance(wall.end) / PIXELS_PER_METER;
            let mut angle = (wall.end - wall.start).angle().to_degrees();
            if angle < 0.0 {
                angle += 360.0;
            }
            painter.text(
                app.world_to_screen(
                    wall.start.lerp(wall.end, 0.5) + vec2(0.0, -12.0 / app.zoom_factor),
                ),
                egui::Align2::CENTER_BOTTOM,
                format!("{:.2} m | {:.0}°", length_m, angle),
                egui::FontId::proportional(13.0),
                Color32::from_rgb(0, 80, 200),
            );
        }

        if is_hovered && app.current_tool == Tool::Select && !is_selected {
            let mut split_points = vec![wall.start, wall.end];
            for (other_idx, other_wall) in app.walls.iter().enumerate() {
                if other_idx == idx {
                    continue;
                }
                for &ep in &[other_wall.start, other_wall.end] {
                    let proj = closest_point_on_segment(ep, wall.start, wall.end);
                    if ep.distance(proj) < 2.0 {
                        split_points.push(proj);
                    }
                }
            }

            split_points.sort_by(|a, b| {
                wall.start
                    .distance(*a)
                    .partial_cmp(&wall.start.distance(*b))
                    .unwrap()
            });
            split_points.dedup_by(|a, b| a.distance(*b) < 2.0);

            let dir = (wall.end - wall.start).normalized();
            let normal = vec2(-dir.y, dir.x);
            let mut offset_dir = normal;
            if let Some(mouse_pos) = pointer {
                if (mouse_pos - wall.start).dot(normal) < 0.0 {
                    offset_dir = -normal;
                }
            }

            let dim_color = Color32::from_rgb(255, 140, 0);
            let text_color = Color32::from_rgb(220, 100, 0);
            let stroke = Stroke::new(1.5, dim_color);
            let faint_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 140, 0, 80));

            let total_offset = offset_dir * (40.0 / app.zoom_factor);
            let p_start = wall.start + total_offset;
            let p_end = wall.end + total_offset;
            let total_len = wall.start.distance(wall.end) / PIXELS_PER_METER;

            painter.line_segment(
                [app.world_to_screen(p_start), app.world_to_screen(p_end)],
                stroke,
            );
            painter.line_segment(
                [
                    app.world_to_screen(wall.start),
                    app.world_to_screen(p_start),
                ],
                faint_stroke,
            );
            painter.line_segment(
                [app.world_to_screen(wall.end), app.world_to_screen(p_end)],
                faint_stroke,
            );
            painter.circle_filled(app.world_to_screen(p_start), 2.5, dim_color);
            painter.circle_filled(app.world_to_screen(p_end), 2.5, dim_color);

            painter.text(
                app.world_to_screen(
                    p_start.lerp(p_end, 0.5) + offset_dir * (10.0 / app.zoom_factor),
                ),
                egui::Align2::CENTER_CENTER,
                format!("{:.2} m", total_len),
                egui::FontId::proportional(14.0),
                text_color,
            );

            if split_points.len() > 2 {
                let sub_offset = offset_dir * (20.0 / app.zoom_factor);
                for i in 0..split_points.len() - 1 {
                    let s1 = split_points[i];
                    let s2 = split_points[i + 1];
                    let ps1 = s1 + sub_offset;
                    let ps2 = s2 + sub_offset;
                    let sub_len = s1.distance(s2) / PIXELS_PER_METER;

                    if sub_len > 0.1 {
                        painter.line_segment(
                            [app.world_to_screen(ps1), app.world_to_screen(ps2)],
                            stroke,
                        );
                        painter.line_segment(
                            [app.world_to_screen(s1), app.world_to_screen(ps1)],
                            faint_stroke,
                        );
                        painter.line_segment(
                            [app.world_to_screen(s2), app.world_to_screen(ps2)],
                            faint_stroke,
                        );
                        painter.circle_filled(app.world_to_screen(ps1), 2.5, dim_color);
                        painter.circle_filled(app.world_to_screen(ps2), 2.5, dim_color);
                        painter.text(
                            app.world_to_screen(
                                ps1.lerp(ps2, 0.5) + offset_dir * (10.0 / app.zoom_factor),
                            ),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.2} m", sub_len),
                            egui::FontId::proportional(12.0),
                            text_color,
                        );
                    }
                }
            }
        }
    }

    if app.current_tool == Tool::Select && app.dragging_endpoints.is_empty() {
        if !hovered_endpoints.is_empty() {
            let (idx, is_start) = hovered_endpoints[0];

            // FIX: Safely try to get the wall. If it was just deleted, this returns None and safely skips drawing!
            if let Some(hovered_wall) = app.walls.get(idx) {
                let ep = if is_start {
                    hovered_wall.start
                } else {
                    hovered_wall.end
                };

                painter.circle_stroke(
                    app.world_to_screen(ep),
                    8.0,
                    Stroke::new(2.5, Color32::from_rgb(255, 150, 0)),
                );
                painter.circle_filled(
                    app.world_to_screen(ep),
                    4.0,
                    Color32::from_rgba_unmultiplied(255, 150, 0, 150),
                );

                if hovered_endpoints.len() == 1 {
                    let mut shared = false;
                    for (i, w) in app.walls.iter().enumerate() {
                        if i != idx && (w.start.distance(ep) < 2.0 || w.end.distance(ep) < 2.0) {
                            shared = true;
                            break;
                        }
                    }
                    if shared {
                        painter.line_segment(
                            [
                                app.world_to_screen(hovered_wall.start),
                                app.world_to_screen(hovered_wall.end),
                            ],
                            Stroke::new(4.0, Color32::from_rgba_unmultiplied(255, 150, 0, 200)),
                        );
                    }
                }
            }
        }
    }

    if let Some(rect) = app.selection_rect {
        let screen_rect =
            Rect::from_min_max(app.world_to_screen(rect.min), app.world_to_screen(rect.max));
        painter.rect_stroke(
            screen_rect,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(50, 100, 255)),
            egui::StrokeKind::Middle,
        );
        painter.rect_filled(
            screen_rect,
            0.0,
            Color32::from_rgba_unmultiplied(50, 100, 255, 20),
        );
    }
}
