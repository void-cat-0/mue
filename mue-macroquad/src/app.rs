use std::{mem, rc::Rc, sync::atomic::Ordering};

use crate::animation;
use glam::vec2;
use mue_core::batch;
use ordered_float::NotNan;
use taffy::{AvailableSpace, Size};

use crate::{
    event::pointer::PointerManager,
    node::{IntoNode, Node},
    runtime::get_time,
    runtime::{Runtime, TimeoutKey},
};

/// Owns the root view and drives animation, layout, input, and rendering.
pub struct App {
    root_node: Node,
    layout_id: Option<taffy::NodeId>,

    pointer_manager: PointerManager,
}

impl App {
    /// Build and retain a root view.
    pub fn new(root_node: impl IntoNode) -> Self {
        crate::viewport::set(crate::viewport::Viewport {
            width: macroquad::window::screen_width(),
            height: macroquad::window::screen_height(),
        });
        let root_node = root_node.into_node();
        root_node.resolve_font_sizes();
        let layout_id = Runtime::with(|rt| rt.node(root_node.id).layout_id);

        Self {
            root_node,
            layout_id,

            pointer_manager: PointerManager::new(),
        }
    }

    /// Advance animations and render one frame, then dispatch pointer events.
    /// Call once per frame before `macroquad::next_frame`.
    pub fn frame(&mut self) {
        let root_node = *self.root_node;
        let viewport = crate::viewport::Viewport {
            width: macroquad::window::screen_width(),
            height: macroquad::window::screen_height(),
        };
        batch(|| {
            crate::viewport::set(viewport);
            animation::advance(macroquad::time::get_frame_time());
        });
        batch(|| {
            let time = get_time();
            Runtime::with(|rt| {
                let key = TimeoutKey {
                    time: NotNan::new(time.next_up()).unwrap(),
                    aborted: Rc::default(),
                };
                #[allow(clippy::mutable_key_type)]
                let due_timeouts = {
                    let mut timeouts = rt.timeouts.borrow_mut();
                    let mut due = timeouts.split_off(&key);
                    mem::swap(&mut *timeouts, &mut due);
                    due
                };
                for (key, callback) in due_timeouts {
                    if key.aborted.load(Ordering::Relaxed) {
                        continue;
                    }
                    callback();
                }
            });
        });
        self.root_node.resolve_font_sizes();
        for pass in 0..64 {
            let mut changed = false;
            batch(|| {
                if let Some(layout_id) = self.layout_id {
                    Runtime::with_taffy_mut(|taffy| {
                        taffy
                            .compute_layout_with_measure(
                                layout_id,
                                Size {
                                    width: AvailableSpace::Definite(viewport.width),
                                    height: AvailableSpace::Definite(viewport.height),
                                },
                                |known_dimensions,
                                 available_space,
                                 _node_id,
                                 node_context,
                                 _style| {
                                    node_context.map_or(Size::ZERO, |f| {
                                        f.measure(known_dimensions, available_space)
                                    })
                                },
                            )
                            .unwrap();
                    });
                }
                changed = self.root_node.sync_layout();
            });
            if !changed {
                break;
            }
            self.root_node.resolve_font_sizes();
            assert!(
                pass < 63,
                "layout dependencies did not stabilize before rendering"
            );
        }
        Runtime::begin_text_render_pass();
        batch(|| {
            self.root_node.render(vec2(0., 0.));
            self.pointer_manager.process(root_node);
            crate::event::wheel::check_wheel(root_node);
        });
    }
}
