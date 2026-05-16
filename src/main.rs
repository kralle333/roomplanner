pub mod draw;
pub mod helpers;
pub mod input;
pub mod models;

use eframe::egui;
use egui::{Color32, Frame, Pos2, Rect, Vec2};
use std::collections::HashSet;

use crate::{
    draw::draw_scene,
    helpers::{find_closest_wall, get_hovered_endpoints},
    input::handle_input,
    models::Wall,
};

pub const PIXELS_PER_METER: f32 = 50.0;

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

    // --- Camera State ---
    pub pan_offset: Vec2,
    pub zoom_factor: f32, // NEW
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
            zoom_factor: 1.0, // Default 100% scale
        }
    }
}

// --- Coordinate Converters ---
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
}

impl eframe::App for RoomPlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("📐 RoomPlanner");
                ui.separator();

                if ui
                    .selectable_value(&mut self.current_tool, Tool::Select, "✋ Select / Edit")
                    .clicked()
                {
                    self.wall_start_point = None;
                }
                ui.selectable_value(&mut self.current_tool, Tool::DrawWall, "🧱 Draw Wall");

                ui.separator();
                if ui.button("🗑️ Clear").clicked() {
                    self.walls.clear();
                    self.selected_walls.clear();
                    self.rooms.clear();
                    self.wall_start_point = None;
                    self.pan_offset = Vec2::ZERO;
                    self.zoom_factor = 1.0;
                }
            });
        });

        let frame = Frame::central_panel(ui.style()).fill(Color32::WHITE);

        egui::CentralPanel::default()
            .frame(frame)
            .show_inside(ui, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

                // --- CAMERA ZOOMING LOGIC ---
                let mut zoom_multiplier = ui.ctx().input(|i| i.zoom_delta());
                let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);

                // Allow standard mouse wheel to zoom (without needing Ctrl)
                if scroll_delta != 0.0 {
                    zoom_multiplier *= 1.0 + (scroll_delta * 0.002);
                }

                if zoom_multiplier != 1.0 {
                    if let Some(mouse_pos) = ui.ctx().pointer_hover_pos() {
                        let old_zoom = self.zoom_factor;
                        self.zoom_factor *= zoom_multiplier;
                        self.zoom_factor = self.zoom_factor.clamp(0.1, 20.0); // Restrict to 10% - 2000% scale

                        // Zoom exactly towards the mouse pointer
                        let mouse_vec = mouse_pos.to_vec2();
                        self.pan_offset = mouse_vec
                            - (mouse_vec - self.pan_offset) * (self.zoom_factor / old_zoom);
                    }
                }

                // --- CAMERA PANNING LOGIC ---
                let is_panning = ui.ctx().input(|i| {
                    i.pointer.button_down(egui::PointerButton::Middle)
                        || i.key_down(egui::Key::Space)
                });

                if is_panning {
                    if response.dragged() {
                        self.pan_offset += response.drag_delta();
                    }
                }

                // Convert pointer coordinates to World Space
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

                // Only handle tool input if we aren't panning
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
    eframe::run_native(
        "RoomPlanner",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(RoomPlannerApp::default()))),
    )
}
