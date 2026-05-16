use epaint::{Pos2, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Wall {
    pub(crate) start: Pos2,
    pub(crate) end: Pos2,
}

#[derive(Serialize, Deserialize)]
pub struct AppSaveState {
    pub walls: Vec<Wall>,
    pub pan_offset: Vec2,
    pub zoom_factor: f32,
}
