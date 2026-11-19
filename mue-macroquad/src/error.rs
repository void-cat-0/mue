use std::error::Error;

/// Error type accepted from application-owned callbacks.
pub type BoxedError = Box<dyn Error + 'static>;

/// Result type for Scene and transition callbacks.
pub type CallbackResult<T> = std::result::Result<T, BoxedError>;

/// An error returned by an application-owned Scene or transition callback.
#[derive(Debug, thiserror::Error)]
#[error("Scene {phase} failed: {source}")]
pub struct CallbackError {
    phase: &'static str,
    #[source]
    source: BoxedError,
}

impl CallbackError {
    pub(crate) fn new(phase: &'static str, source: BoxedError) -> Self {
        Self { phase, source }
    }

    /// The scheduling phase in which the callback failed.
    pub fn phase(&self) -> &'static str {
        self.phase
    }
}

/// Errors raised by Scene layout, root submission, or controlled drawing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SceneError {
    #[error("Scene tried to draw a root it did not submit for layout")]
    UndeclaredDrawRoot,
    #[error("Scene root drawn more than once in one frame")]
    DuplicateDrawRoot,
    #[error("Scene tried to draw a disposed root")]
    DisposedDrawRoot,
    #[error("invalid Scene root bounds")]
    InvalidRootBounds,
    #[error("Scene submitted a disposed root")]
    DisposedRoot,
    #[error("Scene roots overlap or contain a duplicate Node")]
    DuplicateNode,
    #[error("Scene must submit a tree root, not a Node attached to another layout")]
    AttachedRoot,
    #[error("Scene root geometry did not stabilize")]
    UnstableGeometry,
}

/// Errors raised while capturing or composing Scene transition pictures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransitionError {
    #[error("the viewport changed between live drawing and composition")]
    ChangedViewport,
    #[error("Scene pictures require an origin-zero logical window viewport")]
    NonzeroViewportOrigin,
    #[error("Scene pictures require finite positive dimensions and DPI")]
    InvalidPictureDimensions,
    #[error("Scene picture target dimensions overflow")]
    PictureDimensionsOverflow,
    #[error(
        "Scene picture transitions currently require OpenGL; Metal is unsupported (use an overlay)"
    )]
    UnsupportedBackend,
    #[error("{operation}: {source}")]
    Material {
        operation: &'static str,
        #[source]
        source: macroquad::Error,
    },
}

/// Aggregate error returned by fallible App frame operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    #[error(transparent)]
    Callback(#[from] CallbackError),
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error(transparent)]
    Transition(#[from] TransitionError),
    #[error("Scene is faulted; replace or reset it explicitly before continuing")]
    Faulted,
}

/// Result type returned by fallible App operations.
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn callback_error_preserves_phase_and_source() {
        let error = AppError::from(CallbackError::new(
            "update",
            Box::new(io::Error::other("callback failed")),
        ));
        assert_eq!(error.to_string(), "Scene update failed: callback failed");
        assert!(error.source().unwrap().is::<io::Error>());
        assert!(matches!(error, AppError::Callback(callback) if callback.phase() == "update"));
    }

    #[test]
    fn app_error_wraps_domain_errors() {
        let error: AppError = SceneError::DisposedRoot.into();
        assert!(matches!(error, AppError::Scene(SceneError::DisposedRoot)));
    }
}
