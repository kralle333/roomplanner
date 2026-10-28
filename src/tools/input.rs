use crate::{
    app::{CameraHandle, CameraMode, FurnitureDrag, RoomPlannerApp, PIXELS_PER_METER},
    furniture::furniture_at,
    geometry::{
        openings::{opening_at, place_on_wall, wall_frame},
        rooms::extract_rooms,
        snapping::{compute_snapping, find_closest_wall, get_hovered_endpoints, snap_furniture},
    },
    models::{Opening, Tool, Wall},
    ui::furniture_panel::rotate,
};
use eframe::egui::{self, Pos2, Rect, Response, Ui};

pub fn handle_input(
    app: &mut RoomPlannerApp,
    ui: &Ui,
    response: &Response,
    pointer: Option<Pos2>,
    interact_pointer: Option<Pos2>,
) -> (Vec<Pos2>, Option<Pos2>, Option<usize>) {
    let mut active_alignments = Vec::new();
    let mut snapped_preview = None;
    let mut snapped_wall_idx = None;

    // Don't treat typing in the side panel (e.g. a furniture name) as canvas shortcuts.
    let shortcuts_enabled = !ui.ctx().egui_wants_keyboard_input();

    if shortcuts_enabled && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        app.wall_start_point = None;
        if app.current_tool == Tool::PlaceOpening {
            app.current_tool = Tool::Select;
            app.opening_preview = None;
        }
    }

    if shortcuts_enabled
        && ui
            .ctx()
            .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
    {
        app.delete_selection();
    }

    if shortcuts_enabled {
        let (rotate_pressed, shift, next_option) = ui.ctx().input(|i| {
            let plain = !i.modifiers.command && !i.modifiers.alt;
            (
                plain && i.key_pressed(egui::Key::R),
                i.modifiers.shift,
                plain && i.key_pressed(egui::Key::O),
            )
        });
        if let Some(item) = app
            .selected_furniture
            .and_then(|i| app.furniture.get_mut(i))
        {
            if rotate_pressed {
                rotate(item, if shift { -90.0 } else { 90.0 });
                if let Some(drag) = &mut app.furniture_drag {
                    drag.raw_rotation = item.rotation;
                }
            }
            if next_option {
                item.next_option();
            }
        }
    }

    if handle_viewer_drag(app, ui, response, interact_pointer) {
        return (active_alignments, snapped_preview, snapped_wall_idx);
    }

    match app.current_tool {
        Tool::DrawWall => {
            let active_pos = interact_pointer.or(pointer);

            if let Some(mouse_pos) = active_pos {
                let (snapped_pos, alignments, wall_idx) = compute_snapping(
                    mouse_pos,
                    app.wall_start_point,
                    &app.walls,
                    &[],
                    app.zoom_factor,
                );
                active_alignments = alignments;
                snapped_preview = Some(snapped_pos);
                snapped_wall_idx = wall_idx;

                if response.clicked() {
                    if let Some(start) = app.wall_start_point {
                        if start.distance(snapped_pos) > (5.0 / app.zoom_factor) {
                            app.walls.push(Wall::new(start, snapped_pos));
                            app.rooms = extract_rooms(&app.walls);
                            app.wall_start_point = Some(snapped_pos);
                        }
                    } else {
                        app.wall_start_point = Some(snapped_pos);
                    }
                }
            }
        }
        Tool::PlaceOpening => {
            app.opening_preview = None;
            if let (Some(template), Some(mouse_pos)) = (app.opening_template, pointer) {
                if let Some((wi, offset)) =
                    place_on_wall(&app.walls, mouse_pos, app.zoom_factor, &template)
                {
                    let opening = Opening { offset, ..template };
                    app.opening_preview = Some((wi, opening));
                    if response.clicked() {
                        app.add_opening(wi, opening);
                    }
                }
            }
        }
        Tool::Select => {
            let pressed_now = response.drag_started() || response.clicked();

            if pressed_now {
                let press_origin = ui
                    .ctx()
                    .input(|i| i.pointer.press_origin())
                    .map(|p| app.screen_to_world(p));
                let exact_pos = press_origin.or(interact_pointer).or(pointer);

                if let Some(mouse_pos) = exact_pos {
                    let exact_endpoints =
                        get_hovered_endpoints(&app.walls, mouse_pos, app.zoom_factor);
                    let clicked_opening = if exact_endpoints.is_empty() {
                        opening_at(&app.walls, mouse_pos, app.zoom_factor)
                    } else {
                        None
                    };
                    let clicked_wall = if exact_endpoints.is_empty() && clicked_opening.is_none() {
                        find_closest_wall(&app.walls, mouse_pos, app.zoom_factor)
                    } else {
                        None
                    };
                    let clicked_furniture = if exact_endpoints.is_empty()
                        && clicked_wall.is_none()
                        && clicked_opening.is_none()
                    {
                        furniture_at(&app.furniture, mouse_pos)
                    } else {
                        None
                    };
                    let shift = ui.ctx().input(|i| i.modifiers.shift);
                    if !shift || clicked_furniture.is_some() {
                        app.selected_furniture = None;
                    }
                    app.selected_opening = None;

                    if !exact_endpoints.is_empty() {
                        app.dragging_endpoints = exact_endpoints.clone();
                        if !ui.ctx().input(|i| i.modifiers.shift) {
                            app.selected_walls.clear();
                        }
                        for &(idx, _) in &exact_endpoints {
                            app.selected_walls.insert(idx);
                        }
                    } else if let Some(opening) = clicked_opening {
                        app.selected_walls.clear();
                        app.selected_furniture = None;
                        app.selected_opening = Some(opening);
                        app.opening_drag = true;
                    } else if let Some(idx) = clicked_furniture {
                        app.selected_walls.clear();
                        app.selected_furniture = Some(idx);
                        let item = &app.furniture[idx];
                        app.furniture_drag = Some(FurnitureDrag {
                            raw_position: item.position,
                            raw_rotation: item.rotation,
                            snapped_walls: Vec::new(),
                        });
                    } else if let Some(idx) = clicked_wall {
                        if !ui.ctx().input(|i| i.modifiers.shift)
                            && !app.selected_walls.contains(&idx)
                        {
                            app.selected_walls.clear();
                        }
                        app.selected_walls.insert(idx);
                    } else {
                        if !ui.ctx().input(|i| i.modifiers.shift) {
                            app.selected_walls.clear();
                        }
                        if response.drag_started() {
                            app.selection_rect = Some(Rect::from_two_pos(mouse_pos, mouse_pos));
                        }
                    }
                }
            }

            if response.dragged() {
                if let Some(mouse_pos) = interact_pointer {
                    if app.opening_drag {
                        // Slide the window or door along its wall.
                        if let Some((wi, oi)) = app.selected_opening {
                            if let Some(wall) = app.walls.get_mut(wi) {
                                let (dir, _) = wall_frame(wall);
                                let along = (mouse_pos - wall.start).dot(dir) / PIXELS_PER_METER;
                                let length = wall.length();
                                if let Some(opening) = wall.openings.get_mut(oi) {
                                    opening.offset = (along * 100.0).round() / 100.0;
                                    opening.offset = opening.clamped_offset(length);
                                }
                            }
                        }
                    } else if let Some(drag) = &mut app.furniture_drag {
                        drag.raw_position += response.drag_delta() / app.zoom_factor;
                        let snapping = !ui.ctx().input(|i| i.modifiers.alt);
                        if let Some(item) = app
                            .selected_furniture
                            .and_then(|i| app.furniture.get_mut(i))
                        {
                            item.position = drag.raw_position;
                            item.rotation = drag.raw_rotation;
                            drag.snapped_walls.clear();
                            if snapping {
                                let snap = snap_furniture(item, &app.walls, app.zoom_factor);
                                item.position = snap.position;
                                item.rotation = snap.rotation;
                                drag.snapped_walls = snap.walls;
                            }
                        }
                    } else if !app.dragging_endpoints.is_empty() {
                        let (first_idx, first_is_start) = app.dragging_endpoints[0];
                        let other_point = if first_is_start {
                            app.walls[first_idx].end
                        } else {
                            app.walls[first_idx].start
                        };
                        let ignored_indices: Vec<usize> =
                            app.dragging_endpoints.iter().map(|(i, _)| *i).collect();
                        let (snapped_pos, alignments, _) = compute_snapping(
                            mouse_pos,
                            Some(other_point),
                            &app.walls,
                            &ignored_indices,
                            app.zoom_factor,
                        );

                        active_alignments = alignments;
                        snapped_preview = Some(snapped_pos);

                        for &(idx, is_start) in &app.dragging_endpoints {
                            if is_start {
                                app.walls[idx].start = snapped_pos;
                            } else {
                                app.walls[idx].end = snapped_pos;
                            }
                        }
                        app.rooms = extract_rooms(&app.walls);
                    } else if !app.selected_walls.is_empty() && app.selection_rect.is_none() {
                        let delta = response.drag_delta() / app.zoom_factor;
                        for &idx in &app.selected_walls {
                            if let Some(wall) = app.walls.get_mut(idx) {
                                wall.start += delta;
                                wall.end += delta;
                            }
                        }
                        app.rooms = extract_rooms(&app.walls);
                    } else if app.selection_rect.is_some() {
                        // 1. Extract the values using an immutable borrow first
                        if let Some(press_origin) = ui.ctx().input(|i| i.pointer.press_origin()) {
                            let origin_world = app.screen_to_world(press_origin);

                            // 2. Safely apply the mutable borrow after the math is done
                            app.selection_rect = Some(Rect::from_two_pos(origin_world, mouse_pos));
                        }
                    }
                }
            }

            if response.drag_stopped() {
                if let Some(rect) = app.selection_rect {
                    for (idx, wall) in app.walls.iter().enumerate() {
                        if rect.contains(wall.start) || rect.contains(wall.end) {
                            app.selected_walls.insert(idx);
                        }
                    }
                }
                app.selection_rect = None;
                app.dragging_endpoints.clear();
            }
            if !response.dragged() {
                app.opening_drag = false;
                app.furniture_drag = None;
            }
        }
    }

    (active_alignments, snapped_preview, snapped_wall_idx)
}

/// Screen positions of the walk-mode viewer marker: its body and the arrow handle used to turn it.
pub fn viewer_handles(app: &RoomPlannerApp) -> (Pos2, Pos2) {
    let body = app.world_to_screen(app.camera_pos);
    let dir = egui::vec2(app.camera_angle.cos(), app.camera_angle.sin());
    (body, body + dir * 30.0)
}

/// Lets the walk-mode viewer be moved and turned by dragging its marker on the plan.
/// Returns true while the marker owns the pointer.
fn handle_viewer_drag(
    app: &mut RoomPlannerApp,
    ui: &Ui,
    response: &Response,
    interact_pointer: Option<Pos2>,
) -> bool {
    if app.camera_mode != CameraMode::Walk {
        app.camera_drag = None;
        return false;
    }
    let (body, handle) = viewer_handles(app);

    if response.drag_started() {
        let origin = ui.ctx().input(|i| i.pointer.press_origin());
        if let Some(origin) = origin {
            if origin.distance(handle) < 10.0 {
                app.camera_drag = Some(CameraHandle::Direction);
            } else if origin.distance(body) < 11.0 {
                app.camera_drag = Some(CameraHandle::Body);
            }
        }
    }

    if let Some(hover) = response.hover_pos() {
        if app.camera_drag.is_none()
            && (hover.distance(handle) < 10.0 || hover.distance(body) < 11.0)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    let Some(drag) = app.camera_drag else {
        return false;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if response.dragged() {
        match drag {
            CameraHandle::Body => app.camera_pos += response.drag_delta() / app.zoom_factor,
            CameraHandle::Direction => {
                if let Some(p) = interact_pointer {
                    let d = p - app.camera_pos;
                    if d.length() > 1e-3 {
                        app.camera_angle = d.y.atan2(d.x);
                    }
                }
            }
        }
    }
    if !response.dragged() || response.drag_stopped() {
        app.camera_drag = None;
    }
    true
}
