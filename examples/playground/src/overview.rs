use super::controls::*;
use mue_components::*;
use mue_components::system::ControlSize;
use mue_core::prelude::*;
use mue_macroquad::{input::hover::hover_capable, style::Styleable};
use std::rc::Rc;

const PAGE: &str = "Overview";

fn platform() -> String {
    let os = if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "ios") {
        "iOS"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_arch = "wasm32") {
        "Web"
    } else {
        "Other"
    };
    format!("{os} · {}", std::env::consts::ARCH)
}

fn diagnostics() -> Node {
    let viewport = mue_macroquad::app::viewport::use_viewport();
    let safe = use_safe_area();
    let keyboard = use_keyboard();
    let hover = hover_capable();
    let color = use_color_mode();
    let reduced = use_media_query("(prefers-reduced-motion: reduce)");
    let show = |value: ReadSignal<String>| value.map(|text| Rc::<str>::from(text.as_str()));
    card(
        "Device",
        row((
            stat("Platform", Rc::<str>::from(platform())),
            stat(
                "Window (logical px)",
                show(viewport.map(|v| format!("{:.0} × {:.0}", v.width, v.height))),
            ),
            stat(
                "DPI scale",
                show(viewport.map(|v| format!("{:.2}", v.dpi_scale))),
            ),
            stat(
                "Orientation",
                show(viewport.map(|v| {
                    String::from(if v.is_portrait() {
                        "portrait"
                    } else {
                        "landscape"
                    })
                })),
            ),
            stat(
                "Safe area (t r b l)",
                show(
                    safe.map(|e| {
                        format!("{:.0} {:.0} {:.0} {:.0}", e.top, e.right, e.bottom, e.left)
                    }),
                ),
            ),
            stat(
                "Pointer",
                show(
                    hover.map(|hover| String::from(if hover { "hover (mouse)" } else { "touch" })),
                ),
            ),
            stat(
                "Soft keyboard",
                show(computed(move |_| {
                    if keyboard.visible.get() {
                        format!("visible, {:.0} px", keyboard.inset.get())
                    } else {
                        String::from("hidden")
                    }
                })),
            ),
            stat(
                "Color scheme",
                show(color.scheme.map(|scheme| format!("{scheme:?}"))),
            ),
            stat(
                "Reduced motion",
                show(reduced.map(|on| String::from(if on { "yes" } else { "no" }))),
            ),
            stat(
                "Frame rate",
                show(super::fps().map(|fps| format!("{fps} FPS"))),
            ),
        )),
    )
}

fn results() -> Node {
    let summary = computed(move |_| check_summary());
    let totals = summary.map(|pages| {
        let passed: usize = pages.iter().map(|page| page.1).sum();
        let failed: usize = pages.iter().map(|page| page.2).sum();
        let total: usize = pages.iter().map(|page| page.3).sum();
        Rc::<str>::from(format!(
            "{passed} passed · {failed} failed · {} untested",
            total - passed - failed
        ))
    });
    let rows = map_keyed(
        summary.map(|pages| {
            pages
                .iter()
                .map(|page| page.0)
                .collect::<Rc<[&'static str]>>()
        }),
        |page| *page,
        move |page| {
            let page = *page;
            let line = summary.map(move |pages| {
                let (_, passed, failed, total) = pages
                    .iter()
                    .copied()
                    .find(|entry| entry.0 == page)
                    .unwrap_or((page, 0, 0, 0));
                Rc::<str>::from(format!(
                    "{passed}/{total} passed{}",
                    if failed > 0 {
                        format!(", {failed} failed")
                    } else {
                        String::new()
                    }
                ))
            });
            let index = super::PAGES
                .iter()
                .position(|entry| entry.0 == page)
                .unwrap_or(0);
            row((
                button(page)
                    .variant(ButtonVariant::Outline)
                    .size(ControlSize::Sm)
                    .on_activate(move || super::open_page(index)),
                mono(line),
            ))
        },
    );
    card(
        "Check results",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(6.)}; })
            .children((
                para(totals),
                rows,
                row((button("Clear results").variant(ButtonVariant::Outline).size(ControlSize::Sm).on_activate(reset_checks),)),
            )),
    )
}

/// One numbered step of the test procedure.
fn step(number: u32, label: &'static str) -> Node {
    hstack()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            gap: {u(10.)};
            align-items: center;
        })
        .children((
            center()
                .style(mue_macroquad::style! {
                    flex: none;
                    width: {u(22.)}; height: {u(22.)};
                    border-radius: 9999px;
                    border-width: 1px;
                    border-color: {border()};
                })
                .children(text(Rc::<str>::from(number.to_string())).style(mue_macroquad::style! {
                    font-size: {f(12.)}; line-height: {u(16.)};
                    font-weight: 600;
                    color: {muted()};
                })),
            text(label).style(mue_macroquad::style! {
                flex: 1; min-width: 0px;
                font-size: {f(14.)}; line-height: {u(20.)};
                color: {foreground()};
            }),
        ))
        .into_node()
}

fn procedure() -> Node {
    card(
        "Test a new platform",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(8.)}; })
            .children((
                step(1, "Pass every self test"),
                step(2, "Try Components in light and dark mode, at several zoom levels"),
                step(3, "Mark the checks at the end of each page"),
                step(4, "Compare the totals in Check results"),
                row((button("Run self tests").primary(true).size(ControlSize::Sm).on_activate(|| super::open_page(1)),)),
            )),
    )
}

pub fn page() -> Node {
    page_body(
        (
            procedure(),
            diagnostics(),
            results(),
            checklist(
                PAGE,
                &[
                    "Window size and DPI match the device",
                    "Resizing or rotating updates the readouts",
                    "Pointer switches between touch and mouse",
                ],
            ),
        ),
    )
}
