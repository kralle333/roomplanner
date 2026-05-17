use eframe::egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Copy)]
pub enum Tool {
    Select,
    DrawWall,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Wall {
    pub start: Pos2,
    pub end: Pos2,
}

#[derive(Serialize, Deserialize)]
pub struct AppSaveState {
    pub walls: Vec<Wall>,
    pub pan_offset: Vec2,
    pub zoom_factor: f32,
}
