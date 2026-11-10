use crate::{
    app::{same_type, CameraMode, RoomPlannerApp, TimeOfDay, PIXELS_PER_METER},
    furniture::{furniture_at, Category, PRESETS},
    geometry::{
        openings::{opening_at, place_on_wall},
        snapping::{find_closest_wall, get_hovered_endpoints, snap_furniture},
    },
    models::{Opening, Tool},
    tools::{
        draw::{draw_drop_preview, draw_scale_bar, draw_scene},
        input::handle_input,
    },
    ui::{
        furniture_panel::{rotate, LibraryDrop, OpeningDrop},
        theme::CANVAS_BG,
        view3d,
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
            let rect = response.rect;
            app.canvas_rect = rect;
            view3d::update_projection(app, rect);
            // Wait for the first few frames: panel sizes settle after the first layout pass.
            if app.pending_fit && ui.ctx().cumulative_pass_nr() > 3 {
                app.pending_fit = false;
                app.zoom_to_fit();
                view3d::fit_orbit(app);
            }

            let navigating = view3d::handle_camera_input(app, ui, &response);
            view3d::update_projection(app, rect);
            let walk = app.camera_mode == CameraMode::Walk;

            // Pick and snap distances are given in pixels; convert them at the pointer.
            let center = app.screen_to_world(rect.center());
            app.view_scale = if app.camera_mode == CameraMode::Top {
                app.zoom_factor
            } else {
                let anchor = response
                    .hover_pos()
                    .map(|p| app.screen_to_world(p))
                    .unwrap_or(center);
                let a = app.world_to_screen(anchor);
                let x = app.world_to_screen(anchor + egui::vec2(PIXELS_PER_METER, 0.0));
                let z = app.world_to_screen(anchor + egui::vec2(0.0, PIXELS_PER_METER));
                ((a.distance(x) * a.distance(z)).sqrt() / PIXELS_PER_METER).clamp(0.05, 50.0)
            };
            app.visible_world_rect =
                egui::Rect::from_center_size(center, rect.size() / app.view_scale.max(0.05));

            let floor_pointer = response.hover_pos().map(|p| app.screen_to_world(p));
            app.pointer_world = floor_pointer;
            // What's under the pointer in 3D: a wall or piece, or the floor. Walls are drawn
            // on the floor, so that tool uses the floor point.
            let pointer = if walk || navigating {
                None
            } else if app.current_tool == Tool::DrawWall {
                floor_pointer
            } else {
                response.hover_pos().map(|p| app.pick(p))
            };
            let interact_pointer = response
                .interact_pointer_pos()
                .map(|p| app.screen_to_world(p));

            // In 3D a piece in front of a wall is what you point at, not the wall behind it.
            let in_front = response
                .hover_pos()
                .filter(|_| pointer.is_some() && app.current_tool == Tool::Select)
                .and_then(|p| app.pick_furniture(p));

            let hovered_endpoints = pointer
                .filter(|_| in_front.is_none())
                .map(|p| get_hovered_endpoints(&app.walls, p, app.scale()))
                .unwrap_or_default();

            let hovered_wall_idx = if hovered_endpoints.is_empty() && in_front.is_none() {
                pointer.and_then(|p| find_closest_wall(&app.walls, p, app.scale()))
            } else {
                None
            };

            let hovered_furniture = if in_front.is_some() {
                in_front
            } else if hovered_endpoints.is_empty() && hovered_wall_idx.is_none() {
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
                    let snap = snap_furniture(&item, &app.walls, app.scale());
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
                        let world = app.pick(pos);
                        let (wi, offset) = place_on_wall(&app.walls, world, app.scale(), &drop.0)?;
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
                app.context_menu_pos = pointer.or(floor_pointer);
                app.wall_start_point = None;
                let opening = pointer
                    .filter(|_| in_front.is_none())
                    .and_then(|p| opening_at(&app.walls, p, app.scale()));
                if let Some(opening) = opening {
                    app.clear_selection();
                    app.selected_opening = Some(opening);
                } else if let Some(idx) = hovered_furniture {
                    if !app.is_furniture_selected(idx) {
                        app.select_furniture(Some(idx));
                    }
                    app.selected_walls.clear();
                } else if let Some(idx) = hovered_wall_idx {
                    app.select_furniture(None);
                    if !app.selected_walls.contains(&idx) {
                        app.selected_walls.clear();
                        app.selected_walls.insert(idx);
                    }
                } else {
                    app.select_furniture(None);
                    app.selected_walls.clear();
                }
            }

            let mut active_alignments = Vec::new();
            let mut snapped_preview = None;
            let mut snapped_wall_idx = None;

            if !navigating && !walk {
                let (alignments, preview, wall_idx) =
                    handle_input(app, ui, &response, pointer, interact_pointer);
                active_alignments = alignments;
                snapped_preview = preview;
                snapped_wall_idx = wall_idx;
            }

            let hovered_wall = hovered_wall_idx.filter(|_| app.current_tool == Tool::Select);
            view3d::paint(app, &painter, rect, library_drop.as_ref(), hovered_wall);
            if !walk {
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
                if let Some(item) = &library_drop {
                    draw_drop_preview(app, &painter, item);
                }
            }
            if app.camera_mode == CameraMode::Top && app.show_grid {
                draw_scale_bar(app, &painter, rect);
            }

            if app.walls.is_empty() && app.furniture.is_empty() && app.wall_start_point.is_none() {
                empty_state(&painter, rect);
            }

            view_toolbar(app, ui, rect);
            crate::ui::properties::show(app, ui, rect);
            if app.camera_mode == CameraMode::Top {
                zoom_controls(app, ui, rect);
            }
            response.context_menu(|ui| context_menu(app, ui));
        });
}

/// Camera and lighting switches floating in the top-left corner of the viewport.
fn view_toolbar(app: &mut RoomPlannerApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    egui::Area::new(egui::Id::new("view_toolbar"))
        .fixed_pos(canvas.left_top() + egui::vec2(12.0, 12.0))
        .order(egui::Order::Middle)
        .constrain_to(canvas)
        .show(ui.ctx(), |ui| {
            floating_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let mut mode = app.camera_mode;
                    ui.selectable_value(&mut mode, CameraMode::Top, "▦ Top")
                        .on_hover_text("Straight down, like a floor plan. Easiest for drawing walls [T]");
                    ui.selectable_value(&mut mode, CameraMode::Orbit, "🧊 3D")
                        .on_hover_text("Look around the home. Two-finger scroll or right-drag to rotate, pinch to zoom [T]");
                    ui.selectable_value(&mut mode, CameraMode::Walk, "🚶 Walk")
                        .on_hover_text("Stand in the room at eye height");
                    app.set_camera_mode(mode);
                    ui.separator();
                    for (time, icon, tip) in [
                        (TimeOfDay::Day, "☀", "Daylight"),
                        (TimeOfDay::Evening, "🌆", "Evening"),
                        (TimeOfDay::Night, "🌙", "Night: see your lamps"),
                    ] {
                        ui.selectable_value(&mut app.time_of_day, time, icon)
                            .on_hover_text(tip);
                    }
                    if app.camera_mode == CameraMode::Orbit {
                        ui.separator();
                        ui.toggle_value(&mut app.cut_walls, "✂ Cut walls")
                            .on_hover_text("Lower the walls so you can see into the rooms");
                    }
                    ui.separator();
                    if ui
                        .button("⛶")
                        .on_hover_text("Show the whole plan · F frames what is selected")
                        .clicked()
                    {
                        app.fit_whole_plan();
                    }
                });
            });
        });
}

fn floating_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 240))
        .stroke(egui::Stroke::new(1.0, crate::ui::theme::BORDER))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(6, 4))
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
        "Pick ✏ Walls and click to place corners · grid squares are 10 cm, bold lines every meter",
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
        let selection = app.selected_furniture_indices();
        let title = match selection.len() {
            1 => app.furniture[idx].name.clone(),
            n => format!("{} pieces", n),
        };
        ui.label(egui::RichText::new(title).strong());
        ui.separator();
        if ui
            .add(egui::Button::new("Rotate 90°").shortcut_text("R"))
            .clicked()
        {
            for &i in &selection {
                rotate(&mut app.furniture[i], 90.0);
            }
            ui.close();
        }
        let similar = app
            .furniture
            .iter()
            .filter(|other| same_type(other, &app.furniture[idx]))
            .count();
        if similar > selection.len()
            && ui
                .button(format!("Select all {similar} like this"))
                .clicked()
        {
            app.select_similar_furniture();
            ui.close();
        }
        let item = &mut app.furniture[idx];
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
        if ui
            .add(egui::Button::new("Copy").shortcut_text("Ctrl+C"))
            .clicked()
        {
            app.copy_selected_furniture(ui.ctx());
            ui.close();
        }
        if ui
            .add(egui::Button::new("Cut").shortcut_text("Ctrl+X"))
            .clicked()
        {
            app.cut_selected_furniture(ui.ctx());
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
    if !app.furniture_clipboard.is_empty()
        && ui
            .add(egui::Button::new("Paste here").shortcut_text("Ctrl+V"))
            .clicked()
    {
        app.paste_furniture(pos, None);
        ui.close();
    }
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
    if app.camera_mode != CameraMode::Walk {
        if let Some(p) = pos {
            if ui.button("🚶 Stand here").clicked() {
                app.set_camera_mode(CameraMode::Walk);
                app.camera_pos = p;
                ui.close();
            }
        }
    }
    ui.separator();
    let label = if app.selection_bounds().is_some() {
        "Zoom to selection"
    } else {
        "Show whole plan"
    };
    if ui
        .add(egui::Button::new(label).shortcut_text("F"))
        .clicked()
    {
        app.fit_view();
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
