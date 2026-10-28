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

    // Trace the faces of the planar wall graph: from every directed edge, keep taking the
    // sharpest right turn (measured from the edge we arrived along) until we are back at
    // the starting edge. Each directed edge belongs to exactly one face; enclosed rooms
    // come out clockwise on screen, the outside of the plan counter-clockwise.
    let mut rooms = Vec::new();
    let mut visited_edges = HashSet::new();

    for &(start_u, start_v) in &edges {
        if visited_edges.contains(&(start_u, start_v)) {
            continue;
        }

        let mut polygon = Vec::new();
        let (mut curr_u, mut curr_v) = (start_u, start_v);
        let mut closed = false;

        for _ in 0..edges.len() {
            visited_edges.insert((curr_u, curr_v));
            polygon.push(vertices[curr_u]);

            let back_angle = (vertices[curr_u] - vertices[curr_v]).angle();
            let mut best: Option<(f32, usize)> = None;
            for &(from, to) in &edges {
                if from != curr_v {
                    continue;
                }
                let mut diff = (vertices[to] - vertices[curr_v]).angle() - back_angle;
                diff = diff.rem_euclid(std::f32::consts::TAU);
                // Going back the way we came is only allowed at a dead end.
                if to == curr_u {
                    diff = 0.0;
                }
                if best.is_none_or(|(d, _)| diff > d) {
                    best = Some((diff, to));
                }
            }
            let Some((_, next)) = best else {
                break;
            };
            (curr_u, curr_v) = (curr_v, next);
            if (curr_u, curr_v) == (start_u, start_v) {
                closed = true;
                break;
            }
        }

        if !closed {
            continue;
        }
        remove_spikes(&mut polygon);
        if polygon.len() < 3 {
            continue;
        }
        let mut area = 0.0;
        for i in 0..polygon.len() {
            let p1 = polygon[i];
            let p2 = polygon[(i + 1) % polygon.len()];
            area += (p2.x - p1.x) * (p2.y + p1.y);
        }
        // Rooms are traced clockwise on screen, which gives a negative area here.
        if area < -2500.0 {
            rooms.push(polygon);
        }
    }
    rooms
}

/// Removes back-and-forth detours left in a face by walls that end inside a room.
fn remove_spikes(polygon: &mut Vec<Pos2>) {
    let mut changed = true;
    while changed && polygon.len() >= 3 {
        changed = false;
        let n = polygon.len();
        for i in 0..n {
            let prev = polygon[(i + n - 1) % n];
            let next = polygon[(i + 1) % n];
            if prev.distance(next) < 1e-3 {
                // prev -> p -> prev: drop p and the duplicate.
                let (a, b) = (i, (i + 1) % n);
                let (first, second) = if a > b { (a, b) } else { (b, a) };
                polygon.remove(first);
                polygon.remove(second);
                changed = true;
                break;
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(a: (f32, f32), b: (f32, f32)) -> Wall {
        Wall::new(Pos2::new(a.0, a.1), Pos2::new(b.0, b.1))
    }

    #[test]
    fn finds_two_rooms_split_by_a_wall_ending_on_other_walls() {
        let walls = vec![
            wall((0.0, 0.0), (500.0, 0.0)),
            wall((500.0, 0.0), (500.0, 200.0)),
            wall((500.0, 200.0), (0.0, 200.0)),
            wall((0.0, 200.0), (0.0, 0.0)),
            wall((300.0, 0.0), (300.0, 200.0)),
        ];
        assert_eq!(extract_rooms(&walls).len(), 2);
    }

    #[test]
    fn single_room_is_found_once() {
        let walls = vec![
            wall((0.0, 0.0), (200.0, 0.0)),
            wall((200.0, 0.0), (200.0, 200.0)),
            wall((200.0, 200.0), (0.0, 200.0)),
            wall((0.0, 200.0), (0.0, 0.0)),
        ];
        let rooms = extract_rooms(&walls);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].len(), 4);
    }

    #[test]
    fn half_wall_inside_a_room_does_not_hide_it() {
        let walls = vec![
            wall((0.0, 0.0), (200.0, 0.0)),
            wall((200.0, 0.0), (200.0, 200.0)),
            wall((200.0, 200.0), (0.0, 200.0)),
            wall((0.0, 200.0), (0.0, 0.0)),
            wall((100.0, 0.0), (100.0, 80.0)),
        ];
        let rooms = extract_rooms(&walls);
        assert_eq!(rooms.len(), 1);
        assert!(!triangulate(&rooms[0]).is_empty());
    }

    #[test]
    fn open_walls_make_no_room() {
        let walls = vec![
            wall((0.0, 0.0), (200.0, 0.0)),
            wall((200.0, 0.0), (200.0, 200.0)),
        ];
        assert!(extract_rooms(&walls).is_empty());
    }
}
