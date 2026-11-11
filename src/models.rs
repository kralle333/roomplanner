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
    /// Height from the floor, in meters.
    #[serde(default = "default_wall_height")]
    pub height: f32,
}

fn default_wall_height() -> f32 {
    crate::app::WALL_HEIGHT
}

impl Wall {
    pub fn new(start: Pos2, end: Pos2) -> Self {
        Wall {
            start,
            end,
            openings: Vec::new(),
            height: default_wall_height(),
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
    /// A bar on the ceiling with a row of spots hanging from it, spread along its width.
    Track,
    /// A row of spots set into the ceiling, spread along the width.
    Downlights,
    /// A long bar light hanging from the ceiling on two wires, e.g. over a dining table.
    LinearPendant,
    /// A thin strip of light, e.g. under kitchen wall cabinets.
    Strip,
}

impl LampStyle {
    /// Several spots in a row, set by `FurnitureKind::Lamp::spots`.
    pub fn has_spots(self) -> bool {
        matches!(self, LampStyle::Track | LampStyle::Downlights)
    }

    /// Lit along its length rather than from one bulb.
    pub fn is_linear(self) -> bool {
        matches!(self, LampStyle::LinearPendant | LampStyle::Strip)
    }

    /// Has a lamp shade (cone, drum or globe).
    pub fn has_shade(self) -> bool {
        matches!(
            self,
            LampStyle::Floor | LampStyle::Table | LampStyle::Pendant | LampStyle::Wall
        )
    }
}

fn one() -> u32 {
    1
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
    /// An open grid of square compartments with a thick outer frame and thin dividers.
    CubeShelf {
        columns: u32,
        rows: u32,
        /// Inner width of each column, left to right. Treated as proportions of the
        /// available space; empty (or the wrong length) means equal columns.
        #[serde(default)]
        column_widths: Vec<f32>,
        /// Inner height of each row, bottom to top. Same rules as `column_widths`.
        #[serde(default)]
        row_heights: Vec<f32>,
    },
    Plant,
    /// A standing person, to check heights, reach and where shadows fall. Faces +z.
    Person,
    Lamp {
        style: LampStyle,
        shade: ShadeShape,
        /// Only used by floor and table lamps; pendants and wall lamps are all shade.
        shade_height: f32,
        /// Per spot for lamps with several spots.
        lumens: f32,
        kelvin: f32,
        on: bool,
        /// Number of spots, for spot tracks and rows of ceiling spots.
        #[serde(default = "one")]
        spots: u32,
        /// Spots: how far the outermost ones are turned outwards along the row, in
        /// degrees; the ones between turn in proportion.
        #[serde(default)]
        spread: f32,
        /// Spots: how far all of them lean towards the front (+) or back (−), in degrees.
        #[serde(default)]
        tilt: f32,
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
    /// A different color for the top surface, e.g. a countertop on a cabinet, a tabletop
    /// or the top board of a shelf (see `Furniture::has_top`).
    #[serde(default)]
    pub top_color: Option<[u8; 3]>,
    /// The top is solid wood like a butcher-block worktop, tinted by the top color (or the
    /// piece's color when the top has none of its own).
    #[serde(default)]
    pub top_wood: bool,
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

/// One version of how the home is furnished and finished. All layouts of a plan share its
/// walls, windows and doors.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    #[serde(default)]
    pub furniture: Vec<Furniture>,
    #[serde(default)]
    pub finishes: Finishes,
}

impl Layout {
    pub fn named(name: impl Into<String>) -> Self {
        Layout {
            name: name.into(),
            furniture: Vec::new(),
            finishes: Finishes::default(),
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
    /// Where you were looking when the plan was saved. Missing in older files, which then
    /// open showing the whole plan.
    #[serde(default)]
    pub camera: Option<SavedCamera>,
    /// Every layout, the shown one included. `furniture` and `finishes` above are a copy of
    /// the shown one, so older versions of the app still open the file. Empty in older
    /// files, which have just the one layout.
    #[serde(default)]
    pub layouts: Vec<Layout>,
    #[serde(default)]
    pub active_layout: usize,
}

/// The camera, as stored in a plan file.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedCamera {
    pub mode: crate::app::CameraMode,
    /// Top view: the plan point in the middle of the view, and the zoom. Kept as a point
    /// rather than a pixel offset, so it still fits a window of another size.
    pub top_center: Pos2,
    pub zoom: f32,
    /// 3D view: the floor point it turns around (meters), angles (radians) and distance.
    pub orbit_target: Vec2,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub orbit_distance: f32,
    /// Walking: where you stand (plan units) and which way you look (radians).
    pub walk_position: Pos2,
    pub walk_angle: f32,
    pub walk_pitch: f32,
}
