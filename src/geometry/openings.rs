use crate::{
    app::{PIXELS_PER_METER, WALL_THICKNESS},
    models::{Opening, OpeningKind, Wall},
};
use eframe::egui::{Pos2, Vec2};

pub struct OpeningPreset {
    pub label: &'static str,
    pub opening: Opening,
}

const fn preset(kind: OpeningKind, width: f32, height: f32, sill: f32) -> Opening {
    Opening {
        kind,
        offset: 0.0,
        width,
        height,
        sill,
        flip_hinge: false,
        flip_side: false,
    }
}

pub const OPENING_PRESETS: &[OpeningPreset] = &[
    OpeningPreset {
        label: "Window",
        opening: preset(OpeningKind::Window, 1.2, 1.2, 0.9),
    },
    OpeningPreset {
        label: "Wide window",
        opening: preset(OpeningKind::Window, 2.0, 1.4, 0.8),
    },
    OpeningPreset {
        label: "Tall window",
        opening: preset(OpeningKind::Window, 0.9, 2.1, 0.1),
    },
    OpeningPreset {
        label: "Door",
        opening: preset(OpeningKind::Door, 0.9, 2.1, 0.0),
    },
    OpeningPreset {
        label: "Wide door",
        opening: preset(OpeningKind::Door, 1.2, 2.1, 0.0),
    },
    OpeningPreset {
        label: "Balcony door",
        opening: preset(OpeningKind::GlassDoor, 0.9, 2.2, 0.0),
    },
];

/// A wall's unit direction and left-hand normal, in world (canvas) coordinates.
pub fn wall_frame(wall: &Wall) -> (Vec2, Vec2) {
    let dir = (wall.end - wall.start).normalized();
    (dir, dir.rot90())
}

/// The world position of the middle of an opening.
pub fn opening_center(wall: &Wall, opening: &Opening) -> Pos2 {
    let (dir, _) = wall_frame(wall);
    wall.start + dir * opening.clamped_offset(wall.length()) * PIXELS_PER_METER
}

/// The window or door under a point, as (wall index, opening index).
pub fn opening_at(walls: &[Wall], p: Pos2, zoom: f32) -> Option<(usize, usize)> {
    let tolerance = WALL_THICKNESS / 2.0 * PIXELS_PER_METER + 6.0 / zoom;
    for (wi, wall) in walls.iter().enumerate() {
        let (dir, normal) = wall_frame(wall);
        let rel = p - wall.start;
        if rel.dot(normal).abs() > tolerance {
            continue;
        }
        let along = rel.dot(dir) / PIXELS_PER_METER;
        for (oi, opening) in wall.openings.iter().enumerate() {
            let center = opening.clamped_offset(wall.length());
            if (along - center).abs() <= opening.width / 2.0 {
                return Some((wi, oi));
            }
        }
    }
    None
}

/// Where an opening would go if dropped at `p`: the nearest wall within reach, and the
/// offset along it (rounded to whole centimeters) that keeps the opening inside the wall.
pub fn place_on_wall(
    walls: &[Wall],
    p: Pos2,
    zoom: f32,
    opening: &Opening,
) -> Option<(usize, f32)> {
    let reach = 25.0 / zoom;
    let mut best: Option<(f32, usize, f32)> = None;
    for (wi, wall) in walls.iter().enumerate() {
        let length = wall.length();
        if length < opening.width * 0.5 {
            continue;
        }
        let (dir, normal) = wall_frame(wall);
        let rel = p - wall.start;
        let along = rel.dot(dir) / PIXELS_PER_METER;
        if along < 0.0 || along > length {
            continue;
        }
        let distance = rel.dot(normal).abs();
        if distance > reach || best.is_some_and(|b| distance >= b.0) {
            continue;
        }
        let candidate = Opening {
            offset: along,
            ..*opening
        };
        let offset = (candidate.clamped_offset(length) * 100.0).round() / 100.0;
        best = Some((distance, wi, offset));
    }
    best.map(|(_, wi, offset)| (wi, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall() -> Wall {
        // 4 m long, along x.
        Wall::new(Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0))
    }

    #[test]
    fn placing_keeps_the_opening_inside_the_wall() {
        let walls = vec![wall()];
        let window = OPENING_PRESETS[0].opening; // 1.2 m wide
        let (wi, offset) = place_on_wall(&walls, Pos2::new(10.0, 5.0), 1.0, &window).unwrap();
        assert_eq!(wi, 0);
        assert!((offset - 0.6).abs() < 1e-4);
        assert!(place_on_wall(&walls, Pos2::new(100.0, 60.0), 1.0, &window).is_none());
    }

    #[test]
    fn finds_the_opening_under_the_pointer() {
        let mut w = wall();
        w.openings.push(Opening {
            offset: 2.0,
            ..OPENING_PRESETS[3].opening
        }); // 0.9 m door
        let walls = vec![w];
        assert_eq!(opening_at(&walls, Pos2::new(100.0, 1.0), 1.0), Some((0, 0)));
        assert_eq!(opening_at(&walls, Pos2::new(130.0, 1.0), 1.0), None);
    }
}
