//! Gameplay state and the business meaning of a result returned by ResultsScene.

// App transports this value; only GameScene decides what it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameResult {
    Return,
    Retry,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct GameModel {
    pub elapsed: f64,
    pub user_paused: bool,
}

impl GameModel {
    pub fn toggle_pause(&mut self) {
        self.user_paused = !self.user_paused;
    }

    pub fn resume(&mut self) {
        self.user_paused = false;
    }

    pub fn advance(&mut self, dt: f32) {
        if !self.user_paused {
            self.elapsed += f64::from(dt);
        }
    }

    pub fn apply_result(&mut self, result: GameResult) {
        match result {
            GameResult::Return => {} // Keep elapsed time and the user's pause choice.
            GameResult::Retry => {
                self.elapsed = 0.;
                self.user_paused = false;
            }
        }
    }
}
