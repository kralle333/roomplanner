use crate::{
    app::{FurnitureDrag, FurnitureResize, FurnitureRotate, RoomPlannerApp, PIXELS_PER_METER},
    furniture::furniture_at,
    geometry::{
        openings::{opening_at, place_on_wall, wall_frame},
        rooms::extract_rooms,
        snapping::{
            compute_snapping, find_closest_wall, get_hovered_endpoints, snap_furniture,
            snap_resized_edge, snap_to_furniture, ResizeFrame,
        },
        walls::{move_walls, snap_wall_move, WallDrag},
    },
    models::{Opening, Tool, Wall},
    ui::furniture_panel::rotate,
};
use eframe::egui::{self, emath::Rot2, vec2, Pos2, Rect, Response, Ui, Vec2};

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
    let scale = app.scale();

    // Don't treat typing in the side panel (e.g. a furniture name) as canvas shortcuts.
    let shortcuts_enabled = !ui.ctx().egui_wants_keyboard_input();

    // While drawing, typing a number gives the next wall an exact length.
    let mut place_typed = false;
    if app.current_tool != Tool::DrawWall || app.wall_start_point.is_none() {
        app.length_input.clear();
    } else if shortcuts_enabled {
        let typed = &mut app.length_input;
        ui.ctx().input_mut(|i| {
            i.events.retain(|e| match e {
                egui::Event::Text(t)
                    if t.chars()
                        .all(|c| c.is_ascii_digit() || c == '.' || c == ',') =>
                {
                    typed.push_str(t);
                    false
                }
                _ => true,
            });
            if !typed.is_empty() {
                use egui::{Key, Modifiers};
                if i.consume_key(Modifiers::NONE, Key::Backspace) {
                    typed.pop();
                }
                if i.consume_key(Modifiers::NONE, Key::Escape) {
                    typed.clear();
                }
                place_typed = i.consume_key(Modifiers::NONE, Key::Enter);
            }
        });
    }

    // Esc backs out one step at a time: the wall being drawn, then the tool, then the selection.
    if shortcuts_enabled && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        if app.wall_start_point.is_some() {
            app.wall_start_point = None;
        } else if app.current_tool != Tool::Select {
            app.current_tool = Tool::Select;
            app.opening_preview = None;
        } else {
            app.clear_selection();
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
        if rotate_pressed {
            for idx in app.selected_furniture_indices() {
                rotate(&mut app.furniture[idx], if shift { -90.0 } else { 90.0 });
            }
            if let (Some(drag), Some(item)) = (
                &mut app.furniture_drag,
                app.selected_furniture.and_then(|i| app.furniture.get(i)),
            ) {
                drag.raw_rotation = item.rotation;
            }
        }
        if next_option {
            for idx in app.selected_furniture_indices() {
                app.furniture[idx].next_option();
            }
        }
    }

    if app.current_tool == Tool::Select
        && (handle_furniture_rotate(app, ui, response, interact_pointer)
            || handle_furniture_resize(app, ui, response))
    {
        return (active_alignments, snapped_preview, snapped_wall_idx);
    }

    match app.current_tool {
        Tool::DrawWall => {
            let active_pos = interact_pointer.or(pointer);

            if let Some(mouse_pos) = active_pos {
                let (snapped_pos, alignments, wall_idx) =
                    compute_snapping(mouse_pos, app.wall_start_point, &app.walls, &[], scale);
                active_alignments = alignments;
                snapped_preview = Some(snapped_pos);
                snapped_wall_idx = wall_idx;

                // A typed length keeps the pointer's direction but sets the exact length.
                let typed = parse_length(&app.length_input);
                if let (Some(start), Some(length)) = (app.wall_start_point, typed) {
                    let dir = (snapped_pos - start).normalized();
                    let dir = if dir.is_finite() && dir != Vec2::ZERO {
                        dir
                    } else {
                        vec2(1.0, 0.0)
                    };
                    let end = start + dir * length * PIXELS_PER_METER;
                    snapped_preview = Some(end);
                    active_alignments.clear();
                    snapped_wall_idx = None;
                    if place_typed {
                        app.walls.push(Wall::new(start, end));
                        app.rooms = extract_rooms(&app.walls);
                        app.wall_start_point = Some(end);
                        app.length_input.clear();
                    }
                } else if response.clicked() {
                    if let Some(start) = app.wall_start_point {
                        if start.distance(snapped_pos) > (5.0 / scale) {
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
                if let Some((wi, offset)) = place_on_wall(&app.walls, mouse_pos, scale, &template) {
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
                let press_screen = ui.ctx().input(|i| i.pointer.press_origin());
                let press_origin = press_screen.map(|p| app.pick(p));
                // In 3D a piece in front of a wall is what was pressed, not the wall behind.
                let in_front = press_screen
                    .or(response.hover_pos())
                    .and_then(|p| app.pick_furniture(p));
                // The picked point (wall or piece under the pointer in 3D) beats the floor point.
                let exact_pos = press_origin.or(pointer).or(interact_pointer);

                if let Some(mouse_pos) = exact_pos {
                    let exact_endpoints = if in_front.is_some() {
                        Vec::new()
                    } else {
                        get_hovered_endpoints(&app.walls, mouse_pos, scale)
                    };
                    let clicked_opening = if exact_endpoints.is_empty() && in_front.is_none() {
                        opening_at(&app.walls, mouse_pos, scale)
                    } else {
                        None
                    };
                    let clicked_wall = if exact_endpoints.is_empty()
                        && clicked_opening.is_none()
                        && in_front.is_none()
                    {
                        find_closest_wall(&app.walls, mouse_pos, scale)
                    } else {
                        None
                    };
                    let clicked_furniture = if in_front.is_some() {
                        in_front
                    } else if exact_endpoints.is_empty()
                        && clicked_wall.is_none()
                        && clicked_opening.is_none()
                    {
                        furniture_at(&app.furniture, mouse_pos)
                    } else {
                        None
                    };
                    let shift = ui.ctx().input(|i| i.modifiers.shift);
                    if !shift && clicked_furniture.is_none_or(|i| !app.is_furniture_selected(i)) {
                        app.select_furniture(None);
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
                        app.select_furniture(None);
                        app.selected_opening = Some(opening);
                        app.opening_drag = true;
                    } else if let Some(idx) = clicked_furniture {
                        app.selected_walls.clear();
                        if response.clicked() {
                            if shift {
                                app.toggle_furniture(idx);
                            } else {
                                app.select_furniture(Some(idx));
                            }
                        } else if !app.is_furniture_selected(idx) {
                            app.toggle_furniture(idx);
                        } else if app.selected_furniture != Some(idx) {
                            // Drag the grabbed piece; the others follow it.
                            if let Some(old) = app.selected_furniture.replace(idx) {
                                app.extra_furniture.retain(|&i| i != idx);
                                app.extra_furniture.push(old);
                            }
                        }
                        if app.is_furniture_selected(idx) && response.drag_started() {
                            let item = &app.furniture[idx];
                            app.furniture_drag = Some(FurnitureDrag {
                                raw_position: item.position,
                                raw_rotation: item.rotation,
                                snapped_walls: Vec::new(),
                                guides: Vec::new(),
                                start: item.position,
                                others: app
                                    .extra_furniture
                                    .iter()
                                    .filter_map(|&i| Some((i, app.furniture.get(i)?.position)))
                                    .collect(),
                            });
                        }
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
                let world_delta = app.world_drag_delta(response);
                if let Some(mouse_pos) = interact_pointer {
                    if app.opening_drag {
                        // In 3D, follow the point on the wall under the pointer.
                        let mouse_pos = response
                            .interact_pointer_pos()
                            .map_or(mouse_pos, |p| app.pick(p));
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
                        drag.raw_position += world_delta;
                        let snapping = !ui.ctx().input(|i| i.modifiers.alt);
                        let selected = app.selected_furniture;
                        if let Some(idx) = selected.filter(|&i| i < app.furniture.len()) {
                            let item = &mut app.furniture[idx];
                            item.position = drag.raw_position;
                            item.rotation = drag.raw_rotation;
                            drag.snapped_walls.clear();
                            drag.guides.clear();
                            if snapping {
                                let snap = snap_furniture(item, &app.walls, scale);
                                item.position = snap.position;
                                item.rotation = snap.rotation;
                                drag.snapped_walls = snap.walls;

                                // Then against other furniture, where no wall holds it.
                                let held: Vec<Vec2> = drag
                                    .snapped_walls
                                    .iter()
                                    .filter_map(|&i| app.walls.get(i))
                                    .map(|w| (w.end - w.start).normalized().rot90())
                                    .collect();
                                let moving = |i: usize| {
                                    i == idx || drag.others.iter().any(|&(other, _)| other == i)
                                };
                                let others = app
                                    .furniture
                                    .iter()
                                    .enumerate()
                                    .filter(|&(i, _)| !moving(i))
                                    .map(|(_, f)| f);
                                let snap =
                                    snap_to_furniture(&app.furniture[idx], others, &held, scale);
                                app.furniture[idx].position = snap.position;
                                drag.guides = snap.guides;
                            }
                            let item = &app.furniture[idx];
                            let moved = item.position - drag.start;
                            for &(other, start) in &drag.others {
                                if let Some(item) = app.furniture.get_mut(other) {
                                    item.position = start + moved;
                                }
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
                            scale,
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
                        let drag = app
                            .wall_drag
                            .get_or_insert_with(|| WallDrag::new(&app.walls, &app.selected_walls));
                        drag.raw_delta += world_delta;
                        // Alt inverts the "Resize rooms" setting for this drag.
                        let alt = ui.ctx().input(|i| i.modifiers.alt);
                        let stretch = app.stretch_walls != alt;
                        drag.stretching = stretch;
                        let normal = drag.normal.filter(|_| stretch);
                        let delta = match normal {
                            Some(n) => n * drag.raw_delta.dot(n),
                            None => drag.raw_delta,
                        };
                        // Ctrl moves without lining up with other walls.
                        let (delta, guides) = if ui.ctx().input(|i| i.modifiers.command) {
                            (delta, Vec::new())
                        } else {
                            let radius = 10.0 / scale;
                            snap_wall_move(
                                drag,
                                &app.selected_walls,
                                stretch,
                                delta,
                                normal,
                                radius,
                            )
                        };
                        drag.guides = guides;
                        app.walls = move_walls(drag, &app.selected_walls, stretch, delta);
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
                    for idx in 0..app.furniture.len() {
                        if rect.contains(app.furniture[idx].position)
                            && !app.is_furniture_selected(idx)
                        {
                            app.toggle_furniture(idx);
                        }
                    }
                }
                app.selection_rect = None;
                app.dragging_endpoints.clear();
                app.wall_drag = None;
            }
            if !response.dragged() {
                app.opening_drag = false;
                app.furniture_drag = None;
                app.wall_drag = None;
                app.dragging_endpoints.clear();
            }
        }
    }

    (active_alignments, snapped_preview, snapped_wall_idx)
}

/// Screen positions of the rotation handle of the selected furniture: where its stem leaves
/// the back edge, and the knob to drag.
pub fn rotation_handle(app: &RoomPlannerApp) -> Option<(Pos2, Pos2)> {
    let item = app.selected_furniture.and_then(|i| app.furniture.get(i))?;
    let back_world = item.local_to_world(0.0, -item.depth / 2.0);
    let outward = Rot2::from_angle(item.rotation) * vec2(0.0, -1.0);
    let height = item.elevation + item.height;
    let back = app.world3_to_screen(back_world, height);
    // Floor-level direction, so the handle points the right way in perspective too.
    let a = app.world_to_screen(back_world);
    let b = app.world_to_screen(back_world + outward * (26.0 / app.scale()));
    let dir = (b - a).normalized();
    if !dir.is_finite() || dir == Vec2::ZERO {
        return Some((back, back + vec2(0.0, -26.0)));
    }
    Some((back, back + dir * 26.0))
}

/// Turns the selected furniture by dragging its rotation handle: 15° steps, or freely with
/// Alt. A group turns around its middle. Returns true while the handle owns the pointer.
fn handle_furniture_rotate(
    app: &mut RoomPlannerApp,
    ui: &Ui,
    response: &Response,
    interact_pointer: Option<Pos2>,
) -> bool {
    let Some((_, knob)) = rotation_handle(app) else {
        app.furniture_rotate = None;
        return false;
    };
    let near_knob = |p: Pos2| p.distance(knob) < 9.0;

    if response.hover_pos().is_some_and(near_knob) && app.furniture_rotate.is_none() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let pressed_on_knob = ui
        .ctx()
        .input(|i| i.pointer.press_origin())
        .is_some_and(near_knob);
    if response.drag_started() && pressed_on_knob {
        let selection = app.selected_furniture_indices();
        let center = (selection.iter().fold(Vec2::ZERO, |sum, &i| {
            sum + app.furniture[i].position.to_vec2()
        }) / selection.len() as f32)
            .to_pos2();
        let press = ui.ctx().input(|i| i.pointer.press_origin()).unwrap_or(knob);
        let origin = app.screen_to_world(press);
        app.furniture_rotate = Some(FurnitureRotate {
            center,
            start_angle: (origin - center).angle(),
            originals: selection
                .iter()
                .map(|&i| (i, app.furniture[i].position, app.furniture[i].rotation))
                .collect(),
        });
    }
    // A click on the knob shouldn't deselect the piece.
    if response.clicked() && pressed_on_knob {
        return true;
    }

    let Some(rotate) = &app.furniture_rotate else {
        return false;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if let Some(mouse) = interact_pointer.filter(|_| response.dragged()) {
        let mut angle = (mouse - rotate.center).angle() - rotate.start_angle;
        if !ui.ctx().input(|i| i.modifiers.alt) {
            let step = 15f32.to_radians();
            // Snap the primary piece's final rotation, not the turn, to whole steps.
            let first = rotate.originals[0].2;
            angle = ((first + angle) / step).round() * step - first;
        }
        let turn = Rot2::from_angle(angle);
        for &(idx, position, rotation) in &rotate.originals {
            if let Some(item) = app.furniture.get_mut(idx) {
                item.position = rotate.center + turn * (position - rotate.center);
                item.rotation = (rotation + angle).rem_euclid(std::f32::consts::TAU);
            }
        }
    }
    if !response.dragged() || response.drag_stopped() {
        app.furniture_rotate = None;
    }
    true
}

/// Smallest size a piece can be dragged to, in meters.
const MIN_FURNITURE_SIZE: f32 = 0.05;

/// The eight resize handles of the selected piece, on its top: screen position and which
/// side of the piece each sits on along its local x and z axes. Only for a single piece.
pub fn resize_handles(app: &RoomPlannerApp) -> Option<[(Pos2, (i8, i8)); 8]> {
    if !app.extra_furniture.is_empty() || app.camera_mode == crate::app::CameraMode::Walk {
        return None;
    }
    let item = app.selected_furniture.and_then(|i| app.furniture.get(i))?;
    let sides = [
        (-1, -1),
        (1, -1),
        (1, 1),
        (-1, 1),
        (0, -1),
        (1, 0),
        (0, 1),
        (-1, 0),
    ];
    Some(sides.map(|(sx, sz): (i8, i8)| {
        let world = item.local_to_world(sx as f32 * item.width / 2.0, sz as f32 * item.depth / 2.0);
        (app.world3_to_screen(world, item.top()), (sx, sz))
    }))
}

/// The handle at a screen position, and the height (meters) of the level it was grabbed at.
/// Corners come first, then anywhere along a side. In 3D the whole outline box works: its
/// top and bottom edges resize a side, its upright edges a corner.
fn resize_handle_at(app: &RoomPlannerApp, p: Pos2) -> Option<(Pos2, (i8, i8), f32)> {
    use crate::geometry::math::closest_point_on_segment;
    let handles = resize_handles(app)?;
    let item = app.selected_furniture.and_then(|i| app.furniture.get(i))?;
    let (top, bottom) = (item.top(), item.elevation);
    let footprint = item.footprint();
    let in_3d = app.camera_mode != crate::app::CameraMode::Top;

    // Corner squares, then the upright edges below them.
    let corners: Vec<Pos2> = handles[..4].iter().map(|h| h.0).collect();
    let shortest = (0..4)
        .map(|i| corners[i].distance(corners[(i + 1) % 4]))
        .fold(f32::INFINITY, f32::min);
    let corner_reach = (shortest / 3.0).clamp(4.0, 10.0);
    for (i, &corner) in corners.iter().enumerate() {
        if corner.distance(p) < corner_reach {
            return Some((corner, handles[i].1, top));
        }
    }
    if in_3d {
        for (i, &corner) in corners.iter().enumerate() {
            let foot = app.world3_to_screen(footprint[i], bottom);
            let on_edge = closest_point_on_segment(p, corner, foot);
            if on_edge.distance(p) < 5.0 {
                let down = on_edge.distance(corner) / corner.distance(foot).max(1.0);
                return Some((on_edge, handles[i].1, top + (bottom - top) * down));
            }
        }
    }

    // Sides: generous outside the piece, and inside only as far as leaves the middle half
    // free for moving it, however small it is on screen.
    let levels: &[f32] = if in_3d { &[top, bottom] } else { &[top] };
    for &level in levels {
        let ring: Vec<Pos2> = footprint
            .iter()
            .map(|c| app.world3_to_screen(*c, level))
            .collect();
        let center = app.world3_to_screen(item.position, level);
        // Side i runs from corner i to the next one (see the order in `resize_handles`).
        for i in 0..4 {
            let (a, b) = (ring[i], ring[(i + 1) % 4]);
            let on_side = closest_point_on_segment(p, a, b);
            let distance = on_side.distance(p);
            let side_of = |q: Pos2| (b - a).x * (q - a).y - (b - a).y * (q - a).x;
            let inside = side_of(p) * side_of(center) > 0.0;
            let across = closest_point_on_segment(center, a, b).distance(center) * 2.0;
            let reach = if inside { (across / 4.0).min(6.0) } else { 9.0 };
            // Leave the ends to the corners.
            let at_end = on_side.distance(a) < 1.0 || on_side.distance(b) < 1.0;
            if distance < reach && !at_end {
                return Some((on_side, handles[4 + i].1, level));
            }
        }
    }
    None
}

/// Resizes the selected piece by dragging a corner or side; the opposite corner or side
/// stays where it is. Returns true while a handle owns the pointer.
fn handle_furniture_resize(app: &mut RoomPlannerApp, ui: &Ui, response: &Response) -> bool {
    let scale = app.scale();
    let Some(idx) = app.selected_furniture.filter(|&i| i < app.furniture.len()) else {
        app.furniture_resize = None;
        return false;
    };

    if app.furniture_resize.is_none() {
        if let Some((at, handle, _)) = response.hover_pos().and_then(|p| resize_handle_at(app, p)) {
            ui.ctx()
                .set_cursor_icon(resize_cursor(app, idx, at, handle));
        }
    }
    let press = ui.ctx().input(|i| i.pointer.press_origin());
    let pressed_handle = press.and_then(|p| resize_handle_at(app, p));
    if response.drag_started() && app.furniture_resize.is_none() {
        if let (Some(press), Some((_, handle, level))) = (press, pressed_handle) {
            let item = &app.furniture[idx];
            let handle_world = item.local_to_world(
                handle.0 as f32 * item.width / 2.0,
                handle.1 as f32 * item.depth / 2.0,
            );
            app.furniture_resize = Some(FurnitureResize {
                handle,
                position: item.position,
                rotation: item.rotation,
                size: vec2(item.width, item.depth),
                level,
                guides: Vec::new(),
                grab_offset: app.screen_to_plane(press, level) - handle_world,
            });
        }
    }
    // A click on a handle shouldn't deselect the piece or pick what is behind it.
    if response.clicked() && pressed_handle.is_some() {
        return true;
    }

    let Some(resize) = &app.furniture_resize else {
        return false;
    };
    let (sx, sz) = (resize.handle.0 as f32, resize.handle.1 as f32);
    let (alt, shift) = ui.ctx().input(|i| (i.modifiers.alt, i.modifiers.shift));
    let mouse = ui.ctx().input(|i| i.pointer.interact_pos());
    if let Some(mouse) = mouse.filter(|_| response.dragged()) {
        let turn = Rot2::from_angle(resize.rotation);
        let target = app.screen_to_plane(mouse, resize.level) - resize.grab_offset;
        // Where the handle should be, in the piece's own axes (meters from its old center).
        let local = turn.inverse() * ((target - resize.position) / PIXELS_PER_METER);
        let old = resize.size;
        let round = |v: f32| if alt { v } else { (v * 100.0).round() / 100.0 };
        let mut width = old.x;
        let mut depth = old.y;
        // Each dragged edge goes to a nearby edge of other furniture or a wall surface,
        // otherwise to whole centimeters.
        let proportional = shift && sx != 0.0 && sz != 0.0;
        let frame = ResizeFrame {
            item: &app.furniture[idx],
            origin: resize.position,
            rotation: resize.rotation,
            half: old / 2.0 * PIXELS_PER_METER,
        };
        let mut guides = Vec::new();
        let mut size_along = |axis: usize, side: f32| {
            let edge = local[axis];
            let others = app.furniture.iter().enumerate();
            let others = others.filter(|&(i, _)| i != idx).map(|(_, f)| f);
            let snapped = (!alt && !proportional)
                .then(|| {
                    let at = edge * PIXELS_PER_METER;
                    snap_resized_edge(&frame, others, &app.walls, axis, at, scale)
                })
                .flatten()
                .filter(|(line, _)| side * line / PIXELS_PER_METER + old[axis] / 2.0 > 0.0);
            match snapped {
                Some((line, guide)) => {
                    guides.push(guide);
                    side * line / PIXELS_PER_METER + old[axis] / 2.0
                }
                None => round(side * edge + old[axis] / 2.0),
            }
            .max(MIN_FURNITURE_SIZE)
        };
        if sx != 0.0 {
            width = size_along(0, sx);
        }
        if sz != 0.0 {
            depth = size_along(1, sz);
        }
        if shift && sx != 0.0 && sz != 0.0 && old.x > 0.0 && old.y > 0.0 {
            let factor = (width / old.x).max(depth / old.y);
            width = round(old.x * factor).max(MIN_FURNITURE_SIZE);
            depth = round(old.y * factor).max(MIN_FURNITURE_SIZE);
        }
        // The side opposite the handle stays put, so the center moves by half the change.
        let shift_local = vec2(sx * (width - old.x) / 2.0, sz * (depth - old.y) / 2.0);
        let position = resize.position + turn * shift_local * PIXELS_PER_METER;
        let cursor = resize_cursor(app, idx, mouse, resize.handle);
        ui.ctx().set_cursor_icon(cursor);
        let item = &mut app.furniture[idx];
        item.width = width;
        item.depth = depth;
        item.position = position;
        if let Some(resize) = &mut app.furniture_resize {
            resize.guides = guides;
        }
    }
    if !response.dragged() || response.drag_stopped() {
        app.furniture_resize = None;
    }
    true
}

/// A resize cursor pointing the way the handle moves on screen.
fn resize_cursor(app: &RoomPlannerApp, idx: usize, at: Pos2, handle: (i8, i8)) -> egui::CursorIcon {
    let item = &app.furniture[idx];
    let center = app.world3_to_screen(item.position, item.top());
    // Sides move straight out from the piece; corners along the diagonal.
    let outward = item.local_to_world(
        handle.0 as f32 * item.width / 2.0,
        handle.1 as f32 * item.depth / 2.0,
    );
    let from = if handle.0 == 0 || handle.1 == 0 {
        app.world3_to_screen(outward, item.top())
    } else {
        at
    };
    let dir = from - center;
    let angle = dir.y.atan2(dir.x).to_degrees().rem_euclid(180.0);
    match angle {
        a if !(22.5..157.5).contains(&a) => egui::CursorIcon::ResizeHorizontal,
        a if a < 67.5 => egui::CursorIcon::ResizeNwSe,
        a if a < 112.5 => egui::CursorIcon::ResizeVertical,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}

/// A typed wall length in meters: whole numbers are centimeters ("350"), numbers with a
/// decimal point or comma are meters ("3.5", "3,5").
pub fn parse_length(text: &str) -> Option<f32> {
    let text = text.trim().replace(',', ".");
    let value: f32 = text.parse().ok()?;
    let meters = if text.contains('.') {
        value
    } else {
        value / 100.0
    };
    (meters > 0.01 && meters < 200.0).then_some(meters)
}

#[cfg(test)]
mod tests {
    use super::parse_length;

    #[test]
    fn typed_lengths() {
        assert_eq!(parse_length("350"), Some(3.5));
        assert_eq!(parse_length("3.5"), Some(3.5));
        assert_eq!(parse_length("3,25"), Some(3.25));
        assert_eq!(parse_length(""), None);
        assert_eq!(parse_length("."), None);
        assert_eq!(parse_length("0"), None);
    }
}
