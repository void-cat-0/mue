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
