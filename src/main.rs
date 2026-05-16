pub mod draw;
pub mod helpers;
pub mod input;
pub mod models;

use eframe::egui;
use egui::{Color32, Frame, Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::{
    draw::draw_scene,
    helpers::{extract_rooms, find_closest_wall, get_hovered_endpoints},
    input::handle_input,
    models::{AppSaveState, Wall},
};

pub const PIXELS_PER_METER: f32 = 50.0;
const CONFIG_FILE: &str = ".roomplanner.json";

#[derive(PartialEq, Clone, Copy)]
pub enum Tool {
    Select,
    DrawWall,
}

pub struct RoomPlannerApp {
    pub current_tool: Tool,
    pub walls: Vec<Wall>,
    pub wall_start_point: Option<Pos2>,
    pub selected_walls: HashSet<usize>,
    pub selection_rect: Option<Rect>,
    pub dragging_endpoints: Vec<(usize, bool)>,
    pub rooms: Vec<Vec<Pos2>>,

    pub pan_offset: Vec2,
    pub zoom_factor: f32,

    pub current_file: Option<PathBuf>,
}

impl Default for RoomPlannerApp {
    fn default() -> Self {
        Self {
            current_tool: Tool::DrawWall,
            walls: Vec::new(),
            wall_start_point: None,
            selected_walls: HashSet::new(),
            selection_rect: None,
            dragging_endpoints: Vec::new(),
            rooms: Vec::new(),
            pan_offset: Vec2::ZERO,
            zoom_factor: 1.0,
            current_file: None,
        }
    }
}

impl RoomPlannerApp {
    pub fn world_to_screen(&self, p: Pos2) -> Pos2 {
        Pos2::new(p.x * self.zoom_factor, p.y * self.zoom_factor) + self.pan_offset
    }
    pub fn screen_to_world(&self, p: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - self.pan_offset.x) / self.zoom_factor,
            (p.y - self.pan_offset.y) / self.zoom_factor,
        )
    }

    pub fn load_from_path(&mut self, path: &PathBuf) {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<AppSaveState>(&data) {
                self.walls = state.walls;
                self.pan_offset = state.pan_offset;
                self.zoom_factor = state.zoom_factor;
                self.current_file = Some(path.clone());

                self.rooms = extract_rooms(&self.walls);

                let config =
                    serde_json::json!({ "last_opened_file": path.to_string_lossy().to_string() });
                let _ = std::fs::write(
                    CONFIG_FILE,
                    serde_json::to_string_pretty(&config).unwrap_or_default(),
                );
            }
        }
    }

    pub fn save_to_path(&mut self, path: &PathBuf) {
        let state = AppSaveState {
            walls: self.walls.clone(),
            pan_offset: self.pan_offset,
            zoom_factor: self.zoom_factor,
        };
        if let Ok(data) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(path, data);
            self.current_file = Some(path.clone());

            let config =
                serde_json::json!({ "last_opened_file": path.to_string_lossy().to_string() });
            let _ = std::fs::write(
                CONFIG_FILE,
                serde_json::to_string_pretty(&config).unwrap_or_default(),
            );
        }
    }
}

impl eframe::App for RoomPlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // --- TOP MENU BAR ---
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            egui::MenuBar::default().ui(ui, |ui| {
                ui.heading("📐");
                ui.separator();

                // 1. File Dropdown
                ui.menu_button("File", |ui| {
                    if ui.button("💾 Save").clicked() {
                        if let Some(path) = &self.current_file {
                            self.save_to_path(&path.clone());
                        } else if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Room Plan", &["json"])
                            .set_file_name("my_house.json")
                            .save_file()
                        {
                            self.save_to_path(&path);
                        }
                        ui.close();
                    }

                    if ui.button("📂 Load").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Room Plan", &["json"])
                            .pick_file()
                        {
                            self.load_from_path(&path);
                        }
                        ui.close();
                    }

                    ui.separator();

                    if ui.button(" Close").clicked() {
                        self.walls.clear();
                        self.selected_walls.clear();
                        self.rooms.clear();
                        self.wall_start_point = None;
                        self.pan_offset = Vec2::ZERO;
                        self.zoom_factor = 1.0;
                        self.current_file = None;
                        let _ = std::fs::remove_file(CONFIG_FILE);
                        ui.close();
                    }
                });

                ui.separator();

                // 2. Tools
                if ui
                    .selectable_value(&mut self.current_tool, Tool::Select, "✋ Select / Edit")
                    .clicked()
                {
                    self.wall_start_point = None;
                }
                ui.selectable_value(&mut self.current_tool, Tool::DrawWall, "🧱 Draw Wall");

                // 3. Current File Name (Aligned to the right)
                if let Some(path) = &self.current_file {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!(
                            "📄 {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    });
                }
            });
        });

        // --- CENTRAL CANVAS ---
        let frame = Frame::central_panel(ui.style()).fill(Color32::WHITE);

        egui::CentralPanel::default()
            .frame(frame)
            .show_inside(ui, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

                let mut zoom_multiplier = ui.ctx().input(|i| i.zoom_delta());
                let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);

                if scroll_delta != 0.0 {
                    zoom_multiplier *= 1.0 + (scroll_delta * 0.002);
                }

                if zoom_multiplier != 1.0 {
                    if let Some(mouse_pos) = ui.ctx().pointer_hover_pos() {
                        let old_zoom = self.zoom_factor;
                        self.zoom_factor *= zoom_multiplier;
                        self.zoom_factor = self.zoom_factor.clamp(0.1, 20.0);
                        let mouse_vec = mouse_pos.to_vec2();
                        self.pan_offset = mouse_vec
                            - (mouse_vec - self.pan_offset) * (self.zoom_factor / old_zoom);
                    }
                }

                let is_panning = ui.ctx().input(|i| {
                    i.pointer.button_down(egui::PointerButton::Middle)
                        || i.key_down(egui::Key::Space)
                });

                if is_panning {
                    if response.dragged() {
                        self.pan_offset += response.drag_delta();
                    }
                }

                let pointer = ui
                    .ctx()
                    .pointer_hover_pos()
                    .map(|p| self.screen_to_world(p));
                let interact_pointer = response
                    .interact_pointer_pos()
                    .map(|p| self.screen_to_world(p));

                let hovered_endpoints = pointer
                    .map(|p| get_hovered_endpoints(&self.walls, p, self.zoom_factor))
                    .unwrap_or_default();
                let hovered_wall_idx = if hovered_endpoints.is_empty() {
                    pointer.and_then(|p| find_closest_wall(&self.walls, p, self.zoom_factor))
                } else {
                    None
                };

                let mut active_alignments = Vec::new();
                let mut snapped_preview = None;
                let mut snapped_wall_idx = None;

                if !is_panning {
                    let (alignments, preview, wall_idx) =
                        handle_input(self, ui, &response, pointer, interact_pointer);
                    active_alignments = alignments;
                    snapped_preview = preview;
                    snapped_wall_idx = wall_idx;
                }

                draw_scene(
                    self,
                    &painter,
                    pointer,
                    &hovered_endpoints,
                    hovered_wall_idx,
                    &active_alignments,
                    snapped_preview,
                    snapped_wall_idx,
                );
            });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "RoomPlanner",
        native_options,
        Box::new(|_cc| {
            let mut app = RoomPlannerApp::default();

            if let Ok(data) = std::fs::read_to_string(CONFIG_FILE) {
                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(path_str) = config.get("last_opened_file").and_then(|v| v.as_str())
                    {
                        let path = std::path::PathBuf::from(path_str);
                        if path.exists() {
                            app.load_from_path(&path);
                        }
                    }
                }
            }

            Ok(Box::new(app))
        }),
    )
}
