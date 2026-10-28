use crate::models::{Furniture, FurnitureKind, LampStyle, ShadeShape};
use eframe::egui::Pos2;

#[derive(Clone, Copy, PartialEq)]
pub enum Category {
    Living,
    Dining,
    Bedroom,
    Work,
    Storage,
    Lighting,
    Decor,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Living,
        Category::Dining,
        Category::Bedroom,
        Category::Work,
        Category::Storage,
        Category::Lighting,
        Category::Decor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Living => "Living room",
            Category::Dining => "Dining",
            Category::Bedroom => "Bedroom",
            Category::Work => "Office",
            Category::Storage => "Storage",
            Category::Lighting => "Lighting",
            Category::Decor => "Decor",
        }
    }
}

pub struct Preset {
    pub label: &'static str,
    pub category: Category,
    pub build: fn() -> Furniture,
}

pub fn make(
    name: &str,
    kind: FurnitureKind,
    width: f32,
    depth: f32,
    height: f32,
    color: [u8; 3],
) -> Furniture {
    Furniture {
        name: name.to_owned(),
        kind,
        position: Pos2::ZERO,
        rotation: 0.0,
        width,
        depth,
        height,
        elevation: 0.0,
        color,
        options: Vec::new(),
        active_option: 0,
    }
}

fn table(top_thickness: f32, leg_size: f32, round: bool) -> FurnitureKind {
    FurnitureKind::Table {
        top_thickness,
        leg_size,
        round,
    }
}

fn sofa(seat_height: f32, arm_width: f32, back_depth: f32) -> FurnitureKind {
    FurnitureKind::Sofa {
        seat_height,
        arm_width,
        back_depth,
    }
}

fn lamp(style: LampStyle, shade: ShadeShape, shade_height: f32, lumens: f32) -> FurnitureKind {
    FurnitureKind::Lamp {
        style,
        shade,
        shade_height,
        lumens,
        kelvin: 2700.0,
        on: true,
    }
}

fn elevated(mut item: Furniture, elevation: f32) -> Furniture {
    item.elevation = elevation;
    item
}

const WHITE: [u8; 3] = [240, 240, 235];
const OAK: [u8; 3] = [196, 154, 108];
const WALNUT: [u8; 3] = [120, 85, 60];
const LINEN: [u8; 3] = [235, 228, 212];

pub const PRESETS: &[Preset] = &[
    // Living room
    Preset {
        label: "Sofa",
        category: Category::Living,
        build: || make("Sofa", sofa(0.42, 0.15, 0.2), 2.0, 0.9, 0.8, [90, 100, 115]),
    },
    Preset {
        label: "Armchair",
        category: Category::Living,
        build: || {
            make(
                "Armchair",
                sofa(0.42, 0.12, 0.18),
                0.8,
                0.85,
                0.8,
                [150, 120, 95],
            )
        },
    },
    Preset {
        label: "Coffee table",
        category: Category::Living,
        build: || {
            make(
                "Coffee table",
                table(0.03, 0.04, false),
                1.1,
                0.6,
                0.45,
                OAK,
            )
        },
    },
    Preset {
        label: "Side table",
        category: Category::Living,
        build: || {
            make(
                "Side table",
                table(0.02, 0.03, true),
                0.45,
                0.45,
                0.5,
                WALNUT,
            )
        },
    },
    Preset {
        label: "TV bench",
        category: Category::Living,
        build: || make("TV bench", FurnitureKind::Box, 1.6, 0.4, 0.5, WHITE),
    },
    Preset {
        label: "Rug",
        category: Category::Living,
        build: || make("Rug", FurnitureKind::Box, 2.0, 1.4, 0.01, [180, 160, 140]),
    },
    // Dining
    Preset {
        label: "Dining table",
        category: Category::Dining,
        build: || {
            make(
                "Dining table",
                table(0.03, 0.06, false),
                1.8,
                0.9,
                0.75,
                OAK,
            )
        },
    },
    Preset {
        label: "Round table",
        category: Category::Dining,
        build: || make("Round table", table(0.03, 0.08, true), 1.1, 1.1, 0.75, OAK),
    },
    Preset {
        label: "Chair",
        category: Category::Dining,
        build: || {
            make(
                "Chair",
                FurnitureKind::Chair { seat_height: 0.45 },
                0.45,
                0.5,
                0.9,
                WALNUT,
            )
        },
    },
    Preset {
        label: "Bar stool",
        category: Category::Dining,
        build: || {
            make(
                "Bar stool",
                FurnitureKind::Chair { seat_height: 0.65 },
                0.4,
                0.4,
                0.85,
                [60, 60, 65],
            )
        },
    },
    // Bedroom
    Preset {
        label: "Double bed",
        category: Category::Bedroom,
        build: || {
            make(
                "Double bed",
                FurnitureKind::Bed {
                    headboard_height: 1.0,
                },
                1.6,
                2.0,
                0.5,
                [200, 205, 215],
            )
        },
    },
    Preset {
        label: "Single bed",
        category: Category::Bedroom,
        build: || {
            make(
                "Single bed",
                FurnitureKind::Bed {
                    headboard_height: 0.9,
                },
                0.9,
                2.0,
                0.5,
                [200, 205, 215],
            )
        },
    },
    Preset {
        label: "Nightstand",
        category: Category::Bedroom,
        build: || make("Nightstand", FurnitureKind::Box, 0.45, 0.4, 0.55, WHITE),
    },
    Preset {
        label: "Dresser",
        category: Category::Bedroom,
        build: || make("Dresser", FurnitureKind::Box, 1.2, 0.5, 0.8, WHITE),
    },
    // Office
    Preset {
        label: "Desk",
        category: Category::Work,
        build: || make("Desk", table(0.025, 0.04, false), 1.4, 0.7, 0.73, WHITE),
    },
    Preset {
        label: "Desk chair",
        category: Category::Work,
        build: || {
            make(
                "Desk chair",
                FurnitureKind::Chair { seat_height: 0.48 },
                0.6,
                0.6,
                1.1,
                [50, 50, 55],
            )
        },
    },
    // Storage
    Preset {
        label: "Bookcase",
        category: Category::Storage,
        build: || {
            make(
                "Bookcase",
                FurnitureKind::Shelf { shelves: 6 },
                0.8,
                0.3,
                2.0,
                WHITE,
            )
        },
    },
    Preset {
        label: "Low shelf",
        category: Category::Storage,
        build: || {
            make(
                "Low shelf",
                FurnitureKind::Shelf { shelves: 3 },
                1.5,
                0.4,
                0.8,
                WHITE,
            )
        },
    },
    Preset {
        label: "Cabinet",
        category: Category::Storage,
        build: || make("Cabinet", FurnitureKind::Box, 0.8, 0.4, 0.9, WHITE),
    },
    Preset {
        label: "Wardrobe",
        category: Category::Storage,
        build: || make("Wardrobe", FurnitureKind::Box, 1.0, 0.6, 2.0, WHITE),
    },
    // Lighting
    Preset {
        label: "Floor lamp",
        category: Category::Lighting,
        build: || {
            make(
                "Floor lamp",
                lamp(LampStyle::Floor, ShadeShape::Cone, 0.3, 800.0),
                0.4,
                0.4,
                1.5,
                LINEN,
            )
        },
    },
    Preset {
        label: "Table lamp",
        category: Category::Lighting,
        build: || {
            elevated(
                make(
                    "Table lamp",
                    lamp(LampStyle::Table, ShadeShape::Drum, 0.2, 400.0),
                    0.3,
                    0.3,
                    0.45,
                    LINEN,
                ),
                0.75,
            )
        },
    },
    Preset {
        label: "Pendant",
        category: Category::Lighting,
        build: || {
            elevated(
                make(
                    "Pendant",
                    lamp(LampStyle::Pendant, ShadeShape::Cone, 0.0, 1000.0),
                    0.4,
                    0.4,
                    0.25,
                    [40, 40, 45],
                ),
                1.6,
            )
        },
    },
    Preset {
        label: "Globe pendant",
        category: Category::Lighting,
        build: || {
            elevated(
                make(
                    "Globe pendant",
                    lamp(LampStyle::Pendant, ShadeShape::Globe, 0.0, 800.0),
                    0.35,
                    0.35,
                    0.35,
                    [250, 248, 240],
                ),
                1.7,
            )
        },
    },
    Preset {
        label: "Wall lamp",
        category: Category::Lighting,
        build: || {
            elevated(
                make(
                    "Wall lamp",
                    lamp(LampStyle::Wall, ShadeShape::Drum, 0.0, 400.0),
                    0.2,
                    0.2,
                    0.2,
                    LINEN,
                ),
                1.6,
            )
        },
    },
    // Decor
    Preset {
        label: "Plant",
        category: Category::Decor,
        build: || make("Plant", FurnitureKind::Plant, 0.5, 0.5, 1.2, [190, 110, 80]),
    },
    Preset {
        label: "Small plant",
        category: Category::Decor,
        build: || {
            make(
                "Small plant",
                FurnitureKind::Plant,
                0.3,
                0.3,
                0.5,
                [230, 230, 225],
            )
        },
    },
];
