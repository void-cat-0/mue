use crate::{
    input::SettingsAction,
    localization::ReadTranslations,
    settings::ReadSettings,
    ui::{settings_status, themed_button},
};
use mue_components::{flexbox, text};
use mue_core::{
    scope::{OwnedScope, provide},
    signal::Access,
};
use mue_i18n::tl;
use mue_macroquad::{
    node::{IntoNode, Node, NodeRef},
    paint::shader::Brush,
    style::{Styleable, StyleableExt},
};
use std::{cell::Cell, rc::Rc};

pub struct SettingsView {
    root: Node,
    _owner: OwnedScope,
}

impl SettingsView {
    pub fn new(
        settings: ReadSettings,
        translations: ReadTranslations,
        commands: Rc<Cell<Option<SettingsAction>>>,
    ) -> Self {
        let owner = OwnedScope::new();
        let root = owner.run(|| {
            // Detached view scope: install only this view's read-only dependency explicitly.
            provide(settings);
            provide(translations.clone());
            let palette = settings.theme.map(|theme| theme.palette());
            let theme_label = tl!(translations.settings, "theme-button",
                "theme" => settings.theme.get().translation_key(),
            );
            let motion_label = tl!(translations.settings, "motion-button",
                "reduced" => if settings.reduce_motion.get() { "on" } else { "off" },
            );
            let locale = translations.locale;
            let locale_label = tl!(translations.settings, "locale-button",
                "locale" => locale.get_clone().to_string(),
            );
            flexbox()
                .size_full()
                .flex_column()
                .justify_center()
                .padding((48.,))
                .font_family("Noto Sans CJK SC")
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
                            text(tl!(translations.settings, "title"))
                                .font_size(32.)
                                .line_height(40.)
                                .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            text(tl!(translations.settings, "description"))
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
                            themed_button(locale_label, {
                                let commands = commands.clone();
                                move || commands.set(Some(SettingsAction::CycleLocale))
                            }),
                            text(tl!(translations.settings, "motion-tip"))
                                .font_size(16.)
                                .line_height(22.)
                                .color(palette.map(|colors| Brush::Solid(colors.muted))),
                            themed_button(tl!(translations.settings, "back-scene"), {
                                let commands = commands.clone();
                                move || commands.set(Some(SettingsAction::Back))
                            }),
                            text(tl!(translations.settings, "shortcuts"))
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
