use mue_core::{
    scope::Scope,
    signal::{Access, ReadSignal, Signal},
};

/// The logical viewport used by viewport-relative style units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

thread_local! {
    static VIEWPORT: Signal<Viewport> = Scope::null().run(|| {
        Signal::new(Viewport {
            width: 0.0,
            height: 0.0,
        })
    });
}

pub(crate) fn read() -> ReadSignal<Viewport> {
    VIEWPORT.with(|viewport| **viewport)
}

pub(crate) fn set(viewport: Viewport) {
    VIEWPORT.with(|current| {
        if current.get_untracked() != viewport {
            current.set(viewport);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_state_is_unowned_and_updates() {
        set(Viewport {
            width: 800.0,
            height: 600.0,
        });
        assert_eq!(
            read().get(),
            Viewport {
                width: 800.0,
                height: 600.0
            }
        );
    }
}
