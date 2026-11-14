//! Mock session state and the UI command boundary.
//!
//! The example uses this to validate the §7 contract: the UI reads a snapshot
//! and sends commands; the session applies commands at the frame boundary and
//! publishes a new snapshot after both chart and UI consume the input batch.
//! The UI displays it next frame. This is a mock, not the final Phira session API.

use std::cell::RefCell;
use std::rc::Rc;

/// The subset of game state the pause overlay needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameUiSnapshot {
    pub paused: bool,
    pub dead: bool,
    pub time: f64,
    pub duration: f64,
}

/// Commands the pause overlay can send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameUiCommand {
    Resume,
    Retry,
    Exit,
}

/// A queue the UI writes to and the host drains at the frame boundary.
pub type CommandQueue = Rc<RefCell<Vec<GameUiCommand>>>;

pub fn command_queue() -> CommandQueue {
    Rc::new(RefCell::new(Vec::new()))
}

pub fn drain(queue: &CommandQueue) -> Vec<GameUiCommand> {
    queue.borrow_mut().drain(..).collect()
}

/// Mock chart session.
///
/// `update` advances chart time only while playing; death and exit are
/// simulated so the overlay's dead/exit branches can be exercised.
pub struct MockSession {
    time: f64,
    duration: f64,
    paused: bool,
    dead: bool,
    exited: bool,
}

impl MockSession {
    pub fn new(duration: f64) -> Self {
        Self {
            time: 0.,
            duration,
            paused: false,
            dead: false,
            exited: false,
        }
    }

    /// Apply one UI command. Commands are applied before the chart update.
    pub fn apply(&mut self, command: GameUiCommand) {
        match command {
            GameUiCommand::Resume => {
                if !self.dead && !self.exited {
                    self.paused = false;
                }
            }
            GameUiCommand::Retry => {
                if !self.exited {
                    self.time = 0.;
                    self.dead = false;
                    self.paused = false;
                }
            }
            GameUiCommand::Exit => {
                self.exited = true;
                self.paused = true;
            }
        }
    }

    /// Pause from the host or the chart HUD. A dead session stays paused.
    pub fn pause(&mut self) {
        if !self.exited {
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        self.apply(GameUiCommand::Resume);
    }

    /// Advance chart time. Paused, dead, and exited sessions do not advance.
    pub fn update(&mut self, dt: f32) {
        if self.paused || self.dead || self.exited {
            return;
        }
        self.time += dt as f64;
        if self.time >= self.duration {
            self.time = self.duration;
            self.dead = true;
            self.paused = true;
        }
    }

    pub fn snapshot(&self) -> GameUiSnapshot {
        GameUiSnapshot {
            paused: self.paused,
            dead: self.dead,
            time: self.time,
            duration: self.duration,
        }
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_dead(&self) -> bool {
        self.dead
    }

    pub fn exited(&self) -> bool {
        self.exited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_session_does_not_advance() {
        let mut session = MockSession::new(10.);
        session.pause();
        session.update(0.5);
        assert_eq!(session.snapshot().time, 0.);
        assert!(session.snapshot().paused);
    }

    #[test]
    fn resume_applied_before_update_advances_in_the_same_frame() {
        let mut session = MockSession::new(10.);
        session.pause();
        session.update(0.5);
        session.apply(GameUiCommand::Resume);
        session.update(0.25);
        assert_eq!(session.snapshot().time, 0.25);
        assert!(!session.snapshot().paused);
    }

    #[test]
    fn death_pauses_at_duration() {
        let mut session = MockSession::new(1.);
        session.update(0.75);
        session.update(0.5);
        let snapshot = session.snapshot();
        assert_eq!(snapshot.time, 1.);
        assert!(snapshot.dead);
        assert!(snapshot.paused);
        session.update(0.5);
        assert_eq!(session.snapshot().time, 1.);
    }

    #[test]
    fn retry_resets_time_and_dead() {
        let mut session = MockSession::new(1.);
        session.update(1.);
        session.apply(GameUiCommand::Retry);
        let snapshot = session.snapshot();
        assert_eq!(snapshot.time, 0.);
        assert!(!snapshot.dead);
        assert!(!snapshot.paused);
        session.update(0.25);
        assert_eq!(session.snapshot().time, 0.25);
    }

    #[test]
    fn resume_is_ignored_when_dead() {
        let mut session = MockSession::new(1.);
        session.update(1.);
        session.apply(GameUiCommand::Resume);
        assert!(session.snapshot().paused);
        assert!(session.snapshot().dead);
    }

    #[test]
    fn exit_marks_the_session_exited() {
        let mut session = MockSession::new(10.);
        session.apply(GameUiCommand::Exit);
        assert!(session.exited());
        assert!(session.snapshot().paused);
        session.update(0.5);
        assert_eq!(session.snapshot().time, 0.);
    }

    #[test]
    fn commands_are_drained_once() {
        let queue = command_queue();
        queue.borrow_mut().push(GameUiCommand::Resume);
        queue.borrow_mut().push(GameUiCommand::Retry);
        assert_eq!(
            drain(&queue),
            vec![GameUiCommand::Resume, GameUiCommand::Retry]
        );
        assert!(drain(&queue).is_empty());
    }

    #[test]
    fn snapshot_reflects_session_state() {
        let mut session = MockSession::new(12.5);
        session.update(0.5);
        let snapshot = session.snapshot();
        assert_eq!(snapshot.time, 0.5);
        assert_eq!(snapshot.duration, 12.5);
        assert!(!snapshot.paused);
        assert!(!snapshot.dead);
    }
}
