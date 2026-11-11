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
                    let view = match app.camera_mode {
                        CameraMode::Top => format!("Top · {:.0}%", app.zoom_factor * 100.0),
                        CameraMode::Orbit => "3D".to_owned(),
                        CameraMode::Walk => "Walk".to_owned(),
                    };
                    ui.label(RichText::new(view).size(12.0));
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
                        (
                            " •",
                            "Not saved to a file yet. Your work is kept between sessions anyway; \
                             Ctrl+S writes it to the file.",
                        )
                    } else {
                        ("", "Saved to the file")
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
    if app.peek_return.is_some() {
        return "Peeking at the compared layout · let go of C to go back";
    }
    if app.camera_mode == CameraMode::Walk {
        return "Drag to look around · arrow keys or WASD to walk · right-drag to step · T or the toolbar to leave";
    }
    match app.current_tool {
        Tool::DrawWall if !app.length_input.is_empty() => {
            "Enter places the wall at the typed length · Backspace to correct · Esc to clear"
        }
        Tool::DrawWall if app.wall_start_point.is_some() => {
            "Click the next corner, or type a length (350 = cm, 3.5 = m) + Enter · Esc to finish"
        }
        Tool::DrawWall => "Click to start a wall · V to switch to Select",
        Tool::PlaceOpening if app.opening_preview.is_some() => "Click to place it here · Esc to cancel",
        Tool::PlaceOpening => "Move over a wall to place it · Esc to cancel",
        Tool::Select if app.selected_opening.is_some() => {
            "Drag along the wall to move · Del to delete · set sizes in the panel"
        }
        Tool::Select if app.furniture_resize.is_some() => {
            "Snaps to furniture and walls, else whole centimeters · Alt for any size · Shift on a corner keeps the proportions"
        }
        Tool::Select if app.furniture_rotate.is_some() => {
            "Snaps to 15° steps · hold Alt to rotate freely"
        }
        Tool::Select
            if app
                .furniture_drag
                .as_ref()
                .is_some_and(|d| d.upright.is_some()) =>
        {
            "Moving up, down and along the wall · snaps to other furniture's heights · Alt for any height"
        }
        Tool::Select if app.furniture_drag.is_some() && app.camera_mode != CameraMode::Top => {
            "Snaps to walls and other furniture · hold Ctrl to move up and down · Alt to place freely"
        }
        Tool::Select if app.furniture_drag.is_some() => {
            "Snaps to walls and other furniture · hold Alt to place freely · R to rotate"
        }
        Tool::Select if !app.extra_furniture.is_empty() => {
            "Drag to move together · edits in the panel apply to all · Ctrl+C copy · Del delete"
        }
        Tool::Select if app.selected_furniture.is_some() => {
            "Drag to move · drag a corner or side to resize · drag ⟳ to rotate · Shift+click to add · Ctrl+C/V copy/paste · Del delete · right-click for more"
        }
        Tool::Select if app.wall_drag.is_some() => {
            "Lines up with other walls · hold Ctrl to move freely · Alt to toggle resizing"
        }
        Tool::Select if !app.selected_walls.is_empty() && app.stretch_walls => {
            "Drag to resize the room · Alt+drag to move the wall alone · drag corners to reshape · Del to delete"
        }
        Tool::Select if !app.selected_walls.is_empty() => {
            "Drag to move walls · Alt+drag to resize the room · drag corners to reshape · Del to delete"
        }
        Tool::Select if app.camera_mode == CameraMode::Orbit => {
            "Click to select · rotate: two-finger scroll or right-drag · pan: Shift+two-finger or middle-drag · zoom: pinch or wheel · T for top view"
        }
        Tool::Select => {
            "Click to select · drag empty space to box-select · right-click for menu · T for 3D · F to fit · F1 for shortcuts"
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
