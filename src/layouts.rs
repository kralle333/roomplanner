//! Several layouts in one plan: different furniture, lights and finishes in the same walls,
//! to flip between and compare.
//!
//! The shown layout's furniture and finishes live in `RoomPlannerApp::furniture` and
//! `::finishes`, where everything else works on them. Its entry in `layouts` keeps only the
//! name (empty furniture, default finishes) while it is shown, so there is one copy of it.

use crate::app::{RoomPlannerApp, PIXELS_PER_METER};
use crate::models::{Finishes, Furniture, Layout};
use eframe::egui::Pos2;

/// The layouts with the shown one filled in, as they are saved.
pub fn all(app: &RoomPlannerApp) -> Vec<Layout> {
    let shown = app.active_layout;
    app.layouts
        .iter()
        .enumerate()
        .map(|(i, layout)| {
            if i == shown {
                Layout {
                    name: layout.name.clone(),
                    furniture: app.furniture.clone(),
                    finishes: app.finishes,
                }
            } else {
                layout.clone()
            }
        })
        .collect()
}

/// Replaces all layouts, showing `active`. An empty list becomes one empty layout.
pub fn set_all(app: &mut RoomPlannerApp, mut layouts: Vec<Layout>, active: usize) {
    if layouts.is_empty() {
        layouts.push(Layout::named(first_name()));
    }
    let active = active.min(layouts.len() - 1);
    app.furniture = std::mem::take(&mut layouts[active].furniture);
    app.finishes = std::mem::take(&mut layouts[active].finishes);
    app.layouts = layouts;
    app.active_layout = active;
    app.compare_layout = None;
    app.peek_return = None;
}

pub fn first_name() -> String {
    "Layout 1".to_owned()
}

/// Shows another layout; the one shown so far is kept as it is. The camera stays put.
pub fn switch(app: &mut RoomPlannerApp, index: usize) {
    if index >= app.layouts.len() || index == app.active_layout {
        return;
    }
    let shown = app.active_layout;
    app.layouts[shown].furniture = std::mem::take(&mut app.furniture);
    app.layouts[shown].finishes = std::mem::take(&mut app.finishes);
    app.furniture = std::mem::take(&mut app.layouts[index].furniture);
    app.finishes = std::mem::take(&mut app.layouts[index].finishes);
    app.active_layout = index;
    // What you came from is what you compare with.
    app.compare_layout = Some(shown);
    forget_selection(app);
}

/// Shows another layout while a key is held, without it counting as a change.
pub fn start_peek(app: &mut RoomPlannerApp) {
    let Some(other) = app.compare_layout.filter(|&i| i < app.layouts.len()) else {
        return;
    };
    if app.peek_return.is_none() {
        // Switching compares the peeked layout with the one you came from, and back.
        let back = app.active_layout;
        switch(app, other);
        app.peek_return = Some(back);
    }
}

pub fn end_peek(app: &mut RoomPlannerApp) {
    if let Some(back) = app.peek_return.take() {
        switch(app, back);
    }
}

fn forget_selection(app: &mut RoomPlannerApp) {
    app.clear_selection();
    app.furniture_drag = None;
    app.furniture_rotate = None;
    app.furniture_resize = None;
}

/// A copy of the shown layout, shown next. Returns its index.
pub fn duplicate(app: &mut RoomPlannerApp, index: usize) -> usize {
    let mut copy = all(app)[index].clone();
    copy.name = unique_name(app, &format!("{} copy", copy.name));
    let at = index + 1;
    app.layouts.insert(at, Layout::named(String::new()));
    // Inserting before the shown layout moves it one on.
    if at <= app.active_layout {
        app.active_layout += 1;
    }
    app.layouts[at] = copy;
    switch(app, at);
    app.compare_layout = Some(if at <= index { index + 1 } else { index });
    at
}

pub fn delete(app: &mut RoomPlannerApp, index: usize) {
    if app.layouts.len() <= 1 || index >= app.layouts.len() {
        return;
    }
    if index == app.active_layout {
        let next = if index + 1 < app.layouts.len() {
            index + 1
        } else {
            index - 1
        };
        switch(app, next);
    }
    app.layouts.remove(index);
    let fix = |i: usize| if i > index { i - 1 } else { i };
    app.active_layout = fix(app.active_layout);
    app.compare_layout = app
        .compare_layout
        .filter(|&i| i != index)
        .map(fix)
        .filter(|&i| i != app.active_layout);
}

/// Moves a layout one place left (-1) or right (1) among the tabs.
pub fn reorder(app: &mut RoomPlannerApp, index: usize, by: isize) {
    let target = index as isize + by;
    if target < 0 || target as usize >= app.layouts.len() {
        return;
    }
    let target = target as usize;
    app.layouts.swap(index, target);
    let fix = |i: usize| {
        if i == index {
            target
        } else if i == target {
            index
        } else {
            i
        }
    };
    app.active_layout = fix(app.active_layout);
    app.compare_layout = app.compare_layout.map(fix);
}

pub fn unique_name(app: &RoomPlannerApp, wanted: &str) -> String {
    let taken = |name: &str| app.layouts.iter().any(|l| l.name == name);
    if !taken(wanted) {
        return wanted.to_owned();
    }
    (2..)
        .map(|n| format!("{wanted} {n}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| wanted.to_owned())
}

/// Adds the layouts of another plan file as new tabs, named after the file. Its walls are
/// not used: layouts share this plan's walls. Returns how many were added.
pub fn import(
    app: &mut RoomPlannerApp,
    file_stem: &str,
    state: crate::models::AppSaveState,
) -> usize {
    let mut incoming = state.layouts;
    if incoming.is_empty() {
        incoming.push(Layout {
            name: first_name(),
            furniture: state.furniture,
            finishes: state.finishes,
        });
    }
    // "home - kitchen light" next to "home" becomes just "kitchen light".
    let base = app
        .current_file
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_string());
    let short = match &base {
        Some(base) => file_stem
            .strip_prefix(base.as_str())
            .map(|rest| rest.trim_start_matches([' ', '-', '_']).to_owned())
            .filter(|rest| !rest.is_empty())
            .unwrap_or_else(|| file_stem.to_owned()),
        None => file_stem.to_owned(),
    };
    let several = incoming.len() > 1;
    let count = incoming.len();
    for mut layout in incoming {
        let wanted = if several {
            format!("{short} · {}", layout.name)
        } else {
            short.clone()
        };
        layout.name = unique_name(app, &wanted);
        app.layouts.push(layout);
    }
    count
}

/// How the shown layout differs from another one.
#[derive(Default)]
pub struct Differences {
    /// Indices into the shown furniture: pieces the other layout doesn't have…
    pub added: Vec<usize>,
    /// …and pieces it has too, but moved, turned or changed.
    pub changed: Vec<usize>,
    /// Pieces only the other layout has.
    pub removed: Vec<Furniture>,
    pub finishes_differ: bool,
}

impl Differences {
    pub fn count(&self) -> usize {
        self.added.len() + self.changed.len() + self.removed.len() + self.finishes_differ as usize
    }
}

/// Matches up the pieces of two layouts: identical pieces first, then pieces of the same
/// name and kind, nearest first.
pub fn differences(
    shown: &[Furniture],
    other: &[Furniture],
    finishes: (Finishes, Finishes),
) -> Differences {
    let mut used = vec![false; other.len()];
    let mut matched: Vec<Option<usize>> = vec![None; shown.len()];
    for (i, piece) in shown.iter().enumerate() {
        if let Some(j) = (0..other.len()).find(|&j| !used[j] && other[j] == *piece) {
            used[j] = true;
            matched[i] = Some(j);
        }
    }
    let mut changed = Vec::new();
    let mut added = Vec::new();
    for (i, piece) in shown.iter().enumerate() {
        if matched[i].is_some() {
            continue;
        }
        // The same piece moved (up to 3 m), or a nearby one of the same kind changed or
        // renamed (up to 1 m).
        let distance = |j: usize| other[j].position.distance(piece.position);
        let nearest = |fits: &dyn Fn(&Furniture) -> bool, reach: f32| {
            (0..other.len())
                .filter(|&j| !used[j] && fits(&other[j]))
                .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
                .filter(|&j| distance(j) < reach * PIXELS_PER_METER)
        };
        let same_kind =
            |f: &Furniture| std::mem::discriminant(&f.kind) == std::mem::discriminant(&piece.kind);
        let near =
            nearest(&|f| crate::app::same_type(f, piece), 3.0).or_else(|| nearest(&same_kind, 1.0));
        match near {
            Some(j) => {
                used[j] = true;
                changed.push(i);
            }
            None => added.push(i),
        }
    }
    let removed = other
        .iter()
        .enumerate()
        .filter(|&(j, _)| !used[j])
        .map(|(_, f)| f.clone())
        .collect();
    Differences {
        added,
        changed,
        removed,
        finishes_differ: finishes.0 != finishes.1,
    }
}

/// The differences between the shown layout and the one it is compared with, if any.
pub fn current_differences(app: &RoomPlannerApp) -> Option<Differences> {
    let other = app
        .compare_layout
        .filter(|&i| i != app.active_layout)
        .and_then(|i| app.layouts.get(i))?;
    Some(differences(
        &app.furniture,
        &other.furniture,
        (app.finishes, other.finishes),
    ))
}

/// The middle of a piece, for labels.
pub fn center(item: &Furniture) -> Pos2 {
    item.position
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::PRESETS;

    fn preset(label: &str, x: f32) -> Furniture {
        let mut item = (PRESETS.iter().find(|p| p.label == label).unwrap().build)();
        item.position = Pos2::new(x, 100.0);
        item
    }

    #[test]
    fn layouts_switch_duplicate_and_delete() {
        let mut app = RoomPlannerApp::default();
        app.furniture = vec![preset("Sofa", 100.0)];
        let copy = duplicate(&mut app, 0);
        assert_eq!((copy, app.active_layout), (1, 1));
        assert_eq!(app.layouts[1].name, "Layout 1 copy");
        app.furniture.push(preset("Armchair", 300.0));

        switch(&mut app, 0);
        let compared = app.compare_layout;
        assert_eq!(app.furniture.len(), 1);
        assert_eq!(app.compare_layout, Some(1));
        assert_eq!(all(&app)[1].furniture.len(), 2);

        // Peeking shows the other one and comes back, as it was.
        start_peek(&mut app);
        assert_eq!((app.active_layout, app.furniture.len()), (1, 2));
        assert_eq!(app.compare_layout, Some(0));
        end_peek(&mut app);
        assert_eq!((app.active_layout, app.furniture.len()), (0, 1));
        assert_eq!(app.compare_layout, compared);

        delete(&mut app, 0);
        assert_eq!(app.layouts.len(), 1);
        assert_eq!((app.active_layout, app.furniture.len()), (0, 2));
    }

    #[test]
    fn differences_find_added_moved_and_removed_pieces() {
        let other = vec![
            preset("Sofa", 100.0),
            preset("Armchair", 300.0),
            preset("Rug", 500.0),
        ];
        let mut shown = other.clone();
        shown[1].position.x += 40.0; // moved
        shown.remove(2); // removed
        shown.push(preset("Floor lamp", 700.0)); // added
        let d = differences(&shown, &other, (Finishes::default(), Finishes::default()));
        assert_eq!((d.added, d.changed), (vec![2], vec![1]));
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].name, "Rug");
    }
}
