//! Each Scene owns its palette, return effect and animation.
use crate::{
    effects::Effect,
    input::{self, Action},
};
use macroquad::prelude::*;
use mue_macroquad::{CallbackResult, Scene, SceneContext, SceneEvent, SceneRender};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Palette {
    Blue,
    Amber,
    Green,
}

impl Palette {
    fn next(self) -> Self {
        match self {
            Self::Blue => Self::Amber,
            Self::Amber => Self::Green,
            Self::Green => Self::Blue,
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Blue => Color::from_rgba(35, 95, 170, 255),
            Self::Amber => Color::from_rgba(170, 115, 35, 255),
            Self::Green => Color::from_rgba(35, 130, 100, 255),
        }
    }
}

pub(crate) struct ColorScene {
    palette: Palette,
    // No return effect means this instance is the root.
    back_effect: Option<Effect>,
    active_time: f32,
}

impl ColorScene {
    pub(crate) fn root() -> Self {
        Self {
            palette: Palette::Blue,
            back_effect: None,
            active_time: 0.,
        }
    }

    fn child(&self, effect: Effect) -> Self {
        Self {
            palette: self.palette.next(),
            back_effect: Some(effect),
            active_time: 0.,
        }
    }
}

impl Scene for ColorScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        let SceneEvent::Input(batch) = event else {
            return Ok(());
        };
        // This may be the outgoing Scene's last Input, with no update before commit.
        match input::action(batch) {
            Action::Open(effect) => {
                ctx.push(self.child(effect)).transition(effect.transition());
            }
            Action::Back => {
                if let Some(effect) = self.back_effect {
                    ctx.pop().transition(effect.transition());
                } else {
                    ctx.pop().without_transition();
                }
            }
            Action::Quit => {
                ctx.exit().without_transition();
            }
            Action::None => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.active_time += ctx.frame.dt;
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        // Outgoing picture captures use this same draw-only path.
        let v = render.frame.viewport;
        draw_rectangle(v.x, v.y, v.w, v.h, self.palette.color());
        let title = if self.back_effect.is_none() {
            "Scene transitions - root"
        } else {
            "Scene transitions - child"
        };
        draw_text(title, v.x + 24., v.y + 44., 30., WHITE);
        draw_text(
            "1: Fade   2: Slide   3: Cover",
            v.x + 24.,
            v.y + 80.,
            22.,
            WHITE,
        );
        draw_text("Esc: Back   Q: Quit", v.x + 24., v.y + 108., 22., WHITE);

        let track_width = (v.w - 64.).max(0.);
        let track_y = v.y + v.h * 0.65;
        let progress = 0.5 + self.active_time.sin() * 0.5;
        draw_rectangle(
            v.x + 32.,
            track_y - 2.,
            track_width,
            4.,
            Color::new(1., 1., 1., 0.35),
        );
        draw_circle(v.x + 32. + track_width * progress, track_y, 10., WHITE);
        draw_text(
            "Scene-local animation pauses while this Scene is covered.",
            v.x + 24.,
            v.y + v.h - 24.,
            18.,
            WHITE,
        );
        Ok(())
    }
}
