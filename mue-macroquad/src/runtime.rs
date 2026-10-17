use std::{
    cell::{OnceCell, Ref, RefCell},
    collections::BTreeMap,
    mem,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    thread::AccessError,
};

use ordered_float::NotNan;
use slotmap::SlotMap;
use taffy::TaffyTree;

use crate::{
    font::{FontsAlreadyInitialized, fontdb},
    gesture::{Gesture, GestureId},
    hook::{HookFn, NodeHooks},
    layout::MeasureFn,
    node::{NodeId, NodeInner, text::FontState},
};

thread_local! {
    static RUNTIME: Runtime = Runtime::new();
}

pub(crate) struct TimeoutKey {
    pub time: NotNan<f32>,
    pub aborted: Rc<AtomicBool>,
}

impl PartialEq for TimeoutKey {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && Rc::ptr_eq(&self.aborted, &other.aborted)
    }
}
impl Eq for TimeoutKey {}

impl PartialOrd for TimeoutKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimeoutKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time
            .cmp(&other.time)
            .then_with(|| Rc::as_ptr(&self.aborted).cmp(&Rc::as_ptr(&other.aborted)))
    }
}

#[repr(transparent)]
pub struct TimeoutHandle {
    aborted: Rc<AtomicBool>,
}

impl TimeoutHandle {
    pub fn cancel(&self) {
        self.aborted.store(true, Ordering::Relaxed);
    }
}

pub(crate) struct Runtime {
    pub taffy: RefCell<TaffyTree<Box<dyn MeasureFn>>>,
    pub nodes: RefCell<SlotMap<NodeId, NodeInner>>,

    fonts: OnceCell<RefCell<FontState>>,

    pub gestures: RefCell<SlotMap<GestureId, Box<dyn Gesture>>>,

    pub timeouts: RefCell<BTreeMap<TimeoutKey, Box<dyn FnOnce()>>>,
}

impl Runtime {
    fn new() -> Self {
        let mut taffy = TaffyTree::new();
        // Preserve fractional positions and widths during layout animation.
        taffy.disable_rounding();
        Self {
            taffy: RefCell::new(taffy),
            nodes: RefCell::new(SlotMap::with_key()),

            fonts: OnceCell::new(),

            gestures: RefCell::new(SlotMap::with_key()),

            timeouts: RefCell::new(BTreeMap::new()),
        }
    }

    pub fn with<R>(f: impl FnOnce(&Runtime) -> R) -> R {
        RUNTIME.with(f)
    }
    pub fn try_with<R>(f: impl FnOnce(&Runtime) -> R) -> Result<R, AccessError> {
        RUNTIME.try_with(f)
    }

    pub fn with_taffy_mut<R>(f: impl FnOnce(&mut TaffyTree<Box<dyn MeasureFn>>) -> R) -> R {
        RUNTIME.with(|rt| f(&mut rt.taffy.borrow_mut()))
    }
    pub fn with_fonts_mut<R>(f: impl FnOnce(&mut FontState) -> R) -> R {
        RUNTIME.with(|rt| {
            let fonts = rt.fonts.get_or_init(|| RefCell::new(FontState::default()));
            f(&mut fonts.borrow_mut())
        })
    }

    pub(crate) fn begin_text_render_pass() {
        RUNTIME.with(|rt| {
            if let Some(fonts) = rt.fonts.get() {
                fonts.borrow_mut().begin_render_pass();
            }
        });
    }

    pub(crate) fn initialize_fonts(
        locale: String,
        db: fontdb::Database,
    ) -> Result<(), FontsAlreadyInitialized> {
        RUNTIME.with(|rt| {
            if rt.fonts.get().is_some() {
                return Err(FontsAlreadyInitialized);
            }
            rt.fonts
                .set(RefCell::new(FontState::with_locale_and_db(locale, db)))
                .map_err(|_| FontsAlreadyInitialized)
        })
    }

    pub fn invoke_hook<T: 'static>(
        &self,
        node_id: NodeId,
        hook_fn: impl Fn(&mut NodeHooks) -> &mut HookFn<T>,
        arg: &T,
    ) {
        let mut hook = {
            let mut nodes = self.nodes.borrow_mut();
            let Some(node) = nodes.get_mut(node_id) else {
                return;
            };
            mem::take(hook_fn(&mut node.hooks))
        };
        hook.invoke(arg);
        if let Some(node) = self.nodes.borrow_mut().get_mut(node_id) {
            *hook_fn(&mut node.hooks) = hook;
        }
    }

    pub fn node(&self, id: NodeId) -> Ref<'_, NodeInner> {
        Ref::map(self.nodes.borrow(), |arena| &arena[id])
    }
}

pub fn get_time() -> f32 {
    macroquad::time::get_time() as f32
}

pub fn set_timeout(delay: f32, callback: impl FnOnce() + 'static) -> TimeoutHandle {
    Runtime::with(|rt| {
        let key = TimeoutKey {
            time: NotNan::new(get_time() + delay).unwrap(),
            aborted: Rc::default(),
        };
        let handle = TimeoutHandle {
            aborted: key.aborted.clone(),
        };
        rt.timeouts.borrow_mut().insert(key, Box::new(callback));
        handle
    })
}
