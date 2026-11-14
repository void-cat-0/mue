//! Probe macroquad 0.4 texture ownership models in mue.
//!
//! Compares:
//! - `SharedTexture` (`Rc<Texture2D>`, UI-thread-only);
//! - `mue_macroquad::SafeTexture` (`Arc` + main-thread deletion queue);
//! - raw `Texture2D` (managed slot, `Clone`, thread-affine `Drop`).
//!
//! Run with `DISPLAY=:0 cargo run -p texture-probe --offline`.
//! Exits 0 when all checks pass.

use std::{
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    thread,
};

use macroquad::prelude::*;
use mue_macroquad::{
    App, SafeTexture, SharedTexture, flush_pending_texture_deletions, node::Node,
    pending_texture_deletions,
};

struct Check {
    name: &'static str,
    pass: bool,
    detail: String,
}

fn check(name: &'static str, pass: bool, detail: impl Into<String>) -> Check {
    Check {
        name,
        pass,
        detail: detail.into(),
    }
}

fn texture() -> Texture2D {
    const SIZE: u32 = 4;
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    for (i, px) in pixels.chunks_exact_mut(4).enumerate() {
        let x = (i as u32) % SIZE;
        let y = (i as u32) / SIZE;
        px.copy_from_slice(&[
            if (x + y) % 2 == 0 { 255 } else { 40 },
            if x % 2 == 0 { 200 } else { 80 },
            if y % 2 == 0 { 120 } else { 220 },
            255,
        ]);
    }
    Texture2D::from_rgba8(SIZE as u16, SIZE as u16, &pixels)
}

fn assert_send_sync<T: Send + Sync>() {}

fn raw_id(tex: &Texture2D) -> String {
    format!("{:?}", tex.raw_miniquad_id())
}

async fn run() {
    println!("texture ownership probe (macroquad 0.4)");
    println!("window: {}x{}", screen_width(), screen_height());
    println!();

    let mut checks = Vec::new();

    // 1. Compile-time auto traits.
    assert_send_sync::<Texture2D>();
    assert_send_sync::<SafeTexture>();
    checks.push(check(
        "Texture2D / SafeTexture: Send + Sync",
        true,
        "compile-time assertion",
    ));

    // 2. SharedTexture main-thread clone/drop/equality.
    {
        let tex = texture();
        let a = SharedTexture::from(tex.clone());
        let b = SharedTexture::from(tex);
        let equal = a == b;
        let id = raw_id(&a);
        drop(a);
        drop(b);
        next_frame().await;
        checks.push(check(
            "SharedTexture main-thread clone/drop",
            equal,
            format!("raw id {id}, clones equal"),
        ));
    }

    // 3. Raw Texture2D last clone dropped on a background thread panics.
    {
        let tex = texture();
        let clone = tex.clone();
        drop(tex); // main thread: only `clone` remains
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked =
            thread::spawn(move || catch_unwind(AssertUnwindSafe(|| drop(clone))).is_err())
                .join()
                .unwrap();
        std::panic::set_hook(prev);
        checks.push(check(
            "Texture2D last clone drop on worker panics",
            panicked,
            "thread-affine TextureSlotGuarded::drop",
        ));
        next_frame().await;
    }

    // 4. SafeTexture last clone dropped on a background thread queues deletion.
    {
        let safe = SafeTexture::from(texture());
        let clone = safe.clone();
        drop(safe); // main thread: only `clone` remains
        let ok = thread::spawn(move || drop(clone)).join().is_ok();
        let queued = pending_texture_deletions();
        checks.push(check(
            "SafeTexture worker drop queues, does not panic",
            ok && queued > 0,
            format!("queued {queued}"),
        ));
        let flushed = flush_pending_texture_deletions();
        next_frame().await;
        checks.push(check(
            "SafeTexture queue flush on main thread",
            flushed > 0 && pending_texture_deletions() == 0,
            format!("flushed {flushed}"),
        ));
    }

    // 5. SafeTexture -> SharedTexture shares the managed slot.
    {
        let safe = SafeTexture::from(texture());
        let shared = safe.to_shared();
        let id = raw_id(&shared);
        drop(safe);
        flush_pending_texture_deletions();
        next_frame().await;
        let still_valid = raw_id(&shared) == id;
        draw_texture(&shared, 0., 0., WHITE);
        drop(shared);
        next_frame().await;
        checks.push(check(
            "SafeTexture -> SharedTexture shares slot",
            still_valid,
            format!("raw id {id} survives SafeTexture flush"),
        ));
    }

    // 5b. SharedTexture -> SafeTexture shares the managed slot.
    {
        let shared = SharedTexture::from(texture());
        let id = raw_id(&shared);
        let safe = shared.to_safe();
        drop(shared);
        next_frame().await;
        let still_valid = raw_id(&safe) == id;
        drop(safe);
        flush_pending_texture_deletions();
        next_frame().await;
        checks.push(check(
            "SharedTexture -> SafeTexture shares slot",
            still_valid,
            format!("raw id {id} survives SharedTexture drop"),
        ));
    }

    // 6. try_into_inner: unique succeeds.
    {
        let safe = SafeTexture::from(texture());
        let inner = safe.try_into_inner();
        let ok = inner.is_ok();
        if let Ok(tex) = inner {
            drop(tex);
        }
        next_frame().await;
        checks.push(check(
            "SafeTexture::try_into_inner unique",
            ok,
            "returns Texture2D",
        ));
    }

    // 7. try_into_inner: shared fails and returns the handle.
    {
        let safe = SafeTexture::from(texture());
        let clone = safe.clone();
        let result = safe.try_into_inner();
        let rejected = result.is_err();
        drop(clone);
        if let Err(safe) = result {
            drop(safe);
        }
        flush_pending_texture_deletions();
        next_frame().await;
        checks.push(check(
            "SafeTexture::try_into_inner shared",
            rejected,
            "returns Err(Self) while another clone exists",
        ));
    }

    // 8. into_shared: unique succeeds, shared rejects.
    {
        let safe = SafeTexture::from(texture());
        let unique = safe.into_shared();
        let unique_ok = unique.is_ok();
        if let Ok(shared) = unique {
            draw_texture(&shared, 4., 4., WHITE);
            drop(shared);
        }
        next_frame().await;

        let safe = SafeTexture::from(texture());
        let clone = safe.clone();
        let shared = safe.into_shared();
        let rejected = shared.is_err();
        drop(clone);
        if let Err(safe) = shared {
            drop(safe);
        }
        flush_pending_texture_deletions();
        next_frame().await;
        checks.push(check(
            "SafeTexture::into_shared unique/shared",
            unique_ok && rejected,
            "unique transfers, shared returns Err(Self)",
        ));
    }

    // 9. Raw Texture2D main-thread clone/drop.
    {
        let tex = texture();
        let id = raw_id(&tex);
        let clone = tex.clone();
        drop(clone);
        let still_valid = raw_id(&tex) == id;
        drop(tex);
        next_frame().await;
        checks.push(check(
            "Texture2D main-thread clone/drop",
            still_valid,
            format!("raw id {id}"),
        ));
    }

    // 9. SharedTexture equality by raw id; distinct textures differ.
    {
        let a = SharedTexture::from(texture());
        let b = SharedTexture::from(texture());
        let distinct = a != b;
        drop(a);
        drop(b);
        next_frame().await;
        checks.push(check(
            "SharedTexture distinct textures differ",
            distinct,
            "raw-id equality",
        ));
    }

    // 10. SafeTexture equality is Arc identity.
    {
        let safe = SafeTexture::from(texture());
        let clone = safe.clone();
        let same = safe == clone;
        let other = SafeTexture::from(texture());
        let distinct = safe != other;
        drop(clone);
        drop(other);
        drop(safe);
        flush_pending_texture_deletions();
        next_frame().await;
        checks.push(check(
            "SafeTexture equality is Arc identity",
            same && distinct,
            "clone equal, separate wrapper distinct",
        ));
    }

    // 11. App::frame flushes the queue automatically.
    {
        let mut app = App::new(Node::empty());
        let safe = SafeTexture::from(texture());
        let clone = safe.clone();
        drop(safe);
        thread::spawn(move || drop(clone)).join().unwrap();
        let queued = pending_texture_deletions();
        app.frame();
        let drained = pending_texture_deletions() == 0;
        next_frame().await;
        checks.push(check(
            "App::frame flushes SafeTexture queue",
            queued > 0 && drained,
            format!("queued {queued}, drained {drained}"),
        ));
    }

    // 12. Queue is empty at the end.
    checks.push(check(
        "deletion queue drained",
        pending_texture_deletions() == 0,
        format!("pending {}", pending_texture_deletions()),
    ));

    println!("{:<48} {:<6} detail", "check", "result");
    for c in &checks {
        println!(
            "{:<48} {:<6} {}",
            c.name,
            if c.pass { "PASS" } else { "FAIL" },
            c.detail
        );
    }
    let passed = checks.iter().filter(|c| c.pass).count();
    println!();
    println!("summary: {passed}/{} PASS", checks.len());
    let _ = std::io::stdout().flush();
    std::process::exit(if passed == checks.len() { 0 } else { 1 });
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - texture ownership probe".into(),
            window_width: 320,
            window_height: 240,
            ..Default::default()
        },
        run(),
    );
}
