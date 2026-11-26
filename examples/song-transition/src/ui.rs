//! Small application components resolve read-only context only during construction.
use crate::{localization::ReadTranslations, settings::ReadSettings};
use mue_components::{button, text};
use mue_core::{prop::Prop, scope::inject, signal::Access};
use mue_i18n::tl;
use mue_macroquad::{
    node::{BuildNode, Node, NodeContext},
    paint::shader::Brush,
    style::{Style, Styleable},
};
use std::rc::Rc;

pub struct ThemedButton<F> {
    label: Prop<Rc<str>>,
    action: F,
    style: Style,
}

pub fn themed_button<F: FnMut() + 'static>(
    label: impl Into<Prop<Rc<str>>>,
    action: F,
) -> ThemedButton<F> {
    ThemedButton {
        label: label.into(),
        action,
        style: Style::new(),
    }
}

impl<F> Styleable for ThemedButton<F> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl<F: FnMut() + 'static> BuildNode for ThemedButton<F> {
    fn build(self, mut ctx: NodeContext) -> Node {
        let settings = inject::<ReadSettings>().expect("view must provide read-only settings");
        ctx.push_style(self.style);
        button(self.label)
            .background(
                settings
                    .theme
                    .map(|theme| Brush::Solid(theme.palette().accent)),
            )
            .font_size(17.)
            .line_height(24.)
            .on_activate(self.action)
            .build(ctx)
    }
}

pub struct SettingsStatus;

pub fn settings_status() -> SettingsStatus {
    SettingsStatus
}

impl BuildNode for SettingsStatus {
    fn build(self, ctx: NodeContext) -> Node {
        let settings = inject::<ReadSettings>().expect("view must provide read-only settings");
        let translations =
            inject::<ReadTranslations>().expect("view must provide read-only translations");
        let locale = translations.locale;
        // This component build is a live consumer scope. Providers only read existing handles.
        let label = tl!(translations.common, "settings-status",
            "theme" => settings.theme.get().translation_key(),
            "motion" => if settings.reduce_motion.get() { "off" } else { "on" },
            "locale" => locale.get_clone().to_string(),
        );
        text(label)
            .font_size(14.)
            .line_height(20.)
            .color(
                settings
                    .theme
                    .map(|theme| Brush::Solid(theme.palette().muted)),
            )
            .build(ctx)
    }
}
