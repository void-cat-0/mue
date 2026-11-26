//! Optional screenshot capture for visual checks: set `MUE_CAPTURE=out.png`
//! to save the third frame and exit.

pub struct Capture {
    path: Option<String>,
    frame: u32,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            path: std::env::var("MUE_CAPTURE").ok(),
            frame: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// Call after drawing, before `next_frame`. Returns true when the app should exit.
    pub fn after_draw(&mut self) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        self.frame += 1;
        if self.frame < 3 {
            return false;
        }
        let image = macroquad::texture::get_screen_data();
        image.export_png(path);
        true
    }
}
