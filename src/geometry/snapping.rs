use crate::{
    geometry::math::{closest_point_on_segment, line_intersection},
    models::Wall,
};
use eframe::egui::{vec2, Pos2};

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
