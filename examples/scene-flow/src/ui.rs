//! Declarative views. Button callbacks capture only their Scene's command slot.
use crate::{
    flow::{AttemptResult, EndReason, FlowResult},
    scene::{
        details::Action as DetailsAction,
        play_flow::{Action as FlowAction, Phase},
    },
};
use macroquad::prelude::*;
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::SceneRender;
use std::{cell::Cell, rc::Rc};

fn command<A: Copy + 'static>(slot: Rc<Cell<A>>, label: &'static str, action: A) -> impl IntoNode {
    button(label).on_activate(move || slot.set(action))
}

/// Created inside Details' OwnedScope; its root and signals stay with that Scene.
pub(crate) struct DetailsView {
    pub(crate) root: Node,
    pub(crate) selected: Signal<u32>,
    pub(crate) filter: Signal<bool>,
    pub(crate) summary: Signal<Rc<str>>,
}

pub(crate) fn details(action: Rc<Cell<DetailsAction>>) -> DetailsView {
    let selected = signal(7_u32);
    let filter = signal(true);
    let summary = signal(Rc::<str>::from("No child result yet."));
    let root = flexbox()
        .style(mue_macroquad::style! {
            width: 65%; height: 100%; flex-direction: column; justify-content: center;
            padding: 30px; gap: 10px; font-family: "Noto Sans"; font-size: 17px;
        })
        .children((
            text("Resource details").font_size(28.),
            text("Offline 3-second trajectory + decoded RGBA texture."),
            text(selected.map(|n| Rc::<str>::from(format!("Selected entry: {n}")))),
            text(filter.map(|v| Rc::<str>::from(format!("Filter enabled: {v}")))),
            command(
                action.clone(),
                "Change selected entry",
                DetailsAction::SelectNext,
            ),
            command(action.clone(), "Toggle filter", DetailsAction::ToggleFilter),
            command(action.clone(), "Load resource", DetailsAction::Load),
            command(
                action.clone(),
                "Load with a business failure",
                DetailsAction::LoadFailure,
            ),
            text(summary),
            command(action, "Exit root Scene", DetailsAction::Quit),
            text("Space: load    Esc: exit").font_size(14.),
        ))
        .into_node();
    DetailsView {
        root,
        selected,
        filter,
        summary,
    }
}

/// The PlayFlow Scene owns all reactive handles used by this view.
pub(crate) struct PlayFlowView {
    pub(crate) root: Node,
    pub(crate) phase: Signal<Phase>,
    pub(crate) title: Signal<Rc<str>>,
    pub(crate) body: Signal<Rc<str>>,
}

pub(crate) fn play_flow(action: Rc<Cell<FlowAction>>) -> PlayFlowView {
    let phase = signal(Phase::Loading);
    let title = signal(Rc::<str>::from("Loading CPU resource"));
    let body = signal(Rc::<str>::from(""));
    let root = flexbox()
        .style(mue_macroquad::style! {
            width: 65%; height: 100%; flex-direction: column;
            padding: 30px; gap: 14px; justify-content: center;
            font-family: "Noto Sans"; font-size: 17px;
        })
        .children((
            text(title).font_size(28.),
            text(body),
            command(action.clone(), "Cancel loading / Back", FlowAction::Cancel)
                .show_if(phase.map(|p| p == Phase::Loading)),
            command(
                action.clone(),
                "Retry loading successfully",
                FlowAction::RetryLoad,
            )
            .show_if(phase.map(|p| p == Phase::Failure)),
            command(action.clone(), "Start prepared session", FlowAction::Start)
                .show_if(phase.map(|p| p == Phase::Ready)),
            command(action.clone(), "Pause / resume", FlowAction::TogglePause)
                .show_if(phase.map(|p| p == Phase::Running)),
            command(action.clone(), "Stop attempt", FlowAction::ExitAttempt)
                .show_if(phase.map(|p| p == Phase::Running)),
            command(
                action.clone(),
                "Retry with the same resources",
                FlowAction::Retry,
            )
            .show_if(phase.map(|p| p == Phase::Results)),
            command(action, "Return to original details", FlowAction::Return)
                .show_if(phase.map(|p| p != Phase::Loading)),
            text("Space: start    P: pause    R: retry    Esc: back").font_size(14.),
        ))
        .into_node();
    PlayFlowView {
        root,
        phase,
        title,
        body,
    }
}

pub(crate) fn attempt_summary(result: &AttemptResult) -> String {
    let reason = match result.reason {
        EndReason::Completed => "Completed",
        EndReason::Exited => "Exited",
    };
    format!(
        "{reason} attempt {} · {:.2}/{:.2}s · {} points reached",
        result.attempt, result.elapsed, result.duration, result.points_reached,
    )
}

pub(crate) fn result_summary(result: &FlowResult) -> Rc<str> {
    match result {
        FlowResult::AttemptResult(result) => attempt_summary(result).into(),
        FlowResult::Failure(failure) => format!("Loading failed: {failure}").into(),
        FlowResult::Cancelled { generation_id } => {
            format!("Returned without an attempt (request {generation_id}).").into()
        }
    }
}

pub(crate) fn paint_base(render: &SceneRender<'_>) {
    let v = render
        .transition()
        .map_or(render.frame.viewport, |transition| transition.viewport);
    // Both live drawing and source capture use App's current target/camera.
    draw_rectangle(v.x, v.y, v.w, v.h, Color::from_rgba(12, 22, 38, 255));
}
