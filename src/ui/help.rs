use crate::app::RoomPlannerApp;
use eframe::egui;

const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo"),
            ("Ctrl+S / Ctrl+Shift+S", "Save / save as"),
            ("Ctrl+O", "Open plan"),
            ("V / W", "Select tool / wall tool"),
            ("F", "Zoom to fit"),
            ("F1", "Show this help"),
        ],
    ),
    (
        "Plan view",
        &[
            ("Scroll, two-finger swipe", "Pan"),
            ("Right-drag, middle-drag, Space+drag", "Pan"),
            (
                "Pinch, Ctrl+scroll",
                "Zoom (the wheel can zoom instead: View menu)",
            ),
            ("Right-click", "Menu: add furniture here, rotate, delete…"),
            ("Esc", "Stop drawing walls / placing"),
        ],
    ),
    (
        "Furniture",
        &[
            ("Click or drag a picture", "Add from the library"),
            ("Drag", "Move (snaps to walls)"),
            ("Alt+drag", "Move without snapping"),
            ("R / Shift+R", "Rotate 90° right / left"),
            ("O", "Switch to the next option"),
            ("Ctrl+D", "Duplicate"),
            ("Del", "Delete"),
        ],
    ),
    (
        "Doors & windows",
        &[
            ("Click a picture, then a wall", "Place it"),
            ("Drag a picture onto a wall", "Place it there"),
            ("Drag along the wall", "Move it"),
            ("Right-click a door", "Flip hinge side or swing direction"),
        ],
    ),
    (
        "3D view",
        &[
            ("Drag", "Overview: rotate around · Walk: look around"),
            (
                "Right-drag, Shift+drag",
                "Overview: pan · Walk: step around",
            ),
            ("Scroll, pinch", "Overview: zoom · Walk: walk forward/back"),
            ("Arrow keys", "Walk: move and turn"),
            (
                "Drag the person on the plan",
                "Walk: move the viewpoint (drag the arrow to turn)",
            ),
        ],
    ),
];

pub fn show(app: &mut RoomPlannerApp, ctx: &egui::Context) {
    egui::Window::new("Keyboard & mouse")
        .open(&mut app.show_help)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            for (section, rows) in SHORTCUTS {
                crate::ui::theme::section_label(ui, section);
                egui::Grid::new(*section)
                    .num_columns(2)
                    .spacing([24.0, 4.0])
                    .min_col_width(170.0)
                    .show(ui, |ui| {
                        for (keys, action) in *rows {
                            ui.label(egui::RichText::new(*keys).monospace().size(12.0));
                            ui.label(*action);
                            ui.end_row();
                        }
                    });
            }
        });
}
