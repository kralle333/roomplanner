use eframe::egui;
use egui::{Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::models::{Tool, Wall};
use crate::ui;

pub const PIXELS_PER_METER: f32 = 50.0;

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
}

impl eframe::App for RoomPlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The Top Panel MUST be drawn first
        ui::top_bar::show(self, ui);

        // The Canvas takes up the remaining space
        ui::canvas::show(self, ui);
    }
}
