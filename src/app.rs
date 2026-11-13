use eframe::egui;
use egui::{Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::furniture::model::ModelLibrary;
use crate::geometry::rooms::extract_rooms;
use crate::geometry::walls::WallDrag;
use crate::history::{self, History, Snapshot};
use crate::models::{Finishes, Furniture, Opening, Tool, Wall};
use crate::{storage, ui};

pub const PIXELS_PER_METER: f32 = 50.0;
/// Wall height and thickness in meters.
pub const WALL_HEIGHT: f32 = 2.5;
pub const WALL_THICKNESS: f32 = 0.1;

const MY_LIBRARY_KEY: &str = "my_library";
const DRAFT_KEY: &str = "draft";
const PREFS_KEY: &str = "prefs";

/// The plan being worked on, kept between sessions so nothing is lost on quit or reload.
#[derive(serde::Serialize, serde::Deserialize)]
struct Draft {
    plan: crate::models::AppSaveState,
    file: Option<PathBuf>,
    unsaved: bool,
}

/// View settings remembered between sessions.
#[derive(serde::Serialize, serde::Deserialize)]
struct Prefs {
    camera: CameraMode,
    time_of_day: TimeOfDay,
    show_grid: bool,
    scroll_zooms: bool,
    cut_walls: bool,
    show_library: bool,
    #[serde(default = "yes")]
    shadows: bool,
}

fn yes() -> bool {
    true
}

/// An in-progress furniture drag. The unsnapped pose is tracked separately so the
/// piece can be pulled away from a wall once it has snapped.
pub struct FurnitureDrag {
    pub raw_position: Pos2,
    pub raw_rotation: f32,
    pub snapped_walls: Vec<usize>,
    /// Lines of other furniture the piece is snapped to, in plan units.
    pub guides: Vec<(Pos2, Pos2)>,
    /// Where the dragged piece started; the rest of the selection keeps its offset to it.
    pub start: Pos2,
    pub others: Vec<(usize, Pos2)>,
    /// Height (meters) of the level plane the piece slides on: where it was grabbed, so the
    /// part under the pointer stays under it.
    pub grab_height: f32,
    /// Set while Ctrl is held in 3D: the piece moves up, down and sideways on an upright
    /// plane, like along a wall, instead of over the floor.
    pub upright: Option<UprightDrag>,
}

/// Moving furniture on an upright plane (see [`FurnitureDrag::upright`]).
pub struct UprightDrag {
    /// The plane's normal on the plan, and a point on it (plan units).
    pub normal: Vec2,
    pub point: Pos2,
    /// Where the pointer ray first hit the plane, in 3D meters.
    pub anchor: glam::Vec3,
    /// Each moving piece's position and height above the floor when it started.
    pub originals: Vec<(usize, Pos2, f32)>,
}

/// An in-progress rotation of the selected furniture with the rotation handle.
pub struct FurnitureRotate {
    /// The point everything turns around: the piece itself, or the middle of a group.
    pub center: Pos2,
    /// Angle from the center to where the pointer was pressed.
    pub start_angle: f32,
    /// Each selected piece's position and rotation when the drag started.
    pub originals: Vec<(usize, Pos2, f32)>,
}

/// An in-progress resize of the selected furniture by one of its corner or side handles.
pub struct FurnitureResize {
    /// Which handle, as the side of the piece on its local x and z axes (-1, 0 or 1).
    pub handle: (i8, i8),
    /// Position, rotation, width and depth when the drag started.
    pub position: Pos2,
    pub rotation: f32,
    pub size: Vec2,
    /// Height in meters of the level the handle was grabbed at: the top, the bottom, or in
    /// between on an upright edge.
    pub level: f32,
    /// Edges of furniture and walls the dragged edges are snapped to, in plan units.
    pub guides: Vec<(Pos2, Pos2)>,
    /// From the handle to where it was grabbed, in plan units.
    pub grab_offset: Vec2,
}

/// Marks furniture copied to the system clipboard, so it can be pasted into another window.
const CLIPBOARD_PREFIX: &str = "roomplanner-furniture:";

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CameraMode {
    /// Straight down, without perspective: like a floor plan, the easiest for drawing walls.
    Top,
    /// Look at the whole plan from above and around.
    Orbit,
    /// First person, standing in a room at eye height.
    Walk,
}

/// How the viewport maps the 3D scene to the screen this frame. Plan positions sit on the
/// floor (y = 0); 3D space is in meters with the plan's x/y on the x/z axes.
#[derive(Clone, Copy)]
pub struct Projection {
    pub rect: Rect,
    pub view_proj: glam::Mat4,
    pub inverse: glam::Mat4,
    pub eye: glam::Vec3,
}

impl Default for Projection {
    fn default() -> Self {
        Self {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0)),
            view_proj: glam::Mat4::IDENTITY,
            inverse: glam::Mat4::IDENTITY,
            eye: glam::Vec3::Y * 10.0,
        }
    }
}

impl Projection {
    /// Screen position of a 3D point, or None if it is behind the camera.
    pub fn project(&self, p: glam::Vec3) -> Option<Pos2> {
        let clip = self.view_proj * p.extend(1.0);
        if clip.w <= 1e-4 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(Pos2::new(
            self.rect.center().x + ndc.x * self.rect.width() / 2.0,
            self.rect.center().y - ndc.y * self.rect.height() / 2.0,
        ))
    }

    /// The ray through a screen position, as origin and unit direction.
    pub fn ray(&self, screen: Pos2) -> (glam::Vec3, glam::Vec3) {
        let nx = (screen.x - self.rect.center().x) / (self.rect.width() / 2.0).max(1.0);
        let ny = -(screen.y - self.rect.center().y) / (self.rect.height() / 2.0).max(1.0);
        let near = self.inverse * glam::Vec4::new(nx, ny, 0.0, 1.0);
        let far = self.inverse * glam::Vec4::new(nx, ny, 1.0, 1.0);
        let (near, far) = (near.truncate() / near.w, far.truncate() / far.w);
        (near, (far - near).normalize_or(glam::Vec3::NEG_Y))
    }
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TimeOfDay {
    Day,
    Evening,
    Night,
}

pub struct RoomPlannerApp {
    pub current_tool: Tool,
    pub walls: Vec<Wall>,
    pub wall_start_point: Option<Pos2>,
    pub selected_walls: HashSet<usize>,
    pub selection_rect: Option<Rect>,
    pub dragging_endpoints: Vec<(usize, bool)>,
    pub wall_drag: Option<WallDrag>,
    /// Moving a wall stretches the walls connected to it, resizing the rooms.
    pub stretch_walls: bool,
    pub rooms: Vec<Vec<Pos2>>,

    pub furniture: Vec<Furniture>,
    pub finishes: Finishes,
    /// All layouts of the plan; the shown one's furniture and finishes are the fields
    /// above (see `crate::layouts`).
    pub layouts: Vec<crate::models::Layout>,
    pub active_layout: usize,
    /// The layout the shown one is compared with: the one shown before it.
    pub compare_layout: Option<usize>,
    /// While a layout is peeked at, the one to go back to.
    pub peek_return: Option<usize>,
    /// Outline what differs from the compared layout.
    pub show_differences: bool,
    /// Lamps cast shadows in the 3D views.
    pub shadows: bool,
    /// The layout tab being renamed, and whether its text field still needs focus.
    pub renaming_layout: Option<usize>,
    pub rename_focus_requested: bool,
    pub selected_furniture: Option<usize>,
    /// More furniture selected along with `selected_furniture`, which stays the piece the
    /// properties panel shows. Edits made there are copied to these.
    pub extra_furniture: Vec<usize>,
    pub furniture_drag: Option<FurnitureDrag>,
    pub furniture_rotate: Option<FurnitureRotate>,
    pub furniture_resize: Option<FurnitureResize>,
    pub furniture_clipboard: Vec<Furniture>,
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
    pub left_tab: ui::furniture_panel::LeftTab,
    pub library_section: ui::furniture_panel::LibrarySection,
    /// A wall length being typed while drawing, e.g. "350" (cm) or "3.5" (m).
    pub length_input: String,
    /// Resizing a wall in the panel keeps its end in place instead of its start.
    pub wall_anchor_end: bool,

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
    /// The last scroll came from a trackpad (smooth) rather than a mouse wheel.
    pub trackpad_scroll: bool,
    /// The plan from the last session was brought back on start.
    pub restored_draft: bool,
    /// A camera from a loaded plan, applied together with `pending_fit` instead of fitting.
    pub pending_camera: Option<crate::models::SavedCamera>,
    pub show_grid: bool,
    /// The camera this frame, set before input is handled.
    pub projection: Projection,
    /// Pixels per plan unit around the pointer, for pick and snap distances.
    pub view_scale: f32,
    pub show_library: bool,
    pub show_help: bool,

    // 3D camera
    pub camera_mode: CameraMode,
    pub time_of_day: TimeOfDay,
    pub cut_walls: bool,
    pub camera_pos: Pos2,
    pub camera_angle: f32,
    pub camera_pitch: f32,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub orbit_distance: f32,
    /// The point on the floor the overview camera turns around, in meters (x, z). Fixed
    /// in space, so moving furniture or walls doesn't move the camera.
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
            wall_drag: None,
            stretch_walls: true,
            rooms: Vec::new(),
            furniture: Vec::new(),
            finishes: Finishes::default(),
            selected_furniture: None,
            extra_furniture: Vec::new(),
            furniture_drag: None,
            furniture_rotate: None,
            furniture_resize: None,
            furniture_clipboard: Vec::new(),
            selected_opening: None,
            opening_drag: false,
            opening_template: None,
            opening_preview: None,
            models: ModelLibrary::default(),
            model_error: None,
            lock_model_proportions: true,
            my_library: Vec::new(),
            library_filter: String::new(),
            left_tab: ui::furniture_panel::LeftTab::Add,
            library_section: ui::furniture_panel::LibrarySection::Openings,
            length_input: String::new(),
            wall_anchor_end: false,
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
            trackpad_scroll: false,
            restored_draft: false,
            pending_camera: None,
            layouts: vec![crate::models::Layout::named(crate::layouts::first_name())],
            active_layout: 0,
            compare_layout: None,
            peek_return: None,
            show_differences: false,
            shadows: true,
            renaming_layout: None,
            rename_focus_requested: false,
            show_grid: true,
            projection: Projection::default(),
            view_scale: 1.0,
            show_library: true,
            show_help: false,
            camera_mode: CameraMode::Top,
            time_of_day: TimeOfDay::Day,
            cut_walls: true,
            camera_pos: Pos2::new(200.0, 200.0),
            camera_angle: 0.0,
            camera_pitch: -5.0_f32.to_radians(),
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
            if let Some(prefs) = eframe::get_value::<Prefs>(storage, PREFS_KEY) {
                // Walking starts from where you look, so a new session opens in 3D instead.
                app.camera_mode = match prefs.camera {
                    CameraMode::Walk => CameraMode::Orbit,
                    mode => mode,
                };
                app.time_of_day = prefs.time_of_day;
                app.show_grid = prefs.show_grid;
                app.scroll_zooms = prefs.scroll_zooms;
                app.cut_walls = prefs.cut_walls;
                app.show_library = prefs.show_library;
                app.shadows = prefs.shadows;
            }
            if let Some(draft) = eframe::get_value::<Draft>(storage, DRAFT_KEY) {
                app.restore_draft(draft);
            }
        }
        app
    }

    fn restore_draft(&mut self, draft: Draft) {
        // Nothing unsaved: the file is the plan, and may have been changed since (by
        // another copy of the app, or by hand), so open it as it is now.
        let on_disk = draft.file.as_ref().filter(|_| !draft.unsaved);
        if let Some(state) = on_disk.and_then(storage::read_state) {
            storage::apply_state(self, state);
            self.current_file = draft.file.clone();
            self.mark_saved(true);
            self.restored_draft = true;
            return;
        }
        storage::apply_state(self, draft.plan);
        self.current_file = draft.file.filter(|f| f.exists());
        self.mark_saved(true);
        if draft.unsaved {
            // Compare with what is on disk, so the unsaved mark comes back too.
            self.saved = self
                .current_file
                .as_ref()
                .and_then(storage::read_state)
                .map(|state| {
                    // How the app holds that file, to compare with.
                    let mut on_disk = RoomPlannerApp::default();
                    storage::apply_state(&mut on_disk, state);
                    Snapshot::of(&on_disk)
                })
                .unwrap_or_default();
        }
        self.restored_draft = true;
    }

    /// Marks the current plan as the saved state and starts a fresh undo history.
    pub fn mark_saved(&mut self, reset_history: bool) {
        let snapshot = Snapshot::of(self);
        if reset_history {
            self.history.reset(snapshot.clone());
        }
        self.saved = snapshot;
    }

    /// Whether the plan differs from the file. Which layout is shown doesn't count.
    pub fn is_dirty(&self) -> bool {
        !Snapshot::of(self).same_content(&self.saved)
    }

    pub fn new_plan(&mut self) {
        self.walls.clear();
        crate::layouts::set_all(self, Vec::new(), 0);
        self.rooms.clear();
        self.selected_walls.clear();
        self.select_furniture(None);
        self.wall_start_point = None;
        self.current_file = None;
        self.current_tool = Tool::DrawWall;
        storage::forget_last_file();
        // Kept undoable: the old plan is one Ctrl+Z away.
    }

    /// Switches camera, carrying over where you were looking.
    pub fn set_camera_mode(&mut self, mode: CameraMode) {
        if mode == self.camera_mode {
            return;
        }
        let center = self.screen_to_world(self.canvas_rect.center());
        let old = self.camera_mode;
        self.camera_mode = mode;
        match mode {
            CameraMode::Top => {
                // Keep the same spot in the middle of the view.
                let view = self.canvas_rect;
                if old == CameraMode::Orbit {
                    let meters = self.orbit_distance.max(1.0) / 1.2;
                    self.zoom_factor =
                        (view.height() / (meters * PIXELS_PER_METER)).clamp(0.1, 20.0);
                }
                self.pan_offset = view.center().to_vec2() - center.to_vec2() * self.zoom_factor;
            }
            CameraMode::Orbit => {
                self.orbit_pan = Vec2::new(center.x, center.y) / PIXELS_PER_METER;
                if old == CameraMode::Top {
                    let meters = self.canvas_rect.height() / (self.zoom_factor * PIXELS_PER_METER);
                    self.orbit_distance = (meters * 1.2).clamp(1.5, 80.0);
                }
            }
            CameraMode::Walk => {
                if old == CameraMode::Orbit {
                    // Look the same way the overview camera was looking.
                    self.camera_angle = self.orbit_yaw + std::f32::consts::PI;
                }
                (self.camera_pos, self.camera_angle) =
                    ui::view3d::walk_start(self, center, self.camera_angle);
                self.camera_pitch = -5.0_f32.to_radians();
            }
        }
    }

    /// Gives a wall an exact length (meters) and direction, keeping one end in place
    /// (see `wall_anchor_end`). Walls joined at the moving corner move with it.
    pub fn resize_wall(&mut self, idx: usize, length: f32, dir: Vec2) {
        let Some(wall) = self.walls.get(idx) else {
            return;
        };
        let span = dir.normalized() * length * PIXELS_PER_METER;
        let (old, new) = if self.wall_anchor_end {
            (wall.start, wall.end - span)
        } else {
            (wall.end, wall.start + span)
        };
        for (i, w) in self.walls.iter_mut().enumerate() {
            if i == idx {
                if self.wall_anchor_end {
                    w.start = new;
                } else {
                    w.end = new;
                }
                continue;
            }
            if w.start.distance(old) < 2.0 {
                w.start = new;
            }
            if w.end.distance(old) < 2.0 {
                w.end = new;
            }
        }
        self.rooms = extract_rooms(&self.walls);
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
        // Library ceiling lights are made for the usual ceiling; follow this room's.
        if item.on_ceiling() {
            let ceiling = crate::geometry::walls::ceiling_height_at(&self.walls, position);
            item.elevation = (item.elevation + ceiling - WALL_HEIGHT).max(0.0);
        }
        self.furniture.push(item);
        self.select_furniture(Some(self.furniture.len() - 1));
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
        self.select_furniture(None);
        self.selected_opening = None;
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut Furniture> {
        self.selected_furniture
            .and_then(|i| self.furniture.get_mut(i))
    }

    /// Selects just this piece, or no furniture.
    pub fn select_furniture(&mut self, idx: Option<usize>) {
        self.selected_furniture = idx;
        self.extra_furniture.clear();
    }

    /// All selected furniture, the one shown in the properties panel first.
    pub fn selected_furniture_indices(&self) -> Vec<usize> {
        self.selected_furniture
            .into_iter()
            .chain(self.extra_furniture.iter().copied())
            .filter(|&i| i < self.furniture.len())
            .collect()
    }

    pub fn is_furniture_selected(&self, idx: usize) -> bool {
        self.selected_furniture == Some(idx) || self.extra_furniture.contains(&idx)
    }

    /// Adds a piece to the selection, or takes it out if it is already in.
    pub fn toggle_furniture(&mut self, idx: usize) {
        if self.selected_furniture == Some(idx) {
            self.selected_furniture = if self.extra_furniture.is_empty() {
                None
            } else {
                Some(self.extra_furniture.remove(0))
            };
        } else if let Some(pos) = self.extra_furniture.iter().position(|&i| i == idx) {
            self.extra_furniture.remove(pos);
        } else if self.selected_furniture.is_none() {
            self.selected_furniture = Some(idx);
        } else {
            self.extra_furniture.push(idx);
        }
    }

    /// Adds every piece of the same kind and name as the selected one to the selection.
    pub fn select_similar_furniture(&mut self) {
        let Some(item) = self.selected_furniture.and_then(|i| self.furniture.get(i)) else {
            return;
        };
        let primary = self.selected_furniture;
        self.extra_furniture = (0..self.furniture.len())
            .filter(|&i| Some(i) != primary && same_type(&self.furniture[i], item))
            .collect();
    }

    pub fn delete_selected_furniture(&mut self) {
        let mut indices = self.selected_furniture_indices();
        indices.sort_by(|a, b| b.cmp(a));
        indices.dedup();
        for idx in indices {
            self.furniture.remove(idx);
        }
        self.select_furniture(None);
        self.furniture_drag = None;
        self.furniture_rotate = None;
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
            self.wall_drag = None;
            self.selection_rect = None;
        }
        self.delete_selected_furniture();
    }

    pub fn duplicate_selected_furniture(&mut self) {
        let copies: Vec<Furniture> = self
            .selected_furniture_indices()
            .into_iter()
            .map(|i| self.furniture[i].clone())
            .collect();
        self.insert_furniture(copies, None);
    }

    /// Copies the selected furniture, also to the system clipboard.
    pub fn copy_selected_furniture(&mut self, ctx: &egui::Context) {
        let items: Vec<Furniture> = self
            .selected_furniture_indices()
            .into_iter()
            .map(|i| self.furniture[i].clone())
            .collect();
        if items.is_empty() {
            return;
        }
        if let Ok(json) = serde_json::to_string(&items) {
            ctx.copy_text(format!("{CLIPBOARD_PREFIX}{json}"));
        }
        self.furniture_clipboard = items;
    }

    pub fn cut_selected_furniture(&mut self, ctx: &egui::Context) {
        self.copy_selected_furniture(ctx);
        self.delete_selected_furniture();
    }

    /// Pastes copied furniture, centered on `at` if given. `text` is the system clipboard,
    /// used when it holds furniture copied from another window.
    pub fn paste_furniture(&mut self, at: Option<Pos2>, text: Option<&str>) {
        if let Some(json) = text.and_then(|t| t.strip_prefix(CLIPBOARD_PREFIX)) {
            if let Ok(items) = serde_json::from_str::<Vec<Furniture>>(json) {
                self.furniture_clipboard = items;
            }
        }
        if self.furniture_clipboard.is_empty() {
            return;
        }
        let items = self.furniture_clipboard.clone();
        if at.is_none() {
            // Pasting again without pointing somewhere cascades the copies.
            for item in &mut self.furniture_clipboard {
                item.position += Vec2::splat(0.2 * PIXELS_PER_METER);
            }
        }
        self.insert_furniture(items, at);
    }

    /// Adds furniture and selects it. Centered on `at`, or nudged off the originals.
    fn insert_furniture(&mut self, mut items: Vec<Furniture>, at: Option<Pos2>) {
        if items.is_empty() {
            return;
        }
        let center = items
            .iter()
            .fold(Vec2::ZERO, |sum, i| sum + i.position.to_vec2())
            / items.len() as f32;
        let shift = match at {
            Some(p) => p - center.to_pos2(),
            None => Vec2::splat(0.2 * PIXELS_PER_METER),
        };
        let first = self.furniture.len();
        for item in &mut items {
            item.position += shift;
        }
        self.furniture.extend(items);
        self.clear_selection();
        self.selected_furniture = Some(first);
        self.extra_furniture = (first + 1..self.furniture.len()).collect();
        self.current_tool = Tool::Select;
        self.wall_start_point = None;
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

    /// Frames the whole plan, in whichever camera is active.
    /// Frames what is selected, or the whole plan when nothing is.
    pub fn fit_view(&mut self) {
        match self.selection_bounds() {
            Some(bounds) => self.frame(bounds),
            None => self.fit_whole_plan(),
        }
    }

    pub fn fit_whole_plan(&mut self) {
        match self.camera_mode {
            CameraMode::Top => self.zoom_to_fit(),
            _ => ui::view3d::fit_orbit(self),
        }
    }

    /// Plan area covered by the selected walls, furniture and window or door.
    pub fn selection_bounds(&self) -> Option<Rect> {
        let mut bounds = Rect::NOTHING;
        for &i in &self.selected_walls {
            if let Some(w) = self.walls.get(i) {
                bounds.extend_with(w.start);
                bounds.extend_with(w.end);
            }
        }
        let items = self.selected_furniture.iter().chain(&self.extra_furniture);
        for item in items.filter_map(|&i| self.furniture.get(i)) {
            for corner in item.footprint() {
                bounds.extend_with(corner);
            }
        }
        if let Some((wi, oi)) = self.selected_opening {
            let wall = self.walls.get(wi);
            if let Some((wall, opening)) = wall.and_then(|w| Some((w, w.openings.get(oi)?))) {
                let dir = (wall.end - wall.start).normalized();
                let mid = wall.start + dir * opening.offset * PIXELS_PER_METER;
                bounds.extend_with(mid - dir * opening.width * PIXELS_PER_METER / 2.0);
                bounds.extend_with(mid + dir * opening.width * PIXELS_PER_METER / 2.0);
            }
        }
        (bounds.min.x <= bounds.max.x).then_some(bounds)
    }

    /// The direction the selection faces, on the plan: out of the front of a piece of
    /// furniture, or square out of a wall (or its window or door) on the side the 3D camera
    /// is already on.
    fn selection_front(&self) -> Option<Vec2> {
        if let Some(item) = self.selected_furniture.and_then(|i| self.furniture.get(i)) {
            // The rotation handle sits at the back; the front is the other way.
            return Some(egui::emath::Rot2::from_angle(item.rotation) * Vec2::new(0.0, 1.0));
        }
        let wall = match self.selected_opening {
            Some((wi, _)) => self.walls.get(wi),
            None if self.selected_walls.len() == 1 => self
                .selected_walls
                .iter()
                .next()
                .and_then(|&i| self.walls.get(i)),
            None => None,
        }?;
        let normal = (wall.end - wall.start).normalized().rot90();
        let camera = Vec2::new(self.orbit_yaw.cos(), self.orbit_yaw.sin());
        Some(if normal.dot(camera) >= 0.0 {
            normal
        } else {
            -normal
        })
    }

    /// Points the camera at a plan area so it fills the view.
    pub fn frame(&mut self, bounds: Rect) {
        // Small things get some room around them rather than filling the screen.
        let bounds = bounds.expand(PIXELS_PER_METER * 0.8);
        match self.camera_mode {
            CameraMode::Top => self.zoom_to_rect(bounds),
            CameraMode::Orbit => {
                self.orbit_pan = bounds.center().to_vec2() / PIXELS_PER_METER;
                let extent = bounds.size().max_elem() / PIXELS_PER_METER;
                let narrow = (1.0 / self.view3d_aspect.max(0.1)).max(1.0);
                self.orbit_distance = (extent * 1.3 * narrow + 1.0).clamp(1.5, 80.0);
                // Come around to the front, low enough to see it.
                if let Some(front) = self.selection_front() {
                    self.orbit_yaw = front.y.atan2(front.x);
                    self.orbit_pitch = self.orbit_pitch.min(0.55);
                }
            }
            CameraMode::Walk => {}
        }
    }

    /// Fits the walls and furniture into the top view.
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
        self.zoom_to_rect(bounds.expand(PIXELS_PER_METER * 0.5));
    }

    fn zoom_to_rect(&mut self, bounds: Rect) {
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

    /// Screen position of a point on the floor. Points behind the camera are pushed far
    /// off screen.
    pub fn world_to_screen(&self, p: Pos2) -> Pos2 {
        self.world3_to_screen(p, 0.0)
    }

    /// Screen position of a plan point at a height in meters.
    pub fn world3_to_screen(&self, p: Pos2, height: f32) -> Pos2 {
        let v = glam::Vec3::new(p.x / PIXELS_PER_METER, height, p.y / PIXELS_PER_METER);
        self.projection
            .project(v)
            .unwrap_or(Pos2::new(-1.0e5, -1.0e5))
    }

    /// Like [`Self::screen_to_plane`], but None when the ray misses the plane or hits it
    /// very far away (near the horizon), where a small pointer move would be a huge jump.
    pub fn screen_to_plane_near(&self, p: Pos2, height: f32) -> Option<Pos2> {
        let (origin, dir) = self.projection.ray(p);
        if dir.y > -1e-3 || origin.y <= height {
            return None;
        }
        let t = (height - origin.y) / dir.y;
        let hit = origin + dir * t;
        let far = (hit - self.projection.eye).length() > 40.0;
        (!far).then(|| Pos2::new(hit.x, hit.z) * PIXELS_PER_METER)
    }

    /// The point under a screen position on the level plane at `height` meters.
    pub fn screen_to_plane(&self, p: Pos2, height: f32) -> Pos2 {
        let (origin, dir) = self.projection.ray(p);
        let t = if dir.y < -1e-4 {
            ((height - origin.y) / dir.y).clamp(0.0, 150.0)
        } else {
            150.0
        };
        let hit = origin + dir * t;
        Pos2::new(hit.x, hit.z) * PIXELS_PER_METER
    }

    /// The point on the floor under a screen position. Above the horizon, a far point in
    /// that direction.
    pub fn screen_to_world(&self, p: Pos2) -> Pos2 {
        let (origin, dir) = self.projection.ray(p);
        let t = if dir.y < -1e-4 {
            (-origin.y / dir.y).min(150.0)
        } else {
            150.0
        };
        let hit = origin + dir * t;
        Pos2::new(hit.x * PIXELS_PER_METER, hit.z * PIXELS_PER_METER)
    }

    /// The plan position of the wall, furniture or floor under a screen position.
    pub fn pick(&self, p: Pos2) -> Pos2 {
        ui::view3d::pick(self, p)
    }

    /// The piece of furniture directly under a screen position in the 3D views, in front of
    /// any wall. None in the top view, where the plan position says it all.
    pub fn pick_furniture(&self, p: Pos2) -> Option<usize> {
        ui::view3d::pick_with_furniture(self, p).1
    }

    /// How far the pointer moved during a drag on the level plane at `height` meters, in
    /// plan units. Zero where the pointer is near or above the horizon.
    pub fn plane_drag_delta(&self, response: &egui::Response, height: f32) -> Vec2 {
        if self.camera_mode == CameraMode::Top {
            // Straight down without perspective: every level moves alike, no horizon.
            return self.world_drag_delta(response);
        }
        let Some(now) = response.interact_pointer_pos() else {
            return Vec2::ZERO;
        };
        let before = now - response.drag_delta();
        match (
            self.screen_to_plane_near(now, height),
            self.screen_to_plane_near(before, height),
        ) {
            (Some(a), Some(b)) => a - b,
            _ => Vec2::ZERO,
        }
    }

    /// How far the floor point under the pointer moved during a drag, in plan units.
    pub fn world_drag_delta(&self, response: &egui::Response) -> Vec2 {
        match response.interact_pointer_pos() {
            Some(p) => self.screen_to_world(p) - self.screen_to_world(p - response.drag_delta()),
            None => Vec2::ZERO,
        }
    }

    /// Pixels per plan unit: the zoom in the top view, or the scale around the pointer.
    pub fn scale(&self) -> f32 {
        self.view_scale
    }

    /// Arrow keys move the selected furniture 1 cm, or 10 cm with Shift. The arrows follow
    /// the screen: up moves away from you in 3D too, along the nearest wall direction.
    fn nudge_furniture(&mut self, ctx: &egui::Context) {
        if self.selected_furniture.is_none() {
            return;
        }
        use egui::Key;
        // Page Up / Down raise and lower it.
        let (up, down, big) = ctx.input(|i| {
            (
                i.key_pressed(Key::PageUp),
                i.key_pressed(Key::PageDown),
                i.modifiers.shift,
            )
        });
        if up || down {
            let step = if big { 0.1 } else { 0.01 } * if up { 1.0 } else { -1.0 };
            for i in self.selected_furniture_indices() {
                let item = &mut self.furniture[i];
                item.elevation = ((item.elevation + step) * 100.0).round().max(0.0) / 100.0;
            }
        }
        let (screen, big) = ctx.input(|i| {
            let mut d = Vec2::ZERO;
            for (key, dir) in [
                (Key::ArrowUp, Vec2::new(0.0, -1.0)),
                (Key::ArrowDown, Vec2::new(0.0, 1.0)),
                (Key::ArrowLeft, Vec2::new(-1.0, 0.0)),
                (Key::ArrowRight, Vec2::new(1.0, 0.0)),
            ] {
                if i.key_pressed(key) {
                    d += dir;
                }
            }
            (d, i.modifiers.shift)
        });
        if screen == Vec2::ZERO {
            return;
        }
        let c = self.canvas_rect.center();
        let on_floor = |d: Vec2| {
            let v = self.screen_to_world(c + d * 40.0) - self.screen_to_world(c);
            if v.x.abs() > v.y.abs() {
                Vec2::new(v.x.signum(), 0.0)
            } else {
                Vec2::new(0.0, v.y.signum())
            }
        };
        let dir = on_floor(Vec2::new(screen.x, 0.0)) * screen.x.abs()
            + on_floor(Vec2::new(0.0, screen.y)) * screen.y.abs();
        let step = if big { 0.1 } else { 0.01 } * PIXELS_PER_METER;
        let items: Vec<usize> = self
            .selected_furniture
            .iter()
            .chain(&self.extra_furniture)
            .copied()
            .collect();
        for i in items {
            if let Some(item) = self.furniture.get_mut(i) {
                item.position += dir * step;
            }
        }
    }

    /// 1–9 show that layout; holding C shows the compared layout until it is let go.
    fn layout_keys(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let (peek, number) = ctx.input(|i| {
            let plain = !i.modifiers.command && !i.modifiers.alt;
            let keys = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            (
                plain && i.key_down(Key::C),
                keys.iter().position(|&k| plain && i.key_pressed(k)),
            )
        });
        if peek {
            crate::layouts::start_peek(self);
        } else {
            crate::layouts::end_peek(self);
        }
        // While drawing walls, numbers are typed lengths.
        if let Some(n) = number.filter(|_| self.current_tool != Tool::DrawWall && !peek) {
            crate::layouts::switch(self, n);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            crate::layouts::end_peek(self);
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
        if self.camera_mode != CameraMode::Walk {
            self.nudge_furniture(ctx);
        }
        self.layout_keys(ctx);

        // egui turns Ctrl+C/X/V into clipboard events; the key presses are a fallback.
        let (copy, cut, paste, paste_text) = ctx.input(|i| {
            let key = |k| i.modifiers.command && i.key_pressed(k);
            let text = i.events.iter().find_map(|e| match e {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            });
            (
                key(Key::C) || i.events.contains(&egui::Event::Copy),
                key(Key::X) || i.events.contains(&egui::Event::Cut),
                key(Key::V) || text.is_some(),
                text,
            )
        });
        if copy {
            self.copy_selected_furniture(ctx);
        }
        if cut {
            self.cut_selected_furniture(ctx);
        }
        if paste {
            self.paste_furniture(self.pointer_world, paste_text.as_deref());
        }

        let (select, wall, fit, help, top) = ctx.input(|i| {
            let plain = i.modifiers.is_none();
            (
                plain && i.key_pressed(Key::V),
                plain && i.key_pressed(Key::W),
                plain && i.key_pressed(Key::F),
                i.key_pressed(Key::F1) || (i.modifiers.shift && i.key_pressed(Key::Slash)),
                plain && i.key_pressed(Key::T),
            )
        });
        if top {
            let mode = if self.camera_mode == CameraMode::Top {
                CameraMode::Orbit
            } else {
                CameraMode::Top
            };
            self.set_camera_mode(mode);
        }
        if select {
            self.current_tool = Tool::Select;
            self.wall_start_point = None;
        }
        if wall && self.camera_mode != CameraMode::Walk {
            self.current_tool = Tool::DrawWall;
        }
        if fit {
            self.fit_view();
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
        ui::canvas::show(self, ui);
        ui::help::show(self, &ctx);
        ui::furniture_panel::show_drag_ghost(&ctx);

        history::track(self, &ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, MY_LIBRARY_KEY, &self.my_library);
        let draft = Draft {
            plan: storage::current_state(self),
            file: self.current_file.clone(),
            unsaved: self.is_dirty(),
        };
        eframe::set_value(storage, DRAFT_KEY, &draft);
        let prefs = Prefs {
            camera: self.camera_mode,
            time_of_day: self.time_of_day,
            show_grid: self.show_grid,
            scroll_zooms: self.scroll_zooms,
            cut_walls: self.cut_walls,
            show_library: self.show_library,
            shadows: self.shadows,
        };
        eframe::set_value(storage, PREFS_KEY, &prefs);
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }
}

/// Pieces count as the same type when they share a name and a kind, e.g. all "Desk chair"s.
pub fn same_type(a: &Furniture, b: &Furniture) -> bool {
    a.name == b.name && std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::PRESETS;

    fn preset(label: &str) -> Furniture {
        (PRESETS.iter().find(|p| p.label == label).unwrap().build)()
    }

    fn app_with(items: &[(&str, (f32, f32))]) -> RoomPlannerApp {
        let mut app = RoomPlannerApp::default();
        for &(label, (x, y)) in items {
            app.add_furniture_at(preset(label), Pos2::new(x, y));
        }
        app.select_furniture(None);
        app
    }

    #[test]
    fn select_similar_picks_same_name_and_kind() {
        let mut app = app_with(&[
            ("Desk chair", (0.0, 0.0)),
            ("Bookcase", (100.0, 0.0)),
            ("Desk chair", (200.0, 0.0)),
        ]);
        app.select_furniture(Some(0));
        app.select_similar_furniture();
        assert_eq!(app.selected_furniture_indices(), vec![0, 2]);
    }

    #[test]
    fn toggling_the_primary_promotes_the_next_piece() {
        let mut app = app_with(&[("Bookcase", (0.0, 0.0)), ("Bookcase", (100.0, 0.0))]);
        app.toggle_furniture(0);
        app.toggle_furniture(1);
        app.toggle_furniture(0);
        assert_eq!(app.selected_furniture, Some(1));
        assert!(app.extra_furniture.is_empty());
    }

    #[test]
    fn deleting_removes_the_whole_selection() {
        let mut app = app_with(&[
            ("Bookcase", (0.0, 0.0)),
            ("Plant", (100.0, 0.0)),
            ("Bookcase", (200.0, 0.0)),
        ]);
        app.toggle_furniture(2);
        app.toggle_furniture(0);
        app.delete_selected_furniture();
        assert_eq!(app.furniture.len(), 1);
        assert!(app.selected_furniture_indices().is_empty());
    }

    #[test]
    fn paste_keeps_the_layout_and_centers_it_on_the_pointer() {
        let mut app = app_with(&[("Bookcase", (0.0, 0.0)), ("Plant", (100.0, 0.0))]);
        app.toggle_furniture(0);
        app.toggle_furniture(1);
        app.copy_selected_furniture(&egui::Context::default());
        app.paste_furniture(Some(Pos2::new(500.0, 500.0)), None);
        assert_eq!(app.furniture.len(), 4);
        assert_eq!(app.furniture[2].position, Pos2::new(450.0, 500.0));
        assert_eq!(app.furniture[3].position, Pos2::new(550.0, 500.0));
        assert_eq!(app.selected_furniture_indices(), vec![2, 3]);
    }

    #[test]
    fn paste_from_another_window_uses_the_system_clipboard() {
        let mut app = app_with(&[]);
        let json = serde_json::to_string(&vec![preset("Plant")]).unwrap();
        app.paste_furniture(None, Some(&format!("{CLIPBOARD_PREFIX}{json}")));
        assert_eq!(app.furniture.len(), 1);
        assert_eq!(app.furniture[0].name, "Plant");
    }
}
