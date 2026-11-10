use crate::{
    app::{PIXELS_PER_METER, WALL_THICKNESS},
    geometry::math::{closest_point_on_segment, line_intersection},
    models::{Furniture, Wall},
};
use eframe::egui::{emath::Rot2, vec2, Pos2, Vec2};

pub fn compute_snapping(
    raw_pos: Pos2,
    start_pos: Option<Pos2>,
    walls: &[Wall],
    ignore_indices: &[usize],
    zoom: f32,
) -> (Pos2, Vec<Pos2>, Option<usize>) {
    let mut final_pos = raw_pos;
    let mut alignments = Vec::new();
    let snap_rad = 15.0 / zoom;
    let align_thresh = 8.0 / zoom;

    for (idx, wall) in walls.iter().enumerate() {
        if ignore_indices.contains(&idx) {
            continue;
        }
        if final_pos.distance(wall.start) < snap_rad {
            return (wall.start, vec![], Some(idx));
        }
        if final_pos.distance(wall.end) < snap_rad {
            return (wall.end, vec![], Some(idx));
        }
    }

    for (idx, wall) in walls.iter().enumerate() {
        if ignore_indices.contains(&idx) {
            continue;
        }
        let proj = closest_point_on_segment(final_pos, wall.start, wall.end);
        if final_pos.distance(proj) < snap_rad {
            let square = start_pos.and_then(|start| {
                square_landing(start, final_pos, wall, walls, ignore_indices, snap_rad)
            });
            return (square.unwrap_or(proj), vec![], Some(idx));
        }
    }

    let mut locked_dir = None;

    if let Some(start) = start_pos {
        let base_angle = base_angle(start, walls, ignore_indices);
        let dir = final_pos - start;
        let len = dir.length();

        if len > (5.0 / zoom) {
            let current_angle = dir.angle();
            let rel_angle = current_angle - base_angle;
            let snap_step = std::f32::consts::PI / 2.0;
            let snapped_rel_angle = (rel_angle / snap_step).round() * snap_step;
            let snapped_absolute_angle = base_angle + snapped_rel_angle;

            if (rel_angle - snapped_rel_angle).abs() < (std::f32::consts::PI / 36.0) {
                let snap_dir = vec2(snapped_absolute_angle.cos(), snapped_absolute_angle.sin());
                final_pos = start + snap_dir * len;
                locked_dir = Some(snap_dir);
            }
        }
    }

    let mut inference_lines = Vec::new();
    for (idx, wall) in walls.iter().enumerate() {
        if ignore_indices.contains(&idx) {
            continue;
        }
        for &ep in &[wall.start, wall.end] {
            inference_lines.push((ep, vec2(1.0, 0.0)));
            inference_lines.push((ep, vec2(0.0, 1.0)));
            if wall.start.distance(wall.end) > 0.1 {
                let wall_dir = (wall.end - wall.start).normalized();
                inference_lines.push((ep, wall_dir));
                inference_lines.push((ep, vec2(-wall_dir.y, wall_dir.x)));
            }
        }
    }

    let mut snapped_inference = false;
    for (ep, inf_dir) in inference_lines {
        if snapped_inference {
            break;
        }
        if let Some(start) = start_pos {
            if ep.distance(start) < 2.0 {
                continue;
            }
        }
        if ep.distance(final_pos) < 2.0 {
            continue;
        }

        if let Some(l_dir) = locked_dir {
            if let Some(start) = start_pos {
                if let Some(intersect) = line_intersection(start, l_dir, ep, inf_dir) {
                    if final_pos.distance(intersect) < align_thresh
                        && (intersect - start).dot(l_dir) > 0.0
                    {
                        final_pos = intersect;
                        alignments.push(ep);
                        snapped_inference = true;
                    }
                }
            }
        } else {
            let to_point = final_pos - ep;
            let proj_length = to_point.dot(inf_dir);
            let proj_point = ep + inf_dir * proj_length;

            if final_pos.distance(proj_point) < align_thresh {
                final_pos = proj_point;
                alignments.push(ep);
                snapped_inference = true;
            }
        }
    }

    (final_pos, alignments, None)
}

/// The direction a wall drawn from `start` measures its 90° steps from: the wall it
/// continues, or the wall it starts on.
fn base_angle(start: Pos2, walls: &[Wall], ignore_indices: &[usize]) -> f32 {
    let mut base_angle_opt = None;
    for (idx, wall) in walls.iter().enumerate() {
        if ignore_indices.contains(&idx) {
            continue;
        }
        if wall.end.distance(start) < 2.0 {
            base_angle_opt = Some((start - wall.start).angle());
            break;
        } else if wall.start.distance(start) < 2.0 {
            base_angle_opt = Some((start - wall.end).angle());
            break;
        }
    }

    if base_angle_opt.is_none() {
        for (idx, wall) in walls.iter().enumerate() {
            if ignore_indices.contains(&idx) {
                continue;
            }
            let proj = closest_point_on_segment(start, wall.start, wall.end);
            if proj.distance(start) < 2.0 {
                base_angle_opt = Some((wall.end - wall.start).angle());
                break;
            }
        }
    }
    base_angle_opt.unwrap_or(0.0)
}

/// Where a wall drawn from `start` should land on `target` near `raw` so it meets
/// square: at a right angle to the target itself, or along the 90° steps from the wall
/// it continues. The closest of those to the pointer, if any is close enough.
fn square_landing(
    start: Pos2,
    raw: Pos2,
    target: &Wall,
    walls: &[Wall],
    ignore_indices: &[usize],
    snap_rad: f32,
) -> Option<Pos2> {
    let along = target.end - target.start;
    if along.length() < 1e-3 || raw.distance(start) < 1e-3 {
        return None;
    }
    let mut candidates = vec![closest_point_on_segment(start, target.start, target.end)];
    let base = base_angle(start, walls, ignore_indices);
    for quarter in 0..4 {
        let angle = base + quarter as f32 * std::f32::consts::FRAC_PI_2;
        let dir = vec2(angle.cos(), angle.sin());
        if let Some(hit) = line_intersection(start, dir, target.start, along) {
            if (hit - start).dot(dir) > 0.0 {
                candidates.push(hit);
            }
        }
    }
    let max_turn = 10f32.to_radians();
    candidates
        .into_iter()
        .filter(|&c| {
            // On the target segment, near the pointer, and not a sharp turn away from it.
            c.distance(closest_point_on_segment(c, target.start, target.end)) < 0.5
                && c.distance(raw) < snap_rad
                && c.distance(start) > 1.0
                && (c - start).normalized().dot((raw - start).normalized()) > max_turn.cos()
        })
        .min_by(|a, b| a.distance(raw).total_cmp(&b.distance(raw)))
}

pub fn get_hovered_endpoints(walls: &[Wall], pointer: Pos2, zoom: f32) -> Vec<(usize, bool)> {
    let mut in_radius = Vec::new();
    let hover_rad = 15.0 / zoom;

    for (idx, wall) in walls.iter().enumerate() {
        let d_start = pointer.distance(wall.start);
        if d_start < hover_rad {
            in_radius.push((idx, true, d_start));
        }
        let d_end = pointer.distance(wall.end);
        if d_end < hover_rad {
            in_radius.push((idx, false, d_end));
        }
    }

    if in_radius.is_empty() {
        return vec![];
    }
    in_radius.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

    let closest_pos = if in_radius[0].1 {
        walls[in_radius[0].0].start
    } else {
        walls[in_radius[0].0].end
    };
    let closest_dist = in_radius[0].2;

    let mut sharing_closest = Vec::new();
    for &(idx, is_start, _) in &in_radius {
        let pos = if is_start {
            walls[idx].start
        } else {
            walls[idx].end
        };
        if pos.distance(closest_pos) < 2.0 {
            sharing_closest.push((idx, is_start));
        }
    }

    if sharing_closest.len() > 1 {
        if closest_dist < (6.0 / zoom) {
            return sharing_closest;
        } else {
            let mut best_idx = sharing_closest[0];
            let mut min_line_dist = f32::MAX;
            for &(idx, is_start) in &sharing_closest {
                let proj = closest_point_on_segment(pointer, walls[idx].start, walls[idx].end);
                let dist = pointer.distance(proj);
                if dist < min_line_dist {
                    min_line_dist = dist;
                    best_idx = (idx, is_start);
                }
            }
            return vec![best_idx];
        }
    }
    sharing_closest
}

pub fn find_closest_wall(walls: &[Wall], pointer: Pos2, zoom: f32) -> Option<usize> {
    let mut closest_idx = None;
    let mut min_dist = 12.0 / zoom;
    for (idx, wall) in walls.iter().enumerate() {
        let proj = closest_point_on_segment(pointer, wall.start, wall.end);
        if pointer.distance(proj) < min_dist {
            min_dist = pointer.distance(proj);
            closest_idx = Some(idx);
        }
    }
    closest_idx
}

/// Furniture only snaps to walls it is already within this angle of being parallel to.
const FURNITURE_ALIGN_TOLERANCE: f32 = 5.0 * std::f32::consts::PI / 180.0;

pub struct FurnitureSnap {
    pub position: Pos2,
    pub rotation: f32,
    pub walls: Vec<usize>,
}

/// Pushes a piece of furniture flush against the nearest parallel wall, then against a
/// perpendicular one so it can settle into a corner. The rotation is corrected to be
/// exactly parallel to the first wall.
pub fn snap_furniture(item: &Furniture, walls: &[Wall], zoom: f32) -> FurnitureSnap {
    let snap_dist = 15.0 / zoom;
    let mut snap = FurnitureSnap {
        position: item.position,
        rotation: item.rotation,
        walls: Vec::new(),
    };
    let mut first_normal: Option<Vec2> = None;

    for _ in 0..2 {
        // (correction length, correction, rotation, wall index, wall normal)
        let mut best: Option<(f32, Vec2, f32, usize, Vec2)> = None;

        for (idx, wall) in walls.iter().enumerate() {
            let along = wall.end - wall.start;
            let len = along.length();
            if len < 1e-3 {
                continue;
            }
            let dir = along / len;
            let normal = dir.rot90();
            if first_normal.is_some_and(|n| n.dot(normal).abs() > 0.1) {
                continue;
            }

            let misalignment = wrap_to_quarter_turn(dir.angle() - snap.rotation);
            if misalignment.abs() > FURNITURE_ALIGN_TOLERANCE {
                continue;
            }
            let rotation = snap.rotation + misalignment;

            // Half extents of the rotated footprint across and along the wall.
            let rot = Rot2::from_angle(rotation);
            let (axis_x, axis_z) = (rot * vec2(1.0, 0.0), rot * vec2(0.0, 1.0));
            let (hw, hd) = (
                item.width / 2.0 * PIXELS_PER_METER,
                item.depth / 2.0 * PIXELS_PER_METER,
            );
            let half_across = axis_x.dot(normal).abs() * hw + axis_z.dot(normal).abs() * hd;
            let half_along = axis_x.dot(dir).abs() * hw + axis_z.dot(dir).abs() * hd;

            let rel = snap.position - wall.start;
            let (across, along) = (rel.dot(normal), rel.dot(dir));
            if along + half_along < 0.0 || along - half_along > len {
                continue;
            }

            // Flush against the wall's surface, not its center line.
            let gap = half_across + WALL_THICKNESS / 2.0 * PIXELS_PER_METER;
            let target = if across >= 0.0 { gap } else { -gap };
            let correction = target - across;
            if correction.abs() > snap_dist {
                continue;
            }
            if best.is_none_or(|b| correction.abs() < b.0) {
                best = Some((correction.abs(), normal * correction, rotation, idx, normal));
            }
        }

        let Some((_, correction, rotation, idx, normal)) = best else {
            break;
        };
        snap.position += correction;
        snap.rotation = rotation;
        snap.walls.push(idx);
        first_normal = Some(normal);
    }

    snap
}

/// Another piece seen in the axes of the piece being moved: its center and half size, in
/// plan units.
struct Neighbor {
    center: Vec2,
    half: Vec2,
}

/// The pieces that stand square to a frame at `origin` turned by `rotation`, and that
/// `item` could stand against: on the same level, and not a rug under it.
fn square_neighbors<'a>(
    item: &Furniture,
    origin: Pos2,
    rotation: f32,
    others: impl IntoIterator<Item = &'a Furniture>,
) -> Vec<Neighbor> {
    let inverse = Rot2::from_angle(-rotation);
    let flat = |f: &Furniture| f.height < 0.05;
    others
        .into_iter()
        .filter(|o| o.elevation < item.top() && item.elevation < o.top())
        .filter(|o| flat(o) == flat(item))
        .filter_map(|o| {
            let turn = o.rotation - rotation;
            if wrap_to_quarter_turn(turn).abs() > 1f32.to_radians() {
                return None;
            }
            let sideways = (turn / std::f32::consts::FRAC_PI_2).round() as i32 % 2 != 0;
            let (w, d) = if sideways {
                (o.depth, o.width)
            } else {
                (o.width, o.depth)
            };
            Some(Neighbor {
                center: inverse * (o.position - origin),
                half: vec2(w, d) / 2.0 * PIXELS_PER_METER,
            })
        })
        .collect()
}

/// How far apart two pieces may be for their edges to still line up with each other.
const ALIGN_REACH: f32 = 1.5 * PIXELS_PER_METER;

/// The result of [`snap_to_furniture`]: where the piece goes, and the lines it snapped to.
pub struct NeighborSnap {
    pub position: Pos2,
    pub guides: Vec<(Pos2, Pos2)>,
}

/// Moves a piece so it stands flush against nearby furniture, or lines up with its edges or
/// middle. Only pieces standing square to it count. `locked` holds directions the piece may
/// not move in, e.g. the normals of the walls it already snapped to.
pub fn snap_to_furniture<'a>(
    item: &Furniture,
    others: impl IntoIterator<Item = &'a Furniture>,
    locked: &[Vec2],
    zoom: f32,
) -> NeighborSnap {
    let snap_dist = 10.0 / zoom;
    let rot = Rot2::from_angle(item.rotation);
    let half = vec2(item.width, item.depth) / 2.0 * PIXELS_PER_METER;
    let neighbors = square_neighbors(item, item.position, item.rotation, others);
    let mut offset = Vec2::ZERO; // how far the piece moved, in its own axes
    let mut guides = Vec::new();

    for axis in 0..2 {
        let other_axis = 1 - axis;
        let direction = rot
            * if axis == 0 {
                vec2(1.0, 0.0)
            } else {
                vec2(0.0, 1.0)
            };
        if locked.iter().any(|n| n.dot(direction).abs() > 0.5) {
            continue;
        }
        // (preference, distance to move, the line snapped to, the neighbor)
        let mut best: Option<(f32, f32, f32, &Neighbor)> = None;
        for n in &neighbors {
            let c = n.center[axis] - offset[axis];
            let gap = (n.center[other_axis] - offset[other_axis]).abs()
                - half[other_axis]
                - n.half[other_axis];
            let (h, nh) = (half[axis], n.half[axis]);
            // Flush: side by side, when the pieces overlap along the other axis.
            let flush = [(c - nh - h, c - nh), (c + nh + h, c + nh)];
            // In line: matching edges or middles, when they are near each other.
            let in_line = [(c - nh + h, c - nh), (c + nh - h, c + nh), (c, c)];
            let candidates = flush
                .iter()
                .map(|m| (0.0, m, gap < 0.0))
                .chain(in_line.iter().map(|m| (0.5, m, gap < ALIGN_REACH)));
            for (penalty, &(shift, line), allowed) in candidates {
                // Flush wins from an equally near alignment.
                let score = shift.abs() + penalty;
                if allowed && shift.abs() <= snap_dist && best.is_none_or(|b| score < b.0) {
                    best = Some((score, shift, line, n));
                }
            }
        }
        if let Some((_, shift, line, n)) = best {
            offset[axis] += shift;
            // A guide along the line, covering both pieces.
            let mine = offset[other_axis];
            let theirs = n.center[other_axis];
            let from = (mine - half[other_axis]).min(theirs - n.half[other_axis]);
            let to = (mine + half[other_axis]).max(theirs + n.half[other_axis]);
            let point = |along: f32| {
                let mut p = Vec2::ZERO;
                p[axis] = line;
                p[other_axis] = along;
                item.position + rot * p
            };
            guides.push((point(from), point(to)));
        }
    }
    NeighborSnap {
        position: item.position + rot * offset,
        guides,
    }
}

/// The piece being resized, as it was when the drag started. Its own axes are the frame the
/// edges are measured in: plan units from `origin`, turned by `rotation`.
pub struct ResizeFrame<'a> {
    /// The piece itself, for telling which neighbors it can stand against.
    pub item: &'a Furniture,
    pub origin: Pos2,
    pub rotation: f32,
    /// Half its width and depth before the resize, in plan units.
    pub half: Vec2,
}

/// Snaps an edge being dragged along `axis` (0 = the piece's width, 1 = its depth) from
/// `edge` to the nearest edge of other furniture standing square to it, or the surface of a
/// wall running along it. Returns the new place of the edge and a guide line to show.
pub fn snap_resized_edge<'a>(
    frame: &ResizeFrame,
    others: impl IntoIterator<Item = &'a Furniture>,
    walls: &[Wall],
    axis: usize,
    edge: f32,
    zoom: f32,
) -> Option<(f32, (Pos2, Pos2))> {
    let snap_dist = 10.0 / zoom;
    let other_axis = 1 - axis;
    let rot = Rot2::from_angle(frame.rotation);
    let half = frame.half[other_axis];
    // (the line to snap to, and how far it reaches along the other axis)
    let mut lines: Vec<(f32, f32, f32)> = Vec::new();
    for n in square_neighbors(frame.item, frame.origin, frame.rotation, others) {
        let (c, h) = (n.center[other_axis], n.half[other_axis]);
        if c.abs() - h - half < ALIGN_REACH {
            lines.push((n.center[axis] - n.half[axis], c - h, c + h));
            lines.push((n.center[axis] + n.half[axis], c - h, c + h));
        }
    }
    let wall_half = WALL_THICKNESS / 2.0 * PIXELS_PER_METER;
    for wall in walls {
        let a = rot.inverse() * (wall.start - frame.origin);
        let b = rot.inverse() * (wall.end - frame.origin);
        let (from, to) = (
            a[other_axis].min(b[other_axis]),
            a[other_axis].max(b[other_axis]),
        );
        // Only walls running along the edge, next to the piece.
        if (a[axis] - b[axis]).abs() > 0.02 * (to - from) || to < -half || from > half {
            continue;
        }
        let center = (a[axis] + b[axis]) / 2.0;
        lines.push((center - wall_half, from, to));
        lines.push((center + wall_half, from, to));
    }

    let (line, from, to) = lines
        .into_iter()
        .filter(|l| (l.0 - edge).abs() <= snap_dist)
        .min_by(|a, b| (a.0 - edge).abs().total_cmp(&(b.0 - edge).abs()))?;
    let point = |along: f32| {
        let mut p = Vec2::ZERO;
        p[axis] = line;
        p[other_axis] = along;
        frame.origin + rot * p
    };
    Some((line, (point(from.min(-half)), point(to.max(half)))))
}

/// Wraps an angle into [-45°, 45°), i.e. the difference to the nearest multiple of 90°.
fn wrap_to_quarter_turn(angle: f32) -> f32 {
    use std::f32::consts::FRAC_PI_2;
    (angle + FRAC_PI_2 / 2.0).rem_euclid(FRAC_PI_2) - FRAC_PI_2 / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::PRESETS;

    fn table_at(x: f32, y: f32, rotation_deg: f32) -> Furniture {
        let mut table = (PRESETS
            .iter()
            .find(|p| p.label == "Coffee table")
            .unwrap()
            .build)();
        [table.width, table.depth] = [1.4, 0.8]; // 70 × 40 px
        table.position = Pos2::new(x, y);
        table.rotation = rotation_deg.to_radians();
        table
    }

    #[test]
    fn furniture_stands_flush_against_and_in_line_with_other_furniture() {
        // A 70 × 40 table; another one just right of it, a little lower.
        let fixed = table_at(100.0, 100.0, 0.0);
        let moving = table_at(174.0, 104.0, 0.0);
        let snap = snap_to_furniture(&moving, [&fixed], &[], 1.0);
        // Flush against its right side, and top edges in line.
        assert!(
            (snap.position - Pos2::new(170.0, 100.0)).length() < 1e-3,
            "{:?}",
            snap.position
        );
        assert_eq!(snap.guides.len(), 2);

        // Turned a quarter, it is 40 wide: flush at 100 + 35 + 20.
        let turned = table_at(160.0, 150.0, 90.0);
        let snap = snap_to_furniture(&turned, [&fixed], &[], 1.0);
        assert!(
            (snap.position.x - 155.0).abs() < 1e-3,
            "{:?}",
            snap.position
        );

        // Far away, at an odd angle, or held by a wall: left alone.
        for (item, locked) in [
            (table_at(300.0, 104.0, 0.0), vec![]),
            (table_at(174.0, 104.0, 30.0), vec![]),
            (
                table_at(174.0, 160.0, 0.0),
                vec![vec2(1.0, 0.0), vec2(0.0, 1.0)],
            ),
        ] {
            let snap = snap_to_furniture(&item, [&fixed], &locked, 1.0);
            assert_eq!(snap.position, item.position);
        }
    }

    #[test]
    fn a_resized_edge_snaps_to_furniture_and_wall_surfaces() {
        // A 70 × 40 table at the origin; its right edge is dragged from x = 35.
        let item = table_at(0.0, 0.0, 0.0);
        let frame = ResizeFrame {
            item: &item,
            origin: item.position,
            rotation: 0.0,
            half: vec2(35.0, 20.0),
        };
        // Another table whose left edge is at 100 - 35 = 65, and a wall along x = 120
        // with its surface at 117.5.
        let other = table_at(100.0, 10.0, 0.0);
        let walls = [Wall::new(Pos2::new(120.0, -100.0), Pos2::new(120.0, 100.0))];
        let snap =
            |edge: f32| snap_resized_edge(&frame, [&other], &walls, 0, edge, 1.0).map(|s| s.0);
        assert_eq!(snap(60.0), Some(65.0));
        assert_eq!(snap(112.0), Some(117.5));
        assert_eq!(snap(85.0), None);
        // Dragging the depth: the wall runs the other way and the table is out of line.
        assert!(snap_resized_edge(&frame, [&other], &walls, 1, 45.0, 1.0).is_none());
    }

    fn corner_walls() -> Vec<Wall> {
        vec![
            Wall::new(Pos2::new(0.0, 0.0), Pos2::new(400.0, 0.0)),
            Wall::new(Pos2::new(0.0, 300.0), Pos2::new(0.0, 0.0)),
        ]
    }

    #[test]
    fn a_wall_ending_on_another_meets_it_square() {
        // Drawing up towards the horizontal wall from (100, 200), the pointer a bit off to
        // the side: the end lands straight above the start.
        let walls = corner_walls();
        let (end, _, wall) = compute_snapping(
            Pos2::new(106.0, 8.0),
            Some(Pos2::new(100.0, 200.0)),
            &walls,
            &[],
            1.0,
        );
        assert_eq!(wall, Some(0));
        assert!((end - Pos2::new(100.0, 0.0)).length() < 1e-3, "{end:?}");
    }

    #[test]
    fn a_wall_keeps_its_angle_steps_when_landing_on_another() {
        // Continuing a slanted wall straight on until it meets a long horizontal wall.
        let walls = vec![
            Wall::new(Pos2::new(0.0, 0.0), Pos2::new(600.0, 0.0)),
            Wall::new(Pos2::new(100.0, 300.0), Pos2::new(200.0, 200.0)),
        ];
        let start = Pos2::new(200.0, 200.0);
        let (end, _, wall) = compute_snapping(Pos2::new(404.0, 5.0), Some(start), &walls, &[], 1.0);
        assert_eq!(wall, Some(0));
        assert!((end - Pos2::new(400.0, 0.0)).length() < 1e-3, "{end:?}");
    }

    #[test]
    fn a_wall_far_off_square_lands_where_pointed() {
        let walls = corner_walls();
        let start = Pos2::new(200.0, 200.0);
        let (end, _, _) = compute_snapping(Pos2::new(105.0, 4.0), Some(start), &walls, &[], 1.0);
        assert!((end - Pos2::new(105.0, 0.0)).length() < 1e-3, "{end:?}");
    }

    #[test]
    fn snaps_flush_to_a_wall() {
        let snap = snap_furniture(&table_at(200.0, 25.0, 0.0), &corner_walls(), 1.0);
        assert_eq!(snap.walls, vec![0]);
        assert!((snap.position - Pos2::new(200.0, 22.5)).length() < 1e-3);
    }

    #[test]
    fn snaps_into_a_corner_and_straightens() {
        let snap = snap_furniture(&table_at(40.0, 25.0, 3.0), &corner_walls(), 1.0);
        assert_eq!(snap.walls.len(), 2);
        assert!(wrap_to_quarter_turn(snap.rotation).abs() < 1e-4);
        assert!((snap.position - Pos2::new(37.5, 22.5)).length() < 1e-3);
    }

    #[test]
    fn rotated_piece_uses_its_rotated_extent() {
        // Rotated 90°, the 70 px width now runs across the wall.
        let snap = snap_furniture(&table_at(200.0, 40.0, 90.0), &corner_walls(), 1.0);
        assert!((snap.position - Pos2::new(200.0, 37.5)).length() < 1e-3);
    }

    #[test]
    fn ignores_far_or_skewed_walls() {
        let far = snap_furniture(&table_at(200.0, 60.0, 0.0), &corner_walls(), 1.0);
        assert!(far.walls.is_empty());
        let skewed = snap_furniture(&table_at(200.0, 25.0, 20.0), &corner_walls(), 1.0);
        assert!(skewed.walls.is_empty());
    }
}
