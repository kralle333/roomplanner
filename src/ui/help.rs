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
            (
                "F",
                "Zoom to the selection (3D: seen from its front), or the whole plan",
            ),
            ("F1", "Show this help"),
            (
                "Arrow keys (Shift)",
                "Nudge selected furniture 1 cm (10 cm)",
            ),
        ],
    ),
    (
        "Camera",
        &[
            ("T", "Switch between top view and 3D"),
            (
                "Top view: two-finger scroll / pinch / wheel",
                "Pan / zoom / zoom",
            ),
            ("Top view: right-, middle-, Space+drag", "Pan"),
            (
                "3D: two-finger scroll, right-drag, Space+drag",
                "Rotate around",
            ),
            (
                "3D: Shift+two-finger scroll, middle-drag, Shift+right-drag",
                "Pan",
            ),
            ("3D: pinch, mouse wheel", "Zoom"),
            ("Walk: drag / right-drag", "Look around / step"),
            ("Walk: arrows, WASD", "Walk and turn"),
            (
                "Right-click",
                "Menu: add furniture here, stand here, delete…",
            ),
            ("Esc", "Cancel the wall, then leave the tool, then deselect"),
        ],
    ),
    (
        "Walls",
        &[
            (
                "Drag a wall",
                "Move it (Resize rooms on: connected walls stretch)",
            ),
            (
                "Alt+drag a wall",
                "Move it the other way: detached / stretching",
            ),
            ("Drag a corner", "Reshape"),
            (
                "Type 350 or 3.5, Enter",
                "While drawing: next wall exactly 350 cm",
            ),
            (
                "Properties panel",
                "Type exact length, inside length, angle, height",
            ),
            (
                "While moving a wall",
                "Lines up with other walls' corners and ends",
            ),
            ("Ctrl+drag a wall", "Move without lining up"),
            (
                "While dragging",
                "Walls joined to it show their new lengths",
            ),
        ],
    ),
    (
        "Furniture",
        &[
            ("Click or drag a picture", "Add from the library"),
            ("Drag", "Move (snaps to walls)"),
            ("Alt+drag", "Move without snapping"),
            (
                "Drag a corner or side",
                "Resize in whole cm (Alt: freely, Shift on a corner: keep proportions)",
            ),
            ("Drag the ⟳ handle", "Rotate in 15° steps (Alt: freely)"),
            ("R / Shift+R", "Rotate 90° right / left"),
            (
                "Shift+click, box-select",
                "Select several; panel edits apply to all",
            ),
            (
                "Ctrl+C / Ctrl+X / Ctrl+V",
                "Copy / cut / paste at the mouse",
            ),
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
