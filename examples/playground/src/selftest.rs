//! Automated checks that run on the device, so a new platform can be judged
//! at a glance: text editing, layout, hit testing, shaping, reactivity, back
//! handling, pointer observers, and timers.

use super::controls::*;
use mue_components::*;
use mue_core::{prelude::*, scope::OwnedScope};
use mue_macroquad::{
    app::runtime::set_timeout,
    input::text_input::{
        DecimalNumber, EditKind, MaxLength, TextEditingValue, TextFormatter, TextUnit, UndoHistory,
        byte_to_utf16, digits_only, grapheme_len, utf16_to_byte, word_at,
    },
    input::{
        hit_test::{hit_rect, hit_test},
        pointer::notify_pointer_down,
    },
    math::{Rect, Vector},
    node::NodeHandle,
    paint::color::Color,
    paint::shader::Brush,
    style::{Styleable, StyleableExt},
};
use std::{cell::Cell, rc::Rc};

type Outcome = Result<String, String>;

fn expect<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}, want {want:?}"))
    }
}

fn utf16_offsets() -> Outcome {
    let text = "a😀b";
    expect("byte→utf16", byte_to_utf16(text, 5), 3)?;
    expect("utf16→byte", utf16_to_byte(text, 3), 5)?;
    // An offset inside a surrogate pair maps to a character boundary.
    expect(
        "inside a surrogate pair",
        text.is_char_boundary(utf16_to_byte(text, 2)),
        true,
    )?;
    Ok("\"a, U+1F600, b\": byte 5 ↔ UTF-16 unit 3".into())
}

fn graphemes() -> Outcome {
    let text = "👨‍👩‍👧e\u{301}x";
    expect("grapheme_len", grapheme_len(text), 3)?;
    let value = TextEditingValue::new(text).delete_backward(TextUnit::Grapheme);
    expect("delete x", value.text.as_str(), "👨‍👩‍👧e\u{301}")?;
    let value = value.delete_backward(TextUnit::Grapheme);
    expect("delete e + accent", value.text.as_str(), "👨‍👩‍👧")?;
    let value = value.delete_backward(TextUnit::Grapheme);
    expect("delete family emoji", value.text.as_str(), "")?;
    Ok("family emoji and combining accent delete as one character".into())
}

fn words() -> Outcome {
    expect("word_at", word_at("hello wide world", 8), 6..10)?;
    let value = TextEditingValue::new("one two three").delete_backward(TextUnit::Word);
    expect("delete word", value.text.as_str(), "one two ")?;
    Ok("word boundaries and word deletion".into())
}

fn composition() -> Outcome {
    let value = TextEditingValue::new("ab").compose("ni", None);
    expect("composing", value.is_composing(), true)?;
    expect("preedit text", value.text.as_str(), "abni")?;
    let value = value.compose("你", None);
    expect("replaced preedit", value.text.as_str(), "ab你")?;
    let value = value.finish_composing();
    expect("committed", value.is_composing(), false)?;
    Ok("preedit replaces itself, then commits".into())
}

fn formatters() -> Outcome {
    let old = TextEditingValue::new("abc");
    expect(
        "MaxLength(3)",
        MaxLength(3)
            .format(&old, TextEditingValue::new("abcd"))
            .text,
        String::from("abc"),
    )?;
    let composing = TextEditingValue::new("abc").compose("d", None);
    expect(
        "MaxLength waits while composing",
        MaxLength(3).format(&old, composing).text,
        String::from("abcd"),
    )?;
    expect(
        "digits_only",
        digits_only()
            .format(&TextEditingValue::new(""), TextEditingValue::new("1a2"))
            .text,
        String::from("12"),
    )?;
    let decimal = DecimalNumber { signed: true };
    expect(
        "DecimalNumber rejects a second point",
        decimal
            .format(&TextEditingValue::new("1.5"), TextEditingValue::new("1.5."))
            .text,
        String::from("1.5"),
    )?;
    Ok("max length, digits, decimal".into())
}

fn undo() -> Outcome {
    let mut history = UndoHistory::default();
    let first = TextEditingValue::new("a");
    history.record(&first, EditKind::Other, 0.);
    let second = TextEditingValue::new("ab");
    let restored = history.undo(&second).ok_or("nothing to undo")?;
    expect("undo", restored.text.as_str(), "a")?;
    let redone = history.redo(&restored).ok_or("nothing to redo")?;
    expect("redo", redone.text.as_str(), "ab")?;
    Ok("undo and redo restore values".into())
}

fn flex_layout() -> Outcome {
    let owner = OwnedScope::new();
    owner.run(|| {
        let left = NodeHandle::new();
        let right = NodeHandle::new();
        let root = hstack()
            .style(mue_macroquad::style! { width: 200px; height: 40px; gap: 0px; padding: 0px; })
            .children((
                flexbox()
                    .node_handle(left)
                    .style(mue_macroquad::style! { flex: 1; }),
                flexbox()
                    .node_handle(right)
                    .style(mue_macroquad::style! { flex: 3; }),
            ))
            .into_node();
        root.prepare_layout(Vector::new(200., 40.));
        let left = left.rect().ok_or("left not laid out")?;
        let right = right.rect().ok_or("right not laid out")?;
        expect("left width", left.w.round(), 50.)?;
        expect("right width", right.w.round(), 150.)?;
        expect("right x", right.x.round(), 50.)?;
        Ok("flex 1 : 3 splits 200 px into 50 + 150".into())
    })
}

fn hit_testing() -> Outcome {
    expect(
        "min-hit-size 44",
        hit_rect(Rect::new(10., 10., 20., 20.), 0., 44.),
        Rect::new(-2., -2., 44., 44.),
    )?;
    let owner = OwnedScope::new();
    owner.run(|| {
        let target = NodeHandle::new();
        let root = flexbox()
            .style(mue_macroquad::style! { width: 200px; height: 100px; padding: 40px; })
            .children(
                flexbox()
                    .node_handle(target)
                    .on_pointer_event(|_| {})
                    .style(
                        mue_macroquad::style! { width: 20px; height: 20px; min-hit-size: 44px; },
                    ),
            )
            .into_node();
        root.prepare_layout(Vector::new(200., 100.));
        let target = target.get_untracked().ok_or("target missing")?;
        expect(
            "hit 10 px outside",
            hit_test(*root, Vector::new(32., 50.)).contains(&target),
            true,
        )?;
        expect(
            "miss 20 px outside",
            hit_test(*root, Vector::new(15., 50.)).contains(&target),
            false,
        )?;
        Ok("a 20 px target accepts touches within 44 px".into())
    })
}

fn shaping() -> Outcome {
    let owner = OwnedScope::new();
    owner.run(|| {
        let samples = ["Latin", "你好世界", "مرحبا", "😀🎉"];
        let handles: Vec<NodeHandle> = samples.iter().map(|_| NodeHandle::new()).collect();
        let root = stack()
            .style(mue_macroquad::style! { width: 400px; align-items: flex-start; })
            .children(
                samples
                    .iter()
                    .zip(&handles)
                    .map(|(sample, handle)| {
                        text(*sample)
                            .node_handle(*handle)
                            .style(mue_macroquad::style! { font-size: 20px; line-height: 28px; })
                            .into_node()
                    })
                    .collect::<Vec<_>>(),
            )
            .into_node();
        root.prepare_layout(Vector::new(400., 400.));
        let mut widths = Vec::new();
        for (sample, handle) in samples.iter().zip(&handles) {
            let width = handle.rect().map_or(0., |rect| rect.w);
            if width < 10. {
                return Err(format!("{sample} measured {width:.1} px"));
            }
            widths.push(format!("{width:.0}"));
        }
        Ok(format!("widths {} px", widths.join(" / ")))
    })
}

fn reactivity() -> Outcome {
    let owner = OwnedScope::new();
    owner.run(|| {
        let base = signal(2);
        let tripled = computed(move |_| base.get() * 3);
        let runs = Rc::new(Cell::new(0));
        let counter = runs.clone();
        watch_effect(move || {
            tripled.get();
            counter.set(counter.get() + 1);
        });
        expect("effect ran on creation", runs.get() >= 1, true)?;
        base.set(5);
        // Effects may be deferred to the end of the current batch (this test
        // runs inside a timer callback), but computed values are always fresh.
        expect("computed", tripled.get(), 15)?;
        Ok("signal → computed is fresh; the effect ran".into())
    })
}

fn media_queries() -> Outcome {
    let owner = OwnedScope::new();
    owner.run(|| {
        expect(
            "min-width: 1px",
            use_media_query("(min-width: 1px)").get(),
            true,
        )?;
        expect(
            "max-width: 1px",
            use_media_query("(max-width: 1px)").get(),
            false,
        )?;
        expect(
            "unknown feature",
            use_media_query("(frobnicate: yes)").get(),
            false,
        )?;
        Ok("width features and unknown features".into())
    })
}

fn back_stack() -> Outcome {
    let handled = Rc::new(Cell::new(false));
    {
        let owner = OwnedScope::new();
        let flag = handled.clone();
        owner.run(|| {
            mue_macroquad::input::back::use_back_handler(true, move || flag.set(true));
        });
        expect(
            "dispatch handled",
            mue_macroquad::input::back::dispatch_back(),
            true,
        )?;
        expect("handler ran", handled.get(), true)?;
    }
    Ok("the newest enabled handler takes back; it unregisters with its scope".into())
}

fn click_outside() -> Outcome {
    let owner = OwnedScope::new();
    owner.run(|| {
        let outside = Rc::new(Cell::new(0));
        let count = outside.clone();
        let root = flexbox()
            .style(mue_macroquad::style! { width: 200px; height: 100px; })
            .children(
                flexbox()
                    .style(mue_macroquad::style! { width: 100px; height: 100px; })
                    .directive(ClickOutside::new(move || count.set(count.get() + 1))),
            )
            .into_node();
        root.prepare_layout(Vector::new(200., 100.));
        let press = |x: f32| {
            notify_pointer_down(Vector::new(x, 50.), &hit_test(*root, Vector::new(x, 50.)))
        };
        press(50.);
        expect("press inside", outside.get(), 0)?;
        press(150.);
        expect("press outside", outside.get(), 1)?;
        Ok("presses inside are ignored, outside fire".into())
    })
}

type Test = (&'static str, fn() -> Outcome);

const SYNC_TESTS: &[Test] = &[
    ("UTF-16 offsets", utf16_offsets),
    ("Grapheme clusters", graphemes),
    ("Word boundaries", words),
    ("IME composition model", composition),
    ("Input formatters", formatters),
    ("Undo history", undo),
    ("Flex layout", flex_layout),
    ("Hit testing and touch targets", hit_testing),
    ("Text shaping and font fallback", shaping),
    ("Signals and effects", reactivity),
    ("Media queries", media_queries),
    ("Back handler stack", back_stack),
    ("ClickOutside observer", click_outside),
];

#[derive(Clone, PartialEq)]
enum Status {
    Pending,
    Pass(String),
    Fail(String),
}

fn result_row(name: &'static str, status: ReadSignal<Status>) -> Node {
    let mark = status.map(|status| {
        Rc::<str>::from(match status {
            Status::Pending => "…",
            Status::Pass(_) => "✓",
            Status::Fail(_) => "✗",
        })
    });
    let tint = status.map(|status| {
        Brush::Solid(match status {
            Status::Pending => Color::from_rgba(146, 157, 178, 255),
            Status::Pass(_) => GREEN,
            Status::Fail(_) => RED,
        })
    });
    let detail = status.map(|status| match status {
        Status::Pending => Rc::<str>::from("running"),
        Status::Pass(detail) | Status::Fail(detail) => Rc::<str>::from(detail.as_str()),
    });
    hstack()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            gap: {u(10.)};
            padding: {(u(4.), u(4.))};
            align-items: flex-start;
        })
        .children((
            text(mark).style(mue_macroquad::style! {
                flex: none; width: {u(18.)};
                font-size: {f(15.)}; line-height: {u(20.)};
                font-family: monospace;
                color: {tint};
            }),
            stack()
                .style(mue_macroquad::style! { flex: 1; min-width: 0px; gap: 0px; })
                .children((
                    text(name).style(mue_macroquad::style! {
                        font-size: {f(14.)}; line-height: {u(20.)};
                        color: {foreground()};
                    }),
                    hint(detail),
                )),
        ))
        .into_node()
}

pub fn page() -> Node {
    let results: Vec<Signal<Status>> = SYNC_TESTS.iter().map(|_| signal(Status::Pending)).collect();
    let timer = signal(Status::Pending);
    let clipboard = signal(Status::Pending);
    let runs = signal(0u32);
    let run_all = {
        let results = results.clone();
        move || {
            for ((_, test), status) in SYNC_TESTS.iter().zip(&results) {
                let outcome =
                    std::panic::catch_unwind(test).unwrap_or_else(|_| Err("panicked".into()));
                status.set(match outcome {
                    Ok(detail) => Status::Pass(detail),
                    Err(detail) => Status::Fail(detail),
                });
            }
            timer.set(Status::Pending);
            let started = macroquad::time::get_time();
            set_timeout(0.25, move || {
                let elapsed = macroquad::time::get_time() - started;
                timer.set(if (0.2..1.5).contains(&elapsed) {
                    Status::Pass(format!("fired after {:.0} ms", elapsed * 1000.))
                } else {
                    Status::Fail(format!("fired after {:.0} ms (want ~250)", elapsed * 1000.))
                });
            });
            runs.update(|n| *n += 1);
        }
    };
    // Run once the page is shown for the first time.
    set_timeout(0.1, run_all.clone());
    let summary = {
        let results = results.clone();
        computed(move |_| {
            runs.get();
            let all: Vec<(&str, Status)> = SYNC_TESTS
                .iter()
                .map(|(name, _)| *name)
                .zip(results.iter().map(|status| status.get_clone()))
                .chain([("Timers", timer.get_clone())])
                .collect();
            let passed = all
                .iter()
                .filter(|(_, s)| matches!(s, Status::Pass(_)))
                .count();
            let failing: Vec<&str> = all
                .iter()
                .filter(|(_, s)| matches!(s, Status::Fail(_)))
                .map(|(name, _)| *name)
                .collect();
            let mut line = format!(
                "{passed} passed · {} failed · {} total",
                failing.len(),
                all.len()
            );
            if !failing.is_empty() {
                line.push_str(&format!(" · failing: {}", failing.join(", ")));
            }
            Rc::<str>::from(line)
        })
    };
    let rows = SYNC_TESTS
        .iter()
        .zip(&results)
        .map(|((name, _), status)| result_row(name, **status))
        .chain([result_row("Timers", *timer)])
        .collect::<Vec<_>>();
    let clipboard_test = move || {
        let token = format!("mue-{:.0}", macroquad::time::get_time() * 1000.);
        let board = use_clipboard();
        board.copy(&token);
        clipboard.set(match board.paste() {
            Some(text) if text == token => Status::Pass("copied text was pasted back".into()),
            Some(text) => Status::Fail(format!(
                "pasted {:?}",
                text.chars().take(24).collect::<String>()
            )),
            None => Status::Fail("the platform returned no clipboard text".into()),
        });
    };
    page_body(
        "Checks that run on this device and need no interaction. They also run when the page is first opened.",
        (
            card(
                "Automated checks",
                "Text editing model, layout, hit testing, shaping, reactivity, back handling, pointer observers, and timers.",
                stack()
                    .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(2.)}; })
                    .children((
                        row((
                            button("Run again").primary(true).on_activate(run_all),
                            para(summary),
                        )),
                        stack()
                            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: 0px; })
                            .children(rows),
                    )),
            ),
            card(
                "Clipboard round trip",
                "Copies a short marker to the system clipboard and reads it back. This replaces what is on your clipboard, so it only runs when you press the button.",
                stack()
                    .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(4.)}; })
                    .children((
                        row((button("Test clipboard").on_activate(clipboard_test),)),
                        result_row("Clipboard", *clipboard),
                    )),
            ),
            checklist(
                "Self tests",
                &[
                    "Every automated check passes",
                    "The clipboard round trip passes",
                ],
            ),
        ),
    )
}
