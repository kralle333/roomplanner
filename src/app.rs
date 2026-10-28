use eframe::egui;
use egui::{Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::furniture::model::ModelLibrary;
use crate::geometry::rooms::extract_rooms;
use crate::history::{self, History, Snapshot};
use crate::models::{Finishes, Furniture, Opening, Tool, Wall};
use crate::{storage, ui};

pub const PIXELS_PER_METER: f32 = 50.0;
/// Wall height and thickness in meters.
pub const WALL_HEIGHT: f32 = 2.5;
pub const WALL_THICKNESS: f32 = 0.1;

const MY_LIBRARY_KEY: &str = "my_library";

/// An in-progress furniture drag. The unsnapped pose is tracked separately so the
/// piece can be pulled away from a wall once it has snapped.
pub struct FurnitureDrag {
    pub raw_position: Pos2,
    pub raw_rotation: f32,
    pub snapped_walls: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum CameraMode {
    /// Look at the whole plan from above and around.
    Orbit,
    /// First person, standing at the person marker on the plan.
    Walk,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TimeOfDay {
    Day,
    Evening,
    Night,
}

/// Which part of the person marker on the plan is being dragged.
#[derive(Clone, Copy, PartialEq)]
pub enum CameraHandle {
    Body,
    Direction,
}

pub struct RoomPlannerApp {
    pub current_tool: Tool,
    pub walls: Vec<Wall>,
    pub wall_start_point: Option<Pos2>,
    pub selected_walls: HashSet<usize>,
    pub selection_rect: Option<Rect>,
    pub dragging_endpoints: Vec<(usize, bool)>,
    pub rooms: Vec<Vec<Pos2>>,

    pub furniture: Vec<Furniture>,
    pub finishes: Finishes,
    pub selected_furniture: Option<usize>,
    pub furniture_drag: Option<FurnitureDrag>,
    /// Selected window or door, as (wall index, opening index).
    pub selected_opening: Option<(usize, usize)>,
    pub opening_drag: bool,
    /// What the window/door tool places, and where it would go right now.
    pub opening_template: Option<Opening>,
    pub opening_preview: Option<(usize, Opening)>,
    pub models: ModelLibrary,
    /// Last error from importing or loading a model, shown in the furniture panel.
    pub model_error: Option<String>,
    pub lock_model_proportions: bool,
    /// The user's own reusable furniture, kept across sessions.
    pub my_library: Vec<Furniture>,
    pub library_filter: String,

    pub history: History,
    /// The plan as last saved or loaded, to show unsaved changes.
    pub saved: Snapshot,

    pub pan_offset: Vec2,
    pub zoom_factor: f32,
    pub current_file: Option<PathBuf>,
    /// Canvas area on screen and in world coordinates, updated every frame.
    pub canvas_rect: Rect,
    pub visible_world_rect: Rect,
    pub pointer_world: Option<Pos2>,
    /// Where the canvas context menu was opened, for placing furniture there.
    pub context_menu_pos: Option<Pos2>,

    /// Zoom to fit once the canvas size is known (e.g. right after opening a plan).
    pub pending_fit: bool,
    /// Width / height of the 3D view, for framing the overview.
    pub view3d_aspect: f32,
    /// Mouse wheel zooms the plan instead of panning it.
    pub scroll_zooms: bool,
    pub show_grid: bool,
    pub show_3d: bool,
    pub show_library: bool,
    pub show_help: bool,

    // 3D camera
    pub camera_mode: CameraMode,
    pub time_of_day: TimeOfDay,
    pub cut_walls: bool,
    pub camera_pos: Pos2,
    pub camera_angle: f32,
    pub camera_pitch: f32,
    pub camera_drag: Option<CameraHandle>,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub orbit_distance: f32,
    /// Overview camera target offset from the plan's center, in meters (x, z).
    pub orbit_pan: Vec2,
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
            furniture: Vec::new(),
            finishes: Finishes::default(),
            selected_furniture: None,
            furniture_drag: None,
            selected_opening: None,
            opening_drag: false,
            opening_template: None,
            opening_preview: None,
            models: ModelLibrary::default(),
            model_error: None,
            lock_model_proportions: true,
            my_library: Vec::new(),
            library_filter: String::new(),
            history: History::default(),
            saved: Snapshot::default(),
            pan_offset: Vec2::new(300.0, 150.0),
            zoom_factor: 1.0,
            current_file: None,
            canvas_rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0)),
            visible_world_rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0)),
            pointer_world: None,
            context_menu_pos: None,
            pending_fit: false,
            view3d_aspect: 1.0,
            scroll_zooms: false,
            show_grid: true,
            show_3d: true,
            show_library: true,
            show_help: false,
            camera_mode: CameraMode::Orbit,
            time_of_day: TimeOfDay::Day,
            cut_walls: true,
            camera_pos: Pos2::new(200.0, 200.0),
            camera_angle: 0.0,
            camera_pitch: -5.0_f32.to_radians(),
            camera_drag: None,
            orbit_yaw: -2.2,
            orbit_pitch: 0.75,
            orbit_distance: 12.0,
            orbit_pan: Vec2::ZERO,
        }
    }
}

impl RoomPlannerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(render_state) = &cc.wgpu_render_state {
            ui::view3d::init(render_state);
        }
        ui::theme::apply(&cc.egui_ctx);

        let mut app = Self::default();
        if let Some(storage) = cc.storage {
            app.my_library = eframe::get_value(storage, MY_LIBRARY_KEY).unwrap_or_default();
        }
        app
    }

    /// Marks the current plan as the saved state and starts a fresh undo history.
    pub fn mark_saved(&mut self, reset_history: bool) {
        let snapshot = Snapshot::of(self);
        if reset_history {
            self.history.reset(snapshot.clone());
        }
        self.saved = snapshot;
    }

    pub fn is_dirty(&self) -> bool {
        Snapshot::of(self) != self.saved
    }

    pub fn new_plan(&mut self) {
        self.walls.clear();
        self.furniture.clear();
        self.rooms.clear();
        self.selected_walls.clear();
        self.selected_furniture = None;
        self.wall_start_point = None;
        self.current_file = None;
        self.current_tool = Tool::DrawWall;
        storage::forget_last_file();
        // Kept undoable: the old plan is one Ctrl+Z away.
    }

    pub fn set_walls(&mut self, walls: Vec<Wall>) {
        self.walls = walls;
        self.rooms = extract_rooms(&self.walls);
    }

    pub fn add_furniture(&mut self, item: Furniture) {
        let pos = self.visible_world_rect.center();
        self.add_furniture_at(item, pos);
    }

    pub fn add_furniture_at(&mut self, mut item: Furniture, position: Pos2) {
        item.position = position;
        self.furniture.push(item);
        self.selected_furniture = Some(self.furniture.len() - 1);
        self.selected_walls.clear();
        self.current_tool = Tool::Select;
        self.wall_start_point = None;
    }

    pub fn start_placing_opening(&mut self, template: Opening) {
        self.opening_template = Some(template);
        self.current_tool = Tool::PlaceOpening;
        self.wall_start_point = None;
        self.clear_selection();
    }

    pub fn add_opening(&mut self, wall: usize, opening: Opening) {
        if let Some(w) = self.walls.get_mut(wall) {
            w.openings.push(opening);
            let index = w.openings.len() - 1;
            self.clear_selection();
            self.selected_opening = Some((wall, index));
            self.current_tool = Tool::Select;
            self.opening_preview = None;
        }
    }

    pub fn selected_opening_mut(&mut self) -> Option<&mut Opening> {
        let (wi, oi) = self.selected_opening?;
        self.walls.get_mut(wi)?.openings.get_mut(oi)
    }

    pub fn clear_selection(&mut self) {
        self.selected_walls.clear();
        self.selected_furniture = None;
        self.selected_opening = None;
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut Furniture> {
        self.selected_furniture
            .and_then(|i| self.furniture.get_mut(i))
    }

    pub fn delete_selected_furniture(&mut self) {
        if let Some(idx) = self.selected_furniture.take() {
            if idx < self.furniture.len() {
                self.furniture.remove(idx);
            }
        }
        self.furniture_drag = None;
    }

    pub fn delete_selection(&mut self) {
        if let Some((wi, oi)) = self.selected_opening.take() {
            if let Some(wall) = self.walls.get_mut(wi) {
                if oi < wall.openings.len() {
                    wall.openings.remove(oi);
                }
            }
            self.opening_drag = false;
        }
        if !self.selected_walls.is_empty() {
            let mut indices: Vec<usize> = self.selected_walls.drain().collect();
            indices.sort_by(|a, b| b.cmp(a));
            let mut walls = std::mem::take(&mut self.walls);
            for idx in indices {
                walls.remove(idx);
            }
            self.set_walls(walls);
            self.selected_opening = None;
            self.dragging_endpoints.clear();
            self.selection_rect = None;
        }
        self.delete_selected_furniture();
    }

    pub fn duplicate_selected_furniture(&mut self) {
        if let Some(item) = self.selected_furniture.and_then(|i| self.furniture.get(i)) {
            let mut copy = item.clone();
            copy.position += Vec2::splat(0.2 * PIXELS_PER_METER);
            self.furniture.push(copy);
            self.selected_furniture = Some(self.furniture.len() - 1);
        }
    }

    pub fn save_to_library(&mut self) {
        if let Some(item) = self.selected_furniture.and_then(|i| self.furniture.get(i)) {
            let mut copy = item.clone();
            copy.options.clear();
            copy.active_option = 0;
            copy.rotation = 0.0;
            self.my_library
                .retain(|existing| existing.name != copy.name);
            self.my_library.push(copy);
        }
    }

    /// Fits the walls and furniture into the canvas.
    pub fn zoom_to_fit(&mut self) {
        let mut bounds = Rect::NOTHING;
        for wall in &self.walls {
            bounds.extend_with(wall.start);
            bounds.extend_with(wall.end);
        }
        for item in &self.furniture {
            for corner in item.footprint() {
                bounds.extend_with(corner);
            }
        }
        if !bounds.is_positive() {
            return;
        }
        let bounds = bounds.expand(PIXELS_PER_METER * 0.5);
        let view = self.canvas_rect.shrink(20.0);
        self.zoom_factor = (view.width() / bounds.width())
            .min(view.height() / bounds.height())
            .clamp(0.1, 20.0);
        self.pan_offset = view.center().to_vec2() - bounds.center().to_vec2() * self.zoom_factor;
    }

    /// Zooms the plan by `factor`, keeping the given screen point fixed.
    pub fn zoom_at(&mut self, screen_point: Pos2, factor: f32) {
        let old_zoom = self.zoom_factor;
        self.zoom_factor = (self.zoom_factor * factor).clamp(0.1, 20.0);
        let anchor = screen_point.to_vec2();
        self.pan_offset = anchor - (anchor - self.pan_offset) * (self.zoom_factor / old_zoom);
    }

    pub fn world_to_screen(&self, p: Pos2) -> Pos2 {
        Pos2::new(p.x * self.zoom_factor, p.y * self.zoom_factor) + self.pan_offset
    }

    pub fn screen_to_world(&self, p: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - self.pan_offset.x) / self.zoom_factor,
            (p.y - self.pan_offset.y) / self.zoom_factor,
        )
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, KeyboardShortcut, Modifiers};
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let pressed = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)))
        };

        if pressed(cmd_shift, Key::Z) || pressed(cmd, Key::Y) {
            history::redo(self);
        } else if pressed(cmd, Key::Z) {
            history::undo(self);
        }
        if pressed(cmd_shift, Key::S) {
            storage::save(self, true);
        } else if pressed(cmd, Key::S) {
            storage::save(self, false);
        }
        if pressed(cmd, Key::O) {
            storage::open(self);
        }
        if pressed(cmd, Key::D) {
            self.duplicate_selected_furniture();
        }

        let (select, wall, fit, help) = ctx.input(|i| {
            let plain = i.modifiers.is_none();
            (
                plain && i.key_pressed(Key::V),
                plain && i.key_pressed(Key::W),
                plain && i.key_pressed(Key::F),
                i.key_pressed(Key::F1) || (i.modifiers.shift && i.key_pressed(Key::Slash)),
            )
        });
        if select {
            self.current_tool = Tool::Select;
            self.wall_start_point = None;
        }
        if wall {
            self.current_tool = Tool::DrawWall;
        }
        if fit {
            self.zoom_to_fit();
        }
        if help {
            self.show_help = !self.show_help;
        }
    }
}

impl eframe::App for RoomPlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);

        // Panels first; the canvas takes the remaining space.
        ui::top_bar::show(self, ui);
        ui::status_bar::show(self, ui);
        if self.show_library {
            ui::furniture_panel::show(self, ui);
        }
        if self.show_3d {
            ui::view3d::show_panel(self, ui);
        }
        ui::canvas::show(self, ui);
        ui::help::show(self, &ctx);
        ui::furniture_panel::show_drag_ghost(&ctx);

        history::track(self, &ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, MY_LIBRARY_KEY, &self.my_library);
    }
}
