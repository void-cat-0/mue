use crate::{
    input::SettingsAction,
    settings::ReadSettings,
    ui::{settings_status, themed_button},
};
use mue_components::{flexbox, text};
use mue_core::scope::{OwnedScope, provide};
use mue_macroquad::{
    node::{IntoNode, Node, NodeRef},
    shader::Brush,
    style::{Styleable, StyleableExt},
};
use std::{cell::Cell, rc::Rc};

pub struct SettingsView {
    root: Node,
    _owner: OwnedScope,
}

impl SettingsView {
    pub fn new(settings: ReadSettings, commands: Rc<Cell<Option<SettingsAction>>>) -> Self {
        let owner = OwnedScope::new();
        let root = owner.run(|| {
            // Detached view scope: install only this view's read-only dependency explicitly.
            provide(settings);
            let palette = settings.theme.map(|theme| theme.palette());
            let theme_label = settings
                .theme
                .map(|theme| Rc::<str>::from(format!("Theme: {}  [T]", theme.label())));
            let motion_label = settings.reduce_motion.map(|reduced| {
                Rc::<str>::from(format!(
                    "Reduce artwork motion: {}  [M]",
                    if reduced { "on" } else { "off" }
                ))
            });
            flexbox()
                .size_full()
                .flex_column()
                .justify_center()
                .padding((48.,))
                .font_family("Noto Sans")
                .color(palette.map(|colors| Brush::Solid(colors.foreground)))
                .children(
                    flexbox()
                        .w_full()
                        .flex_column()
                        .gap(22.)
                        .padding((32.,))
                        .radius(22.)
                        .background(palette.map(|colors| Brush::Solid(colors.surface)))
                        .children((
                            text("APPLICATION SETTINGS")
                                .font_size(32.)
                                .line_height(40.)
                                .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            text(
                                "Changes apply immediately to every Scene.
Return keeps the original list position and selected song.",
                            )
                            .font_size(17.)
                            .line_height(23.)
                            .color(palette.map(|colors| Brush::Solid(colors.muted))),
                            settings_status(),
                            themed_button(theme_label, {
                                let commands = commands.clone();
                                move || commands.set(Some(SettingsAction::CycleTheme))
                            }),
                            themed_button(motion_label, {
                                let commands = commands.clone();
                                move || commands.set(Some(SettingsAction::ToggleMotion))
                            }),
                            text(
                                "With reduced motion on, the next open/return switches immediately.
Settings navigation itself is always immediate.",
                            )
                            .font_size(16.)
                            .line_height(22.)
                            .color(palette.map(|colors| Brush::Solid(colors.muted))),
                            themed_button("Back to previous Scene  [Esc]", {
                                let commands = commands.clone();
                                move || commands.set(Some(SettingsAction::Back))
                            }),
                            text("T: theme    M: artwork motion    Esc: back    Q: exit")
                                .font_size(14.)
                                .line_height(20.)
                                .color(palette.map(|colors| Brush::Solid(colors.muted))),
                        )),
                )
                .into_node()
        });
        Self {
            root,
            _owner: owner,
        }
    }

    pub fn root(&self) -> NodeRef {
        *self.root
    }
}
