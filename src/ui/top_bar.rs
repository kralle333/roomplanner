use crate::{
    app::{CameraMode, RoomPlannerApp},
    history,
    models::Tool,
    ui::theme::ACCENT,
};
use eframe::egui::{self, Button, RichText};

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::top("top_panel")
        .exact_size(38.0)
        .show_inside(ui, |ui| {
            // The panel toggles are laid out first from the right, so on a narrow window the
            // left side gets clipped instead of drawn on top of them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut app.show_library, "☰ Library")
                    .on_hover_text("Show or hide the furniture panel");
                ui.separator();

                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // On narrow windows drop the labels and keep the icons.
                    let compact = ui.available_width() < 560.0;
                    ui.shrink_clip_rect(ui.max_rect());
                    let title = if compact { "📐" } else { "📐 RoomPlanner" };
                    ui.label(RichText::new(title).strong().size(15.0).color(ACCENT));
                    ui.add_space(6.0);

                    egui::MenuBar::default().ui(ui, |ui| {
                        file_menu(app, ui);
                        edit_menu(app, ui);
                        view_menu(app, ui);
                        ui.menu_button("Help", |ui| {
                            if ui.button("Keyboard & mouse…").clicked() {
                                app.show_help = true;
                                ui.close();
                            }
                        });

                        ui.separator();
                        let (select, walls) = if compact {
                            ("⬉", "✏")
                        } else {
                            ("⬉ Select", "✏ Walls")
                        };
                        tool_button(app, ui, Tool::Select, select, "Select and move [V]");
                        tool_button(app, ui, Tool::DrawWall, walls, "Draw walls [W]");
                        let resize = if compact { "⇔" } else { "⇔ Resize rooms" };
                        ui.toggle_value(&mut app.stretch_walls, resize).on_hover_text(
                            "On: moving a wall stretches the walls connected to it, so the rooms \
                             grow or shrink. Off: the wall moves on its own. Alt+drag does the opposite.",
                        );
                        ui.separator();

                        if ui
                            .add_enabled(app.history.can_undo(), Button::new("⟲"))
                            .on_hover_text("Undo [Ctrl+Z]")
                            .clicked()
                        {
                            history::undo(app);
                        }
                        if ui
                            .add_enabled(app.history.can_redo(), Button::new("⟳"))
                            .on_hover_text("Redo [Ctrl+Shift+Z]")
                            .clicked()
                        {
                            history::redo(app);
                        }
                    });
                });
            });
        });
}

fn tool_button(app: &mut RoomPlannerApp, ui: &mut egui::Ui, tool: Tool, label: &str, hint: &str) {
    if ui
        .selectable_label(app.current_tool == tool, label)
        .on_hover_text(hint)
        .clicked()
    {
        app.current_tool = tool;
        app.wall_start_point = None;
    }
}

fn file_menu(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    ui.menu_button("File", |ui| {
        if ui.button("New plan").clicked() {
            app.new_plan();
            ui.close();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::storage;
            if ui
                .add(Button::new("Open…").shortcut_text("Ctrl+O"))
                .clicked()
            {
                storage::open(app);
                ui.close();
            }
            ui.separator();
            if ui
                .add(Button::new("Save").shortcut_text("Ctrl+S"))
                .clicked()
            {
                storage::save(app, false);
                ui.close();
            }
            if ui
                .add(Button::new("Save as…").shortcut_text("Ctrl+Shift+S"))
                .clicked()
            {
                storage::save(app, true);
                ui.close();
            }
            ui.separator();
            if ui
                .button("Import layouts from file…")
                .on_hover_text(
                    "Bring the furniture of other plan files in as layout tabs, to compare. \
                     Their walls are not used",
                )
                .clicked()
            {
                storage::import_layouts(app);
                ui.close();
            }
        }
    });
}

fn edit_menu(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    ui.menu_button("Edit", |ui| {
        if ui
            .add_enabled(
                app.history.can_undo(),
                Button::new("Undo").shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            history::undo(app);
            ui.close();
        }
        if ui
            .add_enabled(
                app.history.can_redo(),
                Button::new("Redo").shortcut_text("Ctrl+Shift+Z"),
            )
            .clicked()
        {
            history::redo(app);
            ui.close();
        }
        ui.separator();
        let has_furniture = app.selected_furniture.is_some();
        if ui
            .add_enabled(
                has_furniture,
                Button::new("Duplicate").shortcut_text("Ctrl+D"),
            )
            .clicked()
        {
            app.duplicate_selected_furniture();
            ui.close();
        }
        if ui
            .add_enabled(has_furniture, Button::new("Cut").shortcut_text("Ctrl+X"))
            .clicked()
        {
            app.cut_selected_furniture(ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(has_furniture, Button::new("Copy").shortcut_text("Ctrl+C"))
            .clicked()
        {
            app.copy_selected_furniture(ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(
                !app.furniture_clipboard.is_empty(),
                Button::new("Paste").shortcut_text("Ctrl+V"),
            )
            .clicked()
        {
            app.paste_furniture(None, None);
            ui.close();
        }
        if ui
            .add_enabled(has_furniture, Button::new("Select all like this"))
            .on_hover_text("Select every piece with the same name and type")
            .clicked()
        {
            app.select_similar_furniture();
            ui.close();
        }
        ui.separator();
        let has_selection = has_furniture || !app.selected_walls.is_empty();
        if ui
            .add_enabled(has_selection, Button::new("Delete").shortcut_text("Del"))
            .clicked()
        {
            app.delete_selection();
            ui.close();
        }
    });
}

fn view_menu(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    ui.menu_button("View", |ui| {
        if ui
            .add(Button::new("Show whole plan"))
            .clicked()
        {
            app.fit_whole_plan();
            ui.close();
        }
        ui.separator();
        let mut mode = app.camera_mode;
        ui.radio_value(&mut mode, CameraMode::Top, "Top view (T)");
        ui.radio_value(&mut mode, CameraMode::Orbit, "3D view (T)");
        ui.radio_value(&mut mode, CameraMode::Walk, "Walk");
        app.set_camera_mode(mode);
        ui.separator();
        ui.checkbox(&mut app.show_grid, "Floor grid");
        ui.checkbox(&mut app.scroll_zooms, "Touchpad scroll zooms the top view")
            .on_hover_text("Off: two-finger scroll pans the top view and pinch zooms. A mouse wheel always zooms");
        ui.checkbox(&mut app.show_library, "Furniture panel");
    });
}
