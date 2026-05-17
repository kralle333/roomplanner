use crate::{
    geometry::math::{closest_point_on_segment, point_in_triangle},
    models::Wall,
};
use eframe::egui::Pos2;
use std::collections::HashSet;

pub fn extract_rooms(walls: &[Wall]) -> Vec<Vec<Pos2>> {
    let mut vertices: Vec<Pos2> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    let get_or_add_vertex = |p: Pos2, verts: &mut Vec<Pos2>| -> usize {
        if let Some(idx) = verts.iter().position(|v: &Pos2| v.distance(p) < 2.0) {
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
                    let mut diff = out_dir.angle() - in_angle;
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
