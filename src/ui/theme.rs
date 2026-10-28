use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(219, 234, 254);
pub const TEXT_DARK: Color32 = Color32::from_rgb(55, 60, 70);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(107, 114, 128);
pub const CANVAS_BG: Color32 = Color32::from_rgb(250, 250, 251);
pub const PANEL_BG: Color32 = Color32::from_rgb(244, 245, 247);
pub const BORDER: Color32 = Color32::from_rgb(222, 225, 230);

/// A calm light theme with an accent color, rounded controls and a bit more breathing room.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Light);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = Color32::WHITE;
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(236, 238, 241);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = ACCENT_LIGHT;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;

    let radius = CornerRadius::same(5);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.corner_radius = radius;
    widgets.inactive.weak_bg_fill = Color32::WHITE;
    // Slider rails and checkboxes use bg_fill; buttons use weak_bg_fill.
    widgets.inactive.bg_fill = Color32::from_rgb(226, 229, 234);
    widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.inactive.corner_radius = radius;
    widgets.hovered.weak_bg_fill = Color32::from_rgb(239, 243, 250);
    widgets.hovered.bg_fill = Color32::from_rgb(239, 243, 250);
    widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(170, 190, 225));
    widgets.hovered.corner_radius = radius;
    widgets.active.weak_bg_fill = ACCENT_LIGHT;
    widgets.active.bg_fill = ACCENT_LIGHT;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.active.corner_radius = radius;
    widgets.open.corner_radius = radius;

    ctx.set_visuals_of(egui::Theme::Light, visuals);

    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(17.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.5));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.5));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
    });
}

/// Small uppercase section caption.
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(TEXT_MUTED),
    );
}
