use crate::models::Wall;
use eframe::egui;
use egui::{Pos2, vec2};
use std::collections::HashSet;

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

        if let Some(base_angle) = base_angle_opt {
            let dir = final_pos - start;
            let len = dir.length();
            if len > (5.0 / zoom) {
                let current_angle = dir.angle();
                let rel_angle = current_angle - base_angle;

                let snap_step = std::f32::consts::PI / 2.0;
                let snapped_rel_angle = (rel_angle / snap_step).round() * snap_step;
                let snapped_absolute_angle = base_angle + snapped_rel_angle;

                if (rel_angle - snapped_rel_angle).abs() < (std::f32::consts::PI / 36.0) {
                    final_pos = start
                        + vec2(snapped_absolute_angle.cos(), snapped_absolute_angle.sin()) * len;
                }
            }
        }
    }

    let mut snapped_x = false;
    let mut snapped_y = false;

    for (idx, wall) in walls.iter().enumerate() {
        if ignore_indices.contains(&idx) {
            continue;
        }
        for &ep in &[wall.start, wall.end] {
            if !snapped_x && (final_pos.x - ep.x).abs() < align_thresh {
                final_pos.x = ep.x;
                alignments.push(ep);
                snapped_x = true;
            }
            if !snapped_y && (final_pos.y - ep.y).abs() < align_thresh {
                final_pos.y = ep.y;
                alignments.push(ep);
                snapped_y = true;
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

pub fn closest_point_on_segment(p: Pos2, a: Pos2, b: Pos2) -> Pos2 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq == 0.0 {
        return a;
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
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

pub fn extract_rooms(walls: &[Wall]) -> Vec<Vec<Pos2>> {
    let mut vertices: Vec<Pos2> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    let get_or_add_vertex = |p: Pos2, verts: &mut Vec<Pos2>| -> usize {
        if let Some(idx) = verts.iter().position(|v| v.distance(p) < 2.0) {
            idx
        } else {
            verts.push(p);
            verts.len() - 1
        }
    };

    for w in walls {
        let mut segment_points = vec![w.start, w.end];
        for other_w in walls {
            for &ep in &[other_w.start, other_w.end] {
                let proj = closest_point_on_segment(ep, w.start, w.end);
                if ep.distance(proj) < 2.0 {
                    segment_points.push(proj);
                }
            }
        }
        segment_points.sort_by(|a, b| {
            w.start
                .distance(*a)
                .partial_cmp(&w.start.distance(*b))
                .unwrap()
        });
        segment_points.dedup_by(|a, b| a.distance(*b) < 2.0);

        for i in 0..segment_points.len().saturating_sub(1) {
            let u = get_or_add_vertex(segment_points[i], &mut vertices);
            let v = get_or_add_vertex(segment_points[i + 1], &mut vertices);
            if u != v {
                edges.push((u, v));
                edges.push((v, u));
            }
        }
    }

    let mut rooms = Vec::new();
    let mut visited_edges = HashSet::new();

    for &(start_u, start_v) in &edges {
        if visited_edges.contains(&(start_u, start_v)) {
            continue;
        }

        let mut polygon = Vec::new();
        let mut curr_u = start_u;
        let mut curr_v = start_v;
        let mut is_valid_cycle = true;

        loop {
            visited_edges.insert((curr_u, curr_v));
            polygon.push(vertices[curr_u]);

            if curr_v == start_u {
                break;
            }
            if polygon.contains(&vertices[curr_v]) {
                is_valid_cycle = false;
                break;
            }

            let in_dir = vertices[curr_v] - vertices[curr_u];
            let in_angle = in_dir.angle();

            let mut best_w = None;
            let mut max_angle_diff = -1.0;

            for &(from, to) in &edges {
                if from == curr_v && to != curr_u {
                    let out_dir = vertices[to] - vertices[curr_v];
                    let out_angle = out_dir.angle();
                    let mut diff = out_angle - in_angle;
                    while diff < 0.0 {
                        diff += std::f32::consts::TAU;
                    }
                    while diff >= std::f32::consts::TAU {
                        diff -= std::f32::consts::TAU;
                    }

                    if diff > max_angle_diff {
                        max_angle_diff = diff;
                        best_w = Some(to);
                    }
                }
            }

            if let Some(w) = best_w {
                curr_u = curr_v;
                curr_v = w;
            } else {
                is_valid_cycle = false;
                break;
            }
        }

        if is_valid_cycle && polygon.len() >= 3 {
            let mut area = 0.0;
            for i in 0..polygon.len() {
                let p1 = polygon[i];
                let p2 = polygon[(i + 1) % polygon.len()];
                area += (p2.x - p1.x) * (p2.y + p1.y);
            }
            if area > 2500.0 {
                rooms.push(polygon);
            }
        }
    }

    rooms
}

pub fn triangulate(polygon: &[Pos2]) -> Vec<[Pos2; 3]> {
    let mut triangles = Vec::new();
    if polygon.len() < 3 {
        return triangles;
    }

    let mut verts = polygon.to_vec();

    let mut area = 0.0;
    for i in 0..verts.len() {
        let p1 = verts[i];
        let p2 = verts[(i + 1) % verts.len()];
        area += (p2.x - p1.x) * (p2.y + p1.y);
    }

    if area > 0.0 {
        verts.reverse();
    }

    let mut watchdog = 1000;
    while verts.len() > 3 && watchdog > 0 {
        watchdog -= 1;
        let mut ear_found = false;

        for i in 0..verts.len() {
            let prev = verts[(i + verts.len() - 1) % verts.len()];
            let curr = verts[i];
            let next = verts[(i + 1) % verts.len()];

            let v1 = curr - prev;
            let v2 = next - curr;
            let cross = v1.x * v2.y - v1.y * v2.x;

            if cross <= 0.0 {
                continue;
            }

            let mut is_ear = true;
            for j in 0..verts.len() {
                if j == (i + verts.len() - 1) % verts.len() || j == i || j == (i + 1) % verts.len()
                {
                    continue;
                }
                if point_in_triangle(verts[j], prev, curr, next) {
                    is_ear = false;
                    break;
                }
            }

            if is_ear {
                triangles.push([prev, curr, next]);
                verts.remove(i);
                ear_found = true;
                break;
            }
        }
        if !ear_found {
            break;
        }
    }

    if verts.len() == 3 {
        triangles.push([verts[0], verts[1], verts[2]]);
    }
    triangles
}

fn point_in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let v0 = c - a;
    let v1 = b - a;
    let v2 = p - a;

    let dot00 = v0.x * v0.x + v0.y * v0.y;
    let dot01 = v0.x * v1.x + v0.y * v1.y;
    let dot02 = v0.x * v2.x + v0.y * v2.y;
    let dot11 = v1.x * v1.x + v1.y * v1.y;
    let dot12 = v1.x * v2.x + v1.y * v2.y;

    let inv_denom = 1.0 / (dot00 * dot11 - dot01 * dot01);
    let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
    let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;

    (u >= 0.0) && (v >= 0.0) && (u + v < 1.0)
}
