use crate::{
    app::{RoomPlannerApp, PIXELS_PER_METER, WALL_THICKNESS},
    furniture::{draw_order, lamp, Shape as PartShape},
    geometry::{
        math::{closest_point_on_segment, ray_segment_distance},
        openings::{opening_at, opening_center, wall_frame},
        rooms::triangulate,
    },
    models::{Furniture, FurnitureKind, LampStyle, Opening, OpeningKind, Tool, Wall},
    tools::input::viewer_handles,
    ui::theme::{ACCENT, TEXT_MUTED},
};
use eframe::egui::{self, emath::Rot2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

const WALL_COLOR: Color32 = Color32::from_rgb(55, 60, 70);
const SELECTED: Color32 = ACCENT;
const HOVERED: Color32 = Color32::from_rgb(120, 160, 245);

pub fn draw_scene(
    app: &RoomPlannerApp,
    painter: &Painter,
    pointer: Option<Pos2>,
    hovered_endpoints: &[(usize, bool)],
    hovered_wall_idx: Option<usize>,
    active_alignments: &[Pos2],
    snapped_preview: Option<Pos2>,
    snapped_wall_idx: Option<usize>,
    hovered_furniture: Option<usize>,
) {
    for room_polygon in &app.rooms {
        // A pale version of the real floor, so furniture stays readable on top of it.
        let [r, g, b] = app
            .finishes
            .floor
            .color()
            .map(|c| (c as f32 * 0.35 + 255.0 * 0.65) as u8);
        let floor_color = Color32::from_rgb(r, g, b);
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

    // Pools of light under lamps that are on, below everything else.
    for item in &app.furniture {
        draw_light_pool(app, painter, item);
    }

    for idx in draw_order(&app.furniture) {
        let item = &app.furniture[idx];
        let selected = app.selected_furniture == Some(idx);
        let hovered = hovered_furniture == Some(idx) && app.current_tool == Tool::Select;
        draw_furniture(app, painter, item, selected, hovered);
    }

    for room in &app.rooms {
        draw_room_label(app, painter, room);
    }

    if let Some(item) = app.selected_furniture.and_then(|i| app.furniture.get(i)) {
        draw_wall_distances(app, painter, item);
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
        if let Some(drag) = &app.furniture_drag {
            for wall in drag.snapped_walls.iter().filter_map(|&i| app.walls.get(i)) {
                painter.line_segment(
                    [
                        app.world_to_screen(wall.start),
                        app.world_to_screen(wall.end),
                    ],
                    Stroke::new(4.0, Color32::from_rgb(0, 190, 120)),
                );
            }
        }

        if let Some(rect) = app.selection_rect {
            if rect.contains(wall.start) || rect.contains(wall.end) {
                is_selected = true;
            }
        }

        let is_hovered = hovered_wall_idx == Some(idx) && app.current_tool == Tool::Select;
        let wall_color = if is_selected {
            SELECTED
        } else if is_hovered {
            HOVERED
        } else {
            WALL_COLOR
        };
        // Walls are drawn at their real thickness, but never thinner than a few pixels.
        let base_width = (WALL_THICKNESS * PIXELS_PER_METER * app.zoom_factor).max(4.0);
        let stroke_width = if is_selected || is_hovered {
            base_width + 2.0
        } else {
            base_width
        };

        painter.line_segment(
            [
                app.world_to_screen(wall.start),
                app.world_to_screen(wall.end),
            ],
            Stroke::new(stroke_width, wall_color),
        );

        let node_radius = if is_selected {
            stroke_width / 2.0 + 2.0
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

    // Windows and doors are drawn over the walls they sit in.
    let hovered_opening = pointer
        .filter(|_| app.current_tool == Tool::Select)
        .and_then(|p| opening_at(&app.walls, p, app.zoom_factor));
    for (wi, wall) in app.walls.iter().enumerate() {
        for (oi, opening) in wall.openings.iter().enumerate() {
            let state = if app.selected_opening == Some((wi, oi)) {
                OpeningState::Selected
            } else if hovered_opening == Some((wi, oi)) {
                OpeningState::Hovered
            } else {
                OpeningState::Normal
            };
            draw_opening(app, painter, wall, opening, state);
        }
    }
    if let Some((wi, opening)) = &app.opening_preview {
        if let Some(wall) = app.walls.get(*wi) {
            draw_opening(app, painter, wall, opening, OpeningState::Preview);
            draw_opening_dimensions(app, painter, wall, opening);
        }
    }
    if let Some((wi, oi)) = app.selected_opening {
        if let Some((wall, opening)) = app
            .walls
            .get(wi)
            .and_then(|w| Some((w, w.openings.get(oi)?)))
        {
            draw_opening_dimensions(app, painter, wall, opening);
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

    if let Some(drag) = &app.furniture_drag {
        for wall in drag.snapped_walls.iter().filter_map(|&i| app.walls.get(i)) {
            painter.line_segment(
                [
                    app.world_to_screen(wall.start),
                    app.world_to_screen(wall.end),
                ],
                Stroke::new(4.0, Color32::from_rgb(0, 190, 120)),
            );
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

/// Background grid: 10 cm squares with a stronger line every meter, fading out when zoomed away.
pub fn draw_grid(app: &RoomPlannerApp, painter: &Painter, rect: Rect) {
    let world = app.visible_world_rect;
    let meter_px = PIXELS_PER_METER * app.zoom_factor;
    let mut levels = vec![(PIXELS_PER_METER, Color32::from_rgb(226, 228, 233))];
    if meter_px / 10.0 >= 7.0 {
        levels.insert(
            0,
            (PIXELS_PER_METER / 10.0, Color32::from_rgb(240, 241, 244)),
        );
    } else if meter_px < 12.0 {
        levels = vec![(PIXELS_PER_METER * 5.0, Color32::from_rgb(226, 228, 233))];
    }
    for (step, color) in levels {
        let stroke = Stroke::new(1.0, color);
        let mut x = (world.min.x / step).floor() * step;
        while x <= world.max.x {
            let sx = app.world_to_screen(Pos2::new(x, 0.0)).x;
            painter.line_segment(
                [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
                stroke,
            );
            x += step;
        }
        let mut y = (world.min.y / step).floor() * step;
        while y <= world.max.y {
            let sy = app.world_to_screen(Pos2::new(0.0, y)).y;
            painter.line_segment(
                [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
                stroke,
            );
            y += step;
        }
    }

    // Scale bar in the bottom-left corner.
    let length_m = [0.5, 1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|m| m * meter_px >= 60.0)
        .unwrap_or(10.0);
    let start = rect.left_bottom() + vec2(16.0, -16.0);
    let end = start + vec2(length_m * meter_px, 0.0);
    let stroke = Stroke::new(1.5, TEXT_MUTED);
    painter.line_segment([start, end], stroke);
    painter.line_segment([start, start - vec2(0.0, 5.0)], stroke);
    painter.line_segment([end, end - vec2(0.0, 5.0)], stroke);
    painter.text(
        start.lerp(end, 0.5) - vec2(0.0, 6.0),
        egui::Align2::CENTER_BOTTOM,
        format!("{length_m} m"),
        egui::FontId::proportional(11.0),
        TEXT_MUTED,
    );
}

fn draw_room_label(app: &RoomPlannerApp, painter: &Painter, polygon: &[Pos2]) {
    // Shoelace formula for area and centroid.
    let mut area = 0.0;
    let mut centroid = Vec2::ZERO;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let cross = a.x * b.y - b.x * a.y;
        area += cross;
        centroid += (a.to_vec2() + b.to_vec2()) * cross;
    }
    if area.abs() < 1.0 {
        return;
    }
    let centroid = (centroid / (3.0 * area)).to_pos2();
    let area_m2 = area.abs() / 2.0 / (PIXELS_PER_METER * PIXELS_PER_METER);
    if area_m2 * PIXELS_PER_METER * app.zoom_factor < 20.0 {
        return;
    }
    painter.text(
        app.world_to_screen(centroid),
        egui::Align2::CENTER_CENTER,
        format!("{area_m2:.1} m²"),
        egui::FontId::proportional(13.0),
        Color32::from_rgba_unmultiplied(110, 100, 85, 170),
    );
}

fn draw_light_pool(app: &RoomPlannerApp, painter: &Painter, item: &Furniture) {
    let Some(light) = lamp::light(item) else {
        return;
    };
    let center = app.world_to_screen(item.local_to_world(light.local[0], light.local[2]));
    // Rough radius where the floor is still noticeably lit.
    let reach_m = 0.9 * light.color.iter().cloned().fold(0.0, f32::max).sqrt() + 0.6;
    let radius = reach_m * PIXELS_PER_METER * app.zoom_factor;
    let [r, g, b] = lamp::kelvin_to_srgb(match item.kind {
        FurnitureKind::Lamp { kelvin, .. } => kelvin,
        _ => 2700.0,
    });
    for step in 0..12 {
        let t = 1.0 - step as f32 / 12.0;
        painter.circle_filled(
            center,
            radius * t,
            Color32::from_rgba_unmultiplied(r, g, b.saturating_sub(40), 8),
        );
    }
}

/// Draws a piece of furniture top-down: each part's outline, lowest first, so taller parts
/// (a sofa's back and arms, a table top) end up on top.
fn draw_furniture(
    app: &RoomPlannerApp,
    painter: &Painter,
    item: &Furniture,
    selected: bool,
    hovered: bool,
) {
    let outline = Stroke::new(1.0, Color32::from_rgb(90, 90, 95));
    let to_screen = |x: f32, z: f32| app.world_to_screen(item.local_to_world(x, z));
    // Things hanging above head height are drawn see-through, like on architectural plans.
    let overhead = item.elevation > 1.2
        || matches!(
            item.kind,
            FurnitureKind::Lamp {
                style: LampStyle::Pendant,
                ..
            }
        );
    let alpha = if overhead { 120 } else { 235 };

    let mut parts = item.parts();
    parts.sort_by(|a, b| a.max[1].total_cmp(&b.max[1]));

    if parts.is_empty() {
        // Imported model: draw its bounding footprint.
        let [r, g, b] = item.color;
        let corners: Vec<Pos2> = item
            .footprint()
            .iter()
            .map(|p| app.world_to_screen(*p))
            .collect();
        painter.add(Shape::convex_polygon(
            corners.clone(),
            Color32::from_rgba_unmultiplied(r, g, b, alpha),
            outline,
        ));
        painter.line_segment(
            [corners[0], corners[2]],
            Stroke::new(0.5, Color32::from_gray(150)),
        );
        painter.line_segment(
            [corners[1], corners[3]],
            Stroke::new(0.5, Color32::from_gray(150)),
        );
    }

    for part in &parts {
        // Hairline parts like a pendant's cord would only add clutter from above.
        if part.max[0] - part.min[0] < 0.015 {
            continue;
        }
        let [r, g, b] = part.color;
        let fill = Color32::from_rgba_unmultiplied(r, g, b, alpha);
        let points: Vec<Pos2> = match part.shape {
            PartShape::Box => vec![
                to_screen(part.min[0], part.min[2]),
                to_screen(part.max[0], part.min[2]),
                to_screen(part.max[0], part.max[2]),
                to_screen(part.min[0], part.max[2]),
            ],
            shape => {
                let scale = match shape {
                    PartShape::Cone { top_scale } => top_scale.max(1.0),
                    _ => 1.0,
                };
                let c = [
                    (part.min[0] + part.max[0]) / 2.0,
                    (part.min[2] + part.max[2]) / 2.0,
                ];
                let r = [
                    (part.max[0] - part.min[0]) / 2.0 * scale,
                    (part.max[2] - part.min[2]) / 2.0 * scale,
                ];
                (0..32)
                    .map(|i| {
                        let a = i as f32 / 32.0 * std::f32::consts::TAU;
                        to_screen(c[0] + r[0] * a.cos(), c[1] + r[1] * a.sin())
                    })
                    .collect()
            }
        };
        painter.add(Shape::convex_polygon(points, fill, outline));
    }

    if item.is_lamp() {
        // Classic lamp symbol: a circle with a cross.
        let center = app.world_to_screen(item.position);
        let r = (item.width.min(item.depth) * PIXELS_PER_METER * app.zoom_factor * 0.25)
            .clamp(3.0, 9.0);
        let stroke = Stroke::new(1.2, Color32::from_rgb(80, 70, 40));
        painter.circle_stroke(center, r, stroke);
        let d = r * std::f32::consts::FRAC_1_SQRT_2;
        painter.line_segment([center - vec2(d, d), center + vec2(d, d)], stroke);
        painter.line_segment([center - vec2(d, -d), center + vec2(d, -d)], stroke);
    } else if !matches!(item.kind, FurnitureKind::Plant) {
        // Mark the front edge so rotation is readable at a glance.
        let (hw, hd) = (item.width / 2.0, item.depth / 2.0);
        painter.line_segment(
            [to_screen(-hw * 0.6, hd), to_screen(hw * 0.6, hd)],
            Stroke::new(2.0, Color32::from_gray(110)),
        );
    }

    if selected || hovered {
        let color = if selected { SELECTED } else { HOVERED };
        let corners: Vec<Pos2> = item
            .footprint()
            .iter()
            .map(|p| app.world_to_screen(*p))
            .collect();
        painter.add(Shape::closed_line(
            corners.clone(),
            Stroke::new(if selected { 2.0 } else { 1.5 }, color),
        ));
        if selected {
            for corner in corners {
                painter.rect_filled(
                    Rect::from_center_size(corner, vec2(6.0, 6.0)),
                    1.0,
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    Rect::from_center_size(corner, vec2(6.0, 6.0)),
                    1.0,
                    Stroke::new(1.5, color),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }

    let screen_width = item.width * PIXELS_PER_METER * app.zoom_factor;
    let screen_depth = item.depth * PIXELS_PER_METER * app.zoom_factor;
    if selected || hovered || screen_width.min(screen_depth) > 45.0 {
        let center = app.world_to_screen(item.position);
        let mut text = format!(
            "{}\n{:.0} × {:.0} cm",
            item.name,
            item.width * 100.0,
            item.depth * 100.0
        );
        if item.options.len() > 1 {
            let letter = (b'A' + (item.active_option % 26) as u8) as char;
            text += &format!("  ·  option {letter}/{}", item.options.len());
        }
        let font = egui::FontId::proportional(11.0);
        let galley = painter.layout(text, font, Color32::from_rgb(40, 40, 45), f32::INFINITY);
        let offset = if item.is_lamp() {
            vec2(0.0, screen_depth.max(20.0) / 2.0 + 14.0)
        } else {
            Vec2::ZERO
        };
        let rect = Rect::from_center_size(center + offset, galley.size()).expand2(vec2(5.0, 3.0));
        painter.rect_filled(
            rect,
            4.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 215),
        );
        if item.options.len() > 1 {
            painter.rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        painter.galley(rect.min + vec2(5.0, 3.0), galley, Color32::BLACK);
    }
}

/// Dimension lines from the selected piece to the nearest wall in each of its four directions.
fn draw_wall_distances(app: &RoomPlannerApp, painter: &Painter, item: &Furniture) {
    let color = Color32::from_rgb(220, 90, 30);
    let stroke = Stroke::new(1.0, color);
    let rot = Rot2::from_angle(item.rotation);
    let (hw, hd) = (item.width / 2.0, item.depth / 2.0);
    let half_wall = WALL_THICKNESS / 2.0 * PIXELS_PER_METER;

    for (local_dir, half) in [
        (vec2(1.0, 0.0), hw),
        (vec2(-1.0, 0.0), hw),
        (vec2(0.0, 1.0), hd),
        (vec2(0.0, -1.0), hd),
    ] {
        let dir = rot * local_dir;
        let origin = item.local_to_world(local_dir.x * half, local_dir.y * half);
        let hit = app
            .walls
            .iter()
            .filter_map(|w| ray_segment_distance(origin, dir, w.start, w.end))
            .fold(f32::INFINITY, f32::min);
        let gap = hit - half_wall;
        if !hit.is_finite() || gap < 0.5 || gap > 8.0 * PIXELS_PER_METER {
            continue;
        }
        let a = app.world_to_screen(origin);
        let b = app.world_to_screen(origin + dir * gap);
        if a.distance(b) < 12.0 {
            continue;
        }
        painter.add(Shape::dashed_line(&[a, b], stroke, 4.0, 3.0));
        let tick = dir.rot90() * 4.0;
        painter.line_segment([b - tick, b + tick], stroke);
        let label = format!("{:.0} cm", gap / PIXELS_PER_METER * 100.0);
        let galley = painter.layout_no_wrap(label, egui::FontId::proportional(11.0), color);
        let rect = Rect::from_center_size(a.lerp(b, 0.5), galley.size()).expand(2.0);
        painter.rect_filled(
            rect,
            3.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 230),
        );
        painter.galley(rect.min + vec2(2.0, 2.0), galley, color);
    }
}

/// The walk-mode viewer: a person with a view cone and a draggable arrow to turn.
pub fn draw_viewer(app: &RoomPlannerApp, painter: &Painter) {
    let (body, handle) = viewer_handles(app);
    let half_fov = 30.0_f32.to_radians();
    let cone_len = 90.0;
    let cone: Vec<Pos2> = std::iter::once(body)
        .chain((0..=12).map(|i| {
            let a = app.camera_angle - half_fov + 2.0 * half_fov * i as f32 / 12.0;
            body + vec2(a.cos(), a.sin()) * cone_len
        }))
        .collect();
    painter.add(Shape::convex_polygon(
        cone,
        Color32::from_rgba_unmultiplied(37, 99, 235, 28),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(37, 99, 235, 90)),
    ));
    painter.line_segment([body, handle], Stroke::new(2.0, ACCENT));
    painter.circle_filled(handle, 6.0, Color32::WHITE);
    painter.circle_stroke(handle, 6.0, Stroke::new(2.0, ACCENT));
    painter.circle_filled(body, 9.0, ACCENT);
    painter.circle_stroke(body, 9.0, Stroke::new(2.0, Color32::WHITE));
    painter.text(
        body + vec2(0.0, 14.0),
        egui::Align2::CENTER_TOP,
        "You",
        egui::FontId::proportional(11.0),
        ACCENT,
    );
}

/// Furniture being dragged in from the library, before it is dropped.
pub fn draw_drop_preview(app: &RoomPlannerApp, painter: &Painter, item: &Furniture) {
    draw_furniture(app, painter, item, true, false);
}

#[derive(Clone, Copy, PartialEq)]
pub enum OpeningState {
    Normal,
    Hovered,
    Selected,
    /// Being placed with the window/door tool or dragged in from the library.
    Preview,
}

/// Plan symbol for a window or door: a gap in the wall with glass lines, and for doors
/// the leaf and its swing arc.
pub fn draw_opening(
    app: &RoomPlannerApp,
    painter: &Painter,
    wall: &Wall,
    opening: &Opening,
    state: OpeningState,
) {
    let (dir, normal) = wall_frame(wall);
    let center = opening_center(wall, opening);
    let half = opening.width / 2.0 * PIXELS_PER_METER;
    let (a, b) = (center - dir * half, center + dir * half);
    // Same on-screen thickness as the wall stroke, in world units.
    let thickness =
        (WALL_THICKNESS * PIXELS_PER_METER * app.zoom_factor).max(4.0) / app.zoom_factor;
    let across = normal * thickness / 2.0;
    let s = |p: Pos2| app.world_to_screen(p);

    let color = match state {
        OpeningState::Selected => SELECTED,
        OpeningState::Hovered | OpeningState::Preview => HOVERED,
        OpeningState::Normal => WALL_COLOR,
    };
    let line = Stroke::new(1.2, color);

    // Cut the gap out of the wall.
    painter.add(Shape::convex_polygon(
        vec![s(a - across), s(b - across), s(b + across), s(a + across)],
        Color32::WHITE,
        Stroke::NONE,
    ));
    // Jambs.
    painter.line_segment([s(a - across), s(a + across)], Stroke::new(2.0, color));
    painter.line_segment([s(b - across), s(b + across)], Stroke::new(2.0, color));

    if opening.kind.glazed() {
        let inset = across * 0.25;
        painter.line_segment([s(a - inset), s(b - inset)], line);
        painter.line_segment([s(a + inset), s(b + inset)], line);
        if opening.kind == OpeningKind::Window {
            painter.line_segment([s(a - across), s(b - across)], Stroke::new(0.8, color));
            painter.line_segment([s(a + across), s(b + across)], Stroke::new(0.8, color));
        }
    }

    if opening.kind.swings() {
        let (hinge, free) = if opening.flip_hinge { (b, a) } else { (a, b) };
        let side = if opening.flip_side { -normal } else { normal };
        let hinge = hinge + side * thickness / 2.0;
        let free = free + side * thickness / 2.0;
        let width = opening.width * PIXELS_PER_METER;
        let open_end = hinge + side * width;
        painter.line_segment([s(hinge), s(open_end)], Stroke::new(2.0, color));
        // Quarter circle from the open leaf back to the closed position.
        let start_angle = (open_end - hinge).angle();
        let mut sweep = (free - hinge).angle() - start_angle;
        if sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        } else if sweep < -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }
        let arc: Vec<Pos2> = (0..=24)
            .map(|i| {
                let t = start_angle + sweep * i as f32 / 24.0;
                s(hinge + vec2(t.cos(), t.sin()) * width)
            })
            .collect();
        painter.add(Shape::dashed_line(&arc, Stroke::new(1.0, color), 4.0, 3.0));
    }

    if state == OpeningState::Selected || state == OpeningState::Preview {
        let pad = across * 1.6;
        painter.add(Shape::closed_line(
            vec![s(a - pad), s(b - pad), s(b + pad), s(a + pad)],
            Stroke::new(1.5, color),
        ));
    }
}

/// Distances from an opening to both ends of its wall, measured along the wall.
fn draw_opening_dimensions(
    app: &RoomPlannerApp,
    painter: &Painter,
    wall: &Wall,
    opening: &Opening,
) {
    let (dir, normal) = wall_frame(wall);
    let length = wall.length();
    let center = opening.clamped_offset(length);
    let (left, right) = (center - opening.width / 2.0, center + opening.width / 2.0);
    let color = Color32::from_rgb(220, 90, 30);
    let offset = normal * (WALL_THICKNESS * PIXELS_PER_METER / 2.0 + 18.0 / app.zoom_factor);
    let at = |m: f32| app.world_to_screen(wall.start + dir * m * PIXELS_PER_METER + offset);

    for (from, to) in [(0.0, left), (left, right), (right, length)] {
        let gap = to - from;
        if gap < 0.005 {
            continue;
        }
        let (p, q) = (at(from), at(to));
        let stroke = Stroke::new(1.0, color);
        painter.line_segment([p, q], stroke);
        let tick = (q - p).normalized().rot90() * 4.0;
        painter.line_segment([p - tick, p + tick], stroke);
        painter.line_segment([q - tick, q + tick], stroke);
        if p.distance(q) > 30.0 {
            let galley = painter.layout_no_wrap(
                format!("{:.0} cm", gap * 100.0),
                egui::FontId::proportional(11.0),
                color,
            );
            let rect = Rect::from_center_size(p.lerp(q, 0.5), galley.size()).expand(2.0);
            painter.rect_filled(
                rect,
                3.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 230),
            );
            painter.galley(rect.min + vec2(2.0, 2.0), galley, color);
        }
    }
}
