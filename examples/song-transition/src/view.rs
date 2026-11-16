//! Retained UI trees and their Scene-local display geometry.
mod settings;

pub use settings::SettingsView;

use crate::{
    artwork::{self, Anchor, Surface},
    catalog::Song,
    input::{DetailsAction, LibraryAction},
    settings::ReadSettings,
    ui::{settings_status, themed_button},
};
use anyhow::Result;
use mue_components::{ScrollOptions, flexbox, scroll_area, text};
use mue_core::{
    effect::computed,
    scope::{OwnedScope, provide},
    signal::{Access, Signal, signal},
};
use mue_macroquad::{
    RootInput, SceneRender, SceneRoot,
    layout::use_layout,
    math::{Rect, Vector},
    node::{IntoNode, Node, NodeRef},
    shader::Brush,
    style::{Style, Styleable, StyleableExt},
};
use std::{cell::Cell, rc::Rc};

pub struct LibraryView {
    root: Node,
    header: Node,
    offset: Signal<Vector>,
    selected: Signal<Option<usize>>,
    card_geometry: Vec<Rc<Cell<Rect>>>,
    clip: Rc<Cell<Rect>>,
    displayed: Vec<Option<Anchor>>,
    // Derived UI signals and context are released after the retained Node trees.
    _owner: OwnedScope,
}
impl LibraryView {
    pub fn new(
        songs: &[Song],
        settings: ReadSettings,
        commands: Rc<Cell<Option<LibraryAction>>>,
    ) -> Self {
        let owner = OwnedScope::new();
        let card_geometry = songs
            .iter()
            .map(|_| Rc::new(Cell::new(Rect::default())))
            .collect::<Vec<_>>();
        let clip = Rc::new(Cell::new(Rect::default()));
        let (root, header, offset, selected) = owner.run(|| {
            provide(settings);
            let palette = settings.theme.map(|theme| theme.palette());
            let offset = signal(Vector::ZERO);
            let selected = signal(None::<usize>);
            let rows = songs.iter().enumerate().map(|(index, song)| {
                let image = artwork_card(song, index, commands.clone(), card_geometry[index].clone(), clip.clone());
                let background = computed(move |_| {
                    let colors = palette.get();
                    Brush::Solid(if selected.get() == Some(index) {
                        colors.selected
                    } else {
                        colors.surface
                    })
                });
                flexbox()
                    .style(mue_macroquad::style! {
                        width: 100%; height: 242px; flex-shrink: 0; flex-direction: row;
                        align-items: center; padding: 16px; gap: 28px; border-radius: 22px;
                    })
                    .background(background)
                    // Rows are prebuilt before the scroll root; bind their own foreground.
                    .color(palette.map(|colors| Brush::Solid(colors.foreground)))
                    .children((image, flexbox()
                        .style(mue_macroquad::style! { flex-direction: column; gap: 13px; flex-grow: 1; min-width: 0; })
                        .children((
                            text(format!("{:02}  {}", index + 1, song.title)).font_size(28.)
                                .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            text(song.artist).color(palette.map(|colors| Brush::Solid(colors.muted))).font_size(17.),
                            text(song.level).color(palette.map(|colors| Brush::Solid(colors.foreground))).font_size(16.),
                            text("Click the artwork to open details").color(palette.map(|colors| Brush::Solid(colors.muted))).font_size(14.),
                        ))))
                    .into_node()
            }).collect::<Vec<_>>();
            let root = scroll_area().size_full().offset(offset)
                .options(ScrollOptions { momentum: false, mouse_drag: true, ..Default::default() })
                .content_style(Style::new().gap(16.).padding((8.,)).flex_column())
                .font_family("Noto Sans").color(palette.map(|colors| Brush::Solid(colors.foreground))).children(rows).into_node();
            let header = flexbox().style(mue_macroquad::style! {
                width: 100%; height: 100%; flex-direction: column; gap: 9px;
                font-family: "Noto Sans";
            }).color(palette.map(|colors| Brush::Solid(colors.foreground))).children((
                flexbox().w_full().items_center().justify_between().children((
                    text("SONG LIBRARY").font_size(34.).line_height(42.)
                        .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                    themed_button("Settings  [S]", {
                        let commands = commands.clone();
                        move || commands.set(Some(LibraryAction::Settings))
                    }),
                )),
                text("Artwork expansion / retained list state").font_size(16.).line_height(21.).color(palette.map(|colors| Brush::Solid(colors.muted))),
                settings_status(),
                text("Wheel / drag / arrows: scroll    1-3: open visible artwork    Esc: back    Q: exit")
                    .font_size(13.).line_height(18.).color(palette.map(|colors| Brush::Solid(colors.muted))),
            )).into_node();
            (root, header, offset, selected)
        });
        Self {
            root,
            header,
            offset,
            selected,
            card_geometry,
            clip,
            displayed: vec![None; songs.len()],
            _owner: owner,
        }
    }
    pub fn select(&mut self, index: usize) {
        self.selected.set(Some(index));
    }
    pub fn scroll(&mut self, delta: f32) {
        self.offset
            .set(Vector::new(0., (self.offset.get().y + delta).max(0.)));
    }
    pub fn displayed_anchor(&self, index: usize) -> Option<Anchor> {
        self.displayed
            .get(index)
            .copied()
            .flatten()
            .filter(|anchor| {
                anchor
                    .surface
                    .rect
                    .intersect(&anchor.surface.clip)
                    .is_some_and(|r| r.w > 0. && r.h > 0.)
            })
    }
    pub fn layout(&self, viewport: Rect, input: RootInput, roots: &mut Vec<SceneRoot>) {
        let clip = Rect::new(
            28.,
            184.,
            (viewport.w - 56.).max(1.),
            (viewport.h - 212.).max(1.),
        );
        self.clip.set(clip);
        roots.push(
            SceneRoot::new(
                *self.header,
                Rect::new(36., 24., (viewport.w - 72.).max(1.), 140.),
            )
            .input(input),
        );
        roots.push(SceneRoot::new(*self.root, clip).input(input));
    }
    pub fn draw(&mut self, render: &mut SceneRender<'_>, songs: &[Song]) -> Result<()> {
        render.draw(*self.header)?;
        render.draw(*self.root)?;
        if !render.is_transition_source() {
            for (index, rect) in self.card_geometry.iter().enumerate() {
                self.displayed[index] = Some(Anchor {
                    surface: Surface::card(
                        rect.get(),
                        self.clip.get(),
                        songs[index].artwork.aspect(),
                    ),
                    viewport: render.frame.viewport,
                });
            }
        }
        Ok(())
    }
}

fn artwork_card(
    song: &Song,
    index: usize,
    commands: Rc<Cell<Option<LibraryAction>>>,
    geometry: Rc<Cell<Rect>>,
    clip: Rc<Cell<Rect>>,
) -> Node {
    let artwork = song.artwork.clone();
    Node::leaf(
        Style::new()
            .width(320.)
            .height(210.)
            .flex_shrink(0.)
            .on_tap(move |_| commands.set(Some(LibraryAction::Open(index)))),
        move |style| {
            let layout = use_layout(style);
            let rect = layout.rect;
            style.on_layout.append(move |_| geometry.set(rect.get()));
            style.on_render.append(move |_| {
                artwork::draw(
                    &artwork,
                    Surface::card(rect.get(), clip.get(), artwork.aspect()),
                    1.,
                );
            });
        },
    )
}

pub struct DetailsView {
    root: Node,
    _owner: OwnedScope,
}
impl DetailsView {
    pub fn new(
        song: &Song,
        settings: ReadSettings,
        commands: Rc<Cell<Option<DetailsAction>>>,
    ) -> Self {
        let owner = OwnedScope::new();
        let root = owner.run(|| {
            provide(settings);
            let palette = settings.theme.map(|theme| theme.palette());
            flexbox()
                .style(mue_macroquad::style! {
                    width: 100%; height: 100%; flex-direction: column; justify-content: center;
                    padding: 48px; font-family: "Noto Sans";
                })
                .color(palette.map(|colors| Brush::Solid(colors.foreground)))
                .children(
                    flexbox()
                        .style(mue_macroquad::style! {
                            width: 100%; flex-direction: column; gap: 19px;
                            padding: 28px; border-radius: 22px;
                        })
                        .background(palette.map(|colors| Brush::Solid(colors.panel)))
                        .children((
                            text("TRACK DETAILS")
                                .font_size(15.)
                                .color(palette.map(|colors| Brush::Solid(colors.muted))),
                            text(song.title)
                                .font_size(46.)
                                .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            text(song.artist)
                                .font_size(23.)
                                .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            text(song.level)
                                .font_size(18.)
                                .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            text(
                                "Return to the same song card, with the original list position.
The artwork is reused throughout the transition.",
                            )
                            .font_size(17.)
                            .color(palette.map(|colors| Brush::Solid(colors.foreground))),
                            settings_status(),
                            flexbox().gap(16.).children((
                                themed_button("Back to song library", {
                                    let commands = commands.clone();
                                    move || commands.set(Some(DetailsAction::Back))
                                }),
                                themed_button("Settings  [S]", {
                                    let commands = commands.clone();
                                    move || commands.set(Some(DetailsAction::Settings))
                                }),
                            )),
                            text("Esc: return    S: settings    Q: exit")
                                .font_size(14.)
                                .color(palette.map(|colors| Brush::Solid(colors.muted))),
                        )),
                )
                .into_node()
        });
        Self {
            root,
            _owner: owner,
        }
    }
    pub fn root(&self) -> NodeRef {
        *self.root
    }
}
