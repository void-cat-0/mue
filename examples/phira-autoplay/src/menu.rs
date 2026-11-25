//! Retained library and song details. The card effect comes from song-transition.
use std::{cell::Cell, path::PathBuf, rc::Rc};

use macroquad::{miniquad::KeyCode, prelude::draw_rectangle};
use mue_components::{ScrollOptions, ToastKind, ToastSender, scroll_area};
use mue_core::{
    scope::OwnedScope,
    signal::{Signal, signal},
    task::{Task, spawn, spawn_blocking},
};
use mue_macroquad::{
    CallbackResult, RootInput, Scene, SceneContext, SceneEvent, SceneFrame, SceneRender, SceneRoot,
    math::Rect,
    node::{IntoNode, Node, flexbox, text},
    style::{Style, Styleable, StyleableExt, percent},
};

use super::app_log;

use super::{
    artwork::{self, Anchor, Surface},
    assets::{self, DecodedMedia, Preview, Song},
    effects::{self, ReturnTarget},
    flow::{PlayFlow, PlayResult, cover_transition},
    ui::{self, ACCENT, BACK, GOLD, INK, PANEL},
};

#[derive(Clone, Copy)]
enum LibraryAction {
    Open(usize),
    Exit,
}

struct SongCard {
    song: Rc<Song>,
    geometry: Rc<Cell<Rect>>,
}

struct DetailsReturn {
    index: usize,
    target: ReturnTarget,
}

struct Cards {
    root: Node,
    songs: Vec<SongCard>,
    selected: Signal<Option<usize>>,
    clip: Rc<Cell<Rect>>,
    displayed: Vec<Option<Anchor>>,
}

impl Cards {
    fn new(songs: Vec<Rc<Song>>, slot: Rc<Cell<Option<LibraryAction>>>) -> Self {
        let selected = signal(None::<usize>);
        let clip = Rc::new(Cell::new(Rect::default()));
        let songs: Vec<_> = songs
            .into_iter()
            .map(|song| SongCard {
                song,
                geometry: Rc::new(Cell::new(Rect::default())),
            })
            .collect();
        let nodes = songs
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let art = ui::cover(
                    entry.song.artwork.clone(),
                    184.,
                    entry.geometry.clone(),
                    clip.clone(),
                );
                let info = &entry.song.info;
                let badge = format!("{:02}    {}", index + 1, info.level);
                let background = selected.map(move |s| {
                    if s == Some(index) {
                        mue_macroquad::shader::Brush::Solid(mue_macroquad::color::Color::new(
                            0.11, 0.17, 0.25, 1.,
                        ))
                    } else {
                        mue_macroquad::shader::Brush::Solid(PANEL)
                    }
                });
                let open = slot.clone();
                flexbox()
                    .width(percent(100.))
                    .max_width(422.)
                    .min_width(238.)
                    .flex_grow(1.)
                    .flex_column()
                    .gap(7.)
                    .padding((15.,))
                    .radius(18.)
                    .background(background)
                    .on_tap(move |_| open.set(Some(LibraryAction::Open(index))))
                    .children((
                        text(badge).font_size(15.).color(ACCENT),
                        art,
                        ui::title(info.name.clone(), 25.),
                        ui::caption(info.composer.clone()),
                        ui::action(slot.clone(), "打开谱面", LibraryAction::Open(index)),
                    ))
                    .into_node()
            })
            .collect::<Vec<_>>();
        let root = scroll_area()
            .size_full()
            .content_style(
                Style::new()
                    .padding((4.,))
                    .gap(16.)
                    .flex_row()
                    .flex_wrap(taffy::FlexWrap::Wrap),
            )
            .options(ScrollOptions {
                mouse_drag: true,
                ..Default::default()
            })
            .children(nodes)
            .into_node();
        Self {
            root,
            selected,
            clip,
            displayed: vec![None; songs.len()],
            songs,
        }
    }

    fn anchor(&self, index: usize) -> Option<Anchor> {
        let anchor = self.displayed.get(index).copied().flatten()?;
        anchor
            .surface
            .rect
            .intersect(&anchor.surface.clip)
            .map(|_| anchor)
    }

    fn record(&mut self, viewport: Rect) {
        let clip = self.clip.get();
        for (entry, target) in self.songs.iter().zip(&mut self.displayed) {
            let rect = entry.geometry.get();
            *target = (rect.w > 0. && rect.h > 0.).then(|| Anchor {
                surface: Surface::card(rect, clip, entry.song.artwork.aspect()),
                viewport,
            });
        }
    }
}

struct CatalogResult {
    previews: Vec<Preview>,
    media: DecodedMedia,
    errors: Vec<String>,
}

pub struct Library {
    header: Node,
    cards: Option<Cards>,
    slot: Rc<Cell<Option<LibraryAction>>>,
    task: Option<Task<CatalogResult>>,
    status: Signal<Rc<str>>,
    returning: Option<DetailsReturn>,
    active: bool,
    silent: bool,
    toasts: ToastSender,
    owner: OwnedScope,
}

impl Library {
    pub fn new(
        paths: Vec<PathBuf>,
        media_dirs: Vec<PathBuf>,
        silent: bool,
        toasts: ToastSender,
    ) -> Self {
        let owner = OwnedScope::new();
        let slot = Rc::new(Cell::new(None));
        let (header, status) = owner.run(|| {
            let status = signal(Rc::<str>::from("正在读取本地 PEZ 的清单和封面…"));
            let header = ui::column((
                flexbox()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .children((
                        ui::title("本地谱库", 36.),
                        ui::action(slot.clone(), "退出", LibraryAction::Exit),
                    )),
                text(if silent {
                    "PHIRA AUTOPLAY  /  静音时钟"
                } else {
                    "PHIRA AUTOPLAY  /  本地音频"
                })
                .font_size(14.)
                .color(ACCENT),
                ui::caption(status),
            ));
            (header, status)
        });
        let task = spawn(async move {
            spawn_blocking(move || {
                let mut errors = Vec::new();
                let media = DecodedMedia::load(&media_dirs, silent, &mut errors);
                let mut output = CatalogResult {
                    previews: Vec::new(),
                    media,
                    errors,
                };
                for path in paths {
                    match assets::preview(path.clone()) {
                        Ok(preview) => output.previews.push(preview),
                        Err(error) => output.errors.push(format!("{}：{error}", path.display())),
                    }
                }
                output
            })
            .await
        });
        Self {
            header,
            cards: None,
            slot,
            task: Some(task),
            status,
            returning: None,
            active: false,
            silent,
            toasts,
            owner,
        }
    }

    fn accept_catalog(&mut self) {
        let Some(result) = self.task.as_mut().and_then(Task::try_take) else {
            return;
        };
        self.task = None;
        let media = result.media.upload();
        let songs: Vec<_> = result
            .previews
            .into_iter()
            .map(|preview| preview.upload(media.clone()))
            .collect();
        for error in &result.errors {
            app_log("ERROR", &error.to_string());
            self.toasts.show(ToastKind::Error, error.clone());
        }
        if songs.is_empty() {
            self.status
                .set("没有可打开的谱面。请将 PEZ 放在仓库根目录，或传入文件路径。".into());
        } else {
            self.status.set(
                format!(
                    "{} 首可游玩谱面 · 点击封面，或按 1–9 · Esc 退出",
                    songs.len()
                )
                .into(),
            );
            self.cards = Some(self.owner.run(|| Cards::new(songs, self.slot.clone())));
        }
    }
}

impl Scene for Library {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        match event {
            SceneEvent::Input(input)
                if self.active && !ctx.is_transitioning() && !ctx.is_modal_open() =>
            {
                for key in ui::keys(input) {
                    let index = match key {
                        KeyCode::Key1 => 0,
                        KeyCode::Key2 => 1,
                        KeyCode::Key3 => 2,
                        KeyCode::Key4 => 3,
                        KeyCode::Key5 => 4,
                        KeyCode::Key6 => 5,
                        KeyCode::Key7 => 6,
                        KeyCode::Key8 => 7,
                        KeyCode::Key9 => 8,
                        KeyCode::Escape | KeyCode::Back => {
                            self.slot.set(Some(LibraryAction::Exit));
                            break;
                        }
                        _ => continue,
                    };
                    self.slot.set(Some(LibraryAction::Open(index)));
                    break;
                }
            }
            SceneEvent::Returned(value) => {
                if let Ok(result) = value.downcast::<DetailsReturn>() {
                    if let Some(cards) = &self.cards {
                        cards.selected.set(Some(result.index));
                    }
                    self.returning = Some(*result);
                }
            }
            SceneEvent::Pause => {
                self.active = false;
                self.slot.take();
            }
            _ => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        self.accept_catalog();
        let command = self.slot.take();
        self.active = !ctx.is_transitioning();
        if !self.active {
            return Ok(());
        }
        self.returning = None;
        match command {
            Some(LibraryAction::Exit) => {
                ctx.exit().without_transition();
            }
            Some(LibraryAction::Open(index)) => {
                if let Some(cards) = &self.cards
                    && let Some(entry) = cards.songs.get(index)
                {
                    let song = entry.song.clone();
                    cards.selected.set(Some(index));
                    let request = ctx.push(Details::new(index, song.clone(), self.silent));
                    if let Some(anchor) = cards.anchor(index) {
                        request.transition(effects::expand(song.artwork.clone(), anchor));
                    } else {
                        request.transition(cover_transition());
                    }
                    self.active = false;
                }
            }
            None => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        let v = frame.viewport;
        let input = if self.active {
            RootInput::Interactive
        } else {
            RootInput::None
        };
        let width = (v.w - 42.).max(1.);
        let x = v.x + 21.;
        roots.push(SceneRoot::new(*self.header, Rect::new(x, v.y + 18., width, 156.)).input(input));
        if let Some(cards) = &self.cards {
            let bounds = Rect::new(x, v.y + 177., width, (v.h - 193.).max(1.));
            cards.clip.set(bounds);
            roots.push(SceneRoot::new(*cards.root, bounds).input(input));
        }
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        let v = render.frame.viewport;
        draw_rectangle(v.x, v.y, v.w, v.h, BACK.into());
        render.draw(*self.header)?;
        if let Some(cards) = &mut self.cards {
            render.draw(*cards.root)?;
            if !render.is_transition_source() {
                cards.record(v);
                if let Some(result) = &self.returning {
                    result.target.update(cards.anchor(result.index));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum DetailsAction {
    Play,
    Back,
}

struct Details {
    root: Node,
    song: Rc<Song>,
    index: usize,
    slot: Rc<Cell<Option<DetailsAction>>>,
    returned: Signal<Rc<str>>,
    silent: bool,
    active: bool,
    _owner: OwnedScope,
}

impl Details {
    fn new(index: usize, song: Rc<Song>, silent: bool) -> Self {
        let owner = OwnedScope::new();
        let slot = Rc::new(Cell::new(None));
        let (root, returned) = owner.run(|| {
            let returned = signal(Rc::<str>::from("尚未完成自动演示"));
            let info = &song.info;
            let root = ui::card((
                text(info.level.clone()).font_size(17.).color(GOLD),
                ui::title(info.name.clone(), 38.),
                ui::caption(format!(
                    "曲师  {}\n谱师  {}\n曲绘  {}",
                    info.composer, info.charter, info.illustrator
                )),
                text(returned).font_size(17.).color(INK),
                flexbox()
                    .gap(10.)
                    .flex_wrap(taffy::FlexWrap::Wrap)
                    .children((
                        ui::action(slot.clone(), "▶  开始自动游玩", DetailsAction::Play)
                            .primary(true),
                        ui::action(slot.clone(), "返回谱库", DetailsAction::Back),
                    )),
                ui::caption("Enter / Space 开始 · Esc 返回 · 自动成绩不计入记录"),
            ));
            (root, returned)
        });
        Self {
            root,
            song,
            index,
            slot,
            returned,
            active: false,
            silent,
            _owner: owner,
        }
    }
}

impl Scene for Details {
    fn event(&mut self, event: SceneEvent<'_>, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        match event {
            SceneEvent::Input(input)
                if self.active && !ctx.is_transitioning() && !ctx.is_modal_open() =>
            {
                if let Some(action) = ui::keys(input).into_iter().find_map(|key| match key {
                    KeyCode::Escape | KeyCode::Back => Some(DetailsAction::Back),
                    KeyCode::Enter | KeyCode::Space => Some(DetailsAction::Play),
                    _ => None,
                }) {
                    self.slot.set(Some(action));
                }
            }
            SceneEvent::Returned(value) => {
                if let Ok(result) = value.downcast::<PlayResult>() {
                    self.returned.set(
                        format!(
                            "上次演示 · {:07} · {} / {} 键",
                            result.score(),
                            result.hits,
                            result.total
                        )
                        .into(),
                    );
                }
            }
            SceneEvent::Pause => {
                self.active = false;
                self.slot.take();
            }
            _ => {}
        }
        Ok(())
    }

    fn update(&mut self, ctx: &mut SceneContext<'_>) -> CallbackResult<()> {
        let action = self.slot.take();
        self.active = !ctx.is_transitioning();
        if !self.active {
            return Ok(());
        }
        match action {
            Some(DetailsAction::Play) => {
                self.active = false;
                ctx.push(PlayFlow::new(self.song.clone(), self.silent))
                    .transition(cover_transition());
            }
            Some(DetailsAction::Back) => {
                self.active = false;
                let target = ReturnTarget::default();
                ctx.pop_with_result(DetailsReturn {
                    index: self.index,
                    target: target.clone(),
                })
                .transition(effects::shrink(self.song.artwork.clone(), target));
            }
            None => {}
        }
        Ok(())
    }

    fn layout(&mut self, frame: &SceneFrame, roots: &mut Vec<SceneRoot>) -> CallbackResult<()> {
        let v = frame.viewport;
        let w = (v.w - 32.).clamp(1., 490.);
        let h = (v.h - 32.).max(1.);
        let bounds = Rect::new(v.x + 16., v.y + (v.h - 345.).max(20.) / 2., w, h);
        let input = if self.active {
            RootInput::Interactive
        } else {
            RootInput::None
        };
        roots.push(SceneRoot::new(*self.root, bounds).input(input));
        Ok(())
    }

    fn render(&mut self, render: &mut SceneRender<'_>) -> CallbackResult<()> {
        let v = render.frame.viewport;
        draw_rectangle(v.x, v.y, v.w, v.h, BACK.into());
        artwork::draw(
            &self.song.artwork,
            Surface::fullscreen(v, self.song.artwork.aspect()),
            1.,
        );
        render.draw(*self.root)?;
        Ok(())
    }
}
