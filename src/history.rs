use crate::{
    app::RoomPlannerApp,
    geometry::rooms::extract_rooms,
    models::{Finishes, Furniture, Wall},
};

const MAX_STEPS: usize = 200;

#[derive(Clone, PartialEq, Default)]
pub struct Snapshot {
    pub walls: Vec<Wall>,
    pub furniture: Vec<Furniture>,
    pub finishes: Finishes,
}

impl Snapshot {
    pub fn of(app: &RoomPlannerApp) -> Self {
        Snapshot {
            walls: app.walls.clone(),
            furniture: app.furniture.clone(),
            finishes: app.finishes,
        }
    }
}

/// Undo/redo by snapshots. Instead of every edit registering itself, the plan is compared
/// with the last snapshot whenever the user is not mid-gesture, so drags and typing
/// become a single step each.
#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    current: Snapshot,
}

impl History {
    pub fn reset(&mut self, snapshot: Snapshot) {
        self.undo.clear();
        self.redo.clear();
        self.current = snapshot;
    }

    pub fn current(&self) -> &Snapshot {
        &self.current
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Records a step if the plan changed. Call once per frame.
pub fn track(app: &mut RoomPlannerApp, ctx: &eframe::egui::Context) {
    let busy = ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input();
    if busy {
        return;
    }
    let current = &app.history.current;
    if app.walls == current.walls
        && app.furniture == current.furniture
        && app.finishes == current.finishes
    {
        return;
    }
    let snapshot = Snapshot::of(app);
    let previous = std::mem::replace(&mut app.history.current, snapshot);
    app.history.undo.push(previous);
    if app.history.undo.len() > MAX_STEPS {
        app.history.undo.remove(0);
    }
    app.history.redo.clear();
}

pub fn undo(app: &mut RoomPlannerApp) {
    if let Some(previous) = app.history.undo.pop() {
        let current = std::mem::replace(&mut app.history.current, previous);
        app.history.redo.push(current);
        restore(app);
    }
}

pub fn redo(app: &mut RoomPlannerApp) {
    if let Some(next) = app.history.redo.pop() {
        let current = std::mem::replace(&mut app.history.current, next);
        app.history.undo.push(current);
        restore(app);
    }
}

fn restore(app: &mut RoomPlannerApp) {
    let snapshot = app.history.current.clone();
    app.walls = snapshot.walls;
    app.furniture = snapshot.furniture;
    app.finishes = snapshot.finishes;
    app.rooms = extract_rooms(&app.walls);
    app.selected_walls.clear();
    app.selected_furniture = app.selected_furniture.filter(|&i| i < app.furniture.len());
    app.wall_start_point = None;
    app.selected_opening = None;
    app.opening_drag = false;
    app.dragging_endpoints.clear();
    app.furniture_drag = None;
}
