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

#[cfg(test)]
mod tests {
    use super::{GameModel, GameResult};

    #[test]
    fn return_preserves_running_and_user_paused_games() {
        for user_paused in [false, true] {
            let mut game = GameModel {
                elapsed: 12.5,
                user_paused,
            };
            game.apply_result(GameResult::Return);
            assert_eq!(
                game,
                GameModel {
                    elapsed: 12.5,
                    user_paused,
                }
            );
            game.advance(0.25);
            assert_eq!(game.elapsed, if user_paused { 12.5 } else { 12.75 });
        }
    }

    #[test]
    fn retry_restarts_the_timer_and_clears_user_pause() {
        let mut game = GameModel {
            elapsed: 12.5,
            user_paused: true,
        };
        game.apply_result(GameResult::Retry);
        assert_eq!(game, GameModel::default());
        game.advance(0.25);
        assert_eq!(game.elapsed, 0.25);
    }

    #[test]
    fn user_pause_freezes_time_until_resume() {
        let mut game = GameModel::default();
        game.advance(0.5);
        game.toggle_pause();
        game.advance(1.);
        assert_eq!(game.elapsed, 0.5);
        game.resume();
        game.advance(0.25);
        assert_eq!(game.elapsed, 0.75);
    }
}
