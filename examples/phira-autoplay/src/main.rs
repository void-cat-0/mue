//! Desktop entry point for the Phira autoplay example.
#[cfg(not(target_os = "android"))]
#[path = "lib.rs"]
mod demo;

#[cfg(not(target_os = "android"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{cell::Cell, rc::Rc};

    let Some(options) = demo::options()? else {
        return Ok(());
    };
    let exit = Rc::new(Cell::new(None));
    let output = exit.clone();
    macroquad::Window::from_config(demo::window_config(), async move {
        output.set(Some(demo::run(options).await));
    });
    exit.take().unwrap_or(Ok(()))
}

// Android launches the cdylib example through Java, never a Rust executable.
#[cfg(target_os = "android")]
fn main() {}
