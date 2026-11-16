//! Each Scene owns its palette, return effect, animation and pending input.
use crate::{
    effects::Effect,
    input::{self, Action},
};
use anyhow::Result;
use macroquad::prelude::*;
use mue_macroquad::{Scene, SceneContext, SceneEvent, SceneRender};

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
    action: Action,
}

impl ColorScene {
    pub(crate) fn root() -> Self {
        Self {
            palette: Palette::Blue,
            back_effect: None,
            active_time: 0.,
            action: Action::None,
        }
    }

    fn child(&self, effect: Effect) -> Self {
        Self {
            palette: self.palette.next(),
            back_effect: Some(effect),
            active_time: 0.,
            action: Action::None,
        }
    }
}

impl Scene for ColorScene {
    fn event(&mut self, event: SceneEvent<'_>, _: &mut SceneContext<'_>) -> Result<()> {
        if let SceneEvent::Input(batch) = event {
            self.action = input::action(batch);
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        // App advances only the top Scene with the normal host frame delta.
        self.active_time += ctx.frame.dt;
        match std::mem::take(&mut self.action) {
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

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_child_starts_with_independent_scene_state() {
        let mut parent = ColorScene::root();
        parent.active_time = 2.;
        parent.action = Action::Quit;

        let mut child = parent.child(Effect::Slide);
        let sibling = parent.child(Effect::Cover);
        assert_eq!(child.palette, Palette::Amber);
        assert_eq!(child.back_effect, Some(Effect::Slide));
        assert_eq!(child.active_time, 0.);
        assert_eq!(child.action, Action::None);
        assert_eq!(sibling.back_effect, Some(Effect::Cover));

        child.active_time = 3.;
        child.action = Action::Back;
        assert_eq!(child.active_time, 3.);
        assert_eq!(child.action, Action::Back);
        assert_eq!(parent.active_time, 2.);
        assert_eq!(parent.action, Action::Quit);
        assert_eq!(sibling.active_time, 0.);
        assert_eq!(sibling.action, Action::None);
    }

    #[test]
    fn nested_children_cycle_palettes_without_becoming_roots() {
        let root = ColorScene::root();
        let child = root.child(Effect::Fade);
        let grandchild = child.child(Effect::Slide);
        let next = grandchild.child(Effect::Cover);
        assert_eq!(grandchild.palette, Palette::Green);
        assert_eq!(next.palette, root.palette);
        assert_eq!(root.back_effect, None);
        assert_eq!(next.back_effect, Some(Effect::Cover));
    }
}
