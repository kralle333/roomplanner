use crate::{
    app::{CameraMode, RoomPlannerApp, PIXELS_PER_METER, WALL_THICKNESS},
    geometry::{
        math::{closest_point_on_segment, ray_segment_distance},
        openings::{opening_at, opening_center, wall_frame},
        walls::{clear_length, room_interior, touching_walls, wall_face},
    },
    models::{Furniture, Opening, Tool, Wall},
    tools::input::rotation_handle,
    ui::theme::{ACCENT, TEXT_MUTED},
};
use std::collections::HashSet;

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
    for idx in 0..app.furniture.len() {
        let item = &app.furniture[idx];
        let selected = app.is_furniture_selected(idx)
            || app
                .selection_rect
                .is_some_and(|rect| rect.contains(item.position));
        let hovered = hovered_furniture == Some(idx) && app.current_tool == Tool::Select;
        draw_furniture_overlay(app, painter, item, selected, hovered);
    }

    for room in &app.rooms {
        draw_room_label(app, painter, room);
    }

    if let Some(item) = app.selected_furniture.and_then(|i| app.furniture.get(i)) {
        if app.extra_furniture.is_empty() {
            draw_wall_distances(app, painter, item);
        }
        if app.current_tool == Tool::Select {
            draw_rotation_handle(app, painter, item);
        }
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
                    let offset = offset_dir * (25.0 / app.scale());

                    // Clear distances along the face on this side, from the walls already
                    // there to the surfaces of the new wall.
                    let side = if offset_dir.dot(wall_frame(wall).1) >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let face = wall_face(&app.walls, wall_idx, side);
                    let u = (snap_pos - wall.start).dot(dir);
                    let (from, to) = face
                        .pieces
                        .iter()
                        .copied()
                        .find(|&(a, b)| u >= a - 1.0 && u <= b + 1.0)
                        .unwrap_or((face.start, face.end));
                    let half_wall = WALL_THICKNESS * PIXELS_PER_METER / 2.0;
                    let (face_start, face_end) = (face.point(wall, from), face.point(wall, to));

                    let p_start = face_start + offset;
                    let p_end = face_end + offset;
                    let p_cursor = snap_pos + offset;

                    let dist_start = (u - half_wall - from) / PIXELS_PER_METER;
                    let dist_end = (to - u - half_wall) / PIXELS_PER_METER;

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
                            app.world_to_screen(face_start),
                            app.world_to_screen(p_start),
                        ],
                        faint,
                    );
                    painter.line_segment(
                        [app.world_to_screen(snap_pos), app.world_to_screen(p_cursor)],
                        faint,
                    );
                    painter.line_segment(
                        [app.world_to_screen(face_end), app.world_to_screen(p_end)],
                        faint,
                    );

                    painter.circle_filled(app.world_to_screen(p_start), 2.0, dim_color);
                    painter.circle_filled(app.world_to_screen(p_cursor), 3.0, dim_color);
                    painter.circle_filled(app.world_to_screen(p_end), 2.0, dim_color);

                    if dist_start > 0.1 {
                        painter.text(
                            app.world_to_screen(
                                p_start.lerp(p_cursor, 0.5) + offset_dir * (8.0 / app.scale()),
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
                                p_cursor.lerp(p_end, 0.5) + offset_dir * (8.0 / app.scale()),
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

                if start.distance(end) > (5.0 / app.scale()) {
                    // Measured as it will be once placed, against the walls it joins.
                    let mut walls = app.walls.clone();
                    walls.push(Wall::new(start, end));
                    let length_m = clear_length(&walls, walls.len() - 1);
                    painter.text(
                        app.world_to_screen(start.lerp(end, 0.5) + vec2(0.0, -15.0 / app.scale())),
                        egui::Align2::CENTER_BOTTOM,
                        if app.length_input.is_empty() {
                            format!(
                                "{:.2} m\n[ESC] to cancel · type a length for exact size",
                                length_m
                            )
                        } else {
                            let unit = if app.length_input.contains(['.', ',']) {
                                "m"
                            } else {
                                "cm"
                            };
                            format!(
                                "⌨ {} {unit}  ·  Enter to place\n(inside {:.2} m)",
                                app.length_input, length_m
                            )
                        },
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
                        let radius = 30.0 / app.scale();
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
                                * (radius + (15.0 / app.scale()));

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
                                start.lerp(end, 0.5) + vec2(0.0, 15.0 / app.scale()),
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

    // While walls are being moved, the walls joined to them show their changing sizes.
    let affected_walls: HashSet<usize> = if app.wall_drag.is_some() {
        touching_walls(&app.walls, &app.selected_walls)
    } else if let Some(&(i, is_start)) = app.dragging_endpoints.first() {
        // A dragged corner only changes the walls it lies on.
        let corner = if is_start {
            app.walls[i].start
        } else {
            app.walls[i].end
        };
        (0..app.walls.len())
            .filter(|&w| !app.dragging_endpoints.iter().any(|&(d, _)| d == w))
            .filter(|&w| {
                let wall = &app.walls[w];
                corner.distance(closest_point_on_segment(corner, wall.start, wall.end)) < 2.0
            })
            .collect()
    } else {
        HashSet::new()
    };

    for (idx, wall) in app.walls.iter().enumerate() {
        let mut is_selected = app.selected_walls.contains(&idx);

        if let Some(rect) = app.selection_rect {
            if rect.contains(wall.start) || rect.contains(wall.end) {
                is_selected = true;
            }
        }

        let is_hovered = hovered_wall_idx == Some(idx) && app.current_tool == Tool::Select;
        if is_selected {
            // The wall itself is tinted in 3D; mark its corners so they can be grabbed.
            for p in [wall.start, wall.end] {
                let s = app.world_to_screen(p);
                painter.circle_filled(s, 4.5, Color32::WHITE);
                painter.circle_stroke(s, 4.5, Stroke::new(2.0, SELECTED));
            }
        }

        if is_selected && app.current_tool == Tool::Select {
            let length_m = clear_length(&app.walls, idx);
            let mut angle = (wall.end - wall.start).angle().to_degrees();
            if angle < 0.0 {
                angle += 360.0;
            }
            painter.text(
                app.world_to_screen(
                    wall.start.lerp(wall.end, 0.5) + vec2(0.0, -12.0 / app.scale()),
                ),
                egui::Align2::CENTER_BOTTOM,
                format!("{:.2} m | {:.0}°", length_m, angle),
                egui::FontId::proportional(13.0),
                Color32::from_rgb(0, 80, 200),
            );
        }

        let stretched = app
            .wall_drag
            .as_ref()
            .and_then(|d| d.stretched_from(idx))
            .map(|old| wall.length() - old);
        if (is_hovered && app.current_tool == Tool::Select && !is_selected)
            || affected_walls.contains(&idx)
        {
            draw_wall_dimensions(app, painter, idx, pointer, stretched);
        }
    }

    // What a moved wall has lined up with.
    if let Some(drag) = &app.wall_drag {
        let stroke = Stroke::new(1.0, Color32::from_rgb(220, 50, 120));
        for &(a, b) in &drag.guides {
            let (a, b) = (app.world_to_screen(a), app.world_to_screen(b));
            painter.extend(Shape::dashed_line(&[a, b], stroke, 6.0, 4.0));
            painter.circle_stroke(b, 4.0, stroke);
        }
    }

    // Windows and doors are drawn over the walls they sit in.
    let hovered_opening = pointer
        .filter(|_| app.current_tool == Tool::Select && hovered_furniture.is_none())
        .and_then(|p| opening_at(&app.walls, p, app.scale()));
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
            draw_opening_dimensions(app, painter, *wi, opening);
        }
    }
    if let Some((wi, oi)) = app.selected_opening {
        if let Some(opening) = app.walls.get(wi).and_then(|w| w.openings.get(oi)) {
            draw_opening_dimensions(app, painter, wi, opening);
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

    // Lines of other furniture (or walls) a moved or resized piece is snapped to.
    let no_guides = Vec::new();
    let drag_guides = app.furniture_drag.as_ref().map(|d| &d.guides);
    let resize_guides = app.furniture_resize.as_ref().map(|r| &r.guides);
    for guides in [drag_guides, resize_guides] {
        for &(from, to) in guides.unwrap_or(&no_guides) {
            let out = (to - from).normalized() * 0.2 * PIXELS_PER_METER;
            painter.line_segment(
                [
                    app.world_to_screen(from - out),
                    app.world_to_screen(to + out),
                ],
                Stroke::new(1.5, Color32::from_rgb(0, 190, 120)),
            );
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
/// Dimension lines along a wall: its full length, and the pieces between the walls that
/// join it. `change` is how much it was just stretched, in meters.
fn draw_rotation_handle(app: &RoomPlannerApp, painter: &Painter, item: &Furniture) {
    let Some((stem, knob)) = rotation_handle(app) else {
        return;
    };
    let rotating = app.furniture_rotate.is_some();
    painter.line_segment([stem, knob], Stroke::new(1.5, SELECTED));
    painter.circle_filled(knob, 6.0, Color32::WHITE);
    painter.circle_stroke(knob, 6.0, Stroke::new(2.0, SELECTED));
    painter.text(
        knob,
        egui::Align2::CENTER_CENTER,
        "⟳",
        egui::FontId::proportional(9.0),
        SELECTED,
    );
    if rotating {
        let degrees = item.rotation.to_degrees().rem_euclid(360.0).round() % 360.0;
        painter.text(
            knob + (knob - stem).normalized() * 12.0,
            egui::Align2::CENTER_CENTER,
            format!("{degrees:.0}°"),
            egui::FontId::proportional(13.0),
            Color32::from_rgb(0, 80, 200),
        );
    }
}

fn draw_wall_dimensions(
    app: &RoomPlannerApp,
    painter: &Painter,
    idx: usize,
    pointer: Option<Pos2>,
    change: Option<f32>,
) {
    let wall = &app.walls[idx];
    let (_, normal) = wall_frame(wall);
    // Measure the face on the pointer's side, with the dimension lines outside it.
    let side = match pointer {
        Some(p) if (p - wall.start).dot(normal) < 0.0 => -1.0,
        _ => 1.0,
    };
    let face = wall_face(&app.walls, idx, side);
    let offset_dir = normal * side;

    let dim_color = Color32::from_rgb(255, 140, 0);
    let text_color = Color32::from_rgb(220, 100, 0);
    let stroke = Stroke::new(1.5, dim_color);
    let faint_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 140, 0, 80));

    // A dimension line `gap` pixels out from the face, between two distances along the wall.
    let dimension = |from: f32, to: f32, gap: f32, text: String, size: f32| {
        let (s1, s2) = (face.point(wall, from), face.point(wall, to));
        let out = offset_dir * (gap / app.scale());
        let (p1, p2) = (s1 + out, s2 + out);
        painter.line_segment([app.world_to_screen(p1), app.world_to_screen(p2)], stroke);
        painter.line_segment(
            [app.world_to_screen(s1), app.world_to_screen(p1)],
            faint_stroke,
        );
        painter.line_segment(
            [app.world_to_screen(s2), app.world_to_screen(p2)],
            faint_stroke,
        );
        painter.circle_filled(app.world_to_screen(p1), 2.5, dim_color);
        painter.circle_filled(app.world_to_screen(p2), 2.5, dim_color);
        painter.text(
            app.world_to_screen(p1.lerp(p2, 0.5) + offset_dir * (10.0 / app.scale())),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(size),
            text_color,
        );
    };

    let total_len = face.length();
    let total_text = match change {
        Some(change) if change.abs() >= 0.005 => format!("{total_len:.2} m ({change:+.2})"),
        _ => format!("{total_len:.2} m"),
    };
    dimension(face.start, face.end, 40.0, total_text, 14.0);

    if face.pieces.len() > 1 {
        for &(from, to) in &face.pieces {
            let len = (to - from) / PIXELS_PER_METER;
            if len > 0.1 {
                dimension(from, to, 20.0, format!("{len:.2} m"), 12.0);
            }
        }
    }
}

/// Scale bar in the bottom-left corner of the top view.
pub fn draw_scale_bar(app: &RoomPlannerApp, painter: &Painter, rect: Rect) {
    let meter_px = PIXELS_PER_METER * app.zoom_factor;
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
    // The floor area inside the walls, not up to their center lines.
    let polygon = &room_interior(polygon);
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
    if area_m2 * PIXELS_PER_METER * app.scale() < 20.0 {
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

/// The selection outline and the name label, drawn over all furniture so they stay readable.
fn draw_furniture_overlay(
    app: &RoomPlannerApp,
    painter: &Painter,
    item: &Furniture,
    selected: bool,
    hovered: bool,
) {
    if selected || hovered {
        let color = if selected { SELECTED } else { HOVERED };
        let stroke = Stroke::new(if selected { 2.0 } else { 1.5 }, color);
        let footprint = item.footprint();
        let (y0, y1) = (item.elevation, item.elevation + item.height);
        let bottom: Vec<Pos2> = footprint
            .iter()
            .map(|p| app.world3_to_screen(*p, y0))
            .collect();
        let top: Vec<Pos2> = footprint
            .iter()
            .map(|p| app.world3_to_screen(*p, y1))
            .collect();
        // A box around the piece; from straight above it is just the outline.
        painter.add(Shape::closed_line(top.clone(), stroke));
        if app.camera_mode != CameraMode::Top {
            painter.add(Shape::closed_line(bottom.clone(), Stroke::new(1.0, color)));
            for (a, b) in bottom.iter().zip(&top) {
                painter.line_segment([*a, *b], Stroke::new(1.0, color));
            }
        }
        let mut corners = top;
        // Corners and sides can be dragged to resize a single selected piece.
        let primary = app
            .selected_furniture
            .and_then(|i| app.furniture.get(i))
            .is_some_and(|p| std::ptr::eq(p, item));
        if let Some(handles) = crate::tools::input::resize_handles(app).filter(|_| primary) {
            corners = handles.iter().map(|(p, _)| *p).collect();
        }
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

    let screen_depth = item.depth * PIXELS_PER_METER * app.scale();
    if selected || hovered {
        let center = app.world3_to_screen(item.position, item.elevation + item.height);
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

/// Furniture being dragged in from the library, before it is dropped.
pub fn draw_drop_preview(app: &RoomPlannerApp, painter: &Painter, item: &Furniture) {
    draw_furniture_overlay(app, painter, item, true, false);
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
    let thickness = (WALL_THICKNESS * PIXELS_PER_METER * app.scale()).max(4.0) / app.scale();
    let across = normal * thickness / 2.0;
    let s = |p: Pos2| app.world_to_screen(p);

    let color = match state {
        OpeningState::Selected => SELECTED,
        OpeningState::Hovered | OpeningState::Preview => HOVERED,
        OpeningState::Normal => WALL_COLOR,
    };
    // The opening itself is in the 3D walls; from above, doors also get their swing arc.
    let top_view = app.camera_mode == CameraMode::Top;
    if opening.kind.swings() && (top_view || state != OpeningState::Normal) {
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

    if state != OpeningState::Normal {
        let stroke = Stroke::new(
            if state == OpeningState::Hovered {
                1.5
            } else {
                2.0
            },
            color,
        );
        if top_view {
            let pad = across * 1.6;
            painter.add(Shape::closed_line(
                vec![s(a - pad), s(b - pad), s(b + pad), s(a + pad)],
                stroke,
            ));
        } else {
            // Outline the hole in the wall on both faces.
            let top = (opening.sill + opening.height).min(wall.height);
            for side in [-1.0, 1.0] {
                let off = normal * side * WALL_THICKNESS / 2.0 * PIXELS_PER_METER;
                let pts = vec![
                    app.world3_to_screen(a + off, opening.sill),
                    app.world3_to_screen(b + off, opening.sill),
                    app.world3_to_screen(b + off, top),
                    app.world3_to_screen(a + off, top),
                ];
                painter.add(Shape::closed_line(pts, stroke));
            }
        }
    }
}

/// Distances from an opening to both ends of its wall, measured along the wall.
/// Distances from a window or door to the surfaces of the walls around it, measured on the
/// room side (the shorter face).
fn draw_opening_dimensions(app: &RoomPlannerApp, painter: &Painter, wi: usize, opening: &Opening) {
    let wall = &app.walls[wi];
    let (dir, normal) = wall_frame(wall);
    let length = wall.length();
    let center = opening.clamped_offset(length);
    let (left, right) = (center - opening.width / 2.0, center + opening.width / 2.0);
    let color = Color32::from_rgb(220, 90, 30);

    let faces = [
        wall_face(&app.walls, wi, 1.0),
        wall_face(&app.walls, wi, -1.0),
    ];
    let face = if faces[1].length() < faces[0].length() {
        &faces[1]
    } else {
        &faces[0]
    };
    let c = center * PIXELS_PER_METER;
    let (from, to) = face
        .pieces
        .iter()
        .copied()
        .find(|&(a, b)| c >= a && c <= b)
        .unwrap_or((face.start, face.end));
    let (from, to) = (from / PIXELS_PER_METER, to / PIXELS_PER_METER);

    let offset =
        normal * face.side * (WALL_THICKNESS * PIXELS_PER_METER / 2.0 + 18.0 / app.scale());
    let at = |m: f32| app.world_to_screen(wall.start + dir * m * PIXELS_PER_METER + offset);

    for (from, to) in [(from, left), (left, right), (right, to)] {
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
