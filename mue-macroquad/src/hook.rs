use crate::event::{pointer::PointerEvent, wheel::WheelEvent};

type CallbackFunction<T> = Option<Box<dyn FnMut(&T) + 'static>>;

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
                self.callback = Some(Box::new(move |arg| {
                    existing(arg);
                    callback(arg);
                }));
            }
            _ => {
                self.callback = Some(Box::new(callback));
            }
        }
    }

    pub fn extend(&mut self, other: Self) {
        if let Some(other_callback) = other.callback {
            self.append(other_callback);
        }
    }

    pub fn invoke(&mut self, arg: &T) {
        if let Some(callback) = &mut self.callback {
            callback(arg);
        }
    }
}

#[derive(Default)]
pub(crate) struct NodeHooks {
    pub render: CallbackList<()>,
    pub after_children: CallbackList<()>,
    pub pointer_event: CallbackList<PointerEvent>,
    pub hover_event: CallbackList<PointerEvent>,
    pub wheel_event: CallbackList<WheelEvent>,
}
