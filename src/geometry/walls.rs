use crate::{
    app::{PIXELS_PER_METER, WALL_THICKNESS},
    geometry::{
        math::closest_point_on_segment,
        openings::{opening_center, wall_frame},
    },
    models::Wall,
};
use eframe::egui::{Pos2, Vec2};
use std::collections::HashSet;

/// An in-progress move of the selected walls. The walls are rebuilt from `origin` every
/// frame, so stretching neighbours and keeping their doors in place doesn't drift.
pub struct WallDrag {
    pub origin: Vec<Wall>,
    /// Total mouse movement since the drag started, in world units.
    pub raw_delta: Vec2,
    /// Endpoints of unselected walls that touch a selected wall and follow it.
    pub followers: Vec<(usize, bool)>,
    /// When every moved wall is parallel, the move is kept along their normal so the
    /// connected walls keep their angles.
    pub normal: Option<Vec2>,
    /// Whether the followers were stretched in the last update (Alt can turn it off).
    pub stretching: bool,
    /// Alignment guides from the last snap: moved point to the point it lines up with.
    pub guides: Vec<(Pos2, Pos2)>,
}

impl WallDrag {
    pub fn new(walls: &[Wall], selected: &HashSet<usize>) -> Self {
        WallDrag {
            origin: walls.to_vec(),
            raw_delta: Vec2::ZERO,
            followers: attached_endpoints(walls, selected),
            normal: common_normal(walls, selected),
            stretching: false,
            guides: Vec::new(),
        }
    }

    /// The original length of `idx` if it is being stretched, in meters.
    pub fn stretched_from(&self, idx: usize) -> Option<f32> {
        if !self.stretching || !self.followers.iter().any(|&(i, _)| i == idx) {
            return None;
        }
        self.origin.get(idx).map(Wall::length)
    }
}

/// Endpoints of unselected walls lying on a selected wall: shared corners and T-joins.
pub fn attached_endpoints(walls: &[Wall], selected: &HashSet<usize>) -> Vec<(usize, bool)> {
    let mut attached = Vec::new();
    for (idx, wall) in walls.iter().enumerate() {
        if selected.contains(&idx) {
            continue;
        }
        for (is_start, p) in [(true, wall.start), (false, wall.end)] {
            let touches = selected
                .iter()
                .filter_map(|&s| walls.get(s))
                .any(|sel| p.distance(closest_point_on_segment(p, sel.start, sel.end)) < 2.0);
            if touches {
                attached.push((idx, is_start));
            }
        }
    }
    attached
}

/// Unselected walls that touch a selected one, either way round: corners, walls ending
/// on a selected wall, and walls a selected wall ends on.
pub fn touching_walls(walls: &[Wall], selected: &HashSet<usize>) -> HashSet<usize> {
    let mut touching: HashSet<usize> = attached_endpoints(walls, selected)
        .into_iter()
        .map(|(idx, _)| idx)
        .collect();
    for (idx, wall) in walls.iter().enumerate() {
        if selected.contains(&idx) {
            continue;
        }
        let hosts_selected = selected.iter().filter_map(|&s| walls.get(s)).any(|sel| {
            [sel.start, sel.end]
                .into_iter()
                .any(|p| p.distance(closest_point_on_segment(p, wall.start, wall.end)) < 2.0)
        });
        if hosts_selected {
            touching.insert(idx);
        }
    }
    touching
}

fn common_normal(walls: &[Wall], selected: &HashSet<usize>) -> Option<Vec2> {
    let mut normal: Option<Vec2> = None;
    for wall in selected.iter().filter_map(|&i| walls.get(i)) {
        let (_, n) = wall_frame(wall);
        match normal {
            None => normal = Some(n),
            Some(prev) if prev.dot(n).abs() > 0.999 => {}
            Some(_) => return None,
        }
    }
    normal
}

/// Half a wall's thickness, in world units. Walls are drawn centered on their line.
fn half_thickness() -> f32 {
    WALL_THICKNESS * PIXELS_PER_METER / 2.0
}

/// One side of a wall, measured the way plans measure: along its surface, from where it
/// meets the surfaces of the walls joined to it. Inside a corner that is half a wall
/// shorter than the center line at each end, outside it is half a wall longer.
/// Distances are along the wall from its start, in world units.
pub struct Face {
    /// The face's offset from the center line as a multiple of the wall's normal (±1).
    pub side: f32,
    pub start: f32,
    pub end: f32,
    /// The clear stretches between walls joining this face partway along.
    pub pieces: Vec<(f32, f32)>,
}

impl Face {
    pub fn length(&self) -> f32 {
        (self.end - self.start).max(0.0) / PIXELS_PER_METER
    }

    /// The world point on this face at distance `u` along the wall.
    pub fn point(&self, wall: &Wall, u: f32) -> Pos2 {
        let (dir, normal) = wall_frame(wall);
        wall.start + dir * u + normal * self.side * half_thickness()
    }
}

/// A place along a wall where other walls join it, with the directions they leave in.
struct Junction {
    u: f32,
    dirs: Vec<Vec2>,
}

fn junctions(walls: &[Wall], idx: usize) -> Vec<Junction> {
    let wall = &walls[idx];
    let (dir, _) = wall_frame(wall);
    let length = wall.start.distance(wall.end);
    let mut found: Vec<Junction> = Vec::new();
    let mut add = |u: f32, d: Vec2| match found.iter_mut().find(|j| (j.u - u).abs() < 2.0) {
        Some(j) => j.dirs.push(d),
        None => found.push(Junction { u, dirs: vec![d] }),
    };
    for (j, other) in walls.iter().enumerate() {
        if j == idx || other.start.distance(other.end) < 1e-3 {
            continue;
        }
        // The other wall ends on this one: it leaves towards its far end.
        for (q, far) in [(other.start, other.end), (other.end, other.start)] {
            let proj = closest_point_on_segment(q, wall.start, wall.end);
            if q.distance(proj) < 2.0 {
                add((proj - wall.start).dot(dir), (far - q).normalized());
            }
        }
        // This wall ends partway along the other one, which runs off both ways.
        for (p, u) in [(wall.start, 0.0), (wall.end, length)] {
            let on_other = p.distance(closest_point_on_segment(p, other.start, other.end)) < 2.0;
            if on_other && p.distance(other.start) >= 2.0 && p.distance(other.end) >= 2.0 {
                let (other_dir, _) = wall_frame(other);
                add(u, other_dir);
                add(u, -other_dir);
            }
        }
    }
    found.sort_by(|a, b| a.u.total_cmp(&b.u));
    found
}

/// How far (meters) to draw a wall past its start and end so walls meet without
/// overlapping: overlapping walls have faces in the same plane, which flicker in 3D when
/// one of them is highlighted. Negative values stop the wall short of its end point.
///
/// At a corner the wall with the lowest index runs on to the outer corner and the others
/// stop at its face. A wall ending partway along another stops at that wall's face, and a
/// wall continued in a straight line by another stops right at the shared point. A free
/// end runs on by half the thickness, like before.
pub fn wall_end_extensions(walls: &[Wall], idx: usize) -> (f32, f32) {
    let wall = &walls[idx];
    let (dir, _) = wall_frame(wall);
    let half = WALL_THICKNESS / 2.0;
    let near = 2.0; // world units
    let end = |p: Pos2, inward: Vec2| -> f32 {
        // (other wall's index, the direction it leaves the point in)
        let mut corners: Vec<(usize, Vec2)> = Vec::new();
        let mut through: Option<Vec2> = None;
        for (j, other) in walls.iter().enumerate() {
            if j == idx || other.start.distance(other.end) < 1e-3 {
                continue;
            }
            if p.distance(other.start) < near {
                corners.push((j, (other.end - other.start).normalized()));
            } else if p.distance(other.end) < near {
                corners.push((j, (other.start - other.end).normalized()));
            } else if p.distance(closest_point_on_segment(p, other.start, other.end)) < near {
                through = Some(wall_frame(other).0);
            }
        }
        // Leaving the same way as this wall: lying on top of it, nothing to join.
        corners.retain(|(_, d)| d.dot(inward) < 0.98);
        // Trimmed back to the face of a wall running off at `other`, so that this wall's
        // nearer edge just meets it.
        let stop_at = |other: Vec2| {
            let sin = inward.x * other.y - inward.y * other.x;
            let cos = inward.dot(other);
            -half * (1.0 - cos.abs()) / sin.abs().max(0.2)
        };
        if corners.iter().any(|(_, d)| d.dot(inward) < -0.98) {
            0.0
        } else if let Some(other) = through {
            stop_at(other)
        } else if corners.is_empty() {
            half
        } else {
            let owner = corners.iter().map(|c| c.0).min().unwrap_or(idx);
            if idx < owner {
                // Reach the outer corner: half a thickness times cot(angle / 2).
                corners
                    .iter()
                    .map(|(_, d)| {
                        let angle = d.dot(inward).clamp(-1.0, 1.0).acos();
                        half / (angle / 2.0).tan().max(0.33)
                    })
                    .fold(0.0, f32::max)
            } else {
                let owner_dir = corners.iter().find(|c| c.0 == owner).map(|c| c.1);
                owner_dir.map_or(half, stop_at)
            }
        }
    };
    (end(wall.start, dir), end(wall.end, -dir))
}

/// The ceiling height at a plan point: the height of the nearest wall, or the usual height
/// when there are no walls.
pub fn ceiling_height_at(walls: &[Wall], p: Pos2) -> f32 {
    walls
        .iter()
        .map(|w| {
            (
                closest_point_on_segment(p, w.start, w.end).distance(p),
                w.height,
            )
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(crate::app::WALL_HEIGHT, |(_, height)| height)
}

pub fn wall_face(walls: &[Wall], idx: usize, side: f32) -> Face {
    let wall = &walls[idx];
    let (dir, normal) = wall_frame(wall);
    let n = normal * side;
    let t = half_thickness();
    let length = wall.start.distance(wall.end);
    let origin = wall.start + n * t;

    // Where this face meets a face of a wall leaving the junction along `e`. `toward` points
    // from the junction into the stretch being measured; `near` picks the joining wall's
    // face on that side, otherwise the one on the far side (an outside corner).
    let hit = |j: &Junction, e: Vec2, toward: Vec2, near: bool| -> Option<f32> {
        let m = e.rot90();
        let denom = dir.dot(m);
        if denom.abs() < 1e-3 {
            return None;
        }
        let toward_sign = if m.dot(toward) >= 0.0 { 1.0 } else { -1.0 };
        let offset = m * t * if near { toward_sign } else { -toward_sign };
        let junction = wall.start + dir * j.u;
        Some((junction + offset - origin).dot(m) / denom)
    };
    // The end of the stretch leaving a junction towards `toward` (+dir or -dir).
    let cut = |j: &Junction, toward: Vec2, at_end: bool| -> Option<f32> {
        let forward = toward.dot(dir) > 0.0;
        let pick = |a: f32, b: f32| if forward { a.max(b) } else { a.min(b) };
        let blocking: Vec<f32> = j
            .dirs
            .iter()
            .filter(|e| e.dot(n) > 0.05)
            .filter_map(|&e| hit(j, e, toward, true))
            .collect();
        if let Some(&first) = blocking.first() {
            return Some(blocking.into_iter().fold(first, pick));
        }
        if !at_end {
            return None;
        }
        // Nothing joins on this side: an outside corner, the face runs on to the miter.
        let outside: Vec<f32> = j
            .dirs
            .iter()
            .filter(|e| e.dot(n) < -0.05)
            .filter_map(|&e| hit(j, e, toward, false))
            .collect();
        let first = *outside.first()?;
        Some(
            outside
                .into_iter()
                .fold(first, |a, b| if forward { a.min(b) } else { a.max(b) }),
        )
    };

    let mut start = 0.0;
    let mut end = length;
    let mut bounds = Vec::new();
    for j in junctions(walls, idx) {
        if j.u < 2.0 {
            start = cut(&j, dir, true).unwrap_or(0.0);
        } else if j.u > length - 2.0 {
            end = cut(&j, -dir, true).unwrap_or(length);
        } else if let (Some(before), Some(after)) = (cut(&j, -dir, false), cut(&j, dir, false)) {
            bounds.push((before, after));
        }
    }
    let mut pieces = Vec::new();
    let mut from = start;
    for (before, after) in bounds {
        pieces.push((from, before));
        from = after;
    }
    pieces.push((from, end));
    Face {
        side,
        start,
        end,
        pieces,
    }
}

/// The wall's clear length: its shorter face, which is the one inside a room.
pub fn clear_length(walls: &[Wall], idx: usize) -> f32 {
    wall_face(walls, idx, 1.0)
        .length()
        .min(wall_face(walls, idx, -1.0).length())
}

/// A room outline moved in from the wall center lines to the wall surfaces.
pub fn room_interior(polygon: &[Pos2]) -> Vec<Pos2> {
    let n = polygon.len();
    let signed_area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let t = half_thickness();
    let inward = |a: Pos2, b: Pos2| {
        let e = (b - a).normalized();
        let left = Vec2::new(-e.y, e.x);
        if signed_area > 0.0 {
            left
        } else {
            -left
        }
    };
    (0..n)
        .map(|i| {
            let (prev, p, next) = (polygon[(i + n - 1) % n], polygon[i], polygon[(i + 1) % n]);
            let (n1, n2) = (inward(prev, p), inward(p, next));
            crate::geometry::math::line_intersection(p + n1 * t, p - prev, p + n2 * t, next - p)
                .unwrap_or(p + n1 * t)
        })
        .collect()
}

/// Adjusts a wall move so the moved walls line up with corners and wall ends that stay
/// put, e.g. two short walls on opposite sides of a room ending up on one line. Along
/// `normal` when the move is kept to it, otherwise horizontally and vertically.
/// Returns the snapped move and guide lines for what it lined up with.
pub fn snap_wall_move(
    drag: &WallDrag,
    selected: &HashSet<usize>,
    stretch: bool,
    delta: Vec2,
    normal: Option<Vec2>,
    radius: f32,
) -> (Vec2, Vec<(Pos2, Pos2)>) {
    let walls = &drag.origin;
    let ends = |w: &Wall| [w.start, w.end];
    let moved: Vec<Pos2> = selected
        .iter()
        .filter_map(|&i| walls.get(i))
        .flat_map(ends)
        .collect();
    // Stretched walls move along, so they are nothing to line up with.
    let changing = |i: usize| {
        selected.contains(&i) || (stretch && drag.followers.iter().any(|&(f, _)| f == i))
    };
    let targets: Vec<Pos2> = (0..walls.len())
        .filter(|&i| !changing(i))
        .flat_map(|i| ends(&walls[i]))
        .collect();

    // The candidate along `axis` nearest the current move, if within reach.
    let snap_axis = |axis: Vec2, current: f32| -> Option<f32> {
        moved
            .iter()
            .flat_map(|&p| targets.iter().map(move |&q| (q - p).dot(axis)))
            .filter(|c| (c - current).abs() < radius)
            .min_by(|a, b| (a - current).abs().total_cmp(&(b - current).abs()))
    };
    let axes: Vec<Vec2> = match normal {
        Some(n) => vec![n],
        None => vec![Vec2::X, Vec2::Y],
    };
    let mut snapped = delta;
    for &axis in &axes {
        if let Some(c) = snap_axis(axis, delta.dot(axis)) {
            snapped += axis * (c - delta.dot(axis));
        }
    }

    let mut guides = Vec::new();
    for &axis in &axes {
        if snap_axis(axis, snapped.dot(axis)).is_none_or(|c| (c - snapped.dot(axis)).abs() > 0.01) {
            continue;
        }
        for &p in &moved {
            let p = p + snapped;
            // The farthest point on the same line, so the guide spans the whole alignment.
            let far = targets
                .iter()
                .filter(|&&q| (q - p).dot(axis).abs() < 0.01)
                .max_by(|a, b| a.distance(p).total_cmp(&b.distance(p)));
            if let Some(&q) = far {
                guides.push((p, q));
            }
        }
    }
    (snapped, guides)
}

/// Moves the selected walls by `delta`. With `stretch`, walls attached to them grow or
/// shrink to stay connected, resizing the rooms around them.
pub fn move_walls(
    drag: &WallDrag,
    selected: &HashSet<usize>,
    stretch: bool,
    delta: Vec2,
) -> Vec<Wall> {
    let mut walls = drag.origin.clone();
    for &idx in selected {
        if let Some(wall) = walls.get_mut(idx) {
            wall.start += delta;
            wall.end += delta;
        }
    }
    if !stretch {
        return walls;
    }

    let mut stretched = HashSet::new();
    for &(idx, is_start) in &drag.followers {
        let Some(wall) = walls.get_mut(idx) else {
            continue;
        };
        if is_start {
            wall.start += delta;
        } else {
            wall.end += delta;
        }
        stretched.insert(idx);
    }

    // Keep doors and windows where they were on the walls that changed length.
    for idx in stretched {
        let old = &drag.origin[idx];
        let wall = &mut walls[idx];
        let (dir, _) = wall_frame(wall);
        let length = wall.length();
        let start = wall.start;
        for opening in &mut wall.openings {
            let center = opening_center(old, opening);
            opening.offset = (center - start).dot(dir) / PIXELS_PER_METER;
            opening.offset = opening.clamped_offset(length);
        }
    }
    walls
}

#[cfg(test)]
mod end_tests {
    use super::*;

    #[test]
    fn walls_meet_without_overlapping() {
        let half = WALL_THICKNESS / 2.0;
        let walls = vec![
            // An L corner at (0, 0)…
            Wall::new(Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0)),
            Wall::new(Pos2::new(0.0, 0.0), Pos2::new(0.0, 200.0)),
            // …a wall ending halfway along the first one…
            Wall::new(Pos2::new(100.0, 0.0), Pos2::new(100.0, 100.0)),
            // …and the first one carried on in a straight line.
            Wall::new(Pos2::new(200.0, 0.0), Pos2::new(300.0, 0.0)),
        ];
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // The first wall owns the corner, the second stops at its face.
        assert!(close(wall_end_extensions(&walls, 0).0, half));
        assert!(close(wall_end_extensions(&walls, 1).0, -half));
        // Ending on a wall: stop at its face; the far end is free.
        let (start, end) = wall_end_extensions(&walls, 2);
        assert!(close(start, -half) && close(end, half), "{start} {end}");
        // Straight on: both stop at the shared point.
        assert!(close(wall_end_extensions(&walls, 0).1, 0.0));
        assert!(close(wall_end_extensions(&walls, 3).0, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::rooms::extract_rooms;
    use eframe::egui::vec2;

    fn wall(a: (f32, f32), b: (f32, f32)) -> Wall {
        Wall::new(Pos2::new(a.0, a.1), Pos2::new(b.0, b.1))
    }

    fn room_area(room: &[Pos2]) -> f32 {
        let mut area = 0.0;
        for i in 0..room.len() {
            let (a, b) = (room[i], room[(i + 1) % room.len()]);
            area += a.x * b.y - b.x * a.y;
        }
        area.abs() / 2.0
    }

    fn square() -> Vec<Wall> {
        vec![
            wall((0.0, 0.0), (200.0, 0.0)),
            wall((200.0, 0.0), (200.0, 200.0)),
            wall((200.0, 200.0), (0.0, 200.0)),
            wall((0.0, 200.0), (0.0, 0.0)),
        ]
    }

    #[test]
    fn moving_a_wall_out_grows_the_room() {
        let walls = square();
        let selected = HashSet::from([1]);
        let drag = WallDrag::new(&walls, &selected);
        assert_eq!(drag.followers.len(), 2);

        let moved = move_walls(&drag, &selected, true, vec2(100.0, 0.0));
        let rooms = extract_rooms(&moved);
        assert_eq!(rooms.len(), 1);
        assert!((room_area(&rooms[0]) - 300.0 * 200.0).abs() < 1.0);
    }

    #[test]
    fn moving_without_stretch_detaches_the_wall() {
        let walls = square();
        let selected = HashSet::from([1]);
        let drag = WallDrag::new(&walls, &selected);
        let moved = move_walls(&drag, &selected, false, vec2(50.0, 0.0));
        assert!(extract_rooms(&moved).is_empty());
    }

    #[test]
    fn t_joined_walls_follow_and_openings_stay_put() {
        let mut walls = square();
        walls.push(wall((100.0, 200.0), (100.0, 0.0)));
        walls[0].openings.push(crate::models::Opening {
            kind: crate::models::OpeningKind::Window,
            offset: 1.0,
            width: 0.8,
            height: 1.2,
            sill: 0.9,
            flip_hinge: false,
            flip_side: false,
        });
        let selected = HashSet::from([2]);
        let drag = WallDrag::new(&walls, &selected);
        let moved = move_walls(&drag, &selected, true, vec2(0.0, 50.0));

        assert_eq!(moved[4].start, Pos2::new(100.0, 250.0));
        assert_eq!(touching_walls(&walls, &selected), HashSet::from([1, 3, 4]));
        // Moving the partition affects the walls it ends on.
        assert_eq!(
            touching_walls(&walls, &HashSet::from([4])),
            HashSet::from([0, 2])
        );
        assert_eq!(extract_rooms(&moved).len(), 2);
        // The top wall is untouched, its window too.
        assert_eq!(moved[0].openings[0].offset, 1.0);
    }

    #[test]
    fn stretched_wall_keeps_its_window_in_place() {
        let mut walls = square();
        // The left wall runs bottom to top, so its start moves when the bottom wall does.
        walls[3].openings.push(crate::models::Opening {
            kind: crate::models::OpeningKind::Window,
            offset: 3.0,
            width: 0.8,
            height: 1.2,
            sill: 0.9,
            flip_hinge: false,
            flip_side: false,
        });
        let selected = HashSet::from([2]);
        let drag = WallDrag::new(&walls, &selected);
        let moved = move_walls(&drag, &selected, true, vec2(0.0, 50.0));
        assert!((moved[3].openings[0].offset - 4.0).abs() < 1e-4);
    }

    fn face_lengths(walls: &[Wall], idx: usize) -> (f32, f32) {
        let a = wall_face(walls, idx, 1.0).length();
        let b = wall_face(walls, idx, -1.0).length();
        (a.min(b), a.max(b))
    }

    #[test]
    fn room_walls_measure_between_wall_surfaces() {
        // 4 m × 4 m between centers, 10 cm walls: 3.9 m inside, 4.1 m outside.
        let walls = square_of(200.0);
        let (inside, outside) = face_lengths(&walls, 0);
        assert!((inside - 3.9).abs() < 1e-3, "{inside}");
        assert!((outside - 4.1).abs() < 1e-3, "{outside}");
        assert!((clear_length(&walls, 0) - 3.9).abs() < 1e-3);
    }

    #[test]
    fn a_partition_splits_only_the_face_it_touches() {
        let mut walls = square_of(200.0);
        walls.push(wall((100.0, 0.0), (100.0, 200.0)));
        let faces = [wall_face(&walls, 0, 1.0), wall_face(&walls, 0, -1.0)];
        let inner = faces.iter().find(|f| f.pieces.len() == 2).unwrap();
        let outer = faces.iter().find(|f| f.pieces.len() == 1).unwrap();
        // Each room is 2 m between centers, so 1.9 m clear.
        for (a, b) in &inner.pieces {
            assert!(((b - a) / PIXELS_PER_METER - 1.9).abs() < 1e-3);
        }
        assert!((outer.length() - 4.1).abs() < 1e-3);
        // The partition itself is trimmed at both ends by the walls it ends on.
        assert!((clear_length(&walls, 4) - 3.9).abs() < 1e-3);
    }

    #[test]
    fn free_standing_wall_measures_its_full_length() {
        let walls = vec![wall((0.0, 0.0), (150.0, 0.0))];
        assert_eq!(face_lengths(&walls, 0), (3.0, 3.0));
    }

    #[test]
    fn room_interior_shrinks_by_half_a_wall_each_side() {
        let room = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(200.0, 0.0),
            Pos2::new(200.0, 200.0),
            Pos2::new(0.0, 200.0),
        ];
        let inner = room_interior(&room);
        let t = half_thickness();
        assert!(inner.contains(&Pos2::new(t, t)));
        assert!(inner.contains(&Pos2::new(200.0 - t, 200.0 - t)));
    }

    #[test]
    fn a_short_wall_snaps_in_line_with_one_across_the_room() {
        // A room with a short wall sticking in from each long side, 6 px out of line.
        let mut walls = vec![
            wall((0.0, 0.0), (400.0, 0.0)),
            wall((400.0, 0.0), (400.0, 200.0)),
            wall((400.0, 200.0), (0.0, 200.0)),
            wall((0.0, 200.0), (0.0, 0.0)),
        ];
        walls.push(wall((200.0, 0.0), (200.0, 60.0)));
        walls.push(wall((210.0, 200.0), (210.0, 140.0)));
        let selected = HashSet::from([5]);
        let drag = WallDrag::new(&walls, &selected);
        let n = drag.normal.unwrap();
        let delta = n * (-4.0 * n.x.signum()); // 4 px towards the other wall, 6 px short

        let (snapped, guides) = snap_wall_move(&drag, &selected, true, delta, Some(n), 10.0);
        let moved = move_walls(&drag, &selected, true, snapped);
        assert!((moved[5].start.x - 200.0).abs() < 1e-3);
        assert!(!guides.is_empty());

        // Too far off to snap.
        let far = n * (-30.0 * n.x.signum());
        let (snapped, _) = snap_wall_move(&drag, &selected, true, far, Some(n), 10.0);
        assert_eq!(snapped, far);
    }

    fn square_of(size: f32) -> Vec<Wall> {
        vec![
            wall((0.0, 0.0), (size, 0.0)),
            wall((size, 0.0), (size, size)),
            wall((size, size), (0.0, size)),
            wall((0.0, size), (0.0, 0.0)),
        ]
    }
}
