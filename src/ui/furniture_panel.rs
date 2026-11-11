use crate::{
    app::{same_type, RoomPlannerApp},
    furniture::{Category, CubeLayout, PRESETS},
    geometry::openings::{OpeningPreset, OPENING_PRESETS},
    models::{FloorFinish, Furniture, FurnitureKind, LampStyle, Opening, OpeningKind, ShadeShape},
    ui::{
        theme::{section_label, ACCENT, TEXT_DARK, TEXT_MUTED},
        thumbnail,
    },
};
use eframe::egui::{self, Button, RichText};

/// Tabs of the left panel.
#[derive(Clone, Copy, PartialEq)]
pub enum LeftTab {
    Add,
    Home,
}

/// Which part of the library is showing.
#[derive(Clone, Copy, PartialEq)]
pub enum LibrarySection {
    Openings,
    Mine,
    Category(Category),
    Import,
}

pub fn show(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    egui::Panel::left("furniture_panel")
        .resizable(true)
        .default_size((ui.available_width() * 0.22).clamp(220.0, 290.0))
        .size_range(190.0..=420.0)
        .show_inside(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                for (tab, label) in [(LeftTab::Add, "+ Add"), (LeftTab::Home, "🏠 Home")] {
                    let selected = app.left_tab == tab;
                    let text = if selected {
                        RichText::new(label).strong().color(ACCENT)
                    } else {
                        RichText::new(label)
                    };
                    if ui
                        .add_sized([width, 28.0], egui::Button::selectable(selected, text))
                        .clicked()
                    {
                        app.left_tab = tab;
                    }
                }
            });
            ui.add_space(4.0);
            match app.left_tab {
                LeftTab::Add => library(app, ui),
                LeftTab::Home => {
                    crate::ui::color::collect_plan_colors(app, ui.ctx());
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        finishes(app, ui);
                        ui.add_space(8.0);
                        plan_contents(app, ui);
                    });
                }
            }
        });
}

pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, crate::ui::theme::BORDER))
        .corner_radius(8)
        .inner_margin(10)
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: egui::Color32::from_black_alpha(30),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

// --- Library ---

fn library(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    ui.add(
        egui::TextEdit::singleline(&mut app.library_filter)
            .hint_text("🔍 Search everything…")
            .desired_width(f32::INFINITY),
    );
    let filter = app.library_filter.trim().to_lowercase();
    let searching = !filter.is_empty();

    // One section at a time instead of one long list; searching looks through all of them.
    if !searching {
        if app.library_section == LibrarySection::Mine && app.my_library.is_empty() {
            app.library_section = LibrarySection::Openings;
        }
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            let mut chip = |ui: &mut egui::Ui, section: LibrarySection, label: &str| {
                if ui
                    .selectable_label(app.library_section == section, label)
                    .clicked()
                {
                    app.library_section = section;
                }
            };
            chip(ui, LibrarySection::Openings, "🚪 Doors & windows");
            if !app.my_library.is_empty() {
                chip(ui, LibrarySection::Mine, "☆ Mine");
            }
            for category in Category::ALL {
                chip(ui, LibrarySection::Category(category), category.label());
            }
            #[cfg(not(target_arch = "wasm32"))]
            chip(ui, LibrarySection::Import, "📦 Import");
        });
        ui.separator();
    }
    let section = app.library_section;
    let show = |s: LibrarySection| searching || section == s;
    let mut to_add: Option<Furniture> = None;
    let mut remove = None;
    let mut place_opening = None;
    let mut import = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        if show(LibrarySection::Mine) && !app.my_library.is_empty() {
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
                    Some(TileAction::Remove(name)) => remove = Some(name),
                    None => {}
                }
            }
        }

        if show(LibrarySection::Openings) {
            let openings: Vec<&OpeningPreset> = OPENING_PRESETS
                .iter()
                .filter(|p| filter.is_empty() || p.label.to_lowercase().contains(&filter))
                .collect();
            if !openings.is_empty() {
                section_label(ui, "Doors & windows");
                if let Some(template) = opening_grid(ui, &openings) {
                    place_opening = Some(template);
                }
                if !searching {
                    ui.label(
                        RichText::new(
                            "Click one, then a wall · or drag it onto a wall. \
                             Exact sizes can be typed in its properties.",
                        )
                        .small()
                        .color(TEXT_MUTED),
                    );
                }
            }
        }

        for category in Category::ALL {
            if !show(LibrarySection::Category(category)) {
                continue;
            }
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

        if show(LibrarySection::Import) && !searching {
            section_label(ui, "Your own models");
            import = ui
                .add(
                    Button::new("Import 3D model (.glb / .gltf)…")
                        .min_size(egui::vec2(ui.available_width(), 26.0)),
                )
                .on_hover_text("For example a model downloaded from a furniture store")
                .clicked();
        }
        if let Some(err) = &app.model_error {
            ui.colored_label(egui::Color32::from_rgb(200, 40, 40), err);
        }
    });

    if let Some(name) = remove {
        app.my_library.retain(|i| i.name != name);
    }
    if let Some(template) = place_opening {
        app.start_placing_opening(template);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if import {
        import_model(app);
    }
    let _ = import;
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
    section_label(ui, "Walls & floor");
    {
        egui::Grid::new("finishes")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Wall color");
                ui.horizontal(|ui| {
                    crate::ui::color::color_button(ui, &mut app.finishes.wall_color);
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
    }
}

fn plan_contents(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    if app.furniture.is_empty() {
        return;
    }
    section_label(
        ui,
        &format!("Furniture in this plan ({})", app.furniture.len()),
    );
    {
        let mut clicked = None;
        for (idx, item) in app.furniture.iter().enumerate() {
            let selected = app.is_furniture_selected(idx);
            let mut label = format!("{}  {}", item.name, size_text(item));
            if item.options.len() > 1 {
                label += &format!("  · {} options", item.options.len());
            }
            let response = ui
                .selectable_label(selected, label)
                .on_hover_text("Shift+click to select several");
            if response.clicked() {
                clicked = Some((idx, ui.input(|i| i.modifiers.shift)));
            }
        }
        if let Some((idx, shift)) = clicked {
            if shift {
                app.toggle_furniture(idx);
            } else {
                app.select_furniture(Some(idx));
            }
            app.selected_walls.clear();
        }
    }
}

fn size_text(item: &Furniture) -> String {
    format!("{:.0}×{:.0}", item.width * 100.0, item.depth * 100.0)
}

// --- Properties ---

pub fn properties_card(app: &mut RoomPlannerApp, ui: &mut egui::Ui, idx: usize) {
    let mut duplicate = false;
    let mut delete = false;
    let mut save_to_library = false;
    let mut deselect = false;
    let mut select_similar = false;
    let before = app.furniture[idx].clone();
    let ceiling = crate::geometry::walls::ceiling_height_at(&app.walls, before.position);
    let extra = app.extra_furniture.len();
    let similar = app
        .furniture
        .iter()
        .filter(|other| same_type(other, &before))
        .count();

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
            deselect = ui.button("×").on_hover_text("Deselect").clicked();
        });
        if deselect {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if extra > 0 {
                ui.label(
                    RichText::new(format!("+{extra} more selected · changes apply to all"))
                        .color(ACCENT),
                );
            }
            if similar > extra + 1 {
                select_similar = ui
                    .small_button(format!("Select all {similar}"))
                    .on_hover_text("Select every piece with this name and type")
                    .clicked();
            }
        });

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
                    // A turn of 359.99…° shows as 0°, not 360°.
                    let mut degrees = item.rotation.to_degrees().rem_euclid(360.0);
                    if degrees > 359.95 {
                        degrees = 0.0;
                    }
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
                    crate::ui::color::color_button(ui, &mut item.color);
                    ui.end_row();
                }

                if item.has_top() {
                    top_row(ui, item);
                }

                kind_params(ui, item, ceiling);
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

    copy_edits_to_selection(app, idx, &before);
    if deselect {
        app.select_furniture(None);
    }
    if select_similar {
        app.select_similar_furniture();
    }
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

/// Wood tones for tops, like the usual worktop woods.
const TOP_WOODS: [(&str, [u8; 3]); 5] = [
    ("Birch", [226, 198, 156]),
    ("Oak", [200, 154, 104]),
    ("Beech", [212, 162, 118]),
    ("Dark oak", [142, 100, 66]),
    ("Walnut", [116, 78, 52]),
];

/// The look of the top of a cabinet, table or shelf: like the rest, its own color, or wood.
fn top_row(ui: &mut egui::Ui, item: &mut Furniture) {
    #[derive(PartialEq, Clone, Copy)]
    enum Look {
        Same,
        Color,
        Wood,
    }
    let look = if item.top_wood {
        Look::Wood
    } else if item.top_color.is_some() {
        Look::Color
    } else {
        Look::Same
    };
    ui.label("Top")
        .on_hover_text("The top surface, like a countertop on a cabinet");
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut chosen = look;
            ui.selectable_value(&mut chosen, Look::Same, "Same")
                .on_hover_text("The same as the rest of the piece");
            ui.selectable_value(&mut chosen, Look::Color, "Color")
                .on_hover_text("A color of its own, like a stone or laminate countertop");
            ui.selectable_value(&mut chosen, Look::Wood, "Wood")
                .on_hover_text("Solid wood, like a butcher-block worktop");
            if chosen != look {
                match chosen {
                    Look::Same => {
                        item.top_color = None;
                        item.top_wood = false;
                    }
                    Look::Color => {
                        item.top_wood = false;
                        item.top_color = Some(item.top_color.unwrap_or(item.color));
                    }
                    Look::Wood => {
                        item.top_wood = true;
                        item.top_color = Some(TOP_WOODS[1].1);
                    }
                }
            }
        });
        if let Some(top) = &mut item.top_color {
            ui.horizontal(|ui| {
                crate::ui::color::color_button(ui, top);
                if item.top_wood {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for (name, tone) in TOP_WOODS {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                        let selected = *top == tone;
                        ui.painter().rect_filled(
                            rect,
                            3.0,
                            egui::Color32::from_rgb(tone[0], tone[1], tone[2]),
                        );
                        ui.painter().rect_stroke(
                            rect,
                            3.0,
                            egui::Stroke::new(
                                if selected { 2.0 } else { 1.0 },
                                if selected {
                                    ACCENT
                                } else {
                                    crate::ui::theme::BORDER
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        if response.on_hover_text(name).clicked() {
                            *top = tone;
                        }
                    }
                }
            });
        }
    });
    ui.end_row();
}

/// Copies whatever was just changed on the piece in the panel to the rest of the selection.
/// Name and type-specific settings only go to pieces of the same type.
fn copy_edits_to_selection(app: &mut RoomPlannerApp, idx: usize, before: &Furniture) {
    let after = app.furniture[idx].clone();
    if after == *before {
        return;
    }
    let same_kind =
        |f: &Furniture| std::mem::discriminant(&f.kind) == std::mem::discriminant(&before.kind);
    for other in app.extra_furniture.clone() {
        let Some(item) = app.furniture.get_mut(other) else {
            continue;
        };
        if after.width != before.width {
            item.width = after.width;
        }
        if after.depth != before.depth {
            item.depth = after.depth;
        }
        if after.height != before.height {
            item.height = after.height;
        }
        if after.elevation != before.elevation {
            item.elevation = after.elevation;
        }
        if after.color != before.color {
            item.color = after.color;
        }
        if after.top_color != before.top_color && item.has_top() {
            item.top_color = after.top_color;
        }
        if after.top_wood != before.top_wood && item.has_top() {
            item.top_wood = after.top_wood;
        }
        if after.rotation != before.rotation {
            item.rotation = (item.rotation + after.rotation - before.rotation)
                .rem_euclid(std::f32::consts::TAU);
        }
        if after.name != before.name && same_type(item, before) {
            item.name = after.name.clone();
        }
        if after.kind != before.kind && same_kind(item) {
            item.kind = after.kind.clone();
        }
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

/// A "×" at the right end of a card's title row; true when clicked.
pub fn close_button(ui: &mut egui::Ui) -> bool {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.button("×").on_hover_text("Deselect [Esc]").clicked()
    })
    .inner
}

pub fn opening_card(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let Some((wi, _)) = app.selected_opening else {
        return;
    };
    let wall_length = app.walls[wi].length();
    let wall_height = app.walls[wi].height;
    let mut delete = false;
    let mut deselect = false;
    card(ui, |ui| {
        let Some(opening) = app.selected_opening_mut() else {
            return;
        };
        ui.horizontal(|ui| {
            let (picture, _) = ui.allocate_exact_size(egui::vec2(44.0, 36.0), egui::Sense::hover());
            thumbnail::paint_opening(ui.painter(), picture, opening);
            ui.heading(opening.kind.label());
            deselect = close_button(ui);
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
        opening.height = opening.height.clamp(0.2, wall_height.max(0.2));
        opening.sill = opening
            .sill
            .clamp(0.0, (wall_height - opening.height).max(0.0));
        opening.offset = opening.clamped_offset(wall_length);
        ui.add_space(6.0);
        delete = ui.button("🗑 Delete").on_hover_text("[Del]").clicked();
    });
    if delete {
        app.delete_selection();
    } else if deselect {
        app.clear_selection();
    }
}

pub fn rotate(item: &mut Furniture, degrees: f32) {
    let snapped = ((item.rotation.to_degrees() + degrees) / 90.0).round() * 90.0;
    item.rotation = snapped.rem_euclid(360.0).to_radians();
}

/// Edits a length stored in meters as centimeters.
pub fn cm_value(ui: &mut egui::Ui, meters: &mut f32) -> egui::Response {
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

/// Settings that depend on the kind of piece. `ceiling` is the ceiling height where it
/// stands, for lamps hanging from it.
fn kind_params(ui: &mut egui::Ui, item: &mut Furniture, ceiling: f32) {
    let mut new_elevation = None;
    let cable = item.cable_length(ceiling);
    let height = item.height;
    // (width, depth, height) to switch to, e.g. when a lamp becomes a spot track.
    let mut new_size: Option<(f32, f32, f32)> = None;
    let mut new_width = None;
    let width = item.width;
    match &mut item.kind {
        FurnitureKind::Box
        | FurnitureKind::Model { .. }
        | FurnitureKind::Plant
        | FurnitureKind::Person => {}
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
        FurnitureKind::CubeShelf { .. } => cube_shelf_params(ui, item),
        FurnitureKind::Lamp {
            style,
            shade,
            shade_height,
            lumens,
            kelvin,
            on,
            spots,
            spread,
            tilt,
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
                        LampStyle::Track,
                        LampStyle::Downlights,
                        LampStyle::LinearPendant,
                        LampStyle::Strip,
                    ] {
                        ui.selectable_value(style, s, style_label(s));
                    }
                });
            if *style != before {
                // Ceiling lights go up to this room's ceiling.
                // A shape and place that suits the new type.
                let (size, elevation) = match style {
                    LampStyle::Floor => ((0.4, 0.4, 1.5), 0.0),
                    LampStyle::Table => ((0.3, 0.3, 0.45), 0.75),
                    LampStyle::Pendant => ((0.4, 0.4, 0.25), 1.6),
                    LampStyle::Wall => ((0.2, 0.2, 0.2), 1.6),
                    LampStyle::Track => ((1.6, 0.08, 0.18), ceiling - 0.18),
                    LampStyle::Downlights => ((2.4, 0.1, 0.02), ceiling - 0.02),
                    LampStyle::LinearPendant => ((1.2, 0.07, 0.05), 1.75),
                    LampStyle::Strip => ((0.8, 0.03, 0.01), 1.4),
                };
                if before.has_shade() != style.has_shade() {
                    new_size = Some(size);
                }
                if style.has_spots() && *spots < 2 {
                    *spots = 4;
                }
                new_elevation = Some(elevation);
            }
            ui.end_row();

            if style.has_shade() {
                ui.label("Shade");
                ui.horizontal(|ui| {
                    ui.selectable_value(shade, ShadeShape::Cone, "Cone");
                    ui.selectable_value(shade, ShadeShape::Drum, "Drum");
                    ui.selectable_value(shade, ShadeShape::Globe, "Globe");
                });
                ui.end_row();
            }

            if let Some(mut length) = cable {
                ui.label("Cable").on_hover_text(
                    "From the ceiling to the top of the lamp. Longer hangs it lower",
                );
                if cm_value(ui, &mut length).changed() {
                    let length = length.clamp(0.0, (ceiling - height).max(0.0));
                    new_elevation = Some(ceiling - height - length);
                }
                ui.end_row();
            }

            if style.has_spots() {
                ui.label("Spots");
                ui.add(egui::DragValue::new(spots).range(1..=24))
                    .on_hover_text("Spread evenly along the length");
                ui.end_row();

                ui.label("Spacing").on_hover_text(
                    "From one spot to the next; changing it makes the row longer or shorter",
                );
                let count = (*spots).max(1) as f32;
                let mut spacing = width / count;
                if cm_value(ui, &mut spacing).changed() && spacing > 0.02 {
                    new_width = Some(spacing * count);
                }
                ui.end_row();

                ui.label("Spread").on_hover_text(
                    "Turns the outer spots outwards, the ones between in proportion, so a \
                     short track can light a long counter. The lines show where they point",
                );
                ui.add(
                    egui::DragValue::new(spread)
                        .range(-45.0..=45.0)
                        .speed(0.5)
                        .max_decimals(0)
                        .suffix("°"),
                );
                ui.end_row();

                ui.label("Tilt").on_hover_text(
                    "Leans all spots forward (+) or back (−), e.g. towards the wall. \
                     The lines show where they point",
                );
                ui.add(
                    egui::DragValue::new(tilt)
                        .range(-60.0..=60.0)
                        .speed(0.5)
                        .max_decimals(0)
                        .suffix("°"),
                );
                ui.end_row();
            }

            if matches!(style, LampStyle::Floor | LampStyle::Table) {
                ui.label("Shade height");
                cm_value(ui, shade_height);
                ui.end_row();
            }

            ui.label(if style.has_spots() {
                "Per spot"
            } else {
                "Brightness"
            });
            ui.add(
                egui::Slider::new(
                    lumens,
                    100.0..=if style.is_linear() { 6000.0 } else { 3000.0 },
                )
                .suffix(" lm")
                .step_by(50.0),
            )
            .on_hover_text(if style.has_spots() {
                "Brightness of each spot. A typical LED spot is 300–500 lm"
            } else {
                "A classic 60 W bulb is about 800 lm"
            });
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
    if let Some((w, d, h)) = new_size {
        (item.width, item.depth, item.height) = (w, d, h);
    }
    if let Some(w) = new_width {
        item.width = w;
    }
}

/// Column and row counts plus one box size used for every compartment: changing it, or the
/// counts, grows or shrinks the whole shelf. Sizes per column and row are there for the
/// rare shelf that needs them.
fn cube_shelf_params(ui: &mut egui::Ui, item: &mut Furniture) {
    let Some(layout) = CubeLayout::of(item) else {
        return;
    };
    let FurnitureKind::CubeShelf {
        columns,
        rows,
        column_widths,
        row_heights,
    } = &mut item.kind
    else {
        return;
    };
    let custom = !column_widths.is_empty() || !row_heights.is_empty();
    // Outer size of a shelf with these inner sizes.
    let outer = |cells: &[f32]| {
        cells.iter().sum::<f32>()
            + 2.0 * layout.frame
            + layout.divider * cells.len().saturating_sub(1) as f32
    };

    ui.label("Columns");
    let (old_columns, old_rows) = (*columns, *rows);
    ui.add(egui::DragValue::new(columns).range(1..=10));
    ui.end_row();
    ui.label("Rows");
    ui.add(egui::DragValue::new(rows).range(1..=10));
    ui.end_row();
    if (*columns, *rows) != (old_columns, old_rows) {
        // The boxes keep their size and the shelf grows; new ones copy the last.
        let mut widths = layout.columns.clone();
        let mut heights = layout.rows.clone();
        widths.resize(*columns as usize, widths.last().copied().unwrap_or(0.3));
        heights.resize(*rows as usize, heights.last().copied().unwrap_or(0.3));
        item.width = outer(&widths).max(0.05);
        item.height = outer(&heights).max(0.05);
        if custom {
            *column_widths = widths;
            *row_heights = heights;
        }
        return;
    }

    if !custom {
        // All boxes alike: one size for the lot.
        let mut box_width = layout.columns[0];
        let mut box_height = layout.rows[0];
        ui.label("Box width")
            .on_hover_text("Inside width of every box");
        let width_changed = cm_value(ui, &mut box_width).changed();
        ui.end_row();
        ui.label("Box height")
            .on_hover_text("Inside height of every box");
        let height_changed = cm_value(ui, &mut box_height).changed();
        ui.end_row();
        if width_changed {
            item.width = outer(&vec![box_width.max(0.01); layout.columns.len()]);
        }
        if height_changed {
            item.height = outer(&vec![box_height.max(0.01); layout.rows.len()]);
        }
    } else {
        let mut width_delta = 0.0;
        let mut height_delta = 0.0;
        ui.label("Column widths");
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                let mut sizes = layout.columns.clone();
                for i in 0..sizes.len() {
                    if cm_value(ui, &mut sizes[i])
                        .on_hover_text(format!("Inside width of column {}", i + 1))
                        .changed()
                    {
                        width_delta = sizes[i] - layout.columns[i];
                        *column_widths = sizes.clone();
                        *row_heights = layout.rows.clone();
                    }
                }
            });
        });
        ui.end_row();

        ui.label("Row heights");
        ui.vertical(|ui| {
            // Listed top to bottom, like the shelf itself.
            let mut sizes = layout.rows.clone();
            for j in (0..sizes.len()).rev() {
                let label = match j {
                    _ if sizes.len() == 1 => "",
                    0 => " (bottom)",
                    _ if j == sizes.len() - 1 => " (top)",
                    _ => "",
                };
                if cm_value(ui, &mut sizes[j])
                    .on_hover_text(format!("Inside height of row {}{label}", j + 1))
                    .changed()
                {
                    height_delta = sizes[j] - layout.rows[j];
                    *row_heights = sizes.clone();
                    *column_widths = layout.columns.clone();
                }
            }
        });
        ui.end_row();
        item.width = (item.width + width_delta).max(0.05);
        item.height = (item.height + height_delta).max(0.05);
    }

    ui.label("");
    let mut different = custom;
    if ui
        .checkbox(&mut different, "Different sizes")
        .on_hover_text("Set the size of each column and row separately")
        .changed()
    {
        if different {
            *column_widths = layout.columns.clone();
            *row_heights = layout.rows.clone();
        } else {
            // Back to equal boxes, spread over the current size.
            column_widths.clear();
            row_heights.clear();
        }
    }
    ui.end_row();
}

fn style_label(style: LampStyle) -> &'static str {
    match style {
        LampStyle::Floor => "Floor lamp",
        LampStyle::Table => "Table lamp",
        LampStyle::Pendant => "Pendant (ceiling)",
        LampStyle::Wall => "Wall lamp",
        LampStyle::Track => "Spot track (ceiling)",
        LampStyle::Downlights => "Ceiling spots, in a row",
        LampStyle::LinearPendant => "Bar pendant",
        LampStyle::Strip => "LED strip",
    }
}
