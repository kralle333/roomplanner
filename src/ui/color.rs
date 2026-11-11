//! A color button whose popup has the picker, the colors already used in the plan, and
//! copy / paste, so matching colors between pieces is a couple of clicks.

use crate::app::RoomPlannerApp;
use eframe::egui::{self, Color32, RichText};

fn copied_id() -> egui::Id {
    egui::Id::new("copied_color")
}

fn palette_id() -> egui::Id {
    egui::Id::new("plan_colors")
}

/// Remembers the colors used in the plan, for the palette in the color popups. Call once
/// per frame before showing color buttons.
pub fn collect_plan_colors(app: &RoomPlannerApp, ctx: &egui::Context) {
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut add = |c: [u8; 3]| {
        if !colors.contains(&c) {
            colors.push(c);
        }
    };
    add(app.finishes.wall_color);
    for item in &app.furniture {
        add(item.color);
        if let Some(top) = item.top_color {
            add(top);
        }
    }
    colors.truncate(24);
    ctx.data_mut(|d| d.insert_temp(palette_id(), colors));
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A small square of `color` that can be clicked.
fn swatch(ui: &mut egui::Ui, color: [u8; 3], size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let stroke = if response.hovered() {
        egui::Stroke::new(2.0, ui.visuals().strong_text_color())
    } else {
        egui::Stroke::new(1.0, crate::ui::theme::BORDER)
    };
    ui.painter()
        .rect_filled(rect, 3.0, Color32::from_rgb(color[0], color[1], color[2]));
    ui.painter()
        .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
    response.on_hover_text(hex(color))
}

/// Copy and paste between all color buttons in the app, and the system clipboard.
fn copy(ctx: &egui::Context, color: [u8; 3]) {
    ctx.data_mut(|d| d.insert_temp(copied_id(), color));
    ctx.copy_text(hex(color));
}

fn copied(ctx: &egui::Context) -> Option<[u8; 3]> {
    ctx.data(|d| d.get_temp(copied_id()))
}

/// A color button: click for the picker, palette, copy and paste; right-click to copy or
/// paste right away. The response is marked changed when the color changes.
pub fn color_button(ui: &mut egui::Ui, color: &mut [u8; 3]) -> egui::Response {
    let before = *color;
    let popup_id = ui.auto_id_with("color_popup");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let size = ui.spacing().interact_size;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size.x, size.y), egui::Sense::click());
    let visuals = ui.style().interact_selectable(&response, open);
    let rect = rect.expand(visuals.expansion);
    ui.painter().rect_filled(
        rect,
        visuals.corner_radius,
        Color32::from_rgb(color[0], color[1], color[2]),
    );
    ui.painter().rect_stroke(
        rect,
        visuals.corner_radius,
        egui::Stroke::new(visuals.fg_stroke.width.max(1.0), visuals.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let mut response = response.on_hover_text("Click to change · right-click to copy or paste");

    response.context_menu(|ui| {
        if ui.button("Copy color").clicked() {
            copy(ui.ctx(), *color);
            ui.close();
        }
        let pasted = copied(ui.ctx());
        if ui
            .add_enabled(pasted.is_some(), egui::Button::new("Paste color"))
            .clicked()
        {
            if let Some(c) = pasted {
                *color = c;
            }
            ui.close();
        }
    });

    egui::Popup::menu(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.spacing_mut().slider_width = 220.0;
            let mut c32 = Color32::from_rgb(color[0], color[1], color[2]);
            if egui::widgets::color_picker::color_picker_color32(
                ui,
                &mut c32,
                egui::widgets::color_picker::Alpha::Opaque,
            ) {
                *color = [c32.r(), c32.g(), c32.b()];
            }

            let palette: Vec<[u8; 3]> = ui
                .ctx()
                .data(|d| d.get_temp(palette_id()))
                .unwrap_or_default();
            if !palette.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("Used in this plan").small());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for c in palette {
                        if swatch(ui, c, 18.0).clicked() {
                            *color = c;
                        }
                    }
                });
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Copy")
                    .on_hover_text("Copy this color, also as text")
                    .clicked()
                {
                    copy(ui.ctx(), *color);
                }
                let pasted = copied(ui.ctx());
                let paste = ui.add_enabled(pasted.is_some(), egui::Button::new("Paste"));
                if let Some(c) = pasted {
                    // Show what would be pasted.
                    swatch(ui, c, 14.0);
                }
                if paste.clicked() {
                    if let Some(c) = pasted {
                        *color = c;
                    }
                }
                // Hex code, editable: paste one from elsewhere here.
                let text_id = ui.id().with("hex");
                let mut text = ui
                    .ctx()
                    .data(|d| d.get_temp::<String>(text_id))
                    .unwrap_or_else(|| hex(*color));
                let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(64.0));
                if edit.changed() {
                    if let Some(c) = parse_hex(&text) {
                        *color = c;
                    }
                }
                if edit.has_focus() {
                    ui.ctx().data_mut(|d| d.insert_temp(text_id, text));
                } else {
                    ui.ctx().data_mut(|d| d.remove::<String>(text_id));
                }
            });
        });

    if *color != before {
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_codes_round_trip() {
        assert_eq!(parse_hex("#c08040"), Some([0xc0, 0x80, 0x40]));
        assert_eq!(parse_hex(" C08040 "), Some([0xc0, 0x80, 0x40]));
        assert_eq!(parse_hex("#c0804"), None);
        assert_eq!(hex([1, 2, 255]), "#0102ff");
    }
}
