use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    BoxShadows,
    input::back::{can_go_back, dispatch_back, use_back_handler},
    input::cursor::Cursor,
    node::NodeHandle,
    overlay::layer::{Layer, teleport},
    paint::color::Color,
    paint::shader::Brush,
    style::{Styleable, StyleableExt},
};
use mue_components::overlay::anchor::{Placement, anchored};
use mue_components::motion::presence::{PresenceAnimation, presence};
use std::rc::Rc;

const PAGE: &str = "Overlays & focus";

fn dialog_card() -> Node {
    let open = signal(false);
    let (dismissible, dismissible_control) = option_switch("Dismissible", true);
    let name = signal(String::new());
    let dialog = keyed(dismissible, move |dismissible| {
        overlay_host(
            open,
            stack()
                .style(mue_macroquad::style! {
                    width: {u(340.)};
                    font-size: {f(15.)};
                    padding: {(u(20.),)};
                    gap: {u(12.)};
                    border-radius: {u(14.)};
                    border-width: 1px;
                    border-color: {border()};
                    background: {panel_brush()};
                })
                .box_shadow(BoxShadows::elevation(24.))
                .children((
                    heading("Dialog")
                        .level(HeadingLevel::H4)
                        .style(mue_macroquad::style! {
                            font-size: {f(18.)}; line-height: {u(24.)};
                            color: {foreground()};
                        }),
                    input().model(name).placeholder("Type something"),
                    row((
                        button("Cancel").on_activate(move || open.set(false)),
                        button("Save").primary(true).on_activate(move || {
                            log_event(format!("dialog saved {:?}", name.get_clone_untracked()));
                            open.set(false);
                        }),
                    )),
                )),
        )
        .dismissible(*dismissible)
        .on_dismiss(move || {
            log_event("dialog dismissed");
            open.set(false);
        })
        .into_node()
    });
    card(
        "Modal dialog",
        (
            dismissible_control,
            row((button("Open dialog")
                .primary(true)
                .on_activate(move || open.set(true)),)),
            dialog,
        ),
    )
}

fn menu_item(label: &'static str, open: Signal<bool>) -> Node {
    link_overlay()
        .focusable(true)
        .cursor(Cursor::Pointer)
        .with_hovered(|view, hovered| {
            let tint = hover_tint();
            view.style(mue_macroquad::style! {
                background: {computed(move |_| if hovered.get() { tint.get_clone() } else { Brush::Solid(Color::TRANSPARENT) })};
            })
        })
        .focus_ring(2., BLUE, -2.)
        .style(mue_macroquad::style! {
            padding: {(u(12.), u(8.))};
            border-radius: {u(6.)};
        })
        .on_activate(move || {
            log_event(format!("menu {label}"));
            open.set(false);
        })
        .children(text(label).style(mue_macroquad::style! {
            font-size: {f(14.)}; line-height: {u(20.)};
            color: {foreground()};
        }))
        .into_node()
}

fn popover_card() -> Node {
    let open = signal(false);
    let trigger = NodeHandle::new();
    let (placement, placement_control) = segmented(
        "Placement",
        vec![
            ("Bottom start", Placement::BottomStart),
            ("Bottom end", Placement::BottomEnd),
            ("Top start", Placement::TopStart),
            ("Right", Placement::Right),
        ],
    );
    let tip_trigger = NodeHandle::new();
    let tip_visible = signal(false);
    card(
        "Popover and tooltip from primitives",
        (
            placement_control,
            row((
                button("Menu ▾")
                    .node_handle(trigger)
                    .on_activate(move || open.set(true)),
                flexbox()
                    .node_handle(tip_trigger)
                    .style(mue_macroquad::style! {
                        padding: {(u(12.), u(8.))};
                        border-radius: {u(8.)};
                        background: {raised()};
                    })
                    .with_hovered(move |view, hovered| {
                        watch(hovered, move |hovered| tip_visible.set(hovered.get()));
                        view.children(text("Hover here").style(mue_macroquad::style! {
                            font-size: {f(14.)}; line-height: {u(20.)};
                            color: {foreground()};
                        }))
                    }),
            )),
            teleport(Layer::Popover, move || {
                use_back_handler(open, move || open.set(false));
                presence(open, move || {
                    anchored(trigger)
                        .placement(placement)
                        .offset(6.)
                        .directive(ClickOutside::new(move || open.set(false)).enabled(open))
                        .style(mue_macroquad::style! {
                            padding: {(u(6.),)};
                            min-width: {u(180.)};
                            flex-direction: column;
                            border-radius: {u(10.)};
                            border-width: 1px;
                            border-color: {border()};
                            background: {panel_brush()};
                        })
                        .box_shadow(BoxShadows::elevation(10.))
                        .children((
                            menu_item("Rename", open),
                            menu_item("Duplicate", open),
                            menu_item("Delete", open),
                        ))
                })
                .animation(PresenceAnimation::fade().slide(0., -6.))
            }),
            teleport(Layer::Popover, move || {
                presence(tip_visible, move || {
                    anchored(tip_trigger)
                        .placement(Placement::Top)
                        .offset(6.)
                        .style(mue_macroquad::style! {
                            padding: {(u(10.), u(6.))};
                            border-radius: {u(6.)};
                            background: #000000;
                        })
                        .children(
                            text("A tooltip above its anchor").style(mue_macroquad::style! {
                                font-size: {f(12.)}; line-height: {u(16.)};
                                color: #ffffff;
                            }),
                        )
                })
                .animation(PresenceAnimation::fade())
            }),
        ),
    )
}

fn focus_card() -> Node {
    let item = |label: &'static str, tab_index: i32| {
        button(label)
            .tab_index(tab_index)
            .on_focus(move |_| log_event(format!("focus {label}")))
            .on_activate(move || log_event(format!("activate {label}")))
    };
    card(
        "Keyboard focus",
        row((
            item("One", 0),
            item("Two", 0),
            item("Not in tab order", -1),
            item("Three", 0),
        )),
    )
}

fn back_card() -> Node {
    let first = signal(false);
    let second = signal(false);
    // Handlers enabled later win, like Android's OnBackPressedDispatcher.
    use_back_handler(first, move || {
        first.set(false);
        log_event("back closed level 1");
    });
    use_back_handler(second, move || {
        second.set(false);
        log_event("back closed level 2");
    });
    let status = computed(move |_| {
        Rc::<str>::from(format!(
            "level 1 {} · level 2 {}",
            if first.get() { "open" } else { "closed" },
            if second.get() { "open" } else { "closed" },
        ))
    });
    card(
        "Back navigation",
        (
            row((
                button("Open level 1").on_activate(move || first.set(true)),
                button("Open level 2").on_activate(move || second.set(true)),
                button("Back").primary(true).on_activate(move || {
                    let any = can_go_back();
                    let handled = dispatch_back();
                    log_event(format!(
                        "dispatch_back handled {handled}, can_go_back was {any}"
                    ));
                }),
            )),
            mono(status),
            back_swipe()
                .mouse(true)
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: {u(80.)};
                    border-radius: {u(10.)};
                    background: {raised()};
                    justify-content: center;
                    padding-left: {u(16.)};
                })
                .children(hint("Swipe right from the left edge")),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            dialog_card(),
            popover_card(),
            focus_card(),
            back_card(),
            card("Events", event_log()),
            checklist(
                PAGE,
                &[
                    "Tab stays inside the dialog and returns to its button",
                    "Back and backdrop close the dialog only when dismissible",
                    "The menu stays in the window and closes on an outside press",
                    "The tooltip never sticks after a touch",
                    "Tab skips the Not in tab order button",
                    "Back closes level 2, then level 1, then the page",
                    "The edge swipe triggers back",
                ],
            ),
        ),
    )
}
