//! The right-hand panel: exact sizes and settings for whatever is selected.

use crate::{
    app::{RoomPlannerApp, PIXELS_PER_METER},
    geometry::walls::clear_length,
    models::Tool,
    ui::{
        furniture_panel::{card, close_button, cm_value, opening_card, properties_card},
        theme::TEXT_MUTED,
    },
};
use eframe::egui::{self, RichText};

/// Width of the floating properties card.
const WIDTH: f32 = 270.0;

/// Floats over the right side of the viewport, so selecting something never moves the view.
pub fn show(app: &mut RoomPlannerApp, ui: &egui::Ui, canvas: egui::Rect) {
    let furniture = app.selected_furniture.filter(|&i| i < app.furniture.len());
    let has_selection =
        furniture.is_some() || !app.selected_walls.is_empty() || app.selected_opening.is_some();
    let drawing = app.current_tool == Tool::DrawWall;
    if !has_selection && !drawing {
        return;
    }
    crate::ui::color::collect_plan_colors(app, ui.ctx());
    // Leave room for the zoom controls in the bottom-right corner.
    let max_height = (canvas.height() - 12.0 - 56.0).max(120.0);
    egui::Area::new(egui::Id::new("properties_panel"))
        .fixed_pos(canvas.right_top() + egui::vec2(-4.0 - WIDTH, 4.0))
        .order(egui::Order::Middle)
        .constrain_to(canvas)
        .show(ui.ctx(), |ui| {
            // Room around the cards for their shadows, which the scroll area would clip.
            egui::Frame::new()
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(WIDTH - 16.0);
                    // An area starts each frame at last frame's size; let taller cards grow it.
                    ui.set_max_height(max_height);
                    egui::ScrollArea::vertical()
                        .max_height(max_height)
                        .show(ui, |ui| {
                            if app.selected_opening_mut().is_some() {
                                opening_card(app, ui);
                                ui.add_space(8.0);
                            }
                            if !app.selected_walls.is_empty() {
                                wall_card(app, ui);
                                ui.add_space(8.0);
                            }
                            if let Some(idx) = furniture {
                                properties_card(app, ui, idx);
                            }
                            if drawing && !has_selection {
                                drawing_card(app, ui);
                            }
                        });
                });
        });
}

fn drawing_card(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.heading("Drawing walls");
        ui.add_space(2.0);
        ui.label("Click to place corners. Walls snap to corners, other walls and right angles.");
        ui.add_space(4.0);
        ui.label(RichText::new("Exact length").strong());
        ui.label(
            "After the first corner, point in the direction you want and type the length, \
             then press Enter:",
        );
        egui::Grid::new("length_examples")
            .num_columns(2)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                ui.monospace("350");
                ui.label("350 cm");
                ui.end_row();
                ui.monospace("3.5");
                ui.label("3.5 m");
                ui.end_row();
            });
        ui.label(
            RichText::new("Measured along the middle of the wall.")
                .small()
                .color(TEXT_MUTED),
        );
        if !app.length_input.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Typed: {}", app.length_input)).strong());
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("Esc stops drawing · V switches to Select")
                .small()
                .color(TEXT_MUTED),
        );
    });
}

/// Properties of the selected walls; size edits work on a single wall, height on all.
fn wall_card(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let mut selected: Vec<usize> = app
        .selected_walls
        .iter()
        .copied()
        .filter(|&i| i < app.walls.len())
        .collect();
    selected.sort_unstable();
    let Some(&first) = selected.first() else {
        return;
    };
    let mut delete = false;
    let mut deselect = false;
    let mut new_geometry = None;
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(if selected.len() == 1 {
                "Wall".to_owned()
            } else {
                format!("{} walls", selected.len())
            });
            deselect = close_button(ui);
        });
        ui.add_space(2.0);
        egui::Grid::new("wall_props")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                if selected.len() == 1 {
                    let wall = &app.walls[first];
                    let length = wall.length();
                    let dir = (wall.end - wall.start) / (length * PIXELS_PER_METER).max(1e-6);

                    ui.label("Length")
                        .on_hover_text("Along the middle of the wall, corner to corner");
                    let mut l = length;
                    if cm_value(ui, &mut l).changed() && l > 0.01 {
                        new_geometry = Some((l, dir));
                    }
                    ui.end_row();

                    let clear = clear_length(&app.walls, first);
                    ui.label("Inside length").on_hover_text(
                        "The wall surface you could measure with a tape, between the walls \
                         that join it",
                    );
                    let mut c = clear;
                    if cm_value(ui, &mut c).changed() {
                        let l = length + (c - clear);
                        if l > 0.01 {
                            new_geometry = Some((l, dir));
                        }
                    }
                    ui.end_row();

                    ui.label("Angle")
                        .on_hover_text("0° points right, 90° points down");
                    let mut degrees = dir.y.atan2(dir.x).to_degrees().rem_euclid(360.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut degrees)
                                .speed(0.5)
                                .max_decimals(1)
                                .suffix("°"),
                        )
                        .changed()
                    {
                        let a = degrees.to_radians();
                        new_geometry = Some((length, egui::vec2(a.cos(), a.sin())));
                    }
                    ui.end_row();

                    ui.label("Keep fixed");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut app.wall_anchor_end, false, "Start")
                            .on_hover_text("Changing the size moves the end of the wall");
                        ui.selectable_value(&mut app.wall_anchor_end, true, "End")
                            .on_hover_text("Changing the size moves the start of the wall");
                    });
                    ui.end_row();
                }

                ui.label("Height");
                let mut height = app.walls[first].height;
                let mixed = selected.iter().any(|&i| app.walls[i].height != height);
                let mut response = cm_value(ui, &mut height);
                if mixed {
                    response = response.on_hover_text("The selected walls have different heights");
                }
                if response.changed() {
                    let height = height.clamp(0.1, 10.0);
                    for &i in &selected {
                        app.walls[i].height = height;
                    }
                }
                ui.end_row();
            });
        if selected.len() == 1 {
            ui.label(
                RichText::new("Walls joined at the moving corner follow along.")
                    .small()
                    .color(TEXT_MUTED),
            );
        }
        ui.add_space(6.0);
        delete = ui.button("🗑 Delete").on_hover_text("[Del]").clicked();
    });
    if let Some((length, dir)) = new_geometry {
        app.resize_wall(first, length, dir);
    }
    if delete {
        app.delete_selection();
    } else if deselect {
        app.clear_selection();
    }
}
