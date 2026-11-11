//! Tabs along the bottom of the view for the plan's layouts, and the differences overlay.

use crate::{
    app::RoomPlannerApp,
    layouts,
    ui::theme::{ACCENT, TEXT_MUTED},
};
use eframe::egui::{self, RichText};

/// Colors of the differences overlay: added, changed and removed pieces.
pub const ADDED: egui::Color32 = egui::Color32::from_rgb(40, 170, 90);
pub const CHANGED: egui::Color32 = egui::Color32::from_rgb(230, 140, 20);
pub const REMOVED: egui::Color32 = egui::Color32::from_rgb(220, 60, 60);

pub fn show(app: &mut RoomPlannerApp, ui: &egui::Ui, canvas: egui::Rect) {
    egui::Area::new(egui::Id::new("layout_tabs"))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(canvas.left_bottom() + egui::vec2(12.0, -12.0))
        .order(egui::Order::Middle)
        .constrain_to(canvas)
        .show(ui.ctx(), |ui| {
            ui.set_max_width(canvas.width() - 200.0);
            super::canvas::floating_frame().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    tabs(app, ui);
                    if app.layouts.len() > 1 {
                        ui.separator();
                        compare(app, ui);
                    }
                });
            });
        });
}

fn tabs(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let peeking = app.peek_return.is_some();
    let mut action: Option<Box<dyn FnOnce(&mut RoomPlannerApp)>> = None;
    for i in 0..app.layouts.len() {
        if app.renaming_layout == Some(i) {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut app.layouts[i].name)
                    .desired_width(110.0)
                    .font(egui::TextStyle::Button),
            );
            if !edit.has_focus() && app.rename_focus_requested {
                edit.request_focus();
                app.rename_focus_requested = false;
            } else if edit.lost_focus() {
                app.renaming_layout = None;
                if app.layouts[i].name.trim().is_empty() {
                    app.layouts[i].name = layouts::unique_name(app, "Layout");
                }
            }
            continue;
        }
        let shown = i == app.active_layout;
        let compared = app.compare_layout == Some(i) && app.layouts.len() > 1;
        let mut text = RichText::new(&app.layouts[i].name);
        if shown {
            text = text.strong().color(ACCENT);
        }
        let mut response = ui.add(egui::Button::selectable(shown, text));
        let key = if i < 9 {
            format!(" [{}]", i + 1)
        } else {
            String::new()
        };
        response = response.on_hover_text(if compared {
            format!("Compared with the shown layout: hold C to peek{key}")
        } else if shown {
            "Shown now · double-click to rename · right-click for more".to_owned()
        } else {
            format!("Show this layout{key}")
        });
        if compared {
            // A small mark under the layout being compared with.
            let r = response.rect;
            ui.painter().line_segment(
                [
                    r.left_bottom() + egui::vec2(6.0, -2.0),
                    r.right_bottom() + egui::vec2(-6.0, -2.0),
                ],
                egui::Stroke::new(2.0, TEXT_MUTED),
            );
        }
        if response.clicked() && !peeking {
            action = Some(Box::new(move |app| layouts::switch(app, i)));
        }
        if response.double_clicked() {
            action = Some(Box::new(move |app| start_rename(app, i)));
        }
        response.context_menu(|ui| {
            if ui.button("Rename").clicked() {
                action = Some(Box::new(move |app| start_rename(app, i)));
                ui.close();
            }
            if ui.button("Duplicate").clicked() {
                action = Some(Box::new(move |app| {
                    layouts::duplicate(app, i);
                }));
                ui.close();
            }
            ui.separator();
            let last = app.layouts.len() - 1;
            if ui
                .add_enabled(i > 0, egui::Button::new("Move left"))
                .clicked()
            {
                action = Some(Box::new(move |app| layouts::reorder(app, i, -1)));
                ui.close();
            }
            if ui
                .add_enabled(i < last, egui::Button::new("Move right"))
                .clicked()
            {
                action = Some(Box::new(move |app| layouts::reorder(app, i, 1)));
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(app.layouts.len() > 1, egui::Button::new("🗑 Delete"))
                .on_disabled_hover_text("A plan always has one layout")
                .clicked()
            {
                action = Some(Box::new(move |app| layouts::delete(app, i)));
                ui.close();
            }
        });
    }
    if ui
        .button("+")
        .on_hover_text("New layout: a copy of the shown one, to try something different")
        .clicked()
        && !peeking
    {
        let shown = app.active_layout;
        action = Some(Box::new(move |app| {
            layouts::duplicate(app, shown);
        }));
    }
    if let Some(action) = action {
        action(app);
    }
}

fn start_rename(app: &mut RoomPlannerApp, index: usize) {
    app.renaming_layout = Some(index);
    app.rename_focus_requested = true;
}

/// The differences toggle and what it found.
fn compare(app: &mut RoomPlannerApp, ui: &mut egui::Ui) {
    let Some(other) = app
        .compare_layout
        .filter(|&i| i != app.active_layout)
        .and_then(|i| app.layouts.get(i))
        .map(|l| l.name.clone())
    else {
        ui.label(
            RichText::new("Switch layouts to compare")
                .small()
                .color(TEXT_MUTED),
        );
        return;
    };
    ui.toggle_value(&mut app.show_differences, "Δ Differences")
        .on_hover_text(format!(
            "Outline what is different from “{other}”: green added, orange moved or changed, \
             red only in “{other}”. Hold C to peek at it"
        ));
    if app.show_differences {
        if let Some(d) = layouts::current_differences(app) {
            if d.count() == 0 {
                ui.label(RichText::new("same as").small().color(TEXT_MUTED));
                ui.label(RichText::new(other).small());
            } else {
                let mut part = |n: usize, word: &str, color| {
                    if n > 0 {
                        ui.label(RichText::new(format!("{n} {word}")).small().color(color));
                    }
                };
                part(d.added.len(), "added", ADDED);
                part(d.changed.len(), "changed", CHANGED);
                part(d.removed.len(), "removed", REMOVED);
                if d.finishes_differ {
                    ui.label(RichText::new("finishes differ").small().color(CHANGED));
                }
            }
        }
    }
}
