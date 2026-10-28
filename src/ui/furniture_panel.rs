use crate::{
    app::RoomPlannerApp,
    app::WALL_HEIGHT,
    furniture::{Category, PRESETS},
    geometry::openings::{OpeningPreset, OPENING_PRESETS},
    models::{FloorFinish, Furniture, FurnitureKind, LampStyle, Opening, OpeningKind, ShadeShape},
    ui::{
        theme::{section_label, ACCENT, TEXT_DARK, TEXT_MUTED},
        thumbnail,
    },
};
use eframe::egui::{self, Button, RichText};

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::left("furniture_panel")
        .resizable(true)
        .default_size((ui.available_width() * 0.22).clamp(210.0, 280.0))
        .size_range(190.0..=420.0)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0);
                if app.selected_opening_mut().is_some() {
                    opening_card(app, ui);
                    ui.add_space(8.0);
                }
                if let Some(idx) = app.selected_furniture.filter(|&i| i < app.furniture.len()) {
                    properties_card(app, ui, idx);
                    ui.add_space(8.0);
                }
                library(app, ui);
                ui.add_space(8.0);
                finishes(app, ui);
                ui.add_space(8.0);
                plan_contents(app, ui);
            });
        });
}

fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, crate::ui::theme::BORDER))
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

// --- Library ---

fn library(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    ui.heading("Add furniture");
    ui.add(
        egui::TextEdit::singleline(&mut app.library_filter)
            .hint_text("🔍 Search…")
            .desired_width(f32::INFINITY),
    );
    let filter = app.library_filter.trim().to_lowercase();
    let mut to_add: Option<Furniture> = None;

    if !app.my_library.is_empty() {
        let items: Vec<(Furniture, String)> = app
            .my_library
            .iter()
            .filter(|item| filter.is_empty() || item.name.to_lowercase().contains(&filter))
            .map(|item| (item.clone(), item.name.clone()))
            .collect();
        if !items.is_empty() {
            section_label(ui, "My furniture");
            match tile_grid(ui, &items, true) {
                Some(TileAction::Add(item)) => to_add = Some(item),
                Some(TileAction::Remove(name)) => app.my_library.retain(|i| i.name != name),
                None => {}
            }
        }
    }

    let openings: Vec<&OpeningPreset> = OPENING_PRESETS
        .iter()
        .filter(|p| filter.is_empty() || p.label.to_lowercase().contains(&filter))
        .collect();
    if !openings.is_empty() {
        section_label(ui, "Doors & windows");
        if let Some(template) = opening_grid(ui, &openings) {
            app.start_placing_opening(template);
        }
    }

    for category in Category::ALL {
        let items: Vec<(Furniture, String)> = PRESETS
            .iter()
            .filter(|p| p.category == category)
            .filter(|p| filter.is_empty() || p.label.to_lowercase().contains(&filter))
            .map(|p| ((p.build)(), p.label.to_owned()))
            .collect();
        if items.is_empty() {
            continue;
        }
        section_label(ui, category.label());
        if let Some(TileAction::Add(item)) = tile_grid(ui, &items, false) {
            to_add = Some(item);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        section_label(ui, "Your own models");
        if ui
            .add(
                Button::new("Import 3D model (.glb / .gltf)…")
                    .min_size(egui::vec2(ui.available_width(), 26.0)),
            )
            .on_hover_text("For example a model downloaded from a furniture store")
            .clicked()
        {
            import_model(app);
        }
    }
    if let Some(err) = &app.model_error {
        ui.colored_label(egui::Color32::from_rgb(200, 40, 40), err);
    }

    if let Some(item) = to_add {
        app.add_furniture(item);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn import_model(app: &mut RoomPlannerApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("3D model", &["glb", "gltf"])
        .pick_file()
    else {
        return;
    };
    let path_str = path.to_string_lossy().to_string();
    match app.models.get(&path_str) {
        Ok(mesh) => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Model".to_owned());
            app.add_furniture(Furniture::new_model(name, path_str, mesh.size));
            app.model_error = None;
        }
        Err(err) => app.model_error = Some(err),
    }
}

/// A window or door being dragged from the library onto a wall.
pub struct OpeningDrop(pub Opening);

/// Doors and windows as picture tiles. Click to start placing one, or drag it onto a wall.
fn opening_grid(ui: &mut egui::Ui, presets: &[&OpeningPreset]) -> Option<Opening> {
    let gap = 6.0;
    let available = ui.available_width();
    let columns = (((available + gap) / (92.0 + gap)).floor() as usize).max(2);
    let width = ((available - gap * (columns - 1) as f32) / columns as f32).floor() - 1.0;
    let size = egui::vec2(width, (width * 0.75).round() + 22.0);
    let mut chosen = None;
    for row in presets.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for preset in row {
                let opening = &preset.opening;
                let (rect, response) = tile_frame(ui, size);
                let image = egui::Rect::from_min_max(
                    rect.min + egui::vec2(4.0, 4.0),
                    egui::pos2(rect.max.x - 4.0, rect.max.y - 20.0),
                );
                thumbnail::paint_opening(ui.painter(), image, opening);
                tile_label(ui, rect, preset.label);
                response.dnd_set_drag_payload(OpeningDrop(*opening));
                let response = response.on_hover_text(format!(
                    "{}\n{:.0} × {:.0} cm{}\nClick, then click on a wall · or drag onto a wall",
                    preset.label,
                    opening.width * 100.0,
                    opening.height * 100.0,
                    if opening.sill > 0.0 {
                        format!(", {:.0} cm above floor", opening.sill * 100.0)
                    } else {
                        String::new()
                    }
                ));
                if response.clicked() {
                    chosen = Some(*opening);
                }
            }
        });
    }
    chosen
}

/// Furniture being dragged from the library onto the plan.
pub struct LibraryDrop(pub Furniture);

/// A small picture following the cursor while dragging from the library.
pub fn show_drag_ghost(ctx: &egui::Context) {
    let furniture = egui::DragAndDrop::payload::<LibraryDrop>(ctx);
    let opening = egui::DragAndDrop::payload::<OpeningDrop>(ctx);
    if furniture.is_none() && opening.is_none() {
        return;
    }
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("library_drag_ghost"),
    ));
    let rect = egui::Rect::from_min_size(pos + egui::vec2(12.0, 12.0), egui::vec2(64.0, 52.0));
    painter.rect(
        rect,
        8.0,
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 230),
        egui::Stroke::new(1.0, ACCENT),
        egui::StrokeKind::Inside,
    );
    if let Some(drop) = furniture {
        thumbnail::paint(&painter, rect.shrink(4.0), &drop.0);
    } else if let Some(drop) = opening {
        thumbnail::paint_opening(&painter, rect.shrink(4.0), &drop.0);
    }
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

enum TileAction {
    Add(Furniture),
    /// Remove a My furniture entry, by name.
    Remove(String),
}

/// Picture tiles in as many columns as fit. Click a tile to add it, or drag it onto the plan.
fn tile_grid(
    ui: &mut egui::Ui,
    items: &[(Furniture, String)],
    removable: bool,
) -> Option<TileAction> {
    let gap = 6.0;
    let available = ui.available_width();
    let columns = (((available + gap) / (92.0 + gap)).floor() as usize).max(2);
    // Rounded down so the row never ends up wider than the panel and pushes it wider.
    let width = ((available - gap * (columns - 1) as f32) / columns as f32).floor() - 1.0;
    let size = egui::vec2(width, (width * 0.75).round() + 22.0);

    let mut action = None;
    for row in items.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (item, label) in row {
                let response = tile(ui, item, label, size);
                response.dnd_set_drag_payload(LibraryDrop(item.clone()));
                if response.clicked() {
                    action = Some(TileAction::Add(item.clone()));
                }
                if removable {
                    response.context_menu(|ui| {
                        if ui.button("Remove from My furniture").clicked() {
                            action = Some(TileAction::Remove(item.name.clone()));
                            ui.close();
                        }
                    });
                }
            }
        });
    }
    action
}

fn tile_frame(ui: &mut egui::Ui, size: egui::Vec2) -> (egui::Rect, egui::Response) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let hot = response.hovered() || response.dragged();
    ui.painter().rect(
        rect,
        8.0,
        if hot {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(251, 251, 252)
        },
        egui::Stroke::new(
            1.0,
            if hot {
                ACCENT
            } else {
                crate::ui::theme::BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (rect, response)
}

fn tile_label(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    let painter = ui.painter_at(rect);
    let mut font = egui::FontId::proportional(11.5);
    let mut galley = painter.layout_no_wrap(label.to_owned(), font.clone(), TEXT_DARK);
    if galley.size().x > rect.width() - 6.0 {
        font.size = 10.0;
        galley = painter.layout_no_wrap(label.to_owned(), font, TEXT_DARK);
    }
    let text_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.max.y - 17.0);
    painter.galley(text_pos, galley, TEXT_DARK);
}

fn tile(ui: &mut egui::Ui, item: &Furniture, label: &str, size: egui::Vec2) -> egui::Response {
    let (rect, response) = tile_frame(ui, size);
    let image = egui::Rect::from_min_max(
        rect.min + egui::vec2(4.0, 4.0),
        egui::pos2(rect.max.x - 4.0, rect.max.y - 20.0),
    );
    thumbnail::paint(&ui.painter_at(rect), image, item);
    tile_label(ui, rect, label);
    response.on_hover_text(format!(
        "{}\n{:.0} × {:.0} × {:.0} cm\nClick to add, or drag onto the plan",
        item.name,
        item.width * 100.0,
        item.depth * 100.0,
        item.height * 100.0
    ))
}

/// Wall paint and flooring of the home.
fn finishes(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(RichText::new("Walls & floor").strong())
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("finishes")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Wall color");
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut app.finishes.wall_color);
                        for (name, color) in [
                            ("White", [246, 246, 244]),
                            ("Off-white", [240, 236, 226]),
                            ("Light gray", [222, 223, 222]),
                        ] {
                            if ui.small_button(name).clicked() {
                                app.finishes.wall_color = color;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Floor");
                    ui.horizontal(|ui| {
                        let [r, g, b] = app.finishes.floor.color();
                        let (swatch, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(swatch, 4.0, egui::Color32::from_rgb(r, g, b));
                        egui::ComboBox::from_id_salt("floor_finish")
                            .selected_text(app.finishes.floor.label())
                            .show_ui(ui, |ui| {
                                for finish in FloorFinish::ALL {
                                    ui.selectable_value(
                                        &mut app.finishes.floor,
                                        finish,
                                        finish.label(),
                                    );
                                }
                            });
                    });
                    ui.end_row();
                });
        });
}

fn plan_contents(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    if app.furniture.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("In this plan ({})", app.furniture.len()))
        .default_open(false)
        .show(ui, |ui| {
            for (idx, item) in app.furniture.iter().enumerate() {
                let selected = app.selected_furniture == Some(idx);
                let mut label = format!("{}  {}", item.name, size_text(item));
                if item.options.len() > 1 {
                    label += &format!("  · {} options", item.options.len());
                }
                if ui.selectable_label(selected, label).clicked() {
                    app.selected_furniture = Some(idx);
                    app.selected_walls.clear();
                }
            }
        });
}

fn size_text(item: &Furniture) -> String {
    format!("{:.0}×{:.0}", item.width * 100.0, item.depth * 100.0)
}

// --- Properties ---

fn properties_card(app: &mut RoomPlannerApp, ui: &mut egui::Ui, idx: usize) {
    let mut duplicate = false;
    let mut delete = false;
    let mut save_to_library = false;

    card(ui, |ui| {
        let item = &mut app.furniture[idx];
        ui.horizontal(|ui| {
            let (picture, _) = ui.allocate_exact_size(egui::vec2(44.0, 36.0), egui::Sense::hover());
            thumbnail::paint(ui.painter(), picture, item);
            ui.add(
                egui::TextEdit::singleline(&mut item.name)
                    .desired_width(ui.available_width() - 34.0)
                    .font(egui::TextStyle::Heading),
            );
            if ui.button("×").on_hover_text("Deselect").clicked() {
                app.selected_furniture = None;
            }
        });
        if app.selected_furniture.is_none() {
            return;
        }

        options_row(ui, item);
        ui.add_space(2.0);

        egui::Grid::new("furniture_props")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                let is_model = matches!(item.kind, FurnitureKind::Model { .. });
                let before = [item.width, item.depth, item.height];

                ui.label("Width");
                cm_value(ui, &mut item.width);
                ui.end_row();
                ui.label("Depth");
                cm_value(ui, &mut item.depth);
                ui.end_row();
                ui.label(
                    if matches!(
                        item.kind,
                        FurnitureKind::Lamp {
                            style: LampStyle::Pendant | LampStyle::Wall,
                            ..
                        }
                    ) {
                        "Shade height"
                    } else {
                        "Height"
                    },
                );
                cm_value(ui, &mut item.height);
                ui.end_row();

                if is_model && app.lock_model_proportions {
                    keep_proportions(item, before);
                }

                ui.label("Above floor")
                    .on_hover_text("Raise it, e.g. a lamp on a table or a shelf on a wall");
                cm_value(ui, &mut item.elevation);
                ui.end_row();

                ui.label("Rotation");
                ui.horizontal(|ui| {
                    let mut degrees = item.rotation.to_degrees().rem_euclid(360.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut degrees)
                                .speed(1.0)
                                .suffix("°")
                                .max_decimals(0),
                        )
                        .changed()
                    {
                        item.rotation = degrees.rem_euclid(360.0).to_radians();
                    }
                    if ui
                        .small_button("⟲")
                        .on_hover_text("Rotate 90° left [Shift+R]")
                        .clicked()
                    {
                        rotate(item, -90.0);
                    }
                    if ui
                        .small_button("⟳")
                        .on_hover_text("Rotate 90° right [R]")
                        .clicked()
                    {
                        rotate(item, 90.0);
                    }
                });
                ui.end_row();

                if !is_model {
                    ui.label(if item.is_lamp() {
                        "Shade color"
                    } else {
                        "Color"
                    });
                    ui.color_edit_button_srgb(&mut item.color);
                    ui.end_row();
                }

                kind_params(ui, item);
            });

        if let FurnitureKind::Model { path } = &item.kind {
            ui.checkbox(&mut app.lock_model_proportions, "Keep proportions");
            if ui.button("Reset to original size").clicked() {
                if let Ok(mesh) = app.models.get(path) {
                    item.width = mesh.size[0];
                    item.height = mesh.size[1];
                    item.depth = mesh.size[2];
                }
            }
            ui.label(RichText::new(path.as_str()).small().color(TEXT_MUTED));
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            duplicate = ui.button("Duplicate").on_hover_text("[Ctrl+D]").clicked();
            save_to_library = ui
                .button("☆ Save to My furniture")
                .on_hover_text("Reuse this piece in other rooms and plans")
                .clicked();
            delete = ui.button("🗑 Delete").on_hover_text("[Del]").clicked();
        });
    });

    if duplicate {
        app.duplicate_selected_furniture();
    }
    if save_to_library {
        app.save_to_library();
    }
    if delete {
        app.delete_selected_furniture();
    }
}

fn options_row(ui: &mut egui::Ui, item: &mut Furniture) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Options").color(TEXT_MUTED))
            .on_hover_text(
                "Try alternatives in the same spot, e.g. different lamps. Press O to cycle.",
            );
        let mut switch_to = None;
        for (i, option) in item.options.iter().enumerate() {
            let letter = (b'A' + (i % 26) as u8) as char;
            let active = i == item.active_option;
            let name = if active { &item.name } else { &option.name };
            if ui
                .selectable_label(active, RichText::new(letter.to_string()).strong())
                .on_hover_text(name)
                .clicked()
            {
                switch_to = Some(i);
            }
        }
        if let Some(i) = switch_to {
            item.switch_option(i);
        }
        if ui
            .small_button("+")
            .on_hover_text("Add an option to compare (copies the current one)")
            .clicked()
        {
            item.add_option();
        }
        if item.options.len() > 1
            && ui
                .small_button("−")
                .on_hover_text("Remove this option")
                .clicked()
        {
            item.remove_option();
        }
    });
}

fn opening_card(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let Some((wi, _)) = app.selected_opening else {
        return;
    };
    let wall_length = app.walls[wi].length();
    let mut delete = false;
    card(ui, |ui| {
        let Some(opening) = app.selected_opening_mut() else {
            return;
        };
        ui.horizontal(|ui| {
            let (picture, _) = ui.allocate_exact_size(egui::vec2(44.0, 36.0), egui::Sense::hover());
            thumbnail::paint_opening(ui.painter(), picture, opening);
            ui.heading(opening.kind.label());
        });
        ui.add_space(2.0);
        egui::Grid::new("opening_props")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Type");
                ui.horizontal(|ui| {
                    for kind in [
                        OpeningKind::Window,
                        OpeningKind::Door,
                        OpeningKind::GlassDoor,
                    ] {
                        if ui
                            .selectable_value(&mut opening.kind, kind, kind.label())
                            .changed()
                            && kind.swings()
                        {
                            opening.sill = 0.0;
                        }
                    }
                });
                ui.end_row();
                ui.label("Width");
                cm_value(ui, &mut opening.width);
                ui.end_row();
                ui.label("Height");
                cm_value(ui, &mut opening.height);
                ui.end_row();
                if !opening.kind.swings() {
                    ui.label("Above floor")
                        .on_hover_text("Sill height: from the floor to the bottom of the window");
                    cm_value(ui, &mut opening.sill);
                    ui.end_row();
                }
                ui.label("From wall end")
                    .on_hover_text("Distance from the start of the wall to the near edge");
                let mut edge = opening.clamped_offset(wall_length) - opening.width / 2.0;
                if cm_value(ui, &mut edge).changed() {
                    opening.offset = edge + opening.width / 2.0;
                }
                ui.end_row();
                if opening.kind.swings() {
                    ui.label("Swing");
                    ui.horizontal(|ui| {
                        if ui.button("Flip hinge").clicked() {
                            opening.flip_hinge = !opening.flip_hinge;
                        }
                        if ui.button("Flip side").clicked() {
                            opening.flip_side = !opening.flip_side;
                        }
                    });
                    ui.end_row();
                }
            });
        let max_width = wall_length.max(0.1);
        opening.width = opening.width.clamp(0.2, max_width);
        opening.height = opening.height.clamp(0.2, WALL_HEIGHT);
        opening.sill = opening
            .sill
            .clamp(0.0, (WALL_HEIGHT - opening.height).max(0.0));
        opening.offset = opening.clamped_offset(wall_length);
        ui.add_space(6.0);
        delete = ui.button("🗑 Delete").on_hover_text("[Del]").clicked();
    });
    if delete {
        app.delete_selection();
    }
}

pub fn rotate(item: &mut Furniture, degrees: f32) {
    let snapped = ((item.rotation.to_degrees() + degrees) / 90.0).round() * 90.0;
    item.rotation = snapped.rem_euclid(360.0).to_radians();
}

/// Edits a length stored in meters as centimeters.
fn cm_value(ui: &mut egui::Ui, meters: &mut f32) -> egui::Response {
    let mut cm = *meters * 100.0;
    let response = ui.add(
        egui::DragValue::new(&mut cm)
            .range(0.0..=2000.0)
            .speed(1.0)
            .max_decimals(1)
            .suffix(" cm"),
    );
    if response.changed() {
        *meters = (cm / 100.0).max(0.0);
    }
    response
}

/// If one dimension of the item changed, scales the other two by the same factor.
fn keep_proportions(item: &mut Furniture, before: [f32; 3]) {
    let after = [item.width, item.depth, item.height];
    let Some(changed) = (0..3).find(|&i| after[i] != before[i]) else {
        return;
    };
    if before[changed] <= 0.0 {
        return;
    }
    let factor = after[changed] / before[changed];
    let mut scaled = before.map(|v| v * factor);
    scaled[changed] = after[changed];
    [item.width, item.depth, item.height] = scaled;
}

fn kind_params(ui: &mut egui::Ui, item: &mut Furniture) {
    let mut new_elevation = None;
    match &mut item.kind {
        FurnitureKind::Box | FurnitureKind::Model { .. } | FurnitureKind::Plant => {}
        FurnitureKind::Table {
            top_thickness,
            leg_size,
            round,
        } => {
            ui.label("Shape");
            ui.horizontal(|ui| {
                ui.selectable_value(round, false, "Rectangular");
                ui.selectable_value(round, true, "Round");
            });
            ui.end_row();
            ui.label("Top thickness");
            cm_value(ui, top_thickness);
            ui.end_row();
            ui.label(if *round { "Pedestal" } else { "Leg size" });
            cm_value(ui, leg_size);
            ui.end_row();
        }
        FurnitureKind::Chair { seat_height } => {
            ui.label("Seat height");
            cm_value(ui, seat_height);
            ui.end_row();
        }
        FurnitureKind::Bed { headboard_height } => {
            ui.label("Headboard");
            cm_value(ui, headboard_height);
            ui.end_row();
        }
        FurnitureKind::Sofa {
            seat_height,
            arm_width,
            back_depth,
        } => {
            ui.label("Seat height");
            cm_value(ui, seat_height);
            ui.end_row();
            ui.label("Armrest width");
            cm_value(ui, arm_width);
            ui.end_row();
            ui.label("Backrest depth");
            cm_value(ui, back_depth);
            ui.end_row();
        }
        FurnitureKind::Shelf { shelves } => {
            ui.label("Shelf boards");
            ui.add(egui::DragValue::new(shelves).range(2..=30));
            ui.end_row();
        }
        FurnitureKind::Lamp {
            style,
            shade,
            shade_height,
            lumens,
            kelvin,
            on,
        } => {
            ui.label("Light");
            ui.horizontal(|ui| {
                ui.toggle_value(on, if *on { "💡 On" } else { "Off" });
            });
            ui.end_row();

            ui.label("Type");
            let before = *style;
            egui::ComboBox::from_id_salt("lamp_style")
                .selected_text(style_label(*style))
                .show_ui(ui, |ui| {
                    for s in [
                        LampStyle::Floor,
                        LampStyle::Table,
                        LampStyle::Pendant,
                        LampStyle::Wall,
                    ] {
                        ui.selectable_value(style, s, style_label(s));
                    }
                });
            if *style != before {
                new_elevation = Some(match style {
                    LampStyle::Floor => 0.0,
                    LampStyle::Table => 0.75,
                    LampStyle::Pendant | LampStyle::Wall => 1.6,
                });
            }
            ui.end_row();

            ui.label("Shade");
            ui.horizontal(|ui| {
                ui.selectable_value(shade, ShadeShape::Cone, "Cone");
                ui.selectable_value(shade, ShadeShape::Drum, "Drum");
                ui.selectable_value(shade, ShadeShape::Globe, "Globe");
            });
            ui.end_row();

            if matches!(style, LampStyle::Floor | LampStyle::Table) {
                ui.label("Shade height");
                cm_value(ui, shade_height);
                ui.end_row();
            }

            ui.label("Brightness");
            ui.add(
                egui::Slider::new(lumens, 100.0..=3000.0)
                    .suffix(" lm")
                    .step_by(50.0),
            )
            .on_hover_text("A classic 60 W bulb is about 800 lm");
            ui.end_row();

            ui.label("Color temp.");
            ui.vertical(|ui| {
                ui.add(
                    egui::Slider::new(kelvin, 2000.0..=6500.0)
                        .suffix(" K")
                        .step_by(100.0),
                );
                let (label, hint) = match *kelvin {
                    k if k < 2500.0 => ("Extra warm", "candle-like, cozy"),
                    k if k < 3200.0 => ("Warm white", "typical living room light"),
                    k if k < 4500.0 => ("Neutral white", "kitchens, work areas"),
                    _ => ("Daylight", "crisp, for tasks"),
                };
                ui.label(
                    RichText::new(format!("{label} – {hint}"))
                        .small()
                        .color(TEXT_MUTED),
                );
            });
            ui.end_row();
        }
    }
    if let Some(elevation) = new_elevation {
        item.elevation = elevation;
    }
}

fn style_label(style: LampStyle) -> &'static str {
    match style {
        LampStyle::Floor => "Floor lamp",
        LampStyle::Table => "Table lamp",
        LampStyle::Pendant => "Pendant (ceiling)",
        LampStyle::Wall => "Wall lamp",
    }
}
