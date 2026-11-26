use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{event::hover::hover_capable, style::Styleable};
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
    let viewport = mue_macroquad::viewport::use_viewport();
    let safe = use_safe_area();
    let keyboard = use_keyboard();
    let hover = hover_capable();
    let color = use_color_mode();
    let reduced = use_media_query("(prefers-reduced-motion: reduce)");
    let show = |value: ReadSignal<String>| value.map(|text| Rc::<str>::from(text.as_str()));
    card(
        "Device",
        "What the platform reports right now. Rotate the device, resize the window, or open the soft keyboard and watch these update.",
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
                "Safe area t r b l",
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
                chip(page, false, move || super::open_page(index)),
                mono(line),
            ))
        },
    );
    card(
        "Manual check results",
        "Every page ends with a checklist. Results stay while the app runs, so you can walk through the pages and read the totals here.",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(6.)}; })
            .children((
                para(totals),
                rows,
                row((button("Reset all checks").on_activate(reset_checks),)),
            )),
    )
}

pub fn page() -> Node {
    page_body(
        "A test bench for mue. Pick a page in the navigation, try the demos, and mark the checklist at the bottom of each page. Back (Escape, the Android back button) returns to the previous page.",
        (
            diagnostics(),
            results(),
            card(
                "How to use this on a new platform",
                "",
                stack()
                    .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(4.)}; })
                    .children((
                        para("1. Open Self tests and confirm every automated check passes."),
                        para("2. Visit each page and mark its manual checks."),
                        para("3. Text input and Pointer & gestures cover the platform bridges (IME, soft keyboard, multi-touch)."),
                        para("4. Come back here and read the totals."),
                    )),
            ),
            checklist(
                PAGE,
                &[
                    "Window size and DPI match the device",
                    "Rotating or resizing updates the readouts",
                    "The pointer kind switches between touch and mouse when both are used",
                ],
            ),
        ),
    )
}
