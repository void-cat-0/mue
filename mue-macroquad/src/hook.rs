use crate::event::{pointer::PointerEvent, wheel::WheelEvent};

type CallbackFunction<T> = Option<Box<dyn FnMut(&T, &dyn Fn() -> bool) + 'static>>;

/// A growable chain of callbacks invoked in registration order.
///
/// The chained callbacks are stored as one boxed closure rather than a vector, so
/// `append` / `extend` compose instead of pushing: appending to a non-empty list
/// re-wraps the existing callback.
pub struct CallbackList<T> {
    callback: CallbackFunction<T>,
}

impl<T> Default for CallbackList<T> {
    fn default() -> Self {
        Self { callback: None }
    }
}

impl<T: 'static> CallbackList<T> {
    pub fn is_empty(&self) -> bool {
        self.callback.is_none()
    }

    pub fn append(&mut self, mut callback: impl FnMut(&T) + 'static) {
        match self.callback.take() {
            Some(mut existing) => {
                self.callback = Some(Box::new(move |arg, is_live| {
                    existing(arg, is_live);
                    if is_live() {
                        callback(arg);
                    }
                }));
            }
            _ => {
                self.callback = Some(Box::new(move |arg, is_live| {
                    if is_live() {
                        callback(arg);
                    }
                }));
            }
        }
    }

    pub fn extend(&mut self, other: Self) {
        if let Some(mut other_callback) = other.callback {
            match self.callback.take() {
                Some(mut existing) => {
                    self.callback = Some(Box::new(move |arg, is_live| {
                        existing(arg, is_live);
                        if is_live() {
                            other_callback(arg, is_live);
                        }
                    }));
                }
                None => self.callback = Some(other_callback),
            }
        }
    }

    pub fn invoke(&mut self, arg: &T) {
        self.invoke_while(arg, &|| true);
    }

    pub(crate) fn invoke_while(&mut self, arg: &T, is_live: &dyn Fn() -> bool) {
        if let Some(callback) = &mut self.callback {
            callback(arg, is_live);
        }
    }
}

#[derive(Default)]
pub(crate) struct NodeHooks {
    pub update: CallbackList<f32>,
    pub layout: CallbackList<()>,
    pub render: CallbackList<()>,
    pub after_children: CallbackList<()>,
    pub pointer_event: CallbackList<PointerEvent>,
    pub hover_event: CallbackList<PointerEvent>,
    pub wheel_event: CallbackList<WheelEvent>,
}
