//! Each Scene owns its UI, detached scope, and local state.
mod game;
mod results;

pub(crate) use game::GameScene;
use results::ResultsScene;
