use crate::app::RoomPlannerApp;
use crate::models::{AppSaveState, SavedCamera};
use std::path::PathBuf;

/// The plan as it would be written to a file.
pub fn current_state(app: &RoomPlannerApp) -> AppSaveState {
    let layouts = crate::layouts::all(app);
    // While peeking, save the layout that is really being worked on.
    let active = app.peek_return.unwrap_or(app.active_layout);
    let shown = &layouts[active.min(layouts.len() - 1)];
    AppSaveState {
        walls: app.walls.clone(),
        furniture: shown.furniture.clone(),
        finishes: shown.finishes,
        active_layout: active,
        pan_offset: app.pan_offset,
        zoom_factor: app.zoom_factor,
        camera: Some(current_camera(app)),
        // One layout under the default name: written as a plain plan, like before.
        layouts: if layouts.len() > 1 || layouts[0].name != crate::layouts::first_name() {
            layouts
        } else {
            Vec::new()
        },
    }
}

pub fn current_camera(app: &RoomPlannerApp) -> SavedCamera {
    let zoom = app.zoom_factor.max(1e-3);
    SavedCamera {
        mode: app.camera_mode,
        top_center: ((app.canvas_rect.center().to_vec2() - app.pan_offset) / zoom).to_pos2(),
        zoom,
        orbit_target: app.orbit_pan,
        orbit_yaw: app.orbit_yaw,
        orbit_pitch: app.orbit_pitch,
        orbit_distance: app.orbit_distance,
        walk_position: app.camera_pos,
        walk_angle: app.camera_angle,
        walk_pitch: app.camera_pitch,
    }
}

/// Puts the camera back where it was saved. Needs the viewport's size, so it runs once the
/// canvas has been laid out (see `RoomPlannerApp::pending_camera`).
pub fn apply_camera(app: &mut RoomPlannerApp, camera: SavedCamera) {
    app.camera_mode = camera.mode;
    app.zoom_factor = camera.zoom.clamp(0.1, 20.0);
    app.pan_offset =
        app.canvas_rect.center().to_vec2() - camera.top_center.to_vec2() * app.zoom_factor;
    app.orbit_pan = camera.orbit_target;
    app.orbit_yaw = camera.orbit_yaw;
    app.orbit_pitch = camera.orbit_pitch.clamp(0.08, 1.55);
    app.orbit_distance = camera.orbit_distance.clamp(1.0, 80.0);
    app.camera_pos = camera.walk_position;
    app.camera_angle = camera.walk_angle;
    app.camera_pitch = camera.walk_pitch.clamp(-1.3, 1.3);
}

/// Replaces the plan with `state`, leaving file and undo bookkeeping to the caller.
pub fn apply_state(app: &mut RoomPlannerApp, state: AppSaveState) {
    app.set_walls(state.walls);
    let layouts = if state.layouts.is_empty() {
        vec![crate::models::Layout {
            name: crate::layouts::first_name(),
            furniture: state.furniture,
            finishes: state.finishes,
        }]
    } else {
        state.layouts
    };
    crate::layouts::set_all(app, layouts, state.active_layout);
    app.clear_selection();
    app.wall_start_point = None;
    app.pan_offset = state.pan_offset;
    app.zoom_factor = state.zoom_factor;
    if !app.walls.is_empty() {
        app.current_tool = crate::models::Tool::Select;
    }
    // The saved camera, or else a view of the whole plan, once the viewport has its size.
    app.pending_camera = state.camera;
    app.pending_fit = true;
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_state(path: &PathBuf) -> Option<AppSaveState> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn read_state(_path: &PathBuf) -> Option<AppSaveState> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub const CONFIG_FILE: &str = ".roomplanner.json";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_from_path(app: &mut RoomPlannerApp, path: &PathBuf) {
    if let Some(state) = read_state(path) {
        apply_state(app, state);
        app.current_file = Some(path.clone());
        app.mark_saved(true);
        remember_last_file(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_to_path(app: &mut RoomPlannerApp, path: &PathBuf) {
    let state = current_state(app);
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

/// Adds the layouts of other plan files as tabs in this plan; their walls are not used.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_layouts(app: &mut RoomPlannerApp) {
    let Some(paths) = rfd::FileDialog::new()
        .add_filter("Room Plan", &["json"])
        .set_title("Import layouts: their furniture comes into this plan's walls")
        .pick_files()
    else {
        return;
    };
    let first_new = app.layouts.len();
    for path in paths {
        if let Some(state) = read_state(&path) {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Imported".to_owned());
            crate::layouts::import(app, &stem, state);
        }
    }
    if app.layouts.len() > first_new {
        crate::layouts::switch(app, first_new);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn import_layouts(_app: &mut RoomPlannerApp) {}

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

    #[test]
    fn the_camera_is_saved_with_the_plan() {
        use crate::app::CameraMode;
        use eframe::egui::{pos2, vec2, Rect};
        let mut app = RoomPlannerApp::default();
        app.canvas_rect = Rect::from_min_size(pos2(200.0, 40.0), vec2(900.0, 700.0));
        app.camera_mode = CameraMode::Orbit;
        app.orbit_yaw = 1.2;
        app.orbit_pitch = 0.4;
        app.orbit_distance = 9.0;
        app.orbit_pan = vec2(3.0, 2.5);
        app.zoom_factor = 1.5;
        app.pan_offset = vec2(100.0, 50.0);
        let json = serde_json::to_string(&current_state(&app)).unwrap();

        // Opened in a window of another size: same 3D view, same plan point in the middle.
        let mut other = RoomPlannerApp::default();
        other.canvas_rect = Rect::from_min_size(pos2(250.0, 40.0), vec2(600.0, 500.0));
        let state: AppSaveState = serde_json::from_str(&json).unwrap();
        apply_camera(&mut other, state.camera.unwrap());
        assert!(other.camera_mode == CameraMode::Orbit);
        assert_eq!(
            (
                other.orbit_yaw,
                other.orbit_pitch,
                other.orbit_distance,
                other.orbit_pan
            ),
            (1.2, 0.4, 9.0, vec2(3.0, 2.5))
        );
        let middle =
            |a: &RoomPlannerApp| (a.canvas_rect.center().to_vec2() - a.pan_offset) / a.zoom_factor;
        assert!((middle(&app) - middle(&other)).length() < 1e-3);
    }

    #[test]
    fn layouts_are_saved_with_the_plan() {
        let mut app = RoomPlannerApp::default();
        app.furniture = vec![(crate::furniture::PRESETS[0].build)()];
        // A plain plan stays a plain plan in the file.
        assert!(current_state(&app).layouts.is_empty());

        crate::layouts::duplicate(&mut app, 0);
        app.furniture.clear();
        app.layouts[0].name = "Before".to_owned();
        let json = serde_json::to_string(&current_state(&app)).unwrap();
        let state: AppSaveState = serde_json::from_str(&json).unwrap();
        // Older versions read the shown layout.
        assert!(state.furniture.is_empty());

        // Showing another layout is not an unsaved change.
        app.mark_saved(false);
        crate::layouts::switch(&mut app, 0);
        assert!(!app.is_dirty());
        app.furniture.clear();
        assert!(app.is_dirty());

        let mut other = RoomPlannerApp::default();
        apply_state(&mut other, state);
        assert_eq!(other.layouts.len(), 2);
        assert_eq!(other.layouts[0].name, "Before");
        assert_eq!(other.active_layout, 1);
        assert!(other.furniture.is_empty());
        assert_eq!(crate::layouts::all(&other)[0].furniture.len(), 1);
    }

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
