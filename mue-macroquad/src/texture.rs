//! Texture handles with different threading contracts.
//!
//! [`SharedTexture`] is the UI-thread handle (`Rc<Texture2D>`); it is cheap to
//! clone and intentionally `!Send`, so the compiler prevents accidental
//! cross-thread use.
//!
//! [`SafeTexture`] is the `Send + Sync` counterpart. It owns a `Texture2D`
//! through an `Arc` and defers dropping the GPU handle until
//! [`flush_pending_texture_deletions`] runs on the render thread. Use it when a
//! texture may be dropped on a worker thread (for example, a task result that
//! outlives the UI that requested it).
//!
//! Macroquad 0.4's `Texture2D` is `Clone` (not `Copy`) and its managed-slot
//! `Drop` is thread-affine: dropping the last clone on a worker thread panics.
//! `SafeTexture` moves the `Texture2D` into a queue instead, so the actual
//! `Texture2D::drop` runs on the render thread.
//!
//! `App::frame` flushes the queue automatically. Hosts that do not use `App`
//! must call [`flush_pending_texture_deletions`] on the render thread after
//! their draw calls.

use std::{
    fmt,
    ops::Deref,
    sync::{Arc, Mutex},
};

use macroquad::texture::Texture2D;
use mue_core::prop::PropValue;

use crate::shader::SharedTexture;

static PENDING_DELETIONS: Mutex<Vec<Texture2D>> = Mutex::new(Vec::new());

struct SafeTextureInner(Option<Texture2D>);

impl Drop for SafeTextureInner {
    fn drop(&mut self) {
        if let Some(texture) = self.0.take() {
            PENDING_DELETIONS.lock().unwrap().push(texture);
        }
    }
}

/// A `Send + Sync` texture handle whose deletion is deferred to the render thread.
///
/// The handle may be cloned and dropped on any thread. The underlying
/// `Texture2D` is only dropped by [`flush_pending_texture_deletions`] on the
/// render thread; dereferencing the handle for GL calls is still a
/// render-thread operation.
#[derive(Clone)]
pub struct SafeTexture(Arc<SafeTextureInner>);

impl SafeTexture {
    /// Wrap a texture created on the render thread.
    pub fn new(texture: Texture2D) -> Self {
        Self(Arc::new(SafeTextureInner(Some(texture))))
    }

    /// Consume the handle if this is the only owner. If other clones exist,
    /// return the handle unchanged.
    ///
    /// The returned `Texture2D` must be dropped on the render thread.
    pub fn try_into_inner(self) -> Result<Texture2D, Self> {
        match Arc::try_unwrap(self.0) {
            Ok(inner) => {
                let mut inner = std::mem::ManuallyDrop::new(inner);
                let texture = inner.0.take().expect("SafeTexture already consumed");
                Ok(texture)
            }
            Err(arc) => Err(Self(arc)),
        }
    }

    /// Clone the underlying `Texture2D` into a UI-thread [`SharedTexture`].
    /// Both handles share macroquad's managed texture slot.
    ///
    /// Call this on the render thread.
    pub fn to_shared(&self) -> SharedTexture {
        SharedTexture::from((**self).clone())
    }

    /// Transfer ownership into a [`SharedTexture`] if this is the only owner.
    ///
    /// Call this on the render thread.
    pub fn into_shared(self) -> Result<SharedTexture, Self> {
        self.try_into_inner().map(SharedTexture::from)
    }
}

impl Deref for SafeTexture {
    type Target = Texture2D;

    fn deref(&self) -> &Self::Target {
        self.0.0.as_ref().expect("SafeTexture already consumed")
    }
}

impl PartialEq for SafeTexture {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SafeTexture {}

impl fmt::Debug for SafeTexture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Do not read the raw miniquad id here: that touches macroquad's
        // thread-local context, and `SafeTexture` is `Send + Sync`.
        f.debug_struct("SafeTexture")
            .field("strong_count", &Arc::strong_count(&self.0))
            .finish_non_exhaustive()
    }
}

impl From<Texture2D> for SafeTexture {
    fn from(value: Texture2D) -> Self {
        Self::new(value)
    }
}

impl From<&SharedTexture> for SafeTexture {
    fn from(value: &SharedTexture) -> Self {
        Self::new((**value).clone())
    }
}

impl From<SharedTexture> for SafeTexture {
    fn from(value: SharedTexture) -> Self {
        Self::new((*value).clone())
    }
}

impl PropValue for SafeTexture {}

/// Drop textures queued by [`SafeTexture`] on the render thread.
///
/// Call this after the frame's draw calls, on the thread that owns macroquad's
/// GL context. `App::frame` already calls it.
pub fn flush_pending_texture_deletions() -> usize {
    let textures = std::mem::take(&mut *PENDING_DELETIONS.lock().unwrap());
    let count = textures.len();
    drop(textures);
    count
}
