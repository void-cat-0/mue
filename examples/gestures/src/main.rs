use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    event::pointer::{PointerAction, PointerId},
    layout::use_layout,
    node::{Node, flexbox},
    style::{Style, Styleable, StyleableExt},
};
use taffy::Dimension;

const INK: Color = Color::from_rgba(230, 237, 243, 255);
const BLUE: Color = Color::from_rgba(0, 210, 255, 255);
const MUTED: Color = Color::from_rgba(145, 160, 178, 255);
const BACKGROUND: Color = Color::from_rgba(13, 17, 23, 255);
const SURFACE: Color = Color::from_rgba(22, 29, 39, 255);
const BORDER: Color = Color::from_rgba(54, 68, 86, 255);
const PENDING: Color = Color::from_rgba(255, 193, 7, 255);
const SUCCESS: Color = Color::from_rgba(57, 230, 132, 255);
const CANCELLED: Color = Color::from_rgba(255, 92, 112, 255);

#[derive(Clone, Copy)]
enum Demo {
    Click,
    LongPress,
    Hover,
    Scroll,
    Pan,
    Swipe,
    Pinch,
    Rotate,
}
impl Demo {
    fn label(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::LongPress => "Long press",
            Self::Hover => "Hover",
            Self::Scroll => "Scroll",
            Self::Pan => "Pan",
            Self::Swipe => "Swipe",
            Self::Pinch => "Pinch",
            Self::Rotate => "Rotate",
        }
    }
}

fn panel(demo: Demo) -> Node {
    Node::build_with_style(
        Style::new()
            .width(Dimension::percent(0.5))
            .height(Dimension::percent(0.25))
            .flex_shrink(0.),
        move |style| {
            let rect = use_layout(style).rect;
            let count = signal(0);
            let active = signal(false);
            let hold = signal(None::<PointerId>);
            let hold_recognized = signal(false);
            let hold_cancelled = signal(false);
            let position = signal(None::<Vec2>);
            let trail = signal(None::<(Vec2, Vec2)>);
            let contacts = signal(Vec::<(PointerId, Vec2)>::new());
            let amount = signal(0_f32);
            let status = signal(match demo {
                Demo::Click => "0".into(),
                Demo::LongPress => "Idle  0".into(),
                Demo::Hover => "Outside".into(),
                Demo::Pinch => "1.00x".into(),
                Demo::Rotate => "0°".into(),
                Demo::Scroll => "0".into(),
                _ => String::new(),
            });
            style.on_pointer_event.append(move |event| {
                let r = rect.get();
                let point = vec2(event.position().x - r.x, event.position().y - r.y);
                match demo {
                    Demo::LongPress => {
                        if event.action() == PointerAction::Down && hold.get().is_none() {
                            hold.set(Some(event.pointer_id()));
                            hold_recognized.set(false);
                            hold_cancelled.set(false);
                            status.set(format!("Holding  {}", count.get()));
                        } else if hold.get().is_some_and(|id| id == event.pointer_id()) {
                            let moved = (event.position() - event.start_position()).length() > 15.;
                            if event.action() == PointerAction::Cancel
                                || (event.action() == PointerAction::Move
                                    && moved
                                    && !hold_recognized.get())
                                || event.action() == PointerAction::Up
                            {
                                let recognized = hold_recognized.get();
                                hold.set(None);
                                hold_cancelled.set(!recognized);
                                status.set(format!(
                                    "{}  {}",
                                    if recognized {
                                        "Recognized"
                                    } else {
                                        "Cancelled"
                                    },
                                    count.get()
                                ));
                            }
                        }
                    }
                    Demo::Click => {
                        active.set(!matches!(
                            event.action(),
                            PointerAction::Up | PointerAction::Cancel
                        ));
                    }
                    Demo::Swipe => match event.action() {
                        PointerAction::Down => {
                            trail.set(Some((point, point)));
                            active.set(false);
                            status.set(String::new());
                        }
                        PointerAction::Move | PointerAction::Up => trail.update(|line| {
                            if let Some((_, end)) = line {
                                *end = point;
                            }
                        }),
                        PointerAction::Cancel => {
                            trail.set(None);
                            active.set(false);
                        }
                    },
                    Demo::Pinch | Demo::Rotate => contacts.update(|points| match event.action() {
                        PointerAction::Down if points.len() < 2 => {
                            points.push((event.pointer_id(), point))
                        }
                        PointerAction::Move => {
                            if let Some((_, p)) =
                                points.iter_mut().find(|(id, _)| *id == event.pointer_id())
                            {
                                *p = point;
                            }
                        }
                        PointerAction::Up | PointerAction::Cancel => {
                            points.retain(|(id, _)| *id != event.pointer_id())
                        }
                        _ => {}
                    }),
                    _ => {}
                }
            });
            match demo {
                Demo::Click => style.on_tap.append(move |_| {
                    count.update(|n| *n += 1);
                    status.set(count.get().to_string());
                }),
                Demo::LongPress => style.on_long_press.append(move |_| {
                    count.update(|n| *n += 1);
                    hold_recognized.set(true);
                    status.set(format!("Recognized  {}", count.get()));
                }),
                Demo::Hover => style.on_hover_event.append(move |event| {
                    let inside =
                        !matches!(event.action(), PointerAction::Up | PointerAction::Cancel);
                    active.set(inside);
                    status.set(if inside { "Inside" } else { "Outside" }.into());
                }),
                Demo::Scroll => style.on_wheel_event.append(move |event| {
                    amount.update(|n| *n += event.delta.y);
                    status.set(format!("{:.0}", amount.get()));
                }),
                Demo::Pan => style.on_pan.append(move |event| {
                    let r = rect.get();
                    position.set(Some(vec2(event.position.x - r.x, event.position.y - r.y)));
                    status.set(format!(
                        "{:.0}, {:.0}",
                        event.translation.x, event.translation.y
                    ));
                }),
                Demo::Swipe => style.on_swipe.append(move |event| {
                    active.set(true);
                    status.set(format!(
                        "{:?}  {:.0} px/s",
                        event.direction,
                        event.velocity.length()
                    ));
                }),
                Demo::Pinch => style.on_pinch.append(move |event| {
                    status.set(format!("{:.2}x", event.scale));
                }),
                Demo::Rotate => style.on_rotate.append(move |event| {
                    status.set(format!("{:.0}°", event.rotation.to_degrees()));
                }),
            }
            style.on_render.append(move |_| {
                let r = rect.get();
                draw_rectangle(r.x + 6., r.y + 6., r.w - 12., r.h - 12., SURFACE);
                draw_rectangle_lines(r.x + 6., r.y + 6., r.w - 12., r.h - 12., 1., BORDER);
                draw_text(demo.label(), r.x + 22., r.y + 32., 22., INK);
                let readout = status.get_clone();
                let width = measure_text(&readout, None, 17, 1.).width;
                draw_text(
                    &readout,
                    r.x + r.w - 22. - width,
                    r.y + r.h - 20.,
                    17.,
                    MUTED,
                );
                let center = vec2(r.w * 0.5, r.h * 0.54);
                // Keep all feedback inside the test panel, including captured drags.
                let screen = |p: Vec2| {
                    vec2(
                        r.x + p.x.clamp(30., (r.w - 30.).max(30.)),
                        r.y + p.y.clamp(55., (r.h - 44.).max(55.)),
                    )
                };
                let c = screen(center);
                match demo {
                    Demo::LongPress => {
                        let recognized = hold_recognized.get();
                        let color = if recognized {
                            SUCCESS
                        } else if hold_cancelled.get() {
                            CANCELLED
                        } else if hold.get().is_some() {
                            PENDING
                        } else {
                            MUTED
                        };
                        draw_circle(c.x, c.y, 22., if recognized { SUCCESS } else { BACKGROUND });
                        draw_circle_lines(c.x, c.y, 30., 3., color);
                        if hold_cancelled.get() {
                            draw_line(c.x - 8., c.y - 8., c.x + 8., c.y + 8., 3., color);
                            draw_line(c.x + 8., c.y - 8., c.x - 8., c.y + 8., 3., color);
                        } else if !recognized {
                            draw_circle(c.x, c.y, 5., color);
                        }
                    }
                    Demo::Click | Demo::Hover => {
                        draw_circle(
                            c.x,
                            c.y,
                            if active.get() { 26. } else { 22. },
                            if active.get() { BLUE } else { BACKGROUND },
                        );
                        draw_circle_lines(c.x, c.y, 30., 2., BLUE);
                    }
                    Demo::Scroll => {
                        for i in -1..=1 {
                            let y = c.y
                                + (i as f32 * 18. + amount.get() * 5. + 27.).rem_euclid(54.)
                                - 27.;
                            draw_line(c.x - 36., y, c.x + 36., y, 4., BLUE);
                        }
                        draw_line(c.x + 52., c.y - 30., c.x + 52., c.y + 30., 3., BACKGROUND);
                        let y = c.y - (amount.get() * 3.).clamp(-22., 22.);
                        draw_circle(c.x + 52., y, 5., BLUE);
                    }
                    Demo::Pan => {
                        let p = screen(position.get().unwrap_or(center));
                        draw_circle(p.x, p.y, 22., BLUE);
                        draw_circle_lines(p.x, p.y, 29., 2., BACKGROUND);
                    }
                    Demo::Swipe => {
                        let (a, b) = trail
                            .get()
                            .unwrap_or((center - vec2(50., 0.), center + vec2(50., 0.)));
                        let a = screen(a);
                        let b = screen(b);
                        let color = if active.get() { SUCCESS } else { PENDING };
                        draw_line(a.x, a.y, b.x, b.y, 3., color);
                        draw_circle(a.x, a.y, 5., color);
                        draw_circle(b.x, b.y, 9., color);
                    }
                    Demo::Pinch | Demo::Rotate => {
                        let points = contacts.get_clone();
                        let a = screen(
                            points
                                .first()
                                .map(|(_, p)| *p)
                                .unwrap_or(center - vec2(42., 0.)),
                        );
                        let b = screen(
                            points
                                .get(1)
                                .map(|(_, p)| *p)
                                .unwrap_or(center + vec2(42., 0.)),
                        );
                        draw_line(a.x, a.y, b.x, b.y, 2., BACKGROUND);
                        for (i, p) in [a, b].iter().enumerate() {
                            draw_circle_lines(p.x, p.y, 20., 2., BLUE);
                            if i < points.len() {
                                draw_circle(p.x, p.y, 13., BLUE);
                            }
                        }
                    }
                }
            });
        },
    )
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - gestures".into(),
            window_width: 900,
            window_height: 760,
            ..Default::default()
        },
        async {
            let root = flexbox()
                .w_full()
                .h_full()
                .flex_wrap(taffy::FlexWrap::Wrap)
                .children((
                    panel(Demo::Click),
                    panel(Demo::LongPress),
                    panel(Demo::Hover),
                    panel(Demo::Scroll),
                    panel(Demo::Pan),
                    panel(Demo::Swipe),
                    panel(Demo::Pinch),
                    panel(Demo::Rotate),
                ));
            let mut app = App::new(root);
            loop {
                clear_background(BACKGROUND);
                app.frame();
                next_frame().await;
            }
        },
    );
}
