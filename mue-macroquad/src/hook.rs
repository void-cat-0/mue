use crate::event::{pointer::PointerEvent, wheel::WheelEvent};

pub struct HookFn<T> {
    callback: Option<Box<dyn FnMut(&T)>>,
}

impl<T> Default for HookFn<T> {
    fn default() -> Self {
        Self { callback: None }
    }
}

impl<T: 'static> HookFn<T> {
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
    pub render: HookFn<()>,
    pub pointer_event: HookFn<PointerEvent>,
    pub hover_event: HookFn<PointerEvent>,
    pub wheel_event: HookFn<WheelEvent>,
}
