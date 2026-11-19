//! Node construction. Callbacks enqueue commands for their own Scene only.
use std::{cell::Cell, rc::Rc};

use mue_components::prelude::*;
use mue_macroquad::color::Color;

#[derive(Clone, Copy, Default)]
pub(crate) enum GameAction {
    #[default]
    None,
    TogglePause,
    Resume,
    Results,
    Pop,
    Exit,
}

#[derive(Clone, Copy, Default)]
pub(crate) enum ResultsAction {
    #[default]
    None,
    Return,
    Retry,
    Exit,
}

// These small handles never expose gameplay state or another Scene's owner.
fn game_command(action: &Rc<Cell<GameAction>>, command: GameAction) -> impl FnMut() + 'static {
    let action = Rc::clone(action);
    move || action.set(command)
}

fn results_command(
    action: &Rc<Cell<ResultsAction>>,
    command: ResultsAction,
) -> impl FnMut() + 'static {
    let action = Rc::clone(action);
    move || action.set(command)
}

pub(crate) fn game(action: &Rc<Cell<GameAction>>) -> (Node, Node) {
    let hud = flexbox()
        .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            padding: 14px; gap: 12px; align-items: center;
            background: {Color::from_rgba(25, 37, 57, 245)};
            border-radius: 12px;
            font-family: "Noto Sans"; font-size: 18px;
        })
        .children((
            text("Scene HUD"),
            button("Pause / resume").on_activate(game_command(action, GameAction::TogglePause)),
            button("Results").on_activate(game_command(action, GameAction::Results)),
            button("Pop game").on_activate(game_command(action, GameAction::Pop)),
            button("Exit").on_activate(game_command(action, GameAction::Exit)),
        ))
        .into_node();
    let modal = flexbox()
        .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            align-items: center; justify-content: center;
            background: {Color::from_rgba(5, 10, 20, 200)};
            font-family: "Noto Sans"; font-size: 20px;
        })
        .children(
            flexbox()
                .style(mue_macroquad::style! {
                    width: 350px; padding: 28px; gap: 14px;
                    flex-direction: column;
                    background: {Color::from_rgba(31, 45, 68, 255)};
                    border-radius: 16px;
                })
                .children((
                    text("Paused"),
                    text("Gameplay time is frozen. UI still updates."),
                    button("Resume")
                        .primary(true)
                        .on_activate(game_command(action, GameAction::Resume)),
                    button("Show results").on_activate(game_command(action, GameAction::Results)),
                    button("Exit").on_activate(game_command(action, GameAction::Exit)),
                )),
        )
        .into_node();
    (hud, modal)
}

pub(crate) fn results(elapsed: f64, action: &Rc<Cell<ResultsAction>>) -> Node {
    flexbox()
        .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            flex-direction: column; align-items: center; justify-content: center;
            padding: 32px; gap: 20px;
            background: {Color::from_rgba(16, 36, 48, 255)};
            font-family: "Noto Sans"; font-size: 24px;
        })
        .children((
            text("Results: a pure UI Scene on the stack"),
            text(format!("Gameplay ran for {elapsed:.2} seconds")),
            button("Return to game")
                .primary(true)
                .on_activate(results_command(action, ResultsAction::Return)),
            button("Retry").on_activate(results_command(action, ResultsAction::Retry)),
            button("Exit").on_activate(results_command(action, ResultsAction::Exit)),
            text("Return preserves gameplay time and your pause setting."),
            text("Retry asks the same GameScene to restart its timer and run."),
        ))
        .into_node()
}
