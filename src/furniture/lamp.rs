use super::{shade, Part, Shape};
use crate::models::{Furniture, FurnitureKind, LampStyle, ShadeShape};

/// A point light in furniture-local coordinates (meters, y up from the item's elevation).
pub struct LampLight {
    pub local: [f32; 3],
    /// Light color with brightness folded in; 1.0 is roughly an 800 lm bulb.
    pub color: [f32; 3],
    /// Distance beyond which the light no longer contributes.
    pub range: f32,
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

pub fn parts(item: &Furniture) -> Vec<Part> {
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
            let ceiling = item.ceiling_above();
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

pub fn light(item: &Furniture) -> Option<LampLight> {
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
    };
    let intensity = lumens / 800.0;
    let color = kelvin_to_srgb(kelvin).map(|c| c as f32 / 255.0 * intensity);
    Some(LampLight {
        local,
        color,
        range: 2.5 + 3.0 * intensity.sqrt(),
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
    fn warm_light_is_orange_and_daylight_is_neutral() {
        let warm = kelvin_to_srgb(2700.0);
        assert_eq!(warm[0], 255);
        assert!(warm[2] < 200);
        let daylight = kelvin_to_srgb(6500.0);
        assert!(daylight.iter().all(|&c| c > 240));
    }
}
