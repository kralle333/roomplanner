pub mod lamp;
pub mod mesh;
pub mod model;
pub mod presets;

pub use presets::{Category, Preset, PRESETS};

use crate::{
    app::{PIXELS_PER_METER, WALL_HEIGHT},
    models::{Furniture, FurnitureKind},
};
use eframe::egui::{emath::Rot2, vec2, Pos2, Vec2};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Box,
    /// An elliptic cylinder filling the part's bounds.
    Cylinder,
    /// A cylinder whose top radius is scaled by the given factor.
    Cone {
        top_scale: f32,
    },
    /// An ellipsoid filling the part's bounds.
    Sphere,
}

/// A primitive in the furniture's local space (meters), bounded by min/max.
/// x runs along the width, z along the depth (+z is the front), y is up from the item's elevation.
pub struct Part {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub color: [u8; 3],
    pub shape: Shape,
    /// How much the part glows by itself (lamp shades and bulbs).
    pub emissive: f32,
}

impl Part {
    pub fn cuboid(min: [f32; 3], max: [f32; 3], color: [u8; 3]) -> Self {
        Part {
            min,
            max,
            color,
            shape: Shape::Box,
            emissive: 0.0,
        }
    }

    /// A round part centered on (x, z) with the given radius, from y0 to y1.
    pub fn round(
        shape: Shape,
        x: f32,
        z: f32,
        radius: f32,
        y0: f32,
        y1: f32,
        color: [u8; 3],
    ) -> Self {
        Part {
            min: [x - radius, y0, z - radius],
            max: [x + radius, y1, z + radius],
            color,
            shape,
            emissive: 0.0,
        }
    }

    /// Whether the part covers a point (local x, z) seen from above.
    pub fn covers(&self, x: f32, z: f32) -> bool {
        let (cx, cz) = (
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[2] + self.max[2]) / 2.0,
        );
        let (rx, rz) = (
            (self.max[0] - self.min[0]) / 2.0,
            (self.max[2] - self.min[2]) / 2.0,
        );
        if rx <= 0.0 || rz <= 0.0 {
            return false;
        }
        let (dx, dz) = ((x - cx) / rx, (z - cz) / rz);
        match self.shape {
            Shape::Box => dx.abs() <= 1.0 && dz.abs() <= 1.0,
            Shape::Cone { top_scale } => {
                let scale = top_scale.max(1.0);
                (dx * dx + dz * dz) <= scale * scale
            }
            Shape::Cylinder | Shape::Sphere => dx * dx + dz * dz <= 1.0,
        }
    }

    pub fn glowing(mut self, emissive: f32) -> Self {
        self.emissive = emissive;
        self
    }
}

/// Every part of every piece, in plan drawing order: lowest tops first, so what is higher
/// covers what is below it, like looking down from above. A chair pushed under a table
/// has its seat hidden by the table top while its backrest shows over the edge.
/// Imported models count as a single box filling their bounds. Ties keep insertion order.
pub fn plan_layers(furniture: &[Furniture]) -> Vec<(usize, Part)> {
    let mut layers: Vec<(usize, Part)> = Vec::new();
    for (idx, item) in furniture.iter().enumerate() {
        let mut parts = item.parts();
        if parts.is_empty() {
            let (hw, hd) = (item.width / 2.0, item.depth / 2.0);
            parts.push(Part::cuboid(
                [-hw, 0.0, -hd],
                [hw, item.height, hd],
                item.color,
            ));
        }
        layers.extend(parts.into_iter().map(|part| (idx, part)));
    }
    let top = |(idx, part): &(usize, Part)| furniture[*idx].elevation + part.max[1];
    layers.sort_by(|a, b| top(a).total_cmp(&top(b)));
    layers
}

/// The piece whose part is visible at a world point, matching the drawing order. Falls back
/// to the footprints, for gaps between a piece's parts.
pub fn furniture_at(furniture: &[Furniture], p: Pos2) -> Option<usize> {
    let hit = plan_layers(furniture)
        .into_iter()
        .rev()
        .find(|(idx, part)| {
            let local = furniture[*idx].world_to_local(p);
            part.covers(local.x, local.y)
        });
    if let Some((idx, _)) = hit {
        return Some(idx);
    }
    (0..furniture.len())
        .filter(|&i| furniture[i].contains(p))
        .max_by(|&a, &b| furniture[a].top().total_cmp(&furniture[b].top()))
}

pub fn shade(color: [u8; 3], factor: f32) -> [u8; 3] {
    color.map(|c| (c as f32 * factor).round().clamp(0.0, 255.0) as u8)
}

impl Furniture {
    pub fn new_model(name: String, path: String, size: [f32; 3]) -> Self {
        presets::make(
            &name,
            FurnitureKind::Model { path },
            size[0],
            size[2],
            size[1],
            [200, 200, 200],
        )
    }

    pub fn top(&self) -> f32 {
        self.elevation + self.height
    }

    pub fn is_lamp(&self) -> bool {
        matches!(self.kind, FurnitureKind::Lamp { .. })
    }

    /// Converts a point in local space (meters, x/z plane) to world (canvas) coordinates.
    pub fn local_to_world(&self, x: f32, z: f32) -> Pos2 {
        self.position + Rot2::from_angle(self.rotation) * vec2(x, z) * PIXELS_PER_METER
    }

    pub fn world_to_local(&self, p: Pos2) -> Vec2 {
        Rot2::from_angle(-self.rotation) * ((p - self.position) / PIXELS_PER_METER)
    }

    pub fn contains(&self, p: Pos2) -> bool {
        let local = self.world_to_local(p);
        local.x.abs() <= self.width / 2.0 && local.y.abs() <= self.depth / 2.0
    }

    /// The four footprint corners in world coordinates.
    pub fn footprint(&self) -> [Pos2; 4] {
        let (hw, hd) = (self.width / 2.0, self.depth / 2.0);
        [
            self.local_to_world(-hw, -hd),
            self.local_to_world(hw, -hd),
            self.local_to_world(hw, hd),
            self.local_to_world(-hw, hd),
        ]
    }

    /// Thickness of the differently colored top of a box, like a countertop.
    pub const TOP_THICKNESS: f32 = 0.03;

    /// Breaks the parametric shape down into primitives. Imported models have no parts.
    pub fn parts(&self) -> Vec<Part> {
        let (w, d, h) = (self.width, self.depth, self.height);
        let (hw, hd) = (w / 2.0, d / 2.0);
        let c = self.color;
        let cuboid = Part::cuboid;

        match self.kind {
            FurnitureKind::Box => match self.top_color {
                Some(top) => {
                    let slab = Self::TOP_THICKNESS.min(h / 2.0);
                    vec![
                        cuboid([-hw, 0.0, -hd], [hw, h - slab, hd], c),
                        cuboid([-hw, h - slab, -hd], [hw, h, hd], top),
                    ]
                }
                None => vec![cuboid([-hw, 0.0, -hd], [hw, h, hd], c)],
            },
            FurnitureKind::Table {
                top_thickness,
                leg_size,
                round,
            } => {
                let top = top_thickness.min(h);
                if round {
                    let r = hw.min(hd);
                    let foot = (r * 0.5).min(0.3);
                    let mut top_part = Part::round(Shape::Cylinder, 0.0, 0.0, r, h - top, h, c);
                    [top_part.min[0], top_part.max[0]] = [-hw, hw];
                    [top_part.min[2], top_part.max[2]] = [-hd, hd];
                    return vec![
                        top_part,
                        Part::round(
                            Shape::Cylinder,
                            0.0,
                            0.0,
                            leg_size / 2.0,
                            0.0,
                            h - top,
                            shade(c, 0.8),
                        ),
                        Part::round(
                            Shape::Cylinder,
                            0.0,
                            0.0,
                            foot,
                            0.0,
                            0.03_f32.min(h),
                            shade(c, 0.8),
                        ),
                    ];
                }
                let leg = leg_size.min(hw).min(hd);
                let mut parts = vec![cuboid([-hw, h - top, -hd], [hw, h, hd], c)];
                for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let (x, z) = (sx * (hw - leg / 2.0), sz * (hd - leg / 2.0));
                    parts.push(cuboid(
                        [x - leg / 2.0, 0.0, z - leg / 2.0],
                        [x + leg / 2.0, h - top, z + leg / 2.0],
                        shade(c, 0.8),
                    ));
                }
                parts
            }
            FurnitureKind::Chair { seat_height } => {
                let seat = seat_height.min(h);
                let thick = 0.04_f32.min(seat);
                let leg = 0.04_f32.min(hw).min(hd);
                let back = 0.04_f32.min(d);
                let mut parts = vec![
                    cuboid([-hw, seat - thick, -hd], [hw, seat, hd], c),
                    cuboid([-hw, seat, -hd], [hw, h, -hd + back], shade(c, 0.9)),
                ];
                for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let (x, z) = (sx * (hw - leg / 2.0), sz * (hd - leg / 2.0));
                    parts.push(cuboid(
                        [x - leg / 2.0, 0.0, z - leg / 2.0],
                        [x + leg / 2.0, seat - thick, z + leg / 2.0],
                        shade(c, 0.75),
                    ));
                }
                parts
            }
            FurnitureKind::Bed { headboard_height } => {
                // The bed's height is the top of the mattress; the headboard may rise above it.
                let mattress_top = h;
                let frame_top = (mattress_top - 0.2).max(mattress_top * 0.5);
                let board = 0.06_f32.min(d / 2.0);
                let pillow = (0.35_f32).min(d / 4.0);
                vec![
                    cuboid([-hw, 0.0, -hd], [hw, frame_top, hd], shade(c, 0.6)),
                    cuboid(
                        [-hw + 0.02, frame_top, -hd + board],
                        [hw - 0.02, mattress_top, hd - 0.02],
                        c,
                    ),
                    cuboid(
                        [-hw + 0.08, mattress_top, -hd + board + 0.05],
                        [hw - 0.08, mattress_top + 0.08, -hd + board + 0.05 + pillow],
                        [250, 250, 250],
                    ),
                    cuboid(
                        [-hw, 0.0, -hd],
                        [hw, headboard_height.max(mattress_top), -hd + board],
                        shade(c, 0.6),
                    ),
                ]
            }
            FurnitureKind::Sofa {
                seat_height,
                arm_width,
                back_depth,
            } => {
                let seat = seat_height.min(h);
                let arm = arm_width.min(hw);
                let back = back_depth.min(d);
                let arm_top = (seat + 0.2).min(h);
                vec![
                    cuboid([-hw + arm, 0.0, -hd + back], [hw - arm, seat, hd], c),
                    cuboid([-hw, 0.0, -hd], [hw, h, -hd + back], shade(c, 0.85)),
                    cuboid([-hw, 0.0, -hd], [-hw + arm, arm_top, hd], shade(c, 0.9)),
                    cuboid([hw - arm, 0.0, -hd], [hw, arm_top, hd], shade(c, 0.9)),
                ]
            }
            FurnitureKind::Shelf { shelves } => {
                let board = 0.02_f32.min(hw).min(h / 2.0);
                let mut parts = vec![
                    cuboid([-hw, 0.0, -hd], [-hw + board, h, hd], c),
                    cuboid([hw - board, 0.0, -hd], [hw, h, hd], c),
                    cuboid([-hw, 0.0, -hd], [hw, h, -hd + 0.005], shade(c, 0.85)),
                ];
                // Bottom, top and evenly spaced shelves in between.
                let levels = shelves.max(2);
                for i in 0..levels {
                    let y = (h - board) * i as f32 / (levels - 1) as f32;
                    parts.push(cuboid(
                        [-hw + board, y, -hd],
                        [hw - board, y + board, hd],
                        c,
                    ));
                }
                parts
            }
            FurnitureKind::CubeShelf { .. } => {
                let Some(layout) = CubeLayout::of(self) else {
                    return Vec::new();
                };
                let (frame, divider) = (layout.frame, layout.divider);
                let mut parts = vec![
                    cuboid([-hw, 0.0, -hd], [-hw + frame, h, hd], c),
                    cuboid([hw - frame, 0.0, -hd], [hw, h, hd], c),
                    cuboid([-hw + frame, 0.0, -hd], [hw - frame, frame, hd], c),
                    cuboid([-hw + frame, h - frame, -hd], [hw - frame, h, hd], c),
                ];
                let (x0, y0) = (-hw + frame, frame);
                let xs = offsets(x0, &layout.columns, divider);
                let ys = offsets(y0, &layout.rows, divider);
                // Full-height uprights, with the shelves fitted between them.
                for i in 1..layout.columns.len() {
                    let x = xs[i] - divider;
                    parts.push(cuboid([x, y0, -hd], [x + divider, h - frame, hd], c));
                }
                for j in 1..layout.rows.len() {
                    let y = ys[j] - divider;
                    for (i, w) in layout.columns.iter().enumerate() {
                        parts.push(cuboid([xs[i], y, -hd], [xs[i] + w, y + divider, hd], c));
                    }
                }
                parts
            }
            FurnitureKind::Plant => {
                let r = hw.min(hd);
                let pot_h = (h * 0.3).min(0.4);
                vec![
                    Part::round(
                        Shape::Cone { top_scale: 1.25 },
                        0.0,
                        0.0,
                        r * 0.55,
                        0.0,
                        pot_h,
                        c,
                    ),
                    Part::round(
                        Shape::Cylinder,
                        0.0,
                        0.0,
                        0.015,
                        pot_h,
                        h * 0.5,
                        [90, 70, 50],
                    ),
                    Part::round(Shape::Sphere, 0.0, 0.0, r, pot_h * 0.8, h, [80, 135, 70]),
                ]
            }
            FurnitureKind::Lamp { .. } => lamp::parts(self),
            FurnitureKind::Model { .. } => Vec::new(),
        }
    }

    /// Height of the ceiling above this item's base, for things hanging from it.
    pub fn ceiling_above(&self) -> f32 {
        (WALL_HEIGHT - self.elevation).max(self.height)
    }

    /// Fields that belong to an option rather than to the spot it occupies.
    fn option_snapshot(&self) -> Furniture {
        let mut copy = self.clone();
        copy.options.clear();
        copy.active_option = 0;
        copy
    }

    pub fn option_count(&self) -> usize {
        self.options.len().max(1)
    }

    /// Switches to another option. Position and rotation stay with the spot.
    pub fn switch_option(&mut self, index: usize) {
        if index >= self.options.len() || index == self.active_option {
            return;
        }
        let current = self.option_snapshot();
        let mut options = std::mem::take(&mut self.options);
        if let Some(slot) = options.get_mut(self.active_option) {
            *slot = current;
        }
        let (position, rotation) = (self.position, self.rotation);
        *self = options[index].clone();
        self.position = position;
        self.rotation = rotation;
        self.options = options;
        self.active_option = index;
    }

    /// Adds a new option (a copy of the current one) and switches to it.
    pub fn add_option(&mut self) {
        if self.options.is_empty() {
            self.options.push(self.option_snapshot());
            self.active_option = 0;
        }
        let mut copy = self.option_snapshot();
        copy.name = format!("{} {}", base_name(&self.name), self.options.len() + 1);
        self.options.push(copy);
        self.switch_option(self.options.len() - 1);
    }

    /// Removes the active option and switches to a neighbour.
    pub fn remove_option(&mut self) {
        if self.options.len() < 2 {
            return;
        }
        let removed = self.active_option;
        let next = if removed + 1 < self.options.len() {
            removed + 1
        } else {
            removed - 1
        };
        self.switch_option(next);
        self.options.remove(removed);
        if self.active_option > removed {
            self.active_option -= 1;
        }
        if self.options.len() == 1 {
            self.options.clear();
            self.active_option = 0;
        }
    }

    pub fn next_option(&mut self) {
        if !self.options.is_empty() {
            self.switch_option((self.active_option + 1) % self.options.len());
        }
    }
}

/// "Floor lamp 2" -> "Floor lamp", so repeated options don't become "Floor lamp 2 3".
fn base_name(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((base, n)) if n.parse::<u32>().is_ok() => base,
        _ => name,
    }
}

/// Where the compartments of a cube shelf go: the frame and divider thickness and
/// the inner size of every column (left to right) and row (bottom to top), in meters.
pub struct CubeLayout {
    pub frame: f32,
    pub divider: f32,
    pub columns: Vec<f32>,
    pub rows: Vec<f32>,
}

impl CubeLayout {
    pub const DIVIDER: f32 = 0.016;

    pub fn of(item: &Furniture) -> Option<Self> {
        let FurnitureKind::CubeShelf {
            columns,
            rows,
            column_widths,
            row_heights,
        } = &item.kind
        else {
            return None;
        };
        let (columns, rows) = ((*columns).max(1) as usize, (*rows).max(1) as usize);
        let frame = Self::frame(item.width, item.height);
        let divider = Self::DIVIDER;
        let inner_w = item.width - 2.0 * frame - divider * (columns - 1) as f32;
        let inner_h = item.height - 2.0 * frame - divider * (rows - 1) as f32;
        Some(Self {
            frame,
            divider,
            columns: distribute(column_widths, columns, inner_w),
            rows: distribute(row_heights, rows, inner_h),
        })
    }

    pub fn frame(width: f32, height: f32) -> f32 {
        0.04_f32.min(width / 4.0).min(height / 4.0)
    }
}

/// Splits `total` into `n` sizes in proportion to `weights`, or evenly when the
/// weights don't describe exactly `n` positive sizes.
fn distribute(weights: &[f32], n: usize, total: f32) -> Vec<f32> {
    let total = total.max(0.0);
    let sum: f32 = weights.iter().sum();
    if weights.len() == n && sum > 0.0 && weights.iter().all(|w| *w >= 0.0) {
        weights.iter().map(|w| total * w / sum).collect()
    } else {
        vec![total / n as f32; n]
    }
}

/// Start coordinate of each cell, given cell sizes separated by dividers.
fn offsets(start: f32, sizes: &[f32], divider: f32) -> Vec<f32> {
    let mut at = start;
    sizes
        .iter()
        .map(|s| {
            let here = at;
            at += s + divider;
            here
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(label: &str) -> Furniture {
        (PRESETS.iter().find(|p| p.label == label).unwrap().build)()
    }

    #[test]
    fn parts_stay_within_footprint() {
        for preset in PRESETS {
            let item = (preset.build)();
            for part in item.parts() {
                for axis in 0..3 {
                    assert!(
                        part.min[axis] <= part.max[axis],
                        "{}: inverted part",
                        preset.label
                    );
                }
                let r = match part.shape {
                    Shape::Cone { top_scale } => top_scale.max(1.0),
                    _ => 1.0,
                };
                let (hx, hz) = (
                    (part.max[0] - part.min[0]) / 2.0,
                    (part.max[2] - part.min[2]) / 2.0,
                );
                let (cx, cz) = (
                    (part.max[0] + part.min[0]) / 2.0,
                    (part.max[2] + part.min[2]) / 2.0,
                );
                assert!(
                    (cx.abs() + hx * r) <= item.width / 2.0 + 1e-4,
                    "{}",
                    preset.label
                );
                assert!(
                    (cz.abs() + hz * r) <= item.depth / 2.0 + 1e-4,
                    "{}",
                    preset.label
                );
            }
        }
    }

    #[test]
    fn chair_under_a_table_shows_its_back_above_the_top() {
        let mut table = preset("Dining table");
        table.position = Pos2::new(0.0, 0.0);
        let mut chair = preset("Chair");
        // Pushed halfway under the table's front edge, facing it.
        chair.rotation = std::f32::consts::PI;
        chair.position = Pos2::new(0.0, (table.depth / 2.0) * PIXELS_PER_METER);
        let furniture = vec![chair, table];

        let layers = plan_layers(&furniture);
        let top = layers.iter().rposition(|(i, _)| *i == 1).unwrap();
        let seat = layers
            .iter()
            .position(|(i, p)| *i == 0 && p.max[1] < furniture[1].height)
            .unwrap();
        assert!(seat < top, "the seat is drawn before the table top");

        // Clicking the middle of the table top picks the table, the chair's free half the chair.
        assert_eq!(furniture_at(&furniture, Pos2::new(0.0, 5.0)), Some(1));
        let outside = Pos2::new(0.0, (furniture[1].depth / 2.0 + 0.15) * PIXELS_PER_METER);
        assert_eq!(furniture_at(&furniture, outside), Some(0));
    }

    #[test]
    fn hit_test_respects_rotation() {
        let mut table = preset("Dining table"); // 1.8 m × 0.9 m
        table.position = Pos2::new(100.0, 100.0);
        let along_width = Pos2::new(100.0 + 0.7 * PIXELS_PER_METER, 100.0);
        assert!(table.contains(along_width));
        table.rotation = std::f32::consts::FRAC_PI_2;
        assert!(!table.contains(along_width));
        assert!(table.contains(Pos2::new(100.0, 100.0 + 0.7 * PIXELS_PER_METER)));
    }

    #[test]
    fn options_keep_their_own_settings_and_share_the_spot() {
        let mut lamp = preset("Floor lamp");
        lamp.position = Pos2::new(10.0, 20.0);
        lamp.add_option();
        assert_eq!(lamp.options.len(), 2);
        assert_eq!(lamp.active_option, 1);
        assert_eq!(lamp.name, "Floor lamp 2");
        lamp.height = 1.8;

        lamp.position = Pos2::new(50.0, 60.0);
        lamp.switch_option(0);
        assert_eq!(lamp.name, "Floor lamp");
        assert_eq!(lamp.height, 1.5);
        assert_eq!(lamp.position, Pos2::new(50.0, 60.0));

        lamp.next_option();
        assert_eq!(lamp.height, 1.8);

        lamp.remove_option();
        assert!(lamp.options.is_empty());
        assert_eq!(lamp.name, "Floor lamp");
    }

    #[test]
    fn cube_shelf_has_square_compartments_inside_its_bounds() {
        let shelf = presets::make(
            "Cubes",
            FurnitureKind::CubeShelf {
                columns: 2,
                rows: 2,
                column_widths: Vec::new(),
                row_heights: Vec::new(),
            },
            0.77,
            0.39,
            0.77,
            [255, 255, 255],
        );
        let parts = shelf.parts();
        // 4 frame boards, 1 upright, 2 shelf pieces.
        assert_eq!(parts.len(), 7);
        for p in &parts {
            assert!(p.min[0] >= -0.385 - 1e-4 && p.max[0] <= 0.385 + 1e-4);
            assert!(p.min[1] >= 0.0 && p.max[1] <= 0.77 + 1e-4);
        }
        let upright = &parts[4];
        let left_cell = upright.min[0] - (-0.385 + 0.04);
        let lower_cell = parts[5].min[1] - 0.04;
        assert!((left_cell - lower_cell).abs() < 1e-4);
    }

    #[test]
    fn cube_shelf_compartments_follow_custom_sizes() {
        let shelf = presets::make(
            "Cubes",
            FurnitureKind::CubeShelf {
                columns: 2,
                rows: 2,
                column_widths: vec![0.2, 0.4],
                row_heights: vec![0.3, 0.3],
            },
            0.2 + 0.4 + 2.0 * 0.04 + CubeLayout::DIVIDER,
            0.39,
            0.6 + 2.0 * 0.04 + CubeLayout::DIVIDER,
            [255, 255, 255],
        );
        let layout = CubeLayout::of(&shelf).unwrap();
        assert!((layout.columns[0] - 0.2).abs() < 1e-4);
        assert!((layout.columns[1] - 0.4).abs() < 1e-4);
        let parts = shelf.parts();
        let upright = &parts[4];
        assert!((upright.min[0] - (-shelf.width / 2.0 + 0.04 + 0.2)).abs() < 1e-4);
        // Wrong-length sizes fall back to equal compartments.
        let mut uneven = shelf.clone();
        if let FurnitureKind::CubeShelf { columns, .. } = &mut uneven.kind {
            *columns = 3;
        }
        let layout = CubeLayout::of(&uneven).unwrap();
        assert!((layout.columns[0] - layout.columns[2]).abs() < 1e-5);
    }

    #[test]
    fn box_with_top_color_gets_a_top_slab() {
        let mut cabinet = preset("Cabinet");
        assert_eq!(cabinet.parts().len(), 1);
        cabinet.top_color = Some([10, 20, 30]);
        let parts = cabinet.parts();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].color, [10, 20, 30]);
        assert!((parts[1].max[1] - cabinet.height).abs() < 1e-6);
        assert!((parts[0].max[1] - parts[1].min[1]).abs() < 1e-6);
    }
}
