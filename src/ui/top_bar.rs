use crate::{app::RoomPlannerApp, history, models::Tool, ui::theme::ACCENT};
use eframe::egui::{self, Button, RichText};

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::top("top_panel")
        .exact_size(38.0)
        .show_inside(ui, |ui| {
            // The panel toggles are laid out first from the right, so on a narrow window the
            // left side gets clipped instead of drawn on top of them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut app.show_3d, "3D")
                    .on_hover_text("Show or hide the 3D view");
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
                        if ui.button("⛶").on_hover_text("Zoom to fit [F]").clicked() {
                            app.zoom_to_fit();
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
            .add(Button::new("Zoom to fit").shortcut_text("F"))
            .clicked()
        {
            app.zoom_to_fit();
            ui.close();
        }
        ui.checkbox(&mut app.show_grid, "Grid");
        ui.checkbox(&mut app.scroll_zooms, "Mouse wheel zooms the plan")
            .on_hover_text("Off: scrolling pans (best for touchpads), pinch or Ctrl+scroll zooms");
        ui.checkbox(&mut app.show_library, "Furniture panel");
        ui.checkbox(&mut app.show_3d, "3D view");
    });
}
