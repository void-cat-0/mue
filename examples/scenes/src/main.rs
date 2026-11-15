//! A Scene stack preserves gameplay and UI while a result Scene is pushed.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use anyhow::{Result, bail, ensure};
use macroquad::{
    miniquad::{EventHandler, KeyCode, KeyMods, MouseButton, TouchPhase},
    prelude::*,
};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
    color::Color as UiColor, font, math::Rect as UiRect, node::NodeRef,
};

type Phases = Rc<RefCell<Vec<&'static str>>>;
type CheckLog = Rc<RefCell<CheckState>>;

const CHECK_HOST_FRAMES: u32 = 16;
// Stack depth, ordinary Game updates, HUD updates, modal updates after each host frame.
const CHECK_FRAMES: [(usize, u32, u32, u32); 16] = [
    (1, 1, 1, 0),
    (1, 2, 2, 1),
    (1, 3, 3, 2),
    (1, 4, 4, 2),
    (1, 5, 5, 2),
    (2, 6, 5, 2), // push Return
    (2, 6, 5, 2), // host suspension while Game is already covered
    (2, 6, 5, 2), // repeated suspension must not pause either Scene again
    (2, 6, 5, 2), // host restores only the top ResultScene
    (1, 6, 6, 3), // Return result, then Game resume; Node update only, dt=0
    (1, 7, 7, 4), // same GameScene, still paused by the user
    (2, 8, 7, 4), // push Retry
    (2, 8, 7, 4),
    (1, 8, 8, 4), // Game applies Retry before resume; no modal
    (1, 9, 9, 4),
    (0, 10, 9, 4), // pop the root and let the host break
];
const CHECK_LIFECYCLE: [&str; 14] = [
    "game:enter",
    "game:pause",
    "results:enter",
    "results:pause",
    "results:resume",
    "results:pause",
    "game:result:return",
    "game:resume",
    "game:pause",
    "results:enter",
    "results:pause",
    "game:result:retry",
    "game:resume",
    "game:pause",
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Action {
    #[default]
    None,
    Results,
    Return,
    Retry,
    Exit,
    PopRoot,
}

// This is a gameplay decision, not a framework-level result or a new GameScene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameResult {
    Return,
    Retry,
}

#[derive(Clone, Default)]
struct NodeUpdates {
    count: Rc<Cell<u32>>,
    last_dt: Rc<Cell<f32>>,
}

impl NodeUpdates {
    fn record(&self, dt: f32) {
        self.count.set(self.count.get() + 1);
        self.last_dt.set(dt);
    }
}

#[derive(Default)]
struct CheckState {
    lifecycle: Vec<&'static str>,
    game_enters: u32,
    game_inputs: u32,
    game_updates: u32,
    game_pauses: u32,
    game_resumes: u32,
    result_enters: u32,
    result_inputs: u32,
    result_updates: u32,
    result_pauses: u32,
    result_resumes: u32,
    result_node_updates: u32,
    returned: Vec<GameResult>,
}

impl CheckState {
    fn event(&mut self, event: &'static str) -> Result<()> {
        ensure!(
            CHECK_LIFECYCLE.get(self.lifecycle.len()).copied() == Some(event),
            "unexpected lifecycle event: {event}"
        );
        self.lifecycle.push(event);
        Ok(())
    }

    fn host_frame(
        &self,
        frame: u32,
        app: &App,
        hud: &NodeUpdates,
        modal: &NodeUpdates,
    ) -> Result<()> {
        ensure!(frame <= CHECK_HOST_FRAMES, "finite check failed to exit");
        let (depth, updates, hud_updates, modal_updates) = CHECK_FRAMES[(frame - 1) as usize];
        ensure!(
            app.scene_count() == depth,
            "unexpected stack depth at host frame {frame}"
        );
        ensure!(
            app.is_exited() == (depth == 0),
            "empty stack must be Exited"
        );
        ensure!(
            self.game_updates == updates
                && self.game_inputs == updates
                && hud.count.get() == hud_updates
                && modal.count.get() == modal_updates,
            "covered/transition Game input, Scene update, or Node count mismatch at host frame {frame}"
        );
        Ok(())
    }

    fn finish(&self, host_frames: u32, hud: &NodeUpdates, modal: &NodeUpdates) -> Result<()> {
        ensure!(
            host_frames == CHECK_HOST_FRAMES,
            "unexpected bounded host frame count"
        );
        ensure!(
            self.lifecycle.as_slice() == CHECK_LIFECYCLE,
            "lifecycle check incomplete"
        );
        ensure!(
            self.game_enters == 1 && self.result_enters == 2,
            "each of the three Scene instances must enter exactly once"
        );
        ensure!(
            self.game_pauses == 3
                && self.game_resumes == 2
                && self.result_pauses == 3
                && self.result_resumes == 1,
            "coverage and host suspension did not compose correctly"
        );
        ensure!(
            self.game_inputs == 10
                && self.game_updates == 10
                && self.result_inputs == 4
                && self.result_updates == 4,
            "only the top Scene may receive ordinary input/update"
        );
        ensure!(
            self.returned.as_slice() == [GameResult::Return, GameResult::Retry],
            "bounded check must execute both typed pop results"
        );
        ensure!(
            hud.count.get() == 9 && modal.count.get() == 4 && self.result_node_updates == 4,
            "bounded Node update totals differ"
        );
        println!(
            "check: bounded stack round trips passed: Return kept elapsed/user pause/original UI; Retry reset gameplay; enters=1+2; event(Returned) before event(Resume)"
        );
        println!(
            "check: bounded render checks passed: HUD=9/modal=4/results=4 Node updates, top-only scheduling, transition/host-resume dt=0, draw phases and GPU pixels"
        );
        println!(
            "check: root pop -> stack=0/Exited; host stopped at {host_frames} frames (14 normal + 2 paused); no device-input or full Phira verification"
        );
        Ok(())
    }
}

struct CoveredGame {
    elapsed: f64,
    paused: bool,
    frame: u32,
    hud: NodeRef,
    modal: NodeRef,
    hud_updates: u32,
    modal_updates: u32,
}

// InputBatch::repeat preserves event order, physical coordinates, and touch time.
// App dispatches UI first, then Scene sees the complete raw batch (including UI
// and modal hits). Pointer Down here is observation only, not gameplay hit-testing.
// Within this raw observer, only keyboard commands affect gameplay.
// The Scene owns gameplay modal/contact policy; UI buttons still issue commands.
#[derive(Default)]
struct RawInputObserver {
    toggle_pause: bool,
    exit: bool,
    last_raw_down: Option<RawDown>,
}

struct RawDown {
    x: f32,
    y: f32,
    touch_time: Option<f64>,
}

impl EventHandler for RawInputObserver {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, _mods: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        match key {
            KeyCode::P => self.toggle_pause = !self.toggle_pause,
            KeyCode::Escape => self.exit = true,
            _ => {}
        }
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        if button == MouseButton::Left {
            self.last_raw_down = Some(RawDown {
                x,
                y,
                touch_time: None,
            });
        }
    }

    fn touch_event(&mut self, phase: TouchPhase, _id: u64, x: f32, y: f32, time: f64) {
        if phase == TouchPhase::Started {
            self.last_raw_down = Some(RawDown {
                x,
                y,
                touch_time: Some(time),
            });
        }
    }
}

struct GameScene {
    // Nodes drop before the detached owner of signals they read.
    hud: Node,
    modal: Node,
    _scope: OwnedScope,
    paused: Signal<bool>,
    action: Signal<Action>,
    raw_input: RawInputObserver,
    elapsed: f64,
    // Runtime eligibility (host suspension or another Scene on top), not user pause.
    lifecycle_paused: bool,
    check: Option<CheckLog>,
    frame: u32,
    hud_updates: NodeUpdates,
    modal_updates: NodeUpdates,
    expected_hud_updates: u32,
    expected_modal_updates: u32,
    phases: Phases,
    paused_at: f64,
    result_visits: u32,
    covered: Option<CoveredGame>,
    result_frame: Option<u32>,
}

impl GameScene {
    fn new(check: Option<CheckLog>) -> Self {
        let scope = OwnedScope::new();
        let hud_updates = NodeUpdates::default();
        let modal_updates = NodeUpdates::default();
        let phases: Phases = Rc::default();
        let checking = check.is_some();
        let (paused, action, hud, modal) = scope.run(|| {
            let paused = signal(false);
            let action = signal(Action::None);
            let hud = flexbox()
                .style(mue_macroquad::style! {
                    width: 100%; height: 100%;
                    padding: 14px; gap: 12px; align-items: center;
                    background: {UiColor::from_rgba(25, 37, 57, 245)};
                    border-radius: 12px;
                    font-family: "Noto Sans"; font-size: 18px;
                })
                .on_update({
                    let updates = hud_updates.clone();
                    move |dt| updates.record(*dt)
                })
                .on_render({
                    let phases = phases.clone();
                    move |_| {
                        if checking {
                            phases.borrow_mut().push("hud");
                        }
                    }
                })
                .children((
                    text("Scene HUD"),
                    button("Pause / resume").on_activate(move || paused.set(!paused.get())),
                    button("Results").on_activate(move || action.set(Action::Results)),
                    button("Exit").on_activate(move || action.set(Action::Exit)),
                ))
                .into_node();
            let modal = flexbox()
                .style(mue_macroquad::style! {
                    width: 100%; height: 100%;
                    align-items: center; justify-content: center;
                    background: {UiColor::from_rgba(5, 10, 20, 200)};
                    font-family: "Noto Sans"; font-size: 20px;
                })
                .on_update({
                    let updates = modal_updates.clone();
                    move |dt| updates.record(*dt)
                })
                .on_render({
                    let phases = phases.clone();
                    move |_| {
                        if checking {
                            phases.borrow_mut().push("modal");
                        }
                    }
                })
                .children(
                    flexbox()
                        .style(mue_macroquad::style! {
                            width: 350px; padding: 28px; gap: 14px;
                            flex-direction: column;
                            background: {UiColor::from_rgba(31, 45, 68, 255)};
                            border-radius: 16px;
                        })
                        .children((
                            text("Paused"),
                            text("Gameplay time is frozen. UI still updates."),
                            button("Resume")
                                .primary(true)
                                .on_activate(move || paused.set(false)),
                            button("Show results").on_activate(move || action.set(Action::Results)),
                            button("Exit").on_activate(move || action.set(Action::Exit)),
                        )),
                )
                .into_node();
            (paused, action, hud, modal)
        });
        Self {
            hud,
            modal,
            _scope: scope,
            paused,
            action,
            raw_input: RawInputObserver::default(),
            elapsed: 0.,
            lifecycle_paused: false,
            check,
            frame: 0,
            hud_updates,
            modal_updates,
            expected_hud_updates: 0,
            expected_modal_updates: 0,
            phases,
            paused_at: 0.,
            result_visits: 0,
            covered: None,
            result_frame: None,
        }
    }

    fn check_update(&mut self) -> Result<()> {
        // Direct signal commands on a bounded schedule, not injected device input.
        ensure!(self.frame <= 10, "finite game script overran");
        match self.frame {
            2 => {
                self.paused_at = self.elapsed;
                self.paused.set(true);
            }
            3 => ensure!(
                self.elapsed == self.paused_at,
                "gameplay advanced while paused"
            ),
            4 => self.paused.set(false),
            6 => {
                ensure!(
                    self.hud_updates.count.get() == 5 && self.modal_updates.count.get() == 2,
                    "initial HUD/modal update counts must be 5/2"
                );
                self.paused_at = self.elapsed;
                self.paused.set(true);
                self.action.set(Action::Results);
            }
            7 => {
                ensure!(
                    self.elapsed == self.paused_at && self.paused.get(),
                    "Return must preserve elapsed and the user's in-game pause"
                );
                ensure!(self.result_visits == 1, "Return did not reach the old game");
            }
            8 => {
                ensure!(self.paused.get(), "Return unexpectedly unpaused gameplay");
                self.action.set(Action::Results);
            }
            9 => ensure!(
                self.result_visits == 2 && self.elapsed == 0. && !self.paused.get(),
                "Retry was not handled by the old GameScene's business policy"
            ),
            10 => {
                ensure!(
                    self.hud_updates.count.get() == 9 && self.modal_updates.count.get() == 4,
                    "final HUD/modal update counts must be 9/4"
                );
                self.action.set(Action::PopRoot);
            }
            _ => {}
        }
        Ok(())
    }
}

impl Scene for GameScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => {
                if let Some(check) = &self.check {
                    let mut check = check.borrow_mut();
                    check.event("game:enter")?;
                    check.game_enters += 1;
                    ensure!(
                        check.game_enters == 1 && self.frame == 0,
                        "game entered more than once"
                    );
                }
                self.raw_input = RawInputObserver::default();
            }
            // Runtime eligibility due to coverage or host suspension is separate
            // from the user's in-game pause signal. Neither notification changes it.
            SceneEvent::Pause => {
                if let Some(check) = &self.check {
                    ensure!(
                        !self.lifecycle_paused,
                        "game was paused twice without resuming"
                    );
                    let mut check = check.borrow_mut();
                    check.event("game:pause")?;
                    check.game_pauses += 1;
                }
                self.lifecycle_paused = true;
                self.raw_input = RawInputObserver::default();
            }
            SceneEvent::Resume => {
                if let Some(check) = &self.check {
                    ensure!(self.lifecycle_paused, "game resumed without a pause");
                    let mut check = check.borrow_mut();
                    ensure!(
                        check.returned.len() == self.result_visits as usize,
                        "game resumed before receiving its business result"
                    );
                    check.event("game:resume")?;
                    check.game_resumes += 1;
                }
                self.lifecycle_paused = false;
            }
            SceneEvent::Input(input) => {
                if let Some(check) = &self.check {
                    ensure!(!self.lifecycle_paused, "covered game received input");
                    check.borrow_mut().game_inputs += 1;
                }
                if input.is_interrupted() {
                    self.raw_input = RawInputObserver::default();
                }
                // Replay the complete original batch, including HUD/modal events. This
                // observer does not classify pointer input as gameplay or consume UI input.
                // Never normalize coordinates or replace touch time with ctx.frame.time.
                input.repeat(&mut self.raw_input);
                if std::mem::take(&mut self.raw_input.toggle_pause) {
                    self.paused.set(!self.paused.get());
                }
                if std::mem::take(&mut self.raw_input.exit) {
                    ctx.exit();
                }
            }
            SceneEvent::Returned(result) => {
                let result = match result.downcast::<GameResult>() {
                    Ok(result) => *result,
                    Err(_) => bail!("GameScene expected a GameResult from ResultScene"),
                };
                if let Some(check) = &self.check {
                    let covered = self.covered.take().expect("check push snapshot missing");
                    ensure!(
                        self.lifecycle_paused,
                        "event(Returned) must run before event(Resume)"
                    );
                    ensure!(
                        self.elapsed == covered.elapsed
                            && self.paused.get() == covered.paused
                            && self.frame == covered.frame
                            && *self.hud == covered.hud
                            && *self.modal == covered.modal
                            && self.hud_updates.count.get() == covered.hud_updates
                            && self.modal_updates.count.get() == covered.modal_updates,
                        "covered GameScene or its original UI state was not preserved"
                    );
                    let mut check = check.borrow_mut();
                    check.event(match result {
                        GameResult::Return => "game:result:return",
                        GameResult::Retry => "game:result:retry",
                    })?;
                    check.returned.push(result);
                    self.result_frame = Some(self.frame);
                }
                match result {
                    GameResult::Return => {} // Never clear the user's gameplay pause here.
                    GameResult::Retry => {
                        // Only gameplay decides what retry means; no framework auto-reset.
                        self.elapsed = 0.;
                        self.paused.set(false);
                        self.raw_input = RawInputObserver::default();
                    }
                }
                // The newly exposed roots update once at dt=0 even though Scene::update
                // waits for the next frame. Include that zero-time UI update in the check.
                self.expected_hud_updates += 1;
                if self.paused.get() {
                    self.expected_modal_updates += 1;
                }
            }
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.frame += 1;
        if let Some(check) = &self.check {
            ensure!(!self.lifecycle_paused, "covered game received update");
            check.borrow_mut().game_updates += 1;
            self.check_update()?;
        }
        let action = self.action.get();
        self.action.set(Action::None);
        match action {
            Action::Results => {
                self.result_visits += 1;
                if self.check.is_some() {
                    self.covered = Some(CoveredGame {
                        elapsed: self.elapsed,
                        paused: self.paused.get(),
                        frame: self.frame,
                        hud: *self.hud,
                        modal: *self.modal,
                        hud_updates: self.hud_updates.count.get(),
                        modal_updates: self.modal_updates.count.get(),
                    });
                }
                let check_return = if self.result_visits == 1 {
                    GameResult::Return
                } else {
                    GameResult::Retry
                };
                // Keep this exact GameScene, both Node trees, and their signal scope.
                ctx.push(ResultScene::new(
                    self.elapsed,
                    self.check.clone(),
                    check_return,
                ));
                return Ok(());
            }
            Action::Exit => {
                ctx.exit(); // Intentional full-stack shutdown, unlike returning results.
                return Ok(());
            }
            Action::PopRoot => {
                ctx.pop(); // An empty stack is Exited; the host loop must break.
                return Ok(());
            }
            Action::None | Action::Return | Action::Retry => {}
        }
        if !self.paused.get() && !self.lifecycle_paused {
            self.elapsed += f64::from(ctx.frame.dt);
        }
        self.expected_hud_updates += 1;
        if self.paused.get() {
            self.expected_modal_updates += 1;
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        // Pure submission: App can call layout twice. No timers, updates, or draws.
        // The HUD has a nonzero origin in logical window space.
        roots.push(SceneRoot::new(
            *self.hud,
            UiRect::new(
                frame.viewport.x + 24.,
                frame.viewport.y + 24.,
                (frame.viewport.w - 48.).max(0.),
                84.,
            ),
        ));
        if self.paused.get() {
            // Modal blocks new input to lower Node trees, including blank areas.
            // It does not hide any events from Scene raw_input.
            roots.push(SceneRoot::new(*self.modal, frame.viewport).input(RootInput::Modal));
        }
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        self.phases.borrow_mut().clear();
        let view = render.frame.viewport;
        set_default_camera();
        clear_background(Color::from_rgba(12, 20, 34, 255));
        let center = view.center();
        let x = center.x + (self.elapsed as f32 * 1.4).sin() * view.w * 0.28;
        draw_circle(x, center.y, 38., SKYBLUE);
        draw_text("GAMEPLAY / CUSTOM BACKGROUND", 30., 165., 24., LIGHTGRAY);
        draw_text(
            format!("Gameplay time: {:.2}s", self.elapsed),
            30.,
            200.,
            24.,
            WHITE,
        );
        draw_text(
            "P: pause / resume     Esc: exit",
            30.,
            view.h - 78.,
            22.,
            LIGHTGRAY,
        );
        if let Some(down) = &self.raw_input.last_raw_down {
            let stamp = down.touch_time.map_or_else(
                || "mouse (no timestamp)".to_owned(),
                |time| format!("touch time={time:.6}"),
            );
            draw_text(
                format!(
                    "Last raw Down (incl. UI/modal): physical ({:.0}, {:.0}), {stamp}",
                    down.x, down.y
                ),
                30.,
                view.h - 42.,
                18.,
                GRAY,
            );
        }
        if self.check.is_some() {
            self.phases.borrow_mut().push("background");
        }

        render.draw(*self.hud)?;

        // A complete custom drawing stage between two complete UI tree draws.
        // This orange band overlaps the bottom of the HUD and sits below modal.
        set_default_camera();
        draw_rectangle(
            view.x + 12.,
            view.y + 101.,
            (view.w - 24.).max(0.),
            8.,
            ORANGE,
        );
        draw_text(
            "CUSTOM FOREGROUND (after HUD, before modal)",
            30.,
            view.h - 120.,
            20.,
            ORANGE,
        );
        if self.check.is_some() {
            self.phases.borrow_mut().push("foreground");
        }

        if self.paused.get() {
            render.draw(*self.modal)?;
        }
        if let Some(check) = &self.check {
            let expected: &[&str] = if self.paused.get() {
                &["background", "hud", "foreground", "modal"]
            } else {
                &["background", "hud", "foreground"]
            };
            ensure!(
                self.phases.borrow().as_slice() == expected,
                "incorrect interleaved drawing phases"
            );
            ensure!(
                self.hud_updates.count.get() == self.expected_hud_updates,
                "HUD updated more or less than once per displayed game frame"
            );
            ensure!(
                self.modal_updates.count.get() == self.expected_modal_updates,
                "modal root update count mismatch"
            );
            if let Some(result_frame) = self.result_frame.take() {
                ensure!(
                    self.frame == result_frame,
                    "return transition ran ordinary game update"
                );
                ensure!(
                    render.frame.dt == 0. && self.hud_updates.last_dt.get() == 0.,
                    "exposed GameScene and its HUD must use dt=0 on the pop frame"
                );
                if self.paused.get() {
                    ensure!(
                        self.modal_updates.last_dt.get() == 0.,
                        "exposed modal must use dt=0"
                    );
                }
                let check = check.borrow();
                ensure!(
                    check.game_inputs == self.frame && check.game_updates == self.frame,
                    "old input or an extra update reached the newly exposed game"
                );
            }
            check_pixels(view, self.paused.get())?;
        }
        Ok(())
    }
}

// A pure UI Scene on top of the original game, not a replacement game owner.
struct ResultScene {
    root: Node,
    _scope: OwnedScope,
    action: Signal<Action>,
    check: Option<CheckLog>,
    check_return: GameResult,
    frame: u32,
    lifecycle_paused: bool,
    node_updates: NodeUpdates,
    fresh: bool,
    host_resumed: bool,
}

impl ResultScene {
    fn new(elapsed: f64, check: Option<CheckLog>, check_return: GameResult) -> Self {
        let scope = OwnedScope::new();
        let node_updates = NodeUpdates::default();
        let (action, root) = scope.run(|| {
            let action = signal(Action::None);
            let root = flexbox()
                .style(mue_macroquad::style! {
                    width: 100%; height: 100%;
                    flex-direction: column; align-items: center; justify-content: center;
                    padding: 32px; gap: 20px;
                    background: {UiColor::from_rgba(16, 36, 48, 255)};
                    font-family: "Noto Sans"; font-size: 24px;
                })
                .on_update({
                    let updates = node_updates.clone();
                    let check = check.clone();
                    move |dt| {
                        updates.record(*dt);
                        if let Some(check) = &check {
                            check.borrow_mut().result_node_updates += 1;
                        }
                    }
                })
                .children((
                    text("Results: a pure UI Scene on the stack"),
                    text(format!("Gameplay ran for {elapsed:.2} seconds")),
                    button("Return to game")
                        .primary(true)
                        .on_activate(move || action.set(Action::Return)),
                    button("Retry").on_activate(move || action.set(Action::Retry)),
                    button("Exit").on_activate(move || action.set(Action::Exit)),
                    text("Return preserves gameplay time and your pause setting."),
                    text("Retry asks the same GameScene to restart its timer and run."),
                ))
                .into_node();
            (action, root)
        });
        Self {
            root,
            _scope: scope,
            action,
            check,
            check_return,
            frame: 0,
            lifecycle_paused: false,
            node_updates,
            fresh: true,
            host_resumed: false,
        }
    }
}

impl Scene for ResultScene {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> Result<()> {
        match event {
            SceneEvent::Enter => {
                if let Some(check) = &self.check {
                    ensure!(self.frame == 0 && self.fresh, "result Scene entered twice");
                    let mut check = check.borrow_mut();
                    check.event("results:enter")?;
                    check.result_enters += 1;
                }
            }
            SceneEvent::Pause => {
                if let Some(check) = &self.check {
                    ensure!(
                        !self.lifecycle_paused,
                        "results paused twice without resuming"
                    );
                    let mut check = check.borrow_mut();
                    check.event("results:pause")?;
                    check.result_pauses += 1;
                }
                self.lifecycle_paused = true;
            }
            SceneEvent::Resume => {
                if let Some(check) = &self.check {
                    ensure!(self.lifecycle_paused, "results resumed without pausing");
                    let mut check = check.borrow_mut();
                    check.event("results:resume")?;
                    check.result_resumes += 1;
                }
                self.lifecycle_paused = false;
                self.host_resumed = true;
            }
            SceneEvent::Input(input) => {
                if let Some(check) = &self.check {
                    ensure!(
                        !self.lifecycle_paused && !self.fresh,
                        "new/paused results received old input"
                    );
                    check.borrow_mut().result_inputs += 1;
                }
                let mut keys = RawInputObserver::default();
                input.repeat(&mut keys);
                if keys.exit {
                    ctx.exit();
                }
            }
            SceneEvent::Returned(_) => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> Result<()> {
        self.frame += 1;
        if let Some(check) = &self.check {
            ensure!(
                !self.lifecycle_paused && !self.fresh,
                "new/paused results received update"
            );
            ensure!(self.frame <= 2, "finite result script overran");
            ensure!(
                self.node_updates.count.get() == self.frame,
                "result root must update once on its enter frame and ordinary frames"
            );
            if self.host_resumed {
                ensure!(ctx.frame.dt == 0., "host-restored Scene must use dt=0");
            }
            check.borrow_mut().result_updates += 1;
            if self.frame == 2 {
                self.action.set(match self.check_return {
                    GameResult::Return => Action::Return,
                    GameResult::Retry => Action::Retry,
                });
            }
        }
        let action = self.action.get();
        self.action.set(Action::None);
        match action {
            Action::Return => ctx.pop_with_result(GameResult::Return),
            Action::Retry => ctx.pop_with_result(GameResult::Retry),
            Action::Exit => ctx.exit(),
            Action::None | Action::Results | Action::PopRoot => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> Result<()> {
        roots.push(SceneRoot::new(*self.root, frame.viewport));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> Result<()> {
        set_default_camera();
        clear_background(BLACK);
        render.draw(*self.root)?;
        if let Some(check) = &self.check {
            ensure!(
                self.node_updates.count.get() == self.frame + 1,
                "result root updated more or less than once per displayed frame"
            );
            if self.host_resumed {
                ensure!(
                    render.frame.dt == 0. && self.node_updates.last_dt.get() == 0.,
                    "host-restored result root must update at dt=0"
                );
            }
            if self.fresh {
                ensure!(
                    self.frame == 0
                        && render.frame.dt == 0.
                        && self.node_updates.last_dt.get() == 0.,
                    "push frame must skip new Scene::update and update its Node once at dt=0"
                );
                let check = check.borrow();
                ensure!(
                    check.result_inputs == (check.result_enters - 1) * 2,
                    "old input was replayed to a newly pushed result Scene"
                );
            }
        }
        self.fresh = false;
        self.host_resumed = false;
        Ok(())
    }
}

// Read back after the final UI draw: a later pass must preserve the HUD and
// foreground below the modal. Samples avoid text, buttons, and rounded corners.
fn check_pixels(view: UiRect, paused: bool) -> Result<()> {
    let image = get_screen_data();
    let sample = |x: f32, y: f32| {
        let px = (x / view.w * f32::from(image.width)) as u32;
        let py = (y / view.h * f32::from(image.height)) as u32;
        image.get_pixel(px, u32::from(image.height) - 1 - py)
    };
    let hud = sample(view.w - 64., 65.);
    let foreground = sample(view.w - 64., 105.);
    if paused {
        ensure!(
            hud.r < 0.08 && hud.g < 0.12 && hud.b < 0.18,
            "modal did not cover HUD: {hud:?}"
        );
        ensure!(
            foreground.r < 0.4 && foreground.g < 0.3,
            "modal did not cover custom foreground: {foreground:?}"
        );
    } else {
        ensure!(
            hud.r > 0.07 && hud.g > 0.11 && hud.b > 0.18,
            "HUD missing after custom foreground pass: {hud:?}"
        );
        ensure!(
            foreground.r > 0.9 && foreground.g > 0.4 && foreground.b < 0.1,
            "foreground missing after HUD pass: {foreground:?}"
        );
    }
    Ok(())
}

async fn run(check: bool) -> Result<()> {
    // Initialize before creating UI. No system font lookup or runtime assets.
    let mut db = font::fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    db.set_sans_serif_family("Noto Sans");
    font::initialize("en-US", db)?;

    let check_log: Option<CheckLog> = check.then(Rc::default);
    let scene = GameScene::new(check_log.clone());
    let hud_updates = scene.hud_updates.clone();
    let modal_updates = scene.modal_updates.clone();
    let mut app = App::with_scene(scene);
    let mut frames = 0;
    while !app.is_exited() {
        frames += 1;
        if check && matches!(frames, 7 | 8) {
            // Real host lifecycle calls while GameScene remains under results.
            app.try_frame_paused()?;
        } else {
            app.try_frame()?;
        }
        if let Some(check) = &check_log {
            check
                .borrow()
                .host_frame(frames, &app, &hud_updates, &modal_updates)?;
        }
        if !app.is_exited() {
            next_frame().await;
        }
    }
    if let Some(check) = &check_log {
        check
            .borrow()
            .finish(frames, &hud_updates, &modal_updates)?;
    }
    Ok(())
}

fn main() {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - Scenes".to_owned(),
            window_width: 960,
            window_height: 640,
            ..Default::default()
        },
        async move {
            run(check).await.expect("Scene example failed");
        },
    );
}
