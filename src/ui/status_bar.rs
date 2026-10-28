use crate::{
    app::{CameraMode, RoomPlannerApp, PIXELS_PER_METER},
    models::Tool,
    ui::theme::TEXT_MUTED,
};
use eframe::egui::{self, RichText};

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::bottom("status_bar")
        .exact_size(26.0)
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                // Right-hand info first, so the hint gets whatever space is left and truncates.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:.0}%", app.zoom_factor * 100.0)).size(12.0));
                    ui.separator();
                    if let Some(p) = app.pointer_world {
                        ui.label(
                            RichText::new(format!(
                                "x {:.2} m   y {:.2} m",
                                p.x / PIXELS_PER_METER,
                                p.y / PIXELS_PER_METER
                            ))
                            .size(12.0)
                            .monospace(),
                        );
                        ui.separator();
                    }
                    let name = app
                        .current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled plan".to_owned());
                    let (mark, tip) = if app.is_dirty() {
                        (" •", "Unsaved changes")
                    } else {
                        ("", "All changes saved")
                    };
                    ui.label(RichText::new(format!("{name}{mark}")).size(12.0))
                        .on_hover_text(tip);
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} · {} · {}",
                            plural(app.walls.len(), "wall"),
                            plural(app.furniture.len(), "piece"),
                            plural(app.rooms.len(), "room")
                        ))
                        .size(12.0)
                        .color(TEXT_MUTED),
                    );
                    ui.separator();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(hint(app)).color(TEXT_MUTED).size(12.0))
                                .truncate(),
                        );
                    });
                });
            });
        });
}

fn hint(app: &RoomPlannerApp) -> &'static str {
    if app.camera_drag.is_some() {
        return "Moving the viewer · drag the arrow to turn";
    }
    match app.current_tool {
        Tool::DrawWall if app.wall_start_point.is_some() => {
            "Click to place the next corner · Esc to finish · walls snap to corners and alignments"
        }
        Tool::DrawWall => "Click to start a wall · V to switch to Select",
        Tool::PlaceOpening if app.opening_preview.is_some() => "Click to place it here · Esc to cancel",
        Tool::PlaceOpening => "Move over a wall to place it · Esc to cancel",
        Tool::Select if app.selected_opening.is_some() => {
            "Drag along the wall to move · Del to delete · set sizes in the panel"
        }
        Tool::Select if app.furniture_drag.is_some() => {
            "Snaps to walls · hold Alt to place freely · R to rotate"
        }
        Tool::Select if app.selected_furniture.is_some() => {
            "Drag to move · R rotate · O next option · Ctrl+D duplicate · Del delete · right-click for more"
        }
        Tool::Select if !app.selected_walls.is_empty() => {
            "Drag to move walls · drag corners to reshape · Del to delete"
        }
        Tool::Select if app.camera_mode == CameraMode::Walk => {
            "Drag the person to move the 3D viewpoint · drag the arrow to turn · right-click to add furniture"
        }
        Tool::Select => {
            "Click to select · drag empty space to box-select · right-click to add furniture · F to fit · F1 for shortcuts"
        }
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}
