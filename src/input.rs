use crate::{
    RoomPlannerApp, Tool,
    helpers::{compute_snapping, extract_rooms, find_closest_wall, get_hovered_endpoints},
    models::Wall,
};
use eframe::egui;
use egui::{Pos2, Rect, Response, Ui};

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

    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        app.wall_start_point = None;
    }

    if ui
        .ctx()
        .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
    {
        if !app.selected_walls.is_empty() {
            let mut indices: Vec<usize> = app.selected_walls.iter().copied().collect();
            indices.sort_by(|a, b| b.cmp(a));
            for idx in indices {
                app.walls.remove(idx);
            }
            app.selected_walls.clear();
            app.rooms = extract_rooms(&app.walls);
            app.dragging_endpoints.clear();
            app.selection_rect = None;
        }
    }

    match app.current_tool {
        Tool::DrawWall => {
            if let Some(mouse_pos) = pointer {
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
                            app.walls.push(Wall {
                                start,
                                end: snapped_pos,
                            });
                            app.rooms = extract_rooms(&app.walls);
                            app.wall_start_point = Some(snapped_pos);
                        }
                    } else {
                        app.wall_start_point = Some(snapped_pos);
                    }
                }
            }
        }
        Tool::Select => {
            let pressed_now = response.drag_started() || response.clicked();

            if pressed_now {
                let press_origin = ui.ctx().input(|i| i.pointer.press_origin());
                let exact_pos = press_origin.or(interact_pointer).or(pointer);

                if let Some(mouse_pos) = exact_pos {
                    let exact_endpoints =
                        get_hovered_endpoints(&app.walls, mouse_pos, app.zoom_factor);
                    let clicked_wall = if exact_endpoints.is_empty() {
                        find_closest_wall(&app.walls, mouse_pos, app.zoom_factor)
                    } else {
                        None
                    };

                    if !exact_endpoints.is_empty() {
                        app.dragging_endpoints = exact_endpoints.clone();

                        if !ui.ctx().input(|i| i.modifiers.shift) {
                            app.selected_walls.clear();
                        }
                        for &(idx, _) in &exact_endpoints {
                            app.selected_walls.insert(idx);
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
                if let Some(mouse_pos) = interact_pointer {
                    if !app.dragging_endpoints.is_empty() {
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
                        let delta = response.drag_delta() / app.zoom_factor; // Scale delta for proper World Space shift
                        for &idx in &app.selected_walls {
                            if let Some(wall) = app.walls.get_mut(idx) {
                                wall.start += delta;
                                wall.end += delta;
                            }
                        }
                        app.rooms = extract_rooms(&app.walls);
                    } else if let Some(rect) = &mut app.selection_rect {
                        *rect = Rect::from_two_pos(rect.min, mouse_pos);
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
        }
    }

    (active_alignments, snapped_preview, snapped_wall_idx)
}
