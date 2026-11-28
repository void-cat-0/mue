use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    input::gesture::{Gesture, GesturePhase},
    input::pointer::{ClaimToken, PointerAction, PointerEvent, PointerId, PointerType},
    math::{Rect, Vector},
    node::{BuildNode, NodeContext, add_gesture, flexbox::use_container, use_rect},
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};
use std::rc::Rc;

const PAGE: &str = "Pointer & gestures";

type Touches = Rc<[(PointerId, Vector, PointerType)]>;

/// A surface that claims every pointer pressed on it and publishes their
/// positions, so the page does not scroll while fingers are down.
struct TouchPad {
    touches: Signal<Touches>,
    peak: Signal<usize>,
    style: Style,
}

impl Styleable for TouchPad {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for TouchPad {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            touches,
            peak,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            use_container(style);
            let rect = use_rect();
            add_gesture(TouchTracker {
                rect,
                touches,
                peak,
            });
        })
    }
}

struct TouchTracker {
    rect: ReadSignal<Rect>,
    touches: Signal<Touches>,
    peak: Signal<usize>,
}

impl TouchTracker {
    fn set(&self, id: PointerId, position: Option<(Vector, PointerType)>) {
        let origin = self.rect.get_untracked();
        let mut list = self.touches.get_clone_untracked().to_vec();
        match position {
            Some((position, kind)) => {
                let local = position - Vector::new(origin.x, origin.y);
                match list.iter_mut().find(|entry| entry.0 == id) {
                    Some(entry) => entry.1 = local,
                    None => list.push((id, local, kind)),
                }
            }
            None => list.retain(|entry| entry.0 != id),
        }
        if list.len() > self.peak.get_untracked() {
            self.peak.set(list.len());
        }
        self.touches.set(list.into());
    }
}

impl Gesture for TouchTracker {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        match event.action() {
            PointerAction::Down => {
                if token.claim() {
                    self.set(
                        event.pointer_id(),
                        Some((event.position(), event.pointer_type())),
                    );
                }
            }
            PointerAction::Move => {
                if self
                    .touches
                    .get_clone_untracked()
                    .iter()
                    .any(|entry| entry.0 == event.pointer_id())
                {
                    self.set(
                        event.pointer_id(),
                        Some((event.position(), event.pointer_type())),
                    );
                }
            }
            PointerAction::Up | PointerAction::Cancel => self.set(event.pointer_id(), None),
        }
    }

    fn on_rejected(&mut self, pointer_id: PointerId) {
        self.set(pointer_id, None);
    }
}

fn touch_card() -> Node {
    let touches: Signal<Touches> = signal(Rc::from(Vec::new()));
    let peak = signal(0usize);
    let dots = map_keyed(
        touches.map(|list| {
            list.iter()
                .map(|entry| entry.0)
                .collect::<Rc<[PointerId]>>()
        }),
        |id| *id,
        move |id| {
            let id = *id;
            let position = touches.map(move |list| {
                list.iter()
                    .find(|entry| entry.0 == id)
                    .map_or(Vector::ZERO, |entry| entry.1)
            });
            let index =
                touches.map(move |list| list.iter().position(|entry| entry.0 == id).unwrap_or(0));
            center()
                .absolute()
                .inset_left(position.map(|p| Size::Px(p.x - 28.)))
                .inset_top(position.map(|p| Size::Px(p.y - 28.)))
                .style(mue_macroquad::style! {
                    width: 56px; height: 56px;
                    border-radius: 28px;
                    background: {index.map(|i| mue_macroquad::paint::shader::Brush::Solid(if i % 2 == 0 { BLUE } else { CORAL }))};
                })
                .children(text(index.map(|i| Rc::<str>::from(format!("{}", i + 1)))).style(
                    mue_macroquad::style! { font-size: 18px; line-height: 24px; color: #131720; },
                ))
        },
    );
    let readout = computed(move |_| {
        let list = touches.get_clone();
        let mut lines = vec![format!("{} active · peak {}", list.len(), peak.get())];
        for (index, (_, position, kind)) in list.iter().enumerate() {
            lines.push(format!(
                "#{} {:?} at {:.0}, {:.0}",
                index + 1,
                kind,
                position.x,
                position.y
            ));
        }
        Rc::<str>::from(lines.join("\n"))
    });
    card(
        "Multi-touch",
        (
            TouchPad {
                touches,
                peak,
                style: Style::new(),
            }
            .style(mue_macroquad::style! {
                position: relative;
                width: 100%;
                height: {u(260.)};
                border-radius: {mue_components::system::radius::MD};
                background: {raised()};
                overflow: hidden;
                align-items: center;
                justify-content: center;
            })
            .children((
                text("Touch with several fingers")
                    .style(mue_macroquad::style! {
                        font-size: {f(13.)}; line-height: {u(19.)};
                        color: {muted()};
                    })
                    .show_if(touches.map(|list| list.is_empty())),
                dots,
            )),
            mono(readout),
            row((button("Reset peak")
                .on_activate(move || peak.set(touches.get_clone_untracked().len())),)),
        ),
    )
}

fn press_card() -> Node {
    let taps = signal(0u32);
    let holds = signal(0u32);
    let pressed = signal(false);
    let child = signal(0u32);
    card(
        "Tap, long press, hover, and press state",
        link_overlay()
            .style(mue_macroquad::style! {
                width: 100%; min-width: 0px;
                padding: {(u(16.),)};
                gap: {u(6.)};
                border-radius: {mue_components::system::radius::MD};
                background: {raised()};
            })
            .use_pressed(pressed)
            .on_activate(move || {
                taps.update(|n| *n += 1);
                log_event("panel tap");
            })
            .on_long_press(move |_| {
                holds.update(|n| *n += 1);
                log_event("panel long press");
            })
            .with_hovered(move |view, hover| {
                view.style(mue_macroquad::style! {
                    border-width: 2px;
                    border-color: {hover.map(|on| mue_macroquad::paint::shader::Brush::Solid(if on { BLUE } else { mue_macroquad::paint::color::Color::TRANSPARENT }))};
                })
                .children((
                    mono(computed(move |_| {
                        Rc::<str>::from(format!(
                            "taps {} · long presses {} · pressed {} · hovered {}",
                            taps.get(),
                            holds.get(),
                            pressed.get(),
                            hover.get()
                        ))
                    })),
                    link(child.map(|n| Rc::<str>::from(format!("Nested link ({n}×)"))))
                        .style(mue_macroquad::style! { font-size: {f(14.)}; line-height: {u(20.)}; })
                        .on_activate(move || {
                            child.update(|n| *n += 1);
                            log_event("nested link tap");
                        }),
                ))
            }),
    )
}

fn gestures_card() -> Node {
    let swipe = signal(String::from("–"));
    let pinch = signal(1f32);
    let rotate = signal(0f32);
    let pan = signal(Vector::ZERO);
    let pad = |label: &'static str| {
        center()
            .style(mue_macroquad::style! {
                flex: 1;
                min-width: {u(150.)};
                height: {u(120.)};
                border-radius: {mue_components::system::radius::MD};
                background: {raised()};
            })
            .children(text(label).style(mue_macroquad::style! {
                font-size: {f(15.)}; line-height: {u(22.)};
                color: {muted()};
            }))
    };
    card(
        "Recognizers",
        (
            row((
                pad("Swipe").on_swipe(move |event| {
                    swipe.set(format!("{:?}", event.direction));
                    log_event(format!("swipe {:?}", event.direction));
                }),
                pad("Pan").on_pan(move |event| pan.set(event.translation)),
                pad("Pinch").on_pinch(move |event| pinch.set(event.scale)),
                pad("Rotate").on_rotate(move |event| rotate.set(event.rotation)),
            )),
            mono(computed(move |_| {
                let pan = pan.get();
                Rc::<str>::from(format!(
                    "swipe {} · pan {:.0}, {:.0} · pinch {:.2}× · rotate {:.0}°",
                    swipe.get_clone(),
                    pan.x,
                    pan.y,
                    pinch.get(),
                    rotate.get().to_degrees()
                ))
            })),
        ),
    )
}

fn map_card() -> Node {
    let deer = super::assets::background_texture(include_bytes!("../../../deer.png"));
    let offset = signal(Vector::ZERO);
    let scale = signal(1f32);
    let rotation = signal(0f32);
    let start_scale = signal(1f32);
    let start_rotation = signal(0f32);
    let transform = computed(move |_| {
        let offset = offset.get();
        let scale = scale.get();
        Transform(vec![
            TransformOp::Translate(Size::Px(offset.x), Size::Px(offset.y)),
            TransformOp::Rotate(rotation.get()),
            TransformOp::Scale(scale, scale),
        ])
    });
    card(
        "Map-style transform",
        (
            scroll_area()
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: {u(300.)};
                    border-radius: {mue_components::system::radius::MD};
                    background: {raised()};
                })
                .options(ScrollOptions {
                    touch: false,
                    wheel: false,
                    scrollbars: false,
                    chain: false,
                    ..Default::default()
                })
                .content_style(mue_macroquad::style! {
                    width: 100%; height: 100%;
                    align-items: center; justify-content: center;
                })
                .on_transform(move |event| {
                    if event.phase == GesturePhase::Start {
                        start_scale.set(scale.get_untracked());
                        start_rotation.set(rotation.get_untracked());
                    }
                    if matches!(
                        event.phase,
                        GesturePhase::Start | GesturePhase::Update | GesturePhase::End
                    ) {
                        scale.set((start_scale.get_untracked() * event.scale).clamp(0.5, 4.));
                        rotation.set(start_rotation.get_untracked() + event.rotation);
                        if event.phase != GesturePhase::Start {
                            offset.update(|value| *value += event.translation_delta);
                        }
                    }
                })
                .children(
                    image(deer)
                        .object_contain()
                        .style(mue_macroquad::style! { width: 70%; height: 70%; flex: none; })
                        .transform_list(transform),
                ),
            row((
                button("Reset view").on_activate(move || {
                    offset.set(Vector::ZERO);
                    scale.set(1.);
                    rotation.set(0.);
                }),
                mono(computed(move |_| {
                    Rc::<str>::from(format!(
                        "{:.2}× · {:.0}°",
                        scale.get(),
                        rotation.get().to_degrees()
                    ))
                })),
            )),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            touch_card(),
            press_card(),
            gestures_card(),
            map_card(),
            card("Events", event_log()),
            checklist(
                PAGE,
                &[
                    "Each finger on the pad gets its own dot, even 5 or more",
                    "Lifting a finger removes its dot",
                    "Tap and long press count once each",
                    "The nested link does not tap the panel",
                    "Hover never sticks after a touch",
                    "Swipe reports the right direction",
                    "Pinch and rotate track two fingers",
                    "The map image pans, zooms, and rotates together",
                ],
            ),
        ),
    )
}
