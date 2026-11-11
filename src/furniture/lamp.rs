use super::{shade, Part, Shape};
use crate::models::{Furniture, FurnitureKind, LampStyle, ShadeShape};

/// A point light in furniture-local coordinates (meters, y up from the item's elevation).
pub struct LampLight {
    pub local: [f32; 3],
    /// Light color with brightness folded in; 1.0 is roughly an 800 lm bulb.
    pub color: [f32; 3],
    /// Distance beyond which the light no longer contributes.
    pub range: f32,
    /// A spot's beam: local direction, and the cosine of half its angle. None shines all
    /// round.
    pub beam: Option<([f32; 3], f32)>,
}

/// Where the lights of a line of lights sit along its width: evenly spread, each in the
/// middle of an equal share.
pub fn line_positions(width: f32, count: u32) -> Vec<f32> {
    let n = count.max(1);
    (0..n)
        .map(|i| -width / 2.0 + width * (i as f32 + 0.5) / n as f32)
        .collect()
}

/// How many lights a bar or strip is lit by: about one per 60 cm.
fn linear_light_count(width: f32) -> u32 {
    ((width / 0.6).round() as u32).clamp(1, 6)
}

fn shade_part(
    shape: ShadeShape,
    x: f32,
    z: f32,
    radius: f32,
    y0: f32,
    y1: f32,
    color: [u8; 3],
) -> Part {
    let shape = match shape {
        ShadeShape::Cone => Shape::Cone { top_scale: 0.6 },
        ShadeShape::Drum => Shape::Cylinder,
        ShadeShape::Globe => Shape::Sphere,
    };
    Part::round(shape, x, z, radius, y0, y1, color)
}

/// The lamp's parts, hanging from a ceiling at `ceiling` meters where it does.
pub fn parts(item: &Furniture, ceiling: f32) -> Vec<Part> {
    let FurnitureKind::Lamp {
        style,
        shade: shape,
        shade_height,
        on,
        kelvin,
        ..
    } = item.kind
    else {
        return Vec::new();
    };
    let (hw, hd, h) = (item.width / 2.0, item.depth / 2.0, item.height);
    let c = item.color;
    let metal = [60, 60, 62];
    let glow = if on { 0.55 } else { 0.0 };
    let bulb_color = kelvin_to_srgb(kelvin);
    let bulb_glow = if on { 1.2 } else { 0.0 };
    let r = hw.min(hd);

    match style {
        LampStyle::Floor | LampStyle::Table => {
            let shade_h = shade_height.clamp(0.02, h * 0.8);
            let shade_bottom = h - shade_h;
            let base_r = (r * if style == LampStyle::Floor { 0.65 } else { 0.5 }).max(0.03);
            vec![
                Part::round(
                    Shape::Cylinder,
                    0.0,
                    0.0,
                    base_r,
                    0.0,
                    0.025_f32.min(h),
                    metal,
                ),
                Part::round(
                    Shape::Cylinder,
                    0.0,
                    0.0,
                    0.012,
                    0.0,
                    shade_bottom + shade_h * 0.5,
                    metal,
                ),
                Part::round(
                    Shape::Sphere,
                    0.0,
                    0.0,
                    (r * 0.2).min(0.04),
                    shade_bottom,
                    shade_bottom + (r * 0.4).min(0.08),
                    bulb_color,
                )
                .glowing(bulb_glow),
                shade_part(shape, 0.0, 0.0, r, shade_bottom, h, c).glowing(glow),
            ]
        }
        LampStyle::Pendant => {
            let ceiling = item.ceiling_above(ceiling);
            vec![
                Part::round(Shape::Cylinder, 0.0, 0.0, 0.004, h * 0.5, ceiling, metal),
                Part::round(Shape::Cylinder, 0.0, 0.0, 0.05, ceiling - 0.02, ceiling, c),
                Part::round(
                    Shape::Sphere,
                    0.0,
                    0.0,
                    (r * 0.25).min(0.04),
                    h * 0.2,
                    h * 0.2 + (r * 0.5).min(0.08),
                    bulb_color,
                )
                .glowing(bulb_glow),
                shade_part(shape, 0.0, 0.0, r, 0.0, h, c).glowing(if shape == ShadeShape::Globe {
                    glow * 1.4
                } else {
                    glow
                }),
            ]
        }
        LampStyle::Track => {
            let n = spots_of(item);
            let bar = 0.03_f32.min(h);
            let mut parts = vec![Part::cuboid(
                [-hw, h - bar, -hd.min(0.02)],
                [hw, h, hd.min(0.02)],
                c,
            )];
            let pitch = item.width / n as f32;
            let head_r = (pitch * 0.4).min(hd).min(0.04).max(0.01);
            let head_len = (h - bar).min(0.12).max(0.0);
            for x in line_positions(item.width, n) {
                parts.push(Part::round(
                    Shape::Cylinder,
                    x,
                    0.0,
                    0.006,
                    head_len,
                    h - bar,
                    metal,
                ));
                parts.push(Part::round(
                    Shape::Cylinder,
                    x,
                    0.0,
                    head_r,
                    0.004,
                    head_len,
                    c,
                ));
                parts.push(
                    Part::round(
                        Shape::Cylinder,
                        x,
                        0.0,
                        head_r * 0.7,
                        0.0,
                        0.004,
                        bulb_color,
                    )
                    .glowing(bulb_glow),
                );
            }
            parts
        }
        LampStyle::Downlights => {
            let n = spots_of(item);
            let pitch = item.width / n as f32;
            let rim = (pitch * 0.4).min(hd).min(0.06).max(0.01);
            let mut parts = Vec::new();
            for x in line_positions(item.width, n) {
                parts.push(Part::round(Shape::Cylinder, x, 0.0, rim, 0.003, h, c));
                parts.push(
                    Part::round(Shape::Cylinder, x, 0.0, rim * 0.75, 0.0, 0.003, bulb_color)
                        .glowing(bulb_glow),
                );
            }
            parts
        }
        LampStyle::LinearPendant | LampStyle::Strip => {
            let diffuser = 0.004_f32.min(h / 2.0);
            let mut parts = vec![
                Part::cuboid([-hw, diffuser, -hd], [hw, h, hd], c),
                Part::cuboid(
                    [-hw + 0.01_f32.min(hw / 2.0), 0.0, -hd * 0.7],
                    [hw - 0.01_f32.min(hw / 2.0), diffuser, hd * 0.7],
                    bulb_color,
                )
                .glowing(bulb_glow),
            ];
            if style == LampStyle::LinearPendant {
                // Two wires up to the ceiling.
                let ceiling = item.ceiling_above(ceiling);
                let x = (hw - 0.1).max(0.0);
                for sx in [-x, x] {
                    parts.push(Part::round(
                        Shape::Cylinder,
                        sx,
                        0.0,
                        0.002,
                        h,
                        ceiling,
                        metal,
                    ));
                    parts.push(Part::round(
                        Shape::Cylinder,
                        sx,
                        0.0,
                        0.03,
                        ceiling - 0.015,
                        ceiling,
                        c,
                    ));
                }
            }
            parts
        }
        LampStyle::Wall => {
            let plate = 0.02_f32.min(item.depth / 2.0);
            let shade_r = hw.min(hd - plate / 2.0).max(0.01);
            let shade_z = hd - shade_r;
            vec![
                Part::cuboid(
                    [-0.04_f32.min(hw), h * 0.25, -hd],
                    [0.04_f32.min(hw), h * 0.75, -hd + plate],
                    metal,
                ),
                Part::cuboid(
                    [-0.01, h * 0.45, -hd + plate],
                    [0.01, h * 0.55, shade_z],
                    metal,
                ),
                shade_part(shape, 0.0, shade_z, shade_r, 0.0, h, shade(c, 1.0)).glowing(glow),
            ]
        }
    }
}

/// Spots on a track or in a row, at least one.
fn spots_of(item: &Furniture) -> u32 {
    match item.kind {
        FurnitureKind::Lamp { spots, .. } => spots.clamp(1, 24),
        _ => 1,
    }
}

/// The lights a lamp gives: one for most, one per spot for a row of spots, and a few
/// spread along a bar or strip.
pub fn lights(item: &Furniture) -> Vec<LampLight> {
    let FurnitureKind::Lamp {
        style,
        lumens,
        kelvin,
        on,
        spread,
        tilt,
        ..
    } = item.kind
    else {
        return Vec::new();
    };
    if !on || lumens <= 0.0 {
        return Vec::new();
    }
    // Only spots can be aimed.
    let (spread, tilt) = if style.has_spots() {
        (spread, tilt)
    } else {
        (0.0, 0.0)
    };
    // (count, lumens each, cosine of the beam's half angle)
    let (count, each, cos) = match style {
        LampStyle::Track => (spots_of(item), lumens, 30f32.to_radians().cos()),
        LampStyle::Downlights => (spots_of(item), lumens, 45f32.to_radians().cos()),
        LampStyle::LinearPendant | LampStyle::Strip => {
            let n = linear_light_count(item.width);
            (n, lumens / n as f32, 70f32.to_radians().cos())
        }
        _ => return light(item).into_iter().collect(),
    };
    // A spot puts all its light into the beam, so it lights what it points at far more
    // than a bare bulb of the same lumens would.
    let focus = (1.6 / (1.0 - cos).max(0.2)).clamp(1.0, 6.0);
    let intensity = each / 800.0 * focus;
    let color = kelvin_to_srgb(kelvin).map(|c| c as f32 / 255.0 * intensity);
    let positions = line_positions(item.width, count);
    let outer = positions.last().copied().unwrap_or(0.0).abs();
    positions
        .into_iter()
        .map(|x| LampLight {
            local: [x, -0.01, 0.0],
            color,
            range: 2.5 + 3.0 * intensity.sqrt(),
            beam: Some((aim(x, outer, spread, tilt), cos)),
        })
        .collect()
}

/// The direction of a spot at `x` along a row whose outermost spot is at ±`outer`: straight
/// down, leaned `tilt` degrees towards the front (+z), and turned outwards along the row by
/// `spread` degrees at the ends, in proportion in between.
pub fn aim(x: f32, outer: f32, spread: f32, tilt: f32) -> [f32; 3] {
    let share = if outer > 1e-4 { x / outer } else { 0.0 };
    let (s, t) = ((spread * share).to_radians(), tilt.to_radians());
    [t.cos() * s.sin(), -t.cos() * s.cos(), t.sin()]
}

/// The single light of a lamp with one bulb.
fn light(item: &Furniture) -> Option<LampLight> {
    let FurnitureKind::Lamp {
        style,
        shade_height,
        lumens,
        kelvin,
        on,
        ..
    } = item.kind
    else {
        return None;
    };
    if !on || lumens <= 0.0 {
        return None;
    }
    let h = item.height;
    let local = match style {
        LampStyle::Floor | LampStyle::Table => {
            [0.0, h - shade_height.clamp(0.02, h * 0.8) * 0.5, 0.0]
        }
        LampStyle::Pendant => [0.0, h * 0.3, 0.0],
        LampStyle::Wall => [
            0.0,
            h * 0.5,
            item.depth / 2.0 - item.width.min(item.depth) / 2.0,
        ],
        _ => [0.0, h * 0.5, 0.0],
    };
    let intensity = lumens / 800.0;
    let color = kelvin_to_srgb(kelvin).map(|c| c as f32 / 255.0 * intensity);
    Some(LampLight {
        local,
        color,
        range: 2.5 + 3.0 * intensity.sqrt(),
        beam: None,
    })
}

/// Approximate color of a black body at the given temperature (Tanner Helland's fit).
pub fn kelvin_to_srgb(kelvin: f32) -> [u8; 3] {
    let t = kelvin.clamp(1000.0, 40000.0) / 100.0;
    let r = if t <= 66.0 {
        255.0
    } else {
        329.698_73 * (t - 60.0).powf(-0.133_204_76)
    };
    let g = if t <= 66.0 {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_17 * (t - 60.0).powf(-0.075_514_85)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    [r, g, b].map(|v| v.clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spot_track_lights_each_spot() {
        let mut track = (crate::furniture::PRESETS
            .iter()
            .find(|p| p.label == "Spot track")
            .unwrap()
            .build)();
        let FurnitureKind::Lamp { spots, .. } = &mut track.kind else {
            panic!()
        };
        *spots = 5;
        let lights = lights(&track);
        assert_eq!(lights.len(), 5);
        // Evenly spread over the width, pointing down.
        let xs: Vec<f32> = lights.iter().map(|l| l.local[0]).collect();
        assert!((xs[0] + track.width * 0.4).abs() < 1e-5, "{xs:?}");
        assert!((xs[4] - track.width * 0.4).abs() < 1e-5, "{xs:?}");
        assert!(lights.iter().all(|l| l.beam.is_some_and(|b| b.0[1] < 0.0)));
        // A glowing lens per spot.
        let lenses = track.parts().iter().filter(|p| p.emissive > 0.0).count();
        assert_eq!(lenses, 5);
    }

    #[test]
    fn spots_spread_outwards_and_tilt_together() {
        // Three spots, outer ones turned 20° out, all leaning 10° forward.
        let outer = 0.5;
        let left = aim(-outer, outer, 20.0, 10.0);
        let middle = aim(0.0, outer, 20.0, 10.0);
        let right = aim(outer, outer, 20.0, 10.0);
        assert!(left[0] < 0.0 && right[0] > 0.0 && middle[0].abs() < 1e-6);
        assert!((left[0] + right[0]).abs() < 1e-6);
        for d in [left, middle, right] {
            assert!(d[1] < 0.0 && d[2] > 0.0, "{d:?}");
            assert!((d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - 1.0).abs() < 1e-5);
        }
        // Turned 20° out: atan(x / down) of the outer spot is 20° when not tilted.
        let flat = aim(outer, outer, 20.0, 0.0);
        assert!(((flat[0] / -flat[1]).atan().to_degrees() - 20.0).abs() < 1e-3);
    }

    #[test]
    fn warm_light_is_orange_and_daylight_is_neutral() {
        let warm = kelvin_to_srgb(2700.0);
        assert_eq!(warm[0], 255);
        assert!(warm[2] < 200);
        let daylight = kelvin_to_srgb(6500.0);
        assert!(daylight.iter().all(|&c| c > 240));
    }
}
