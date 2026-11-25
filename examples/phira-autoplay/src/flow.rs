//! One Scene owns the load, audio clock and play/result states. Retry never reloads the PEZ.
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use macroquad::{
    color::Color,
    miniquad::KeyCode,
    prelude::{draw_line, draw_rectangle},
};
use mue_components::{ScrollOptions, scroll_area};
use mue_core::{
    scope::OwnedScope,
    signal::{Signal, signal},
    task::{Task, spawn, spawn_blocking},
};
use mue_macroquad::{
    CallbackResult, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
    SceneTransition, TransitionPhase, TransitionTiming,
    math::Rect,
    node::{IntoNode, Node, flexbox, keyed, text},
    style::{Style, Styleable, StyleableExt},
    transition::Transition,
};

use super::app_log;
use super::{
    artwork::{self, Surface},
    assets::{self, Prepared, Song},
    audio::{self, HitCursor, Playback},
    gameplay,
    ui::{self, ACCENT, BACK, GOLD, INK, SUB},
};

pub fn cover_transition() -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::ease(0.18),
            enter: Transition::ease(0.26),
        },
        |frame| {
            let v = frame.viewport;
            let alpha = match frame.phase {
                TransitionPhase::BeforeSwitch => frame.progress,
                TransitionPhase::AfterSwitch => 1. - frame.progress,
            };
            draw_rectangle(
                v.x,
                v.y,
                v.w,
                v.h,
                Color::new(BACK.r, BACK.g, BACK.b, alpha),
            );
            Ok(())
        },
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayResult {
    pub hits: usize,
    pub total: usize,
}

impl PlayResult {
    pub fn score(self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        (1_000_000. * self.hits as f64 / self.total as f64).round() as u32
    }
}

struct Load {
    task: Task<assets::Result<Prepared>>,
    cancelled: Arc<AtomicBool>,
}

impl Load {
    fn new(song: &Song) -> Self {
        let path = song.path.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker = cancelled.clone();
        let task = spawn(async move { spawn_blocking(move || assets::load(&path, &worker)).await });
        Self { task, cancelled }
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        self.cancelled.store(true, Relaxed);
    }
}

struct Session {
    assets: Prepared,
    track: Playback,
    completions: Vec<f64>,
    hit_cursor: HitCursor,
    offset: f64,
    time: f64,
    elapsed: f64,
    hits: usize,
}

impl Session {
    fn new(assets: Prepared, track: Playback, offset: f64) -> Self {
        let offset = offset + assets.chart.offset;
        let mut completions: Vec<_> = assets
            .chart
            .hits
            .iter()
            .map(|hit| assets.chart.lines[hit.line].notes[hit.note].end)
            .collect();
        completions.sort_by(f64::total_cmp);
        Self {
            assets,
            track,
            completions,
            hit_cursor: HitCursor::default(),
            offset,
            time: (-offset).max(0.),
            elapsed: 0.,
            hits: 0,
        }
    }

    fn reset(&mut self) {
        self.elapsed = 0.;
        self.time = (-self.offset).max(0.);
        self.hits = 0;
        self.hit_cursor = HitCursor::default();
    }

    fn update(&mut self) -> audio::Result<bool> {
        let length = self.assets.audio.length();
        let position = self.track.position();
        // The mixer's cursor is computed from samples; rounding at EOF can put
        // it just under the clip length. Use that raw cursor, not output latency.
        let done = position + 1e-9 >= length;
        self.elapsed = if done { length } else { position };
        let chart_time = self.elapsed - self.offset;
        self.time = chart_time.max(0.);
        self.hits = self.completions.partition_point(|&time| time <= self.time);
        self.track
            .play_hits(self.hit_cursor.advance(&self.assets.chart, chart_time))?;
        Ok(done)
    }

    fn result(&self) -> PlayResult {
        PlayResult {
            hits: self.hits,
            total: self.completions.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Loading,
    Playing,
    Paused,
    Results,
    Failed,
}

#[derive(Clone, Copy)]
enum Action {
    Back,
    Pause,
    Resume,
    Retry,
    Continue,
}

struct View {
    root: Node,
    phase: Signal<Phase>,
    combo: Signal<Rc<str>>,
    score: Signal<Rc<str>>,
    status: Signal<Rc<str>>,
    result: Signal<PlayResult>,
}

impl View {
    fn new(song: Rc<Song>, action: Rc<Cell<Option<Action>>>) -> Self {
        let phase = signal(Phase::Loading);
        let combo = signal(Rc::<str>::from(""));
        let score = signal(Rc::<str>::from("0000000"));
        let status = signal(Rc::<str>::from(""));
        let result = signal(PlayResult { hits: 0, total: 0 });
        let root = flexbox()
            .size_full()
            .children(keyed(phase, {
                let song = song.clone();
                move |phase: &Phase| {
                    let a = &action;
                    let info = &song.info;
                    let row = || flexbox().gap(10.).flex_wrap(taffy::FlexWrap::Wrap);
                    let scroll = |content: Node| {
                        scroll_area()
                            .size_full()
                            .options(ScrollOptions {
                                mouse_drag: true,
                                ..Default::default()
                            })
                            .content_style(Style::new().padding((0., 4., 0., 0.)))
                            .children(content)
                            .into_node()
                    };
                    match phase {
                        Phase::Loading => scroll(ui::card((
                            text("LOADING  /  本地资源").font_size(15.).color(ACCENT),
                            ui::title(info.name.clone(), 31.),
                            ui::caption(format!("{}  ·  {}", info.level, info.composer)),
                            ui::caption("正在解析谱面与解码音乐，完成后自动开始。"),
                            ui::action(a.clone(), "取消加载", Action::Back),
                        ))),
                        Phase::Playing => flexbox()
                            .size_full()
                            .justify_between()
                            .items_start()
                            .children((
                                ui::action(a.clone(), "Ⅱ", Action::Pause),
                                flexbox().flex_column().items_center().children((
                                    text(combo).font_size(32.).line_height(37.).color(INK),
                                    text("AUTOPLAY").font_size(13.).line_height(18.).color(GOLD),
                                )),
                                text(score).font_size(29.).line_height(37.).color(INK),
                            ))
                            .into_node(),
                        Phase::Paused => scroll(ui::card((
                            text("AUTOPLAY  /  暂停").font_size(15.).color(ACCENT),
                            ui::title(info.name.clone(), 28.),
                            ui::caption(status),
                            row().children((
                                ui::action(a.clone(), "继续", Action::Resume).primary(true),
                                ui::action(a.clone(), "重试", Action::Retry),
                                ui::action(a.clone(), "返回详情", Action::Back),
                            )),
                            ui::caption("Esc / P 继续 · R 重试"),
                        ))),
                        Phase::Results => scroll(ui::card((
                            text("RESULTS  /  AUTOPLAY").font_size(15.).color(GOLD),
                            ui::title(info.name.clone(), 31.),
                            ui::caption(format!("{}  ·  {}", info.level, info.composer)),
                            text(score).font_size(53.).line_height(61.).color(INK),
                            text(result.map(|r| {
                                Rc::<str>::from(if r.total != 0 && r.hits == r.total {
                                    "ALL PERFECT  ·  100.00%"
                                } else {
                                    "AUTOPLAY  ·  演示结束"
                                })
                            }))
                            .font_size(18.)
                            .color(ACCENT),
                            ui::caption(result.map(|r| {
                                Rc::<str>::from(format!(
                                    "PERFECT  {}     MAX COMBO  {} / {}",
                                    r.hits, r.hits, r.total
                                ))
                            })),
                            ui::caption("自动成绩仅用于演示，不写入成绩记录。"),
                            row().children((
                                ui::action(a.clone(), "重试", Action::Retry),
                                ui::action(a.clone(), "继续 · 返回详情", Action::Continue)
                                    .primary(true),
                            )),
                        ))),
                        Phase::Failed => scroll(ui::card((
                            text("无法开始自动游玩").font_size(25.).color(GOLD),
                            ui::caption(status),
                            row().children((
                                ui::action(a.clone(), "重试", Action::Retry).primary(true),
                                ui::action(a.clone(), "返回详情", Action::Back),
                            )),
                        ))),
                    }
                }
            }))
            .into_node();
        Self {
            root,
            phase,
            combo,
            score,
            status,
            result,
        }
    }
}

pub struct PlayFlow {
    view: View,
    song: Rc<Song>,
    action: Rc<Cell<Option<Action>>>,
    load: Option<Load>,
    session: Option<Session>,
    completed: Option<PlayResult>,
    phase: Phase,
    presented: Option<Phase>,
    accepts_input: bool,
    silent: bool,
    _owner: OwnedScope,
}

impl PlayFlow {
    pub fn new(song: Rc<Song>, silent: bool) -> Self {
        let owner = OwnedScope::new();
        let action = Rc::new(Cell::new(None));
        let view = owner.run(|| View::new(song.clone(), action.clone()));
        let load = Some(Load::new(&song));
        Self {
            view,
            song,
            action,
            load,
            session: None,
            completed: None,
            phase: Phase::Loading,
            presented: None,
            accepts_input: false,
            silent,
            _owner: owner,
        }
    }

    fn show(&mut self, phase: Phase) {
        self.phase = phase;
        self.view.phase.set(phase);
        self.presented = None;
        self.accepts_input = false;
        self.action.take();
    }

    fn fail(&mut self, error: impl std::fmt::Display) {
        let message = error.to_string();
        app_log("ERROR", &format!("{}: {message}", self.song.info.name));
        self.session = None;
        self.view.status.set(message.into());
        self.show(Phase::Failed);
    }

    fn load_ready(&mut self) {
        let Some(result) = self.load.as_mut().and_then(|load| load.task.try_take()) else {
            return;
        };
        self.load = None;
        let prepared = match result {
            Ok(value) => value,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        let track = match Playback::new(&prepared.audio, &self.song.media.sounds, self.silent) {
            Ok(track) => track,
            Err(error) => {
                self.fail(format!(
                    "{error}\n检查系统音频输出，或用 --silent 仅观看谱面。"
                ));
                return;
            }
        };
        let chart = &prepared.chart;
        app_log(
            "INFO",
            &format!(
                "Loaded {}: {} · {:.0} BPM · {} judge lines · {} notes · {:.2}s chart / {:.2}s audio @ {} Hz",
                self.song.info.name,
                chart.format,
                chart.bpm,
                chart.lines.len(),
                chart.hits.len(),
                chart.last_time,
                prepared.audio.length(),
                prepared.audio.sample_rate()
            ),
        );
        self.session = Some(Session::new(prepared, track, self.song.info.offset));
        self.begin();
    }

    fn retry(&mut self) -> audio::Result<()> {
        if let Some(session) = &mut self.session {
            session.track.recover_if_needed()?;
            session.track.rewind()?;
            session.reset();
        }
        self.begin();
        Ok(())
    }

    fn begin(&mut self) {
        self.view.combo.set("".into());
        self.view.score.set("0000000".into());
        self.show(Phase::Playing);
    }

    fn stop_clock(&mut self) -> audio::Result<()> {
        if let Some(session) = &mut self.session {
            session.track.pause()?;
        }
        Ok(())
    }

    fn pause(&mut self) -> audio::Result<()> {
        self.stop_clock()?;
        if let Some(session) = &self.session {
            self.view.status.set(
                format!(
                    "{} / {}  ·  {} COMBO",
                    ui::clock(session.elapsed),
                    ui::clock(session.assets.audio.length()),
                    session.hits
                )
                .into(),
            );
        }
        self.show(Phase::Paused);
        Ok(())
    }

    fn tick(&mut self) -> audio::Result<()> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };
        session.track.recover_if_needed()?;
        if session.track.needs_start() {
            session.track.play()?;
            app_log(
                "INFO",
                "Music playback started after the playing scene presented.",
            );
            return Ok(());
        }
        let previous_hits = session.hits;
        let done = session.update()?;
        let result = session.result();
        if previous_hits != result.hits {
            self.view.combo.set(if result.hits == 0 {
                "".into()
            } else {
                result.hits.to_string().into()
            });
            self.view.score.set(format!("{:07}", result.score()).into());
        }
        if done {
            self.stop_clock()?;
            self.completed = Some(result);
            self.view.result.set(result);
            app_log(
                "INFO",
                &format!(
                    "Finished {}: {:07} · {} / {} (AUTOPLAY)",
                    self.song.info.name,
                    result.score(),
                    result.hits,
                    result.total
                ),
            );
            self.show(Phase::Results);
        }
        Ok(())
    }

    fn handle_action(&mut self, action: Action, ctx: &mut SceneContext<'_>) -> audio::Result<()> {
        match (self.phase, action) {
            (Phase::Playing, Action::Pause | Action::Back) => self.pause()?,
            (Phase::Paused, Action::Resume) => self.show(Phase::Playing),
            (Phase::Paused | Phase::Results, Action::Retry) => self.retry()?,
            (Phase::Failed, Action::Retry) => {
                self.load = Some(Load::new(&self.song));
                self.show(Phase::Loading);
            }
            (_, Action::Back) | (Phase::Results, Action::Continue) => {
                self.stop_clock()?;
                self.load = None;
                self.accepts_input = false;
                match self.completed {
                    Some(result) => {
                        ctx.pop_with_result(result).transition(cover_transition());
                    }
                    None => {
                        ctx.pop().transition(cover_transition());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_action(&self, key: KeyCode) -> Option<Action> {
        match (self.phase, key) {
            (Phase::Paused, KeyCode::Escape | KeyCode::Back | KeyCode::P | KeyCode::Space) => {
                Some(Action::Resume)
            }
            (Phase::Playing, KeyCode::Escape | KeyCode::Back | KeyCode::P | KeyCode::Space) => {
                Some(Action::Pause)
            }
            (_, KeyCode::Escape | KeyCode::Back) => Some(Action::Back),
            (Phase::Results, KeyCode::Enter | KeyCode::Space) => Some(Action::Continue),
            (Phase::Paused | Phase::Results | Phase::Failed, KeyCode::R) => Some(Action::Retry),
            _ => None,
        }
    }

    fn field(&self, viewport: Rect) -> Rect {
        let aspect = self.song.info.aspect_ratio;
        let ratio = if self.song.info.force_aspect_ratio {
            aspect
        } else {
            aspect.min(viewport.w / viewport.h)
        };
        let w = viewport.w.min(viewport.h * ratio);
        let h = w / ratio;
        Rect::new(
            viewport.x + (viewport.w - w) * 0.5,
            viewport.y + (viewport.h - h) * 0.5,
            w,
            h,
        )
    }
}

impl Scene for PlayFlow {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        match event {
            SceneEvent::Input(input) if !ctx.is_modal_open() => {
                for key in ui::keys(input) {
                    if (self.accepts_input
                        || (self.phase == Phase::Loading
                            && matches!(key, KeyCode::Escape | KeyCode::Back)))
                        && let Some(action) = self.key_action(key)
                    {
                        self.action.set(Some(action));
                        break;
                    }
                }
            }
            SceneEvent::Pause => {
                if let Err(error) = self.stop_clock() {
                    self.fail(error);
                }
                self.action.take();
            }
            _ => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.accepts_input = !ctx.is_transitioning() && self.presented == Some(self.phase);
        if let Some(action) = self.action.take()
            && (self.accepts_input
                || (self.phase == Phase::Loading && matches!(action, Action::Back)))
        {
            if let Err(error) = self.handle_action(action, ctx) {
                self.fail(error);
            }
            return Ok(());
        }
        if !self.accepts_input {
            return Ok(());
        }
        if self.phase == Phase::Loading {
            self.load_ready();
        } else if self.phase == Phase::Playing
            && let Err(error) = self.tick()
        {
            self.fail(error);
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        let v = frame.viewport;
        let bounds = match self.phase {
            Phase::Playing => {
                let field = self.field(v);
                Rect::new(field.x + 20., field.y + 16., (field.w - 40.).max(1.), 65.)
            }
            Phase::Results => ui::centered(v, 520., 456.),
            _ => ui::centered(v, 480., 306.),
        };
        let input = if self.accepts_input {
            RootInput::Interactive
        } else {
            RootInput::None
        };
        roots.push(SceneRoot::new(*self.view.root, bounds).input(input));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        let v = render.frame.viewport;
        let playfield = self.field(v);
        let playing = matches!(self.phase, Phase::Playing | Phase::Paused);
        draw_rectangle(v.x, v.y, v.w, v.h, BACK.into());
        let mut artwork = Surface::fullscreen(
            if playing { playfield } else { v },
            self.song.artwork.aspect(),
        );
        if playing {
            artwork.dim = self.song.info.background_dim.clamp(0., 1.);
        }
        artwork::draw(&self.song.artwork, artwork, 1.);
        if playing && let Some(session) = &self.session {
            gameplay::draw(
                &session.assets.chart,
                self.song.media.skin.as_ref(),
                session.time,
                playfield,
                self.song.info.line_length,
                self.song.info.negative_length_hold,
                1.,
            );
            let fraction = (session.elapsed / session.assets.audio.length()).clamp(0., 1.) as f32;
            let y = playfield.bottom() - 4.;
            draw_line(
                playfield.x,
                y,
                playfield.right(),
                y,
                3.,
                Color::new(SUB.r, SUB.g, SUB.b, 0.4),
            );
            draw_line(
                playfield.x,
                y,
                playfield.x + playfield.w * fraction,
                y,
                3.,
                GOLD.into(),
            );
        }
        if self.phase == Phase::Paused {
            draw_rectangle(v.x, v.y, v.w, v.h, Color::new(0., 0., 0., 0.55));
        }
        render.draw(*self.view.root)?;
        if !render.is_transition_source() {
            self.presented = Some(self.phase);
        }
        Ok(())
    }
}
