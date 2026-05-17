pub mod app;
pub mod geometry;
pub mod models;
pub mod storage;
pub mod tools;
pub mod ui;

use app::RoomPlannerApp;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "RoomPlanner",
        native_options,
        Box::new(|_cc| {
            let mut app = RoomPlannerApp::default();

            if let Ok(data) = std::fs::read_to_string(storage::CONFIG_FILE) {
                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(path_str) = config.get("last_opened_file").and_then(|v| v.as_str())
                    {
                        let path = std::path::PathBuf::from(path_str);
                        if path.exists() {
                            storage::load_from_path(&mut app, &path);
                        }
                    }
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    let mut web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_cc| Ok(Box::new(RoomPlannerApp::default()))),
            )
            .await
            .expect("failed to start eframe");
    });
}
