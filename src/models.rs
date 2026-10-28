use eframe::egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Copy)]
pub enum Tool {
    Select,
    DrawWall,
    /// Placing a window or door (the template is `RoomPlannerApp::opening_template`).
    PlaceOpening,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub start: Pos2,
    pub end: Pos2,
    /// Windows and doors in this wall; they move and disappear with it.
    #[serde(default)]
    pub openings: Vec<Opening>,
}

impl Wall {
    pub fn new(start: Pos2, end: Pos2) -> Self {
        Wall {
            start,
            end,
            openings: Vec::new(),
        }
    }

    /// Length in meters.
    pub fn length(&self) -> f32 {
        self.start.distance(self.end) / crate::app::PIXELS_PER_METER
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OpeningKind {
    Window,
    Door,
    /// A glazed door, e.g. to a balcony.
    GlassDoor,
}

impl OpeningKind {
    pub fn label(self) -> &'static str {
        match self {
            OpeningKind::Window => "Window",
            OpeningKind::Door => "Door",
            OpeningKind::GlassDoor => "Glass door",
        }
    }

    pub fn swings(self) -> bool {
        self != OpeningKind::Window
    }

    pub fn glazed(self) -> bool {
        self != OpeningKind::Door
    }
}

/// A window or door in a wall. Lengths in meters.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    pub kind: OpeningKind,
    /// Distance from the wall's start to the middle of the opening.
    pub offset: f32,
    pub width: f32,
    pub height: f32,
    /// Height of the bottom edge above the floor; 0 for doors.
    pub sill: f32,
    /// Doors: hinge at the other end.
    #[serde(default)]
    pub flip_hinge: bool,
    /// Doors: swing to the other side of the wall.
    #[serde(default)]
    pub flip_side: bool,
}

impl Opening {
    /// The offset kept far enough from the wall ends for the opening to fit.
    pub fn clamped_offset(&self, wall_length: f32) -> f32 {
        let half = (self.width / 2.0).min(wall_length / 2.0);
        self.offset.clamp(half, (wall_length - half).max(half))
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LampStyle {
    /// Stands on the floor; the shade sits on top of a pole.
    Floor,
    /// Like a floor lamp, but smaller and usually raised (elevation) onto a table.
    Table,
    /// Hangs from the ceiling; its elevation is the bottom of the shade.
    Pendant,
    /// Mounted on a wall at its elevation.
    Wall,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ShadeShape {
    Cone,
    Drum,
    Globe,
}

/// The parametric shape of a piece of furniture. All lengths are in meters.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum FurnitureKind {
    /// A solid block: cabinets, wardrobes, rugs, appliances...
    Box,
    Table {
        top_thickness: f32,
        leg_size: f32,
        #[serde(default)]
        round: bool,
    },
    Chair {
        seat_height: f32,
    },
    Bed {
        headboard_height: f32,
    },
    Sofa {
        seat_height: f32,
        arm_width: f32,
        back_depth: f32,
    },
    Shelf {
        /// Number of horizontal boards, including top and bottom.
        shelves: u32,
    },
    Plant,
    Lamp {
        style: LampStyle,
        shade: ShadeShape,
        /// Only used by floor and table lamps; pendants and wall lamps are all shade.
        shade_height: f32,
        lumens: f32,
        kelvin: f32,
        on: bool,
    },
    /// An imported glTF/GLB model, scaled to fit the furniture's size.
    Model {
        path: String,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Furniture {
    pub name: String,
    pub kind: FurnitureKind,
    /// Center of the footprint, in world (canvas) units.
    pub position: Pos2,
    /// Rotation around the vertical axis, in radians.
    pub rotation: f32,
    /// Width (local x), depth (local z) and height, in meters.
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    /// Height of the bottom above the floor, e.g. a table lamp on a table.
    #[serde(default)]
    pub elevation: f32,
    pub color: [u8; 3],
    /// Alternatives to try in this spot. When non-empty it holds every option,
    /// with `options[active_option]` being a (possibly stale) copy of this item.
    #[serde(default)]
    pub options: Vec<Furniture>,
    #[serde(default)]
    pub active_option: usize,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FloorFinish {
    LightOak,
    Oak,
    Walnut,
    WhiteWashed,
    GrayOak,
    Concrete,
}

impl FloorFinish {
    pub const ALL: [FloorFinish; 6] = [
        FloorFinish::LightOak,
        FloorFinish::Oak,
        FloorFinish::Walnut,
        FloorFinish::WhiteWashed,
        FloorFinish::GrayOak,
        FloorFinish::Concrete,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FloorFinish::LightOak => "Light oak",
            FloorFinish::Oak => "Oak",
            FloorFinish::Walnut => "Walnut",
            FloorFinish::WhiteWashed => "White-washed wood",
            FloorFinish::GrayOak => "Gray oak",
            FloorFinish::Concrete => "Concrete",
        }
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            FloorFinish::LightOak => [214, 178, 132],
            FloorFinish::Oak => [190, 142, 92],
            FloorFinish::Walnut => [118, 80, 54],
            FloorFinish::WhiteWashed => [226, 214, 196],
            FloorFinish::GrayOak => [168, 152, 136],
            FloorFinish::Concrete => [182, 182, 178],
        }
    }

    pub fn is_wood(self) -> bool {
        self != FloorFinish::Concrete
    }
}

/// How the home itself looks: wall paint and flooring.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Finishes {
    pub wall_color: [u8; 3],
    pub floor: FloorFinish,
}

impl Default for Finishes {
    fn default() -> Self {
        Finishes {
            wall_color: [246, 246, 244],
            floor: FloorFinish::LightOak,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppSaveState {
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub furniture: Vec<Furniture>,
    #[serde(default)]
    pub finishes: Finishes,
    pub pan_offset: Vec2,
    pub zoom_factor: f32,
}
