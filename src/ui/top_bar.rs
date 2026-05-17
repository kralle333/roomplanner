use crate::{app::RoomPlannerApp, models::Tool, storage};
use eframe::egui;

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::top("top_panel").show_inside(ui, |ui| {
        egui::MenuBar::default().ui(ui, |ui| {
            ui.heading("📐");
            ui.separator();

            ui.menu_button("File", |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("💾 Save").clicked() {
                    if let Some(path) = &app.current_file {
                        storage::save_to_path(app, &path.clone());
                    } else if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Room Plan", &["json"])
                        .set_file_name("my_house.json")
                        .save_file()
                    {
                        storage::save_to_path(app, &path);
                    }
                    ui.close();
                }

                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("📂 Load").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Room Plan", &["json"])
                        .pick_file()
                    {
                        storage::load_from_path(app, &path);
                    }
                    ui.close();
                }

                #[cfg(not(target_arch = "wasm32"))]
                ui.separator();

                if ui.button("🗑️ Clear Canvas").clicked() {
                    app.walls.clear();
                    app.selected_walls.clear();
                    app.rooms.clear();
                    app.wall_start_point = None;
                    app.pan_offset = egui::Vec2::ZERO;
                    app.zoom_factor = 1.0;
                    app.current_file = None;

                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = std::fs::remove_file(storage::CONFIG_FILE);

                    ui.close();
                }
            });

            ui.separator();

            if ui
                .selectable_value(&mut app.current_tool, Tool::Select, "✋ Select / Edit")
                .clicked()
            {
                app.wall_start_point = None;
            }
            ui.selectable_value(&mut app.current_tool, Tool::DrawWall, "🧱 Draw Wall");

            if let Some(path) = &app.current_file {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "📄 {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                });
            }
        });
    });
}
