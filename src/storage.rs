use crate::app::RoomPlannerApp;
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use crate::models::AppSaveState;

#[cfg(not(target_arch = "wasm32"))]
pub const CONFIG_FILE: &str = ".roomplanner.json";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_from_path(app: &mut RoomPlannerApp, path: &PathBuf) {
    if let Ok(data) = std::fs::read_to_string(path) {
        if let Ok(state) = serde_json::from_str::<AppSaveState>(&data) {
            app.set_walls(state.walls);
            app.furniture = state.furniture;
            app.finishes = state.finishes;
            app.clear_selection();
            app.wall_start_point = None;
            app.pan_offset = state.pan_offset;
            app.zoom_factor = state.zoom_factor;
            app.current_file = Some(path.clone());
            if !app.walls.is_empty() {
                app.current_tool = crate::models::Tool::Select;
            }
            app.mark_saved(true);
            app.pending_fit = true;
            remember_last_file(path);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_to_path(app: &mut RoomPlannerApp, path: &PathBuf) {
    let state = AppSaveState {
        walls: app.walls.clone(),
        furniture: app.furniture.clone(),
        finishes: app.finishes,
        pan_offset: app.pan_offset,
        zoom_factor: app.zoom_factor,
    };
    if let Ok(data) = serde_json::to_string_pretty(&state) {
        if std::fs::write(path, data).is_ok() {
            app.current_file = Some(path.clone());
            app.mark_saved(false);
            remember_last_file(path);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn remember_last_file(path: &PathBuf) {
    let config = serde_json::json!({ "last_opened_file": path.to_string_lossy().to_string() });
    let _ = std::fs::write(
        CONFIG_FILE,
        serde_json::to_string_pretty(&config).unwrap_or_default(),
    );
}

/// Saves to the current file, or asks for one when there is none or `save_as` is set.
#[cfg(not(target_arch = "wasm32"))]
pub fn save(app: &mut RoomPlannerApp, save_as: bool) {
    let path = match &app.current_file {
        Some(path) if !save_as => Some(path.clone()),
        _ => rfd::FileDialog::new()
            .add_filter("Room Plan", &["json"])
            .set_file_name("my_home.json")
            .save_file(),
    };
    if let Some(path) = path {
        save_to_path(app, &path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn open(app: &mut RoomPlannerApp) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Room Plan", &["json"])
        .pick_file()
    {
        load_from_path(app, &path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn forget_last_file() {
    let _ = std::fs::remove_file(CONFIG_FILE);
}

#[cfg(target_arch = "wasm32")]
pub fn load_from_path(_app: &mut RoomPlannerApp, _path: &PathBuf) {}

#[cfg(target_arch = "wasm32")]
pub fn save_to_path(_app: &mut RoomPlannerApp, _path: &PathBuf) {}

#[cfg(target_arch = "wasm32")]
pub fn save(_app: &mut RoomPlannerApp, _save_as: bool) {}

#[cfg(target_arch = "wasm32")]
pub fn open(_app: &mut RoomPlannerApp) {}

#[cfg(target_arch = "wasm32")]
pub fn forget_last_file() {}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::geometry::rooms::extract_rooms;

    /// The plans in `example_plans/` must keep loading as the file format evolves.
    #[test]
    fn example_plans_load() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("example_plans");
        let expected_rooms = [
            ("studio.json", 2),
            ("two_bedroom.json", 3),
            ("lamp_comparison.json", 1),
        ];
        for (file, rooms) in expected_rooms {
            let data = std::fs::read_to_string(dir.join(file)).unwrap();
            let state: AppSaveState = serde_json::from_str(&data)
                .unwrap_or_else(|e| panic!("{file} does not parse: {e}"));
            assert_eq!(extract_rooms(&state.walls).len(), rooms, "{file}: rooms");
            assert!(!state.furniture.is_empty(), "{file}: furniture");
            for wall in &state.walls {
                for opening in &wall.openings {
                    let fits =
                        (opening.clamped_offset(wall.length()) - opening.offset).abs() < 1e-3;
                    assert!(fits, "{file}: opening sticks out of its wall");
                }
            }
        }
    }
}
