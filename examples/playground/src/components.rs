//! Every control of mue-components in its variants, sizes, and palettes,
//! with live readouts of what the controls report.

use super::controls::*;
use mue_components::system::{ControlSize, Palette};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::style::{Size, Styleable};
use std::rc::Rc;

const PAGE: &str = "Components";

const PALETTES: [(&str, Palette); 10] = [
    ("Gray", Palette::Gray),
    ("Red", Palette::Red),
    ("Orange", Palette::Orange),
    ("Yellow", Palette::Yellow),
    ("Green", Palette::Green),
    ("Teal", Palette::Teal),
    ("Blue", Palette::Blue),
    ("Cyan", Palette::Cyan),
    ("Purple", Palette::Purple),
    ("Pink", Palette::Pink),
];

const SIZES: [(&str, ControlSize); 4] = [
    ("xs", ControlSize::Xs),
    ("sm", ControlSize::Sm),
    ("md", ControlSize::Md),
    ("lg", ControlSize::Lg),
];

fn show<T: Clone + 'static>(
    value: ReadSignal<T>,
    format: impl Fn(&T) -> String + 'static,
) -> Prop<Rc<str>> {
    value
        .map(move |value| Rc::<str>::from(format(&value)))
        .into()
}

/// A small caption above a group of examples.
fn caption(label: &'static str) -> Node {
    text(label)
        .style(mue_macroquad::style! {
            font-size: {f(12.)}; line-height: {u(16.)};
            font-weight: 700;
            color: {muted()};
        })
        .into_node()
}

/// A column of captioned groups.
fn groups(children: impl IntoChildren) -> Node {
    stack()
        .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(12.)}; })
        .children(children)
        .into_node()
}

fn buttons_card() -> Node {
    let palette = signal(Palette::Blue);
    let size = signal(ControlSize::Md);
    let loading = signal(false);
    let disabled = signal(false);
    let clicks = signal(0u32);
    let last = signal(Rc::<str>::from("none yet"));
    let press = move |name: &'static str| {
        move || {
            clicks.update(|n| *n += 1);
            last.set(Rc::from(name));
        }
    };
    let variants = keyed(
        computed(move |_| (palette.get(), size.get())),
        move |(palette, size)| {
            let (palette, size) = (*palette, *size);
            let variant = |name: &'static str, variant: ButtonVariant| {
                button(name)
                    .variant(variant)
                    .palette(palette)
                    .size(size)
                    .loading(loading)
                    .disabled(disabled)
                    .on_activate(press(name))
            };
            row((
                variant("Solid", ButtonVariant::Solid),
                variant("Subtle", ButtonVariant::Subtle),
                variant("Outline", ButtonVariant::Outline),
                variant("Ghost", ButtonVariant::Ghost),
                variant("Link", ButtonVariant::Link),
            ))
        },
    );
    card(
        "Buttons",
        "Five variants in any palette and size. Pressing scales a button down; loading shows a spinner and keeps the width.",
        groups((
            field("Palette", picker(PALETTES.to_vec(), palette)),
            field("Size", picker(SIZES.to_vec(), size)),
            row((
                switch().checked(loading).label("Loading").size(ControlSize::Sm),
                switch().checked(disabled).label("Disabled").size(ControlSize::Sm),
            )),
            variants,
            caption("ICONS, LOADING TEXT, FULL WIDTH"),
            row((
                button("Star").left_icon(text("★")).palette(Palette::Yellow).on_activate(press("Star")),
                button("Next").right_icon(text("→")).variant(ButtonVariant::Outline).palette(Palette::Teal).on_activate(press("Next")),
                button("Save")
                    .primary(true)
                    .loading(true)
                    .loading_text("Saving…"),
                icon_button(text("✕")).on_activate(press("✕")),
                icon_button(text("♥")).variant(ButtonVariant::Solid).palette(Palette::Pink).on_activate(press("♥")),
                icon_button(text("≡")).variant(ButtonVariant::Outline).on_activate(press("⚙")),
            )),
            button("Full width").full_width(true).variant(ButtonVariant::Subtle).palette(Palette::Purple).on_activate(press("Full width")),
            caption("BUTTON GROUPS"),
            stack().style(mue_macroquad::style! { gap: {u(10.)}; align-items: flex-start; }).children((
                button_group(vec![
                    button("Day").on_activate(press("Day")),
                    button("Week").on_activate(press("Week")),
                    button("Month").on_activate(press("Month")),
                ])
                .attached(true)
                .variant(ButtonVariant::Outline)
                .size(ControlSize::Sm),
                button_group(vec![
                    button("Save").on_activate(press("Save")),
                    button("Cancel").variant(ButtonVariant::Ghost).on_activate(press("Cancel")),
                ])
                .palette(Palette::Teal)
                .size(ControlSize::Sm),
            )),
            mono(computed(move |_| {
                Rc::<str>::from(format!("{} presses · last: {}", clicks.get(), last.get_clone()))
            })),
        )),
    )
}

fn inputs_card() -> Node {
    let variant = signal(InputVariant::Outline);
    let invalid = signal(false);
    let name = signal(String::new());
    let domain = signal(String::from("mue"));
    let fields = keyed(variant, move |variant| {
        let variant = *variant;
        groups((
            row(SIZES
                .iter()
                .map(|(label, size)| {
                    flexbox()
                        .style(mue_macroquad::style! { flex: 1; min-width: {u(140.)}; })
                        .children(
                            input()
                                .model(name)
                                .variant(variant)
                                .size(*size)
                                .invalid(invalid)
                                .placeholder(Rc::<str>::from(format!("Size {label}"))),
                        )
                        .into_node()
                })
                .collect::<Vec<_>>()),
            input_group(input().model(domain).variant(variant).invalid(invalid))
                .left_addon(text("https://"))
                .right_addon(text(".dev")),
        ))
    });
    card(
        "Inputs",
        "Outline, filled, flushed, and unstyled fields in four sizes; all fields share one model. The focus border uses the focus ring color.",
        groups((
            field(
                "Variant",
                picker(
                    vec![
                        ("Outline", InputVariant::Outline),
                        ("Filled", InputVariant::Filled),
                        ("Flushed", InputVariant::Flushed),
                        ("Unstyled", InputVariant::Unstyled),
                    ],
                    variant,
                ),
            ),
            switch().checked(invalid).label("Invalid").palette(Palette::Red).size(ControlSize::Sm),
            fields,
            mono(computed(move |_| {
                Rc::<str>::from(format!("name {:?} · url https://{}.dev", name.get_clone(), domain.get_clone()))
            })),
        )),
    )
}

fn choices_card() -> Node {
    let items = [signal(true), signal(false), signal(true)];
    let all = computed(move |_| items.iter().all(|item| item.get()));
    let some = computed(move |_| items.iter().any(|item| item.get()));
    let parent = signal(false);
    watch(all, move |all| parent.set(all.get()));
    let plan = signal(Rc::<str>::from("pro"));
    let size = signal(ControlSize::Md);
    let notifications = signal(true);
    let sized = keyed(size, move |size| {
        let size = *size;
        groups((
            row((
                checkbox().size(size).default_checked(true).label("Checked"),
                checkbox().size(size).label("Unchecked"),
                checkbox().size(size).indeterminate(true).label("Indeterminate"),
                checkbox().size(size).invalid(true).label("Invalid"),
                checkbox().size(size).disabled(true).default_checked(true).label("Disabled"),
            )),
            row(PALETTES
                .iter()
                .skip(1)
                .map(|(name, palette)| {
                    checkbox().size(size).palette(*palette).default_checked(true).label(*name).into_node()
                })
                .collect::<Vec<_>>()),
            radio_group()
                .value(plan)
                .size(size)
                .horizontal(true)
                .palette(Palette::Teal)
                .children((
                    radio("free").label("Free"),
                    radio("pro").label("Pro"),
                    radio("team").label("Team"),
                    radio("enterprise").label("Enterprise").disabled(true),
                )),
            row(PALETTES
                .iter()
                .map(|(name, palette)| {
                    switch().size(size).palette(*palette).default_checked(true).label(*name).into_node()
                })
                .collect::<Vec<_>>()),
            row((
                switch().size(size).checked(notifications).label("Notifications"),
                switch().size(size).disabled(true).label("Disabled"),
            )),
        ))
    });
    let labels = ["Photos", "Music", "Documents"];
    card(
        "Checkbox, radio, and switch",
        "Checks scale in, radio dots grow, switch thumbs slide with a spring. The parent box is indeterminate while only some children are checked. Arrow keys move inside the radio group.",
        groups((
            field("Size", picker(SIZES.to_vec(), size)),
            caption("SELECT ALL"),
            checkbox()
                .checked(parent)
                .indeterminate(computed(move |_| some.get() && !all.get()))
                .label("Back up everything")
                .on_change(move |on: &bool| {
                    for item in items {
                        item.set(*on);
                    }
                }),
            stack()
                .style(mue_macroquad::style! { padding-left: {u(24.)}; gap: {u(6.)}; })
                .children(
                    items
                        .iter()
                        .zip(labels)
                        .map(|(item, label)| checkbox().checked(*item).label(label).into_node())
                        .collect::<Vec<_>>(),
                ),
            caption("STATES, PALETTES, RADIO GROUP, SWITCHES"),
            sized,
            mono(computed(move |_| {
                let checked: Vec<&str> = items
                    .iter()
                    .zip(labels)
                    .filter(|(item, _)| item.get())
                    .map(|(_, label)| label)
                    .collect();
                Rc::<str>::from(format!(
                    "backup [{}] · plan {} · notifications {}",
                    checked.join(", "),
                    plan.get_clone(),
                    if notifications.get() { "on" } else { "off" }
                ))
            })),
        )),
    )
}

fn slider_card() -> Node {
    let volume = signal(40_f32);
    let committed = signal(40_f32);
    let palette = signal(Palette::Blue);
    let opacity = signal(0.8_f32);
    let sliders = keyed(palette, move |palette| {
        let palette = *palette;
        groups(
            SIZES
                .iter()
                .skip(1)
                .map(|(label, size)| {
                    field(
                        &format!("Size {label}"),
                        slider()
                            .value(volume)
                            .palette(palette)
                            .size(*size)
                            .on_change_end(move |value: &f32| committed.set(*value)),
                    )
                })
                .collect::<Vec<_>>(),
        )
    });
    card(
        "Slider",
        "Drag the thumb, press the track to jump, or use the arrow keys, Page Up/Down, Home, and End. The three sliders share one value.",
        groups((
            field("Palette", picker(PALETTES.to_vec(), palette)),
            sliders,
            field(
                "Step 0.05",
                slider().range(0., 1.).step(0.05).value(opacity).palette(Palette::Purple),
            ),
            field("Disabled", slider().default_value(60.).disabled(true)),
            row((
                mono(show(*volume, |v| format!("value {v:.0}"))),
                mono(show(*committed, |v| format!("on_change_end {v:.0}"))),
                mono(show(*opacity, |v| format!("opacity {v:.2}"))),
            )),
            flexbox()
                .style(mue_macroquad::style! {
                    width: 100%; height: {u(28.)};
                    border-radius: {u(8.)};
                    opacity: {opacity};
                })
                .background(accent_gradient()),
        )),
    )
}

fn progress_card() -> Node {
    let value = signal(65_f32);
    let stripe = signal(true);
    let moving = signal(true);
    let size = signal(ControlSize::Md);
    let bars = keyed(size, move |size| {
        let size = *size;
        groups((
            progress().value(value).size(size).has_stripe(stripe).is_animated(moving),
            progress().value(value).size(size).palette(Palette::Green),
            progress().value(value).size(size).palette(Palette::Pink).has_stripe(true),
            progress().indeterminate(true).size(size).palette(Palette::Purple),
        ))
    });
    card(
        "Progress and spinners",
        "Bars and rings glide to a new value; indeterminate ones run forever. The slider drives every determinate indicator.",
        groups((
            field("Value", slider().value(value).palette(Palette::Green)),
            field("Size", picker(SIZES.to_vec(), size)),
            row((
                switch().checked(stripe).label("Stripes").size(ControlSize::Sm),
                switch().checked(moving).label("Animated stripes").size(ControlSize::Sm),
            )),
            bars,
            caption("CIRCULAR"),
            row((
                circular_progress()
                    .value(value)
                    .diameter(Size::Rem(4.))
                    .label(text(show(*value, |v| format!("{v:.0}%")))),
                circular_progress().value(value).palette(Palette::Orange).diameter(Size::Rem(3.)).thickness(0.2),
                circular_progress().value(value).palette(Palette::Teal).diameter(Size::Rem(2.)),
                circular_progress().indeterminate(true).palette(Palette::Purple).diameter(Size::Rem(3.)),
            )),
            caption("SPINNERS"),
            row((
                spinner().size(ControlSize::Xs),
                spinner().size(ControlSize::Sm),
                spinner().size(ControlSize::Md).color(accent(Palette::Teal)),
                spinner().size(ControlSize::Lg).color(accent(Palette::Pink)).speed(0.6),
                spinner().size(ControlSize::Lg).color(accent(Palette::Orange)).thickness(0.25).speed(2.),
            )),
        )),
    )
}

fn accent(palette: Palette) -> Prop<mue_macroquad::paint::shader::Brush> {
    mue_components::system::accent(palette)
}

fn badges_card() -> Node {
    let tags: Signal<Rc<[(u32, &'static str, Palette)]>> = signal(Rc::from(vec![
        (0, "Rust", Palette::Orange),
        (1, "UI", Palette::Blue),
        (2, "Android", Palette::Green),
        (3, "Animation", Palette::Purple),
        (4, "Design", Palette::Pink),
    ]));
    let reset = move || {
        tags.set(Rc::from(vec![
            (0, "Rust", Palette::Orange),
            (1, "UI", Palette::Blue),
            (2, "Android", Palette::Green),
            (3, "Animation", Palette::Purple),
            (4, "Design", Palette::Pink),
        ]))
    };
    let badge_row = |variant: BadgeVariant| {
        row(PALETTES
            .iter()
            .map(|(name, palette)| badge(*name).variant(variant).palette(*palette).into_node())
            .collect::<Vec<_>>())
    };
    let closable = transition_group(
        tags,
        |tag| tag.0,
        move |tag| {
            let id = tag.0;
            mue_components::tag(tag.1)
                .palette(tag.2)
                .variant(BadgeVariant::Solid)
                .on_close(move || {
                    let list: Rc<[_]> = tags
                        .get_clone_untracked()
                        .iter()
                        .copied()
                        .filter(|entry| entry.0 != id)
                        .collect();
                    tags.set(list);
                })
        },
    )
    .animation(PresenceAnimation::fade().scale(0.6))
    // In a row, the group's item wrappers collapse to a sliver; stack the tags.
    .style(mue_macroquad::style! { width: 100%; flex-direction: column; gap: {u(6.)}; align-items: flex-start; });
    card(
        "Badges and tags",
        "Badges label a status; tags label keywords and can be removed. Removed tags shrink away and the others slide over.",
        groups((
            caption("BADGES: SUBTLE, SOLID, OUTLINE"),
            badge_row(BadgeVariant::Subtle),
            badge_row(BadgeVariant::Solid),
            badge_row(BadgeVariant::Outline),
            caption("TAGS IN THREE SIZES"),
            row((
                mue_components::tag("Small").size(ControlSize::Sm).palette(Palette::Teal),
                mue_components::tag("Medium").size(ControlSize::Md).palette(Palette::Teal),
                mue_components::tag("Large").size(ControlSize::Lg).palette(Palette::Teal),
                mue_components::tag("Outline").variant(BadgeVariant::Outline).palette(Palette::Cyan),
                mue_components::tag("With icon").left_icon(text("★")).palette(Palette::Yellow),
            )),
            caption("CLOSABLE"),
            closable,
            row((button("Reset tags").size(ControlSize::Xs).variant(ButtonVariant::Outline).on_activate(reset),)),
        )),
    )
}

fn tabs_card() -> Node {
    let selected = signal(0usize);
    let names = ["Account", "Security", "Billing"];
    let make = |variant: TabsVariant, palette: Palette, fitted: bool| {
        tabs()
            .index(selected)
            .variant(variant)
            .palette(palette)
            .fitted(fitted)
            .children((
                tab_list().children((tab(names[0]), tab(names[1]), tab(names[2]), tab("Archived").disabled(true))),
                tab_panels().children((
                    tab_panel().children(hint("Name, email, and avatar.")),
                    tab_panel().children(hint("Password and two-factor sign-in.")),
                    tab_panel().children(hint("Plan, invoices, and payment method.")),
                    tab_panel().children(hint("Nothing here.")),
                )),
            ))
    };
    card(
        "Tabs",
        "The indicator slides to the selected tab: an underline, an enclosed card, or a soft pill. All three share one selection; arrow keys, Home, and End move between tabs.",
        groups((
            caption("LINE"),
            make(TabsVariant::Line, Palette::Blue, false),
            caption("ENCLOSED, FITTED"),
            make(TabsVariant::Enclosed, Palette::Purple, true),
            caption("SOFT ROUNDED"),
            make(TabsVariant::SoftRounded, Palette::Teal, false),
            mono(show(*selected, move |index| format!("selected tab {index} ({})", names.get(*index).unwrap_or(&"?")))),
        )),
    )
}

fn popups_card() -> Node {
    let action = signal(Rc::<str>::from("none"));
    let pick = move |name: &'static str| move || action.set(Rc::from(name));
    let tip = |placement: Placement, label: &'static str| {
        tooltip(Rc::<str>::from(format!("Tooltip on the {label}")), button(label).size(ControlSize::Sm).variant(ButtonVariant::Outline))
            .placement(placement)
            .has_arrow(true)
    };
    card(
        "Tooltips and menus",
        "Tooltips open after a short hover or a long press. Menus open below their button, flip to stay in the window, and close on a selection, an outside press, Escape, or back; arrow keys move between items.",
        groups((
            caption("TOOLTIP PLACEMENTS"),
            row((
                tip(Placement::Top, "Top"),
                tip(Placement::Bottom, "Bottom"),
                tip(Placement::Left, "Left"),
                tip(Placement::Right, "Right"),
                tooltip("Opens at once", icon_button(text("?")).variant(ButtonVariant::Subtle)).open_delay(0.),
            )),
            caption("MENUS"),
            row((
                menu().children((
                    menu_button("Actions").palette(Palette::Blue).variant(ButtonVariant::Solid),
                    menu_list().children((
                        menu_item("New file").command("Ctrl+N").icon(text("＋")).on_select(pick("New file")),
                        menu_item("Duplicate").command("Ctrl+D").on_select(pick("Duplicate")),
                        menu_item("Rename").on_select(pick("Rename")),
                        menu_divider(),
                        menu_group("Danger zone").children((
                            menu_item("Archive").on_select(pick("Archive")),
                            menu_item("Delete").disabled(true),
                        )),
                    )),
                )),
                menu().placement(Placement::TopStart).children((
                    menu_button("Sort by").variant(ButtonVariant::Outline),
                    menu_list().children((
                        menu_item("Name").on_select(pick("Sort by name")),
                        menu_item("Date").on_select(pick("Sort by date")),
                        menu_item("Size").on_select(pick("Sort by size")),
                    )),
                )),
            )),
            mono(show(*action, |action| format!("last menu action: {action}"))),
        )),
    )
}

fn alerts_card() -> Node {
    let variant = signal(AlertVariant::Subtle);
    let dismissed = signal(false);
    let alerts = keyed(variant, move |variant| {
        let variant = *variant;
        groups((
            alert(AlertStatus::Info).variant(variant).children((
                alert_title("Update available"),
                alert_description("Version 0.2 adds sliders, tabs, and menus."),
            )),
            alert(AlertStatus::Success).variant(variant).children(alert_title("Your settings were saved")),
            alert(AlertStatus::Warning).variant(variant).children((
                alert_title("Storage almost full"),
                alert_description("Free some space to keep syncing."),
            )),
            alert(AlertStatus::Error).variant(variant).children(alert_title("Could not reach the server")),
        ))
    });
    card(
        "Alerts",
        "Four statuses in four variants. The last alert can be dismissed; it fades out and the button brings it back.",
        groups((
            field(
                "Variant",
                picker(
                    vec![
                        ("Subtle", AlertVariant::Subtle),
                        ("Solid", AlertVariant::Solid),
                        ("Left accent", AlertVariant::LeftAccent),
                        ("Top accent", AlertVariant::TopAccent),
                    ],
                    variant,
                ),
            ),
            alerts,
            presence(dismissed.map(|gone| !gone), move || {
                alert(AlertStatus::Info)
                    .variant(AlertVariant::LeftAccent)
                    .palette(Palette::Purple)
                    .on_close(move || dismissed.set(true))
                    .children((
                        alert_title("Dismiss me"),
                        alert_description("Press the close button on the right."),
                    ))
            })
            .animation(PresenceAnimation::fade().slide(16., 0.))
            .style(mue_macroquad::style! { width: 100%; }),
            row((button("Show the dismissed alert")
                .size(ControlSize::Sm)
                .disabled(dismissed.map(|gone| !gone))
                .on_activate(move || dismissed.set(false)),)),
        )),
    )
}

fn people_card() -> Node {
    let loaded = signal(false);
    let people = ["Ada Lovelace", "Alan Turing", "Grace Hopper", "Linus Torvalds", "Margaret Hamilton", "Dennis Ritchie"];
    card(
        "Avatars and skeletons",
        "Avatars show initials on a color picked from the name, a generic person without a name, and an optional status dot. Skeletons hold the place of content that is still loading.",
        groups((
            caption("SIZES"),
            row((
                avatar().name("Ada Lovelace").size(AvatarSize::Xs),
                avatar().name("Ada Lovelace").size(AvatarSize::Sm),
                avatar().name("Ada Lovelace").size(AvatarSize::Md).badge(Palette::Green),
                avatar().name("Ada Lovelace").size(AvatarSize::Lg).badge(Palette::Orange),
                avatar().name("Ada Lovelace").size(AvatarSize::Xl).show_border(true),
                avatar().size(AvatarSize::Lg),
            )),
            caption("NAMES"),
            row(people
                .iter()
                .map(|name| tooltip(*name, avatar().name(*name)).into_node())
                .collect::<Vec<_>>()),
            caption("SKELETON"),
            switch().checked(loaded).label("Loaded").size(ControlSize::Sm),
            hstack()
                .style(mue_macroquad::style! { width: 100%; gap: {u(14.)}; align-items: flex-start; })
                .children((
                    skeleton_circle(AvatarSize::Lg.rem()).loaded(loaded).children(avatar().name("Grace Hopper").size(AvatarSize::Lg)),
                    stack()
                        .style(mue_macroquad::style! { flex: 1; min-width: 0px; gap: {u(8.)}; })
                        .children((
                            skeleton().loaded(loaded).children(text("Grace Hopper").style(mue_macroquad::style! {
                                font-size: {f(16.)}; line-height: {u(22.)}; font-weight: 700; color: {foreground()};
                            })),
                            skeleton_text().lines(3).loaded(loaded).children(hint(
                                "Computer scientist and United States Navy rear admiral. She wrote the first compiler and helped design COBOL.",
                            )),
                        )),
                )),
        )),
    )
}

fn dialog_card() -> Node {
    let open = signal(false);
    let name = signal(String::new());
    let newsletter = signal(true);
    let result = signal(Rc::<str>::from("not submitted"));
    let dialog = overlay_host(
        open,
        stack()
            .style(mue_macroquad::style! {
                width: {u(360.)};
                max-width: 92vw;
                padding: {(u(22.),)};
                gap: {u(14.)};
                border-radius: {u(18.)};
                border-width: 1px;
                border-color: {border()};
                background: {panel_brush()};
                font-size: {f(15.)};
            })
            .box_shadow(mue_macroquad::BoxShadows::elevation(24.))
            .children((
                hstack()
                    .style(mue_macroquad::style! { width: 100%; gap: {u(10.)}; align-items: center; })
                    .children((
                        avatar().name(name.map(|name| Rc::<str>::from(name.as_str()))).size(AvatarSize::Md),
                        heading("Create a profile").level(HeadingLevel::H4).style(mue_macroquad::style! {
                            flex: 1; font-size: {f(18.)}; line-height: {u(24.)}; color: {foreground()};
                        }),
                        icon_button(text("✕")).size(ControlSize::Sm).on_activate(move || open.set(false)),
                    )),
                hint("The avatar follows the name. Tab stays inside the dialog; Escape, back, or the backdrop closes it."),
                input().model(name).placeholder("Your name"),
                checkbox().checked(newsletter).label("Send me the newsletter"),
                hstack()
                    .style(mue_macroquad::style! { width: 100%; gap: {u(8.)}; justify-content: flex-end; })
                    .children((
                        button("Cancel").variant(ButtonVariant::Ghost).on_activate(move || open.set(false)),
                        button("Create")
                            .primary(true)
                            .disabled(name.map(|name| name.trim().is_empty()))
                            .on_activate(move || {
                                result.set(Rc::from(format!(
                                    "created {:?}, newsletter {}",
                                    name.get_clone_untracked(),
                                    if newsletter.get_untracked() { "on" } else { "off" }
                                )));
                                open.set(false);
                            }),
                    )),
            )),
    )
    .dismissible(true)
    .on_dismiss(move || open.set(false));
    card(
        "Dialog",
        "overlay_host with library controls inside: the content rises and scales in over a fading backdrop.",
        groups((
            row((
                button("Open dialog").primary(true).on_activate(move || open.set(true)),
                mono(show(*result, |result| result.to_string())),
            )),
            dialog,
        )),
    )
}

pub fn page() -> Node {
    page_body(
        "The controls of mue-components, modeled on Chakra UI. Every control is sized in rem, so it follows the zoom setting, and follows the light or dark mode.",
        (
            buttons_card(),
            inputs_card(),
            choices_card(),
            slider_card(),
            progress_card(),
            badges_card(),
            tabs_card(),
            popups_card(),
            alerts_card(),
            people_card(),
            dialog_card(),
            checklist(
                PAGE,
                &[
                    "Every button variant reacts to hover and press, in light and dark mode",
                    "Loading buttons keep their width and ignore presses",
                    "The select-all box turns indeterminate when only some children are checked",
                    "Sliders follow drags, track presses, and the arrow keys",
                    "Progress bars and rings glide to the slider value",
                    "Removing a tag animates; the others slide over",
                    "Tab indicators slide, and arrow keys move between tabs",
                    "Tooltips and menus stay inside the window and close as described",
                    "The dismissed alert fades out and comes back",
                    "The dialog traps focus and closes on Escape, back, and the backdrop",
                    "Controls look right at every zoom level and on a phone",
                ],
            ),
        ),
    )
}
