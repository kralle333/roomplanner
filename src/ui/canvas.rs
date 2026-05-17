use crate::{
    app::RoomPlannerApp,
    geometry::snapping::{find_closest_wall, get_hovered_endpoints},
    tools::{draw::draw_scene, input::handle_input},
};
use eframe::egui;

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let frame = egui::Frame::central_panel(ui.style()).fill(egui::Color32::WHITE);

    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(ui, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            let mut zoom_multiplier = ui.ctx().input(|i| i.zoom_delta());
            let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);

            if scroll_delta != 0.0 {
                zoom_multiplier *= 1.0 + (scroll_delta * 0.002);
            }

            if zoom_multiplier != 1.0 {
                if let Some(mouse_pos) = ui.ctx().pointer_hover_pos() {
                    let old_zoom = app.zoom_factor;
                    app.zoom_factor *= zoom_multiplier;
                    app.zoom_factor = app.zoom_factor.clamp(0.1, 20.0);
                    let mouse_vec = mouse_pos.to_vec2();
                    app.pan_offset =
                        mouse_vec - (mouse_vec - app.pan_offset) * (app.zoom_factor / old_zoom);
                }
            }

            let is_panning = ui.ctx().input(|i| {
                i.pointer.button_down(egui::PointerButton::Middle) || i.key_down(egui::Key::Space)
            });

            if is_panning && response.dragged() {
                app.pan_offset += response.drag_delta();
            }

            let pointer = ui.ctx().pointer_hover_pos().map(|p| app.screen_to_world(p));
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

            draw_scene(
                app,
                &painter,
                pointer,
                &hovered_endpoints,
                hovered_wall_idx,
                &active_alignments,
                snapped_preview,
                snapped_wall_idx,
            );
        });
}
