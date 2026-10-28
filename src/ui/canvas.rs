use crate::{
    app::{CameraMode, RoomPlannerApp},
    furniture::{furniture_at, Category, PRESETS},
    geometry::{
        openings::{opening_at, place_on_wall},
        snapping::{find_closest_wall, get_hovered_endpoints, snap_furniture},
    },
    models::{Opening, Tool},
    tools::{
        draw::{draw_drop_preview, draw_grid, draw_scene, draw_viewer},
        input::handle_input,
    },
    ui::{
        furniture_panel::{rotate, LibraryDrop, OpeningDrop},
        theme::CANVAS_BG,
    },
};
use eframe::egui;

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let frame = egui::Frame::central_panel(ui.style())
        .fill(CANVAS_BG)
        .inner_margin(0);

    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(ui, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
            app.canvas_rect = response.rect;
            // Wait for the first few frames: panel sizes settle after the first layout pass.
            if app.pending_fit && ui.ctx().cumulative_pass_nr() > 3 {
                app.pending_fit = false;
                app.zoom_to_fit();
                crate::ui::view3d::fit_orbit(app);
            }

            // Pinch or Ctrl+scroll zooms; two-finger scroll / the wheel pans
            // (or zooms, if the user prefers that in the View menu).
            if response.contains_pointer() {
                let (mut zoom, scroll) =
                    ui.ctx().input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
                if app.scroll_zooms {
                    zoom *= 1.0 + scroll.y * 0.002;
                } else {
                    app.pan_offset += scroll;
                }
                if zoom != 1.0 {
                    if let Some(mouse_pos) = response.hover_pos() {
                        app.zoom_at(mouse_pos, zoom);
                    }
                }
            }

            let typing = ui.ctx().egui_wants_keyboard_input();
            let is_panning = ui.ctx().input(|i| {
                i.pointer.button_down(egui::PointerButton::Middle)
                    || i.pointer.button_down(egui::PointerButton::Secondary)
                    || (!typing && i.key_down(egui::Key::Space))
            });
            if is_panning && response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            if is_panning && response.dragged() {
                app.pan_offset += response.drag_delta();
            }

            app.visible_world_rect = egui::Rect::from_min_max(
                app.screen_to_world(response.rect.min),
                app.screen_to_world(response.rect.max),
            );

            let pointer = response.hover_pos().map(|p| app.screen_to_world(p));
            app.pointer_world = pointer;
            let interact_pointer = response
                .interact_pointer_pos()
                .map(|p| app.screen_to_world(p));

            let hovered_endpoints = pointer
                .map(|p| get_hovered_endpoints(&app.walls, p, app.zoom_factor))
                .unwrap_or_default();

            let hovered_wall_idx = if hovered_endpoints.is_empty() {
                pointer.and_then(|p| find_closest_wall(&app.walls, p, app.zoom_factor))
            } else {
                None
            };

            let hovered_furniture = if hovered_endpoints.is_empty() && hovered_wall_idx.is_none() {
                pointer.and_then(|p| furniture_at(&app.furniture, p))
            } else {
                None
            };
            if app.current_tool == Tool::Select && hovered_furniture.is_some() {
                ui.ctx().set_cursor_icon(if app.furniture_drag.is_some() {
                    egui::CursorIcon::Grabbing
                } else {
                    egui::CursorIcon::Grab
                });
            }

            // Furniture dragged in from the library: preview it snapped to the walls, place on release.
            let library_drop = egui::DragAndDrop::payload::<LibraryDrop>(ui.ctx())
                .filter(|_| response.contains_pointer())
                .and_then(|drop| {
                    let pos = ui.ctx().pointer_latest_pos()?;
                    let mut item = drop.0.clone();
                    item.position = app.screen_to_world(pos);
                    let snap = snap_furniture(&item, &app.walls, app.zoom_factor);
                    item.position = snap.position;
                    item.rotation = snap.rotation;
                    Some(item)
                });
            if let Some(item) = &library_drop {
                if response.dnd_release_payload::<LibraryDrop>().is_some() {
                    let mut item = item.clone();
                    let position = item.position;
                    item.position = egui::Pos2::ZERO;
                    app.add_furniture_at(item, position);
                }
            }

            // Windows and doors dragged in from the library snap onto the nearest wall.
            if response.contains_pointer() {
                if let Some(drop) = egui::DragAndDrop::payload::<OpeningDrop>(ui.ctx()) {
                    app.opening_preview = ui.ctx().pointer_latest_pos().and_then(|pos| {
                        let world = app.screen_to_world(pos);
                        let (wi, offset) =
                            place_on_wall(&app.walls, world, app.zoom_factor, &drop.0)?;
                        Some((wi, Opening { offset, ..drop.0 }))
                    });
                    if response.dnd_release_payload::<OpeningDrop>().is_some() {
                        if let Some((wi, opening)) = app.opening_preview.take() {
                            app.add_opening(wi, opening);
                        }
                    }
                } else if app.current_tool != Tool::PlaceOpening {
                    app.opening_preview = None;
                }
            }

            if response.secondary_clicked() {
                app.selected_opening = None;
                app.context_menu_pos = pointer;
                app.wall_start_point = None;
                let opening = pointer.and_then(|p| opening_at(&app.walls, p, app.zoom_factor));
                if let Some(opening) = opening {
                    app.clear_selection();
                    app.selected_opening = Some(opening);
                } else if let Some(idx) = hovered_furniture {
                    app.selected_furniture = Some(idx);
                    app.selected_walls.clear();
                } else if let Some(idx) = hovered_wall_idx {
                    app.selected_furniture = None;
                    if !app.selected_walls.contains(&idx) {
                        app.selected_walls.clear();
                        app.selected_walls.insert(idx);
                    }
                } else {
                    app.selected_furniture = None;
                    app.selected_walls.clear();
                }
            }

            let mut active_alignments = Vec::new();
            let mut snapped_preview = None;
            let mut snapped_wall_idx = None;

            if !is_panning {
                let (alignments, preview, wall_idx) =
                    handle_input(app, ui, &response, pointer, interact_pointer);
                active_alignments = alignments;
                snapped_preview = preview;
                snapped_wall_idx = wall_idx;
            }

            if app.show_grid {
                draw_grid(app, &painter, response.rect);
            }
            draw_scene(
                app,
                &painter,
                pointer,
                &hovered_endpoints,
                hovered_wall_idx,
                &active_alignments,
                snapped_preview,
                snapped_wall_idx,
                hovered_furniture,
            );
            if app.camera_mode == CameraMode::Walk {
                draw_viewer(app, &painter);
            }
            if let Some(item) = &library_drop {
                draw_drop_preview(app, &painter, item);
            }

            if app.walls.is_empty() && app.furniture.is_empty() && app.wall_start_point.is_none() {
                empty_state(&painter, response.rect);
            }

            zoom_controls(app, ui, response.rect);
            response.context_menu(|ui| context_menu(app, ui));
        });
}

fn empty_state(painter: &egui::Painter, rect: egui::Rect) {
    let color = crate::ui::theme::TEXT_MUTED;
    painter.text(
        rect.center() - egui::vec2(0.0, 14.0),
        egui::Align2::CENTER_CENTER,
        "Start by drawing the walls of your home",
        egui::FontId::proportional(18.0),
        color,
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 14.0),
        egui::Align2::CENTER_CENTER,
        "Click to place corners · each grid square is 10 cm, bold lines every meter",
        egui::FontId::proportional(13.0),
        color,
    );
}

fn context_menu(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    ui.set_min_width(180.0);
    if let Some(opening) = app.selected_opening_mut() {
        ui.label(egui::RichText::new(opening.kind.label()).strong());
        ui.separator();
        if opening.kind.swings() {
            if ui.button("Flip hinge side").clicked() {
                opening.flip_hinge = !opening.flip_hinge;
                ui.close();
            }
            if ui.button("Open to the other side").clicked() {
                opening.flip_side = !opening.flip_side;
                ui.close();
            }
            ui.separator();
        }
        if ui
            .add(egui::Button::new("Delete").shortcut_text("Del"))
            .clicked()
        {
            app.delete_selection();
            ui.close();
        }
        return;
    }
    if let Some(idx) = app.selected_furniture.filter(|&i| i < app.furniture.len()) {
        let item = &mut app.furniture[idx];
        ui.label(egui::RichText::new(&item.name).strong());
        ui.separator();
        if ui
            .add(egui::Button::new("Rotate 90°").shortcut_text("R"))
            .clicked()
        {
            rotate(item, 90.0);
            ui.close();
        }
        if ui.add(egui::Button::new("Add option to compare")).clicked() {
            item.add_option();
            ui.close();
        }
        if item.options.len() > 1
            && ui
                .add(egui::Button::new("Next option").shortcut_text("O"))
                .clicked()
        {
            item.next_option();
            ui.close();
        }
        if let crate::models::FurnitureKind::Lamp { on, .. } = &mut item.kind {
            if ui
                .button(if *on {
                    "Turn light off"
                } else {
                    "Turn light on"
                })
                .clicked()
            {
                *on = !*on;
                ui.close();
            }
        }
        if ui
            .add(egui::Button::new("Duplicate").shortcut_text("Ctrl+D"))
            .clicked()
        {
            app.duplicate_selected_furniture();
            ui.close();
        }
        if ui.button("Save to My furniture").clicked() {
            app.save_to_library();
            ui.close();
        }
        ui.separator();
        if ui
            .add(egui::Button::new("Delete").shortcut_text("Del"))
            .clicked()
        {
            app.delete_selected_furniture();
            ui.close();
        }
        return;
    }

    if !app.selected_walls.is_empty() {
        if ui
            .add(egui::Button::new("Delete wall").shortcut_text("Del"))
            .clicked()
        {
            app.delete_selection();
            ui.close();
        }
        ui.separator();
    }

    let pos = app.context_menu_pos;
    ui.menu_button("Add furniture here", |ui| {
        for category in Category::ALL {
            ui.menu_button(category.label(), |ui| {
                for preset in PRESETS.iter().filter(|p| p.category == category) {
                    if ui.button(preset.label).clicked() {
                        let item = (preset.build)();
                        match pos {
                            Some(p) => app.add_furniture_at(item, p),
                            None => app.add_furniture(item),
                        }
                        ui.close();
                    }
                }
            });
        }
    });
    if app.camera_mode == CameraMode::Walk {
        if let Some(p) = pos {
            if ui.button("Stand here in 3D").clicked() {
                app.camera_pos = p;
                ui.close();
            }
        }
    }
    ui.separator();
    if ui
        .add(egui::Button::new("Zoom to fit").shortcut_text("F"))
        .clicked()
    {
        app.zoom_to_fit();
        ui.close();
    }
}

/// Floating zoom buttons in the bottom-right corner of the plan.
fn zoom_controls(app: &mut RoomPlannerApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    let size = egui::vec2(160.0, 32.0);
    let rect =
        egui::Rect::from_min_size(canvas.right_bottom() - size - egui::vec2(12.0, 12.0), size);
    ui.put(rect, |ui: &mut egui::Ui| {
        egui::Frame::new()
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(1.0, crate::ui::theme::BORDER))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let center = canvas.center();
                    if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                        app.zoom_at(center, 1.0 / 1.25);
                    }
                    if ui
                        .small_button(format!("{:.0}%", app.zoom_factor * 100.0))
                        .on_hover_text("Reset to 100%")
                        .clicked()
                    {
                        app.zoom_at(center, 1.0 / app.zoom_factor);
                    }
                    if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                        app.zoom_at(center, 1.25);
                    }
                    if ui
                        .small_button("⛶")
                        .on_hover_text("Zoom to fit [F]")
                        .clicked()
                    {
                        app.zoom_to_fit();
                    }
                });
            })
            .response
    });
}
