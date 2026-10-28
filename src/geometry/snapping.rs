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
            return (proj, vec![], Some(idx));
        }
    }

    let mut locked_dir = None;

    if let Some(start) = start_pos {
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

        let base_angle = base_angle_opt.unwrap_or(0.0);
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

    fn corner_walls() -> Vec<Wall> {
        vec![
            Wall::new(Pos2::new(0.0, 0.0), Pos2::new(400.0, 0.0)),
            Wall::new(Pos2::new(0.0, 300.0), Pos2::new(0.0, 0.0)),
        ]
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
