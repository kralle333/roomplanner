use crate::app::RoomPlannerApp;
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use crate::{geometry::rooms::extract_rooms, models::AppSaveState};

#[cfg(not(target_arch = "wasm32"))]
pub const CONFIG_FILE: &str = ".roomplanner.json";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_from_path(app: &mut RoomPlannerApp, path: &PathBuf) {
    if let Ok(data) = std::fs::read_to_string(path) {
        if let Ok(state) = serde_json::from_str::<AppSaveState>(&data) {
            app.walls = state.walls;
            app.pan_offset = state.pan_offset;
            app.zoom_factor = state.zoom_factor;
            app.current_file = Some(path.clone());
            app.rooms = extract_rooms(&app.walls);

            let config =
                serde_json::json!({ "last_opened_file": path.to_string_lossy().to_string() });
            let _ = std::fs::write(
                CONFIG_FILE,
                serde_json::to_string_pretty(&config).unwrap_or_default(),
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn load_from_path(_app: &mut RoomPlannerApp, _path: &PathBuf) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_to_path(app: &mut RoomPlannerApp, path: &PathBuf) {
    let state = AppSaveState {
        walls: app.walls.clone(),
        pan_offset: app.pan_offset,
        zoom_factor: app.zoom_factor,
    };
    if let Ok(data) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(path, data);
        app.current_file = Some(path.clone());

        let config = serde_json::json!({ "last_opened_file": path.to_string_lossy().to_string() });
        let _ = std::fs::write(
            CONFIG_FILE,
            serde_json::to_string_pretty(&config).unwrap_or_default(),
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub fn save_to_path(_app: &mut RoomPlannerApp, _path: &PathBuf) {}
