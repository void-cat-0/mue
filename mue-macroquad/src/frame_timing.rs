//! Opt-in CPU wall-clock timings for the playground CSV logger.
//! Timings are inclusive: nested stages must not be added to their parents.
//! Without the feature, or on wasm32, probes are no-ops and never read a clock.

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub transition_ms: f64,
    pub timers_ms: f64,
    pub font_resolve_ms: f64,
    pub layout_ms: f64,
    pub taffy_ms: f64,
    pub layout_sync_ms: f64,
    pub render_ms: f64,
    pub pointer_ms: f64,
    pub wheel_ms: f64,
    pub text_content_ms: f64,
    pub text_measure_ms: f64,
    pub text_render_shape_ms: f64,
    pub text_glyph_draw_ms: f64,
    pub keyed_update_ms: f64,
    pub keyed_build_ms: f64,
    pub keyed_drop_ms: f64,
    pub pages_build_ms: f64,
    pub theme_resolve_ms: f64,
    pub prepare_ms: f64,
    pub seal_ms: f64,
    pub mount_ms: f64,
    pub transition_bind_ms: f64,
    pub paint_bind_ms: f64,
    pub children_mount_ms: f64,
    pub node_register_ms: f64,
    pub dispose_total_ms: f64,
    pub scope_dispose_ms: f64,
    pub layout_create_ms: f64,
    pub hook_build_ms: f64,
    pub theme_projection_ms: f64,
    pub resource_image_ms: f64,
    pub resource_material_ms: f64,
    pub resource_material_flush_ms: f64,
    pub resource_shape_ms: f64,
    /// Tween destructor body; field destruction follows this interval.
    pub tween_drop_ms: f64,
    /// Registry retain scan, including removal and destruction of weak handles.
    pub tween_registry_retain_ms: f64,
    pub tween_drop_count: u64,
    pub tween_registry_entries_scanned: u64,
    pub tween_registry_entries_removed: u64,
    pub tween_registry_retain_skipped_count: u64,
    /// Independent core cleanup stages, drained with this backend snapshot.
    pub core_cleanup: mue_core::frame_timing::Snapshot,
    pub layout_pass_count: u64,
    pub measure_call_count: u64,
    pub text_content_update_count: u64,
    pub text_measure_call_count: u64,
    pub text_render_call_count: u64,
    pub glyph_count: u64,
    pub node_mounted_count: u64,
    pub keyed_node_created_count: u64,
    pub keyed_node_removed_count: u64,
    pub pages_created_count: u64,
    pub destination_ms: [f64; 10],
    /// Union of same-stage recursive intervals, indexed by DETAIL_NAMES.
    pub detail_outer_ms: [f64; DETAIL_NAMES.len()],
    pub detail_calls: [u64; DETAIL_NAMES.len()],
}

#[derive(Clone, Copy)]
pub enum Stage {
    Transition,
    Timers,
    FontResolve,
    Layout,
    Taffy,
    LayoutSync,
    Render,
    Pointer,
    Wheel,
    TextContent,
    TextMeasure,
    TextRenderShape,
    TextGlyphDraw,
    KeyedUpdate,
    KeyedBuild,
    KeyedDrop,
    PagesBuild,
    ThemeResolve,
    Prepare,
    Seal,
    Mount,
    TransitionBind,
    PaintBind,
    ChildrenMount,
    NodeRegister,
    DisposeTotal,
    ScopeDispose,
    LayoutCreate,
    HookBuild,
    ThemeProjection,
    ResourceImage,
    ResourceMaterial,
    ResourceMaterialFlush,
    ResourceShape,
    TweenDrop,
    TweenRegistryRetain,
    Destination(usize),
}
pub const DETAIL_NAMES: [&str; 18] = [
    "prepare",
    "seal",
    "mount",
    "transition_bind",
    "paint_bind",
    "children_mount",
    "node_register",
    "dispose_total",
    "scope_dispose",
    "layout_create",
    "hook_build",
    "theme_projection",
    "resource_image",
    "resource_material",
    "resource_material_flush",
    "resource_shape",
    "tween_drop",
    "tween_registry_retain",
];
impl Stage {
    #[cfg(all(feature = "frame-timing", not(target_arch = "wasm32")))]
    fn detail_index(self) -> Option<usize> {
        match self {
            Stage::Prepare => Some(0),
            Stage::Seal => Some(1),
            Stage::Mount => Some(2),
            Stage::TransitionBind => Some(3),
            Stage::PaintBind => Some(4),
            Stage::ChildrenMount => Some(5),
            Stage::NodeRegister => Some(6),
            Stage::DisposeTotal => Some(7),
            Stage::ScopeDispose => Some(8),
            Stage::LayoutCreate => Some(9),
            Stage::HookBuild => Some(10),
            Stage::ThemeProjection => Some(11),
            Stage::ResourceImage => Some(12),
            Stage::ResourceMaterial => Some(13),
            Stage::ResourceMaterialFlush => Some(14),
            Stage::ResourceShape => Some(15),
            Stage::TweenDrop => Some(16),
            Stage::TweenRegistryRetain => Some(17),
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
pub enum Counter {
    LayoutPass,
    MeasureCall,
    TextContentUpdate,
    TextMeasureCall,
    TextRenderCall,
    Glyph,
    NodeMounted,
    KeyedNodeCreated,
    KeyedNodeRemoved,
    PagesCreated,
    TweenDrop,
    TweenRegistryEntriesScanned,
    TweenRegistryEntriesRemoved,
    TweenRegistryRetainSkipped,
}

#[cfg(all(feature = "frame-timing", not(target_arch = "wasm32")))]
mod enabled {
    use super::{Counter, DETAIL_NAMES, Snapshot, Stage};
    use std::{cell::Cell, time::Instant};
    thread_local! {
        static CURRENT: Cell<Snapshot> = Cell::new(Snapshot::default());
        static DEPTH: Cell<[u32; DETAIL_NAMES.len()]> = const { Cell::new([0; DETAIL_NAMES.len()]) };
    }
    pub struct Span {
        stage: Stage,
        start: Instant,
    }
    impl Drop for Span {
        fn drop(&mut self) {
            let elapsed = self.start.elapsed().as_secs_f64() * 1000.0;
            let index = self.stage.detail_index();
            let outer = index.is_some_and(|index| {
                DEPTH.with(|cell| {
                    let mut depth = cell.get();
                    depth[index] -= 1;
                    cell.set(depth);
                    depth[index] == 0
                })
            });
            CURRENT.with(|cell| {
                let mut s = cell.get();
                match self.stage {
                    Stage::Transition => s.transition_ms += elapsed,
                    Stage::Timers => s.timers_ms += elapsed,
                    Stage::FontResolve => s.font_resolve_ms += elapsed,
                    Stage::Layout => s.layout_ms += elapsed,
                    Stage::Taffy => s.taffy_ms += elapsed,
                    Stage::LayoutSync => s.layout_sync_ms += elapsed,
                    Stage::Render => s.render_ms += elapsed,
                    Stage::Pointer => s.pointer_ms += elapsed,
                    Stage::Wheel => s.wheel_ms += elapsed,
                    Stage::TextContent => s.text_content_ms += elapsed,
                    Stage::TextMeasure => s.text_measure_ms += elapsed,
                    Stage::TextRenderShape => s.text_render_shape_ms += elapsed,
                    Stage::TextGlyphDraw => s.text_glyph_draw_ms += elapsed,
                    Stage::KeyedUpdate => s.keyed_update_ms += elapsed,
                    Stage::KeyedBuild => s.keyed_build_ms += elapsed,
                    Stage::KeyedDrop => s.keyed_drop_ms += elapsed,
                    Stage::PagesBuild => s.pages_build_ms += elapsed,
                    Stage::ThemeResolve => s.theme_resolve_ms += elapsed,
                    Stage::Prepare => s.prepare_ms += elapsed,
                    Stage::Seal => s.seal_ms += elapsed,
                    Stage::Mount => s.mount_ms += elapsed,
                    Stage::TransitionBind => s.transition_bind_ms += elapsed,
                    Stage::PaintBind => s.paint_bind_ms += elapsed,
                    Stage::ChildrenMount => s.children_mount_ms += elapsed,
                    Stage::NodeRegister => s.node_register_ms += elapsed,
                    Stage::DisposeTotal => s.dispose_total_ms += elapsed,
                    Stage::ScopeDispose => s.scope_dispose_ms += elapsed,
                    Stage::LayoutCreate => s.layout_create_ms += elapsed,
                    Stage::HookBuild => s.hook_build_ms += elapsed,
                    Stage::ThemeProjection => s.theme_projection_ms += elapsed,
                    Stage::ResourceImage => s.resource_image_ms += elapsed,
                    Stage::ResourceMaterial => s.resource_material_ms += elapsed,
                    Stage::ResourceMaterialFlush => s.resource_material_flush_ms += elapsed,
                    Stage::ResourceShape => s.resource_shape_ms += elapsed,
                    Stage::TweenDrop => s.tween_drop_ms += elapsed,
                    Stage::TweenRegistryRetain => s.tween_registry_retain_ms += elapsed,
                    Stage::Destination(index) => {
                        if let Some(value) = s.destination_ms.get_mut(index) {
                            *value += elapsed;
                        }
                    }
                }
                if let Some(index) = index {
                    if outer {
                        s.detail_outer_ms[index] += elapsed;
                    }
                }
                cell.set(s);
            });
        }
    }
    pub fn start(stage: Stage) -> Span {
        if let Some(index) = stage.detail_index() {
            DEPTH.with(|cell| {
                let mut depth = cell.get();
                depth[index] += 1;
                cell.set(depth);
            });
            CURRENT.with(|cell| {
                let mut s = cell.get();
                s.detail_calls[index] += 1;
                cell.set(s);
            });
        }
        Span {
            stage,
            start: Instant::now(),
        }
    }
    pub fn count(counter: Counter, amount: u64) {
        CURRENT.with(|cell| {
            let mut s = cell.get();
            match counter {
                Counter::LayoutPass => s.layout_pass_count += amount,
                Counter::MeasureCall => s.measure_call_count += amount,
                Counter::TextContentUpdate => s.text_content_update_count += amount,
                Counter::TextMeasureCall => s.text_measure_call_count += amount,
                Counter::TextRenderCall => s.text_render_call_count += amount,
                Counter::Glyph => s.glyph_count += amount,
                Counter::NodeMounted => s.node_mounted_count += amount,
                Counter::KeyedNodeCreated => s.keyed_node_created_count += amount,
                Counter::KeyedNodeRemoved => s.keyed_node_removed_count += amount,
                Counter::PagesCreated => s.pages_created_count += amount,
                Counter::TweenDrop => s.tween_drop_count += amount,
                Counter::TweenRegistryEntriesScanned => s.tween_registry_entries_scanned += amount,
                Counter::TweenRegistryEntriesRemoved => s.tween_registry_entries_removed += amount,
                Counter::TweenRegistryRetainSkipped => {
                    s.tween_registry_retain_skipped_count += amount
                }
            }
            cell.set(s);
        });
    }
    pub fn take() -> Snapshot {
        debug_assert!(
            DEPTH.with(|cell| cell.get().iter().all(|depth| *depth == 0)),
            "cannot take a snapshot with detail spans still open"
        );
        let mut snapshot = CURRENT.with(|cell| cell.replace(Snapshot::default()));
        snapshot.core_cleanup = mue_core::frame_timing::take();
        snapshot
    }
}
#[cfg(all(feature = "frame-timing", not(target_arch = "wasm32")))]
pub use enabled::{Span, count, start, take};

#[cfg(any(not(feature = "frame-timing"), target_arch = "wasm32"))]
pub struct Span;
#[cfg(any(not(feature = "frame-timing"), target_arch = "wasm32"))]
#[inline(always)]
pub fn start(_: Stage) -> Span {
    Span
}
#[cfg(any(not(feature = "frame-timing"), target_arch = "wasm32"))]
#[inline(always)]
pub fn count(_: Counter, _: u64) {}
#[cfg(any(not(feature = "frame-timing"), target_arch = "wasm32"))]
pub fn take() -> Snapshot {
    Snapshot::default()
}

#[cfg(all(test, feature = "frame-timing", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn nested_stages_accumulate_without_losing_counts_and_take_resets() {
        let _ = take();
        let parent = start(Stage::Pointer);
        {
            let _child = start(Stage::KeyedBuild);
            count(Counter::KeyedNodeCreated, 1);
            let _destination = start(Stage::Destination(1));
            count(Counter::NodeMounted, 3);
        }
        drop(parent);
        let snapshot = take();
        assert_eq!(snapshot.keyed_node_created_count, 1);
        assert_eq!(snapshot.node_mounted_count, 3);
        assert!(snapshot.pointer_ms >= snapshot.keyed_build_ms);
        assert!(snapshot.keyed_build_ms >= snapshot.destination_ms[1]);
        let empty = take();
        assert_eq!(empty.node_mounted_count, 0);
        assert_eq!(empty.pointer_ms, 0.0);
        assert_eq!(empty.destination_ms, [0.0; 10]);
    }

    #[test]
    fn cleanup_snapshot_and_tween_counters_accumulate_and_reset() {
        let _ = take();
        {
            let _core = mue_core::frame_timing::start(mue_core::frame_timing::Stage::EffectDestroy);
            let _tween = start(Stage::TweenDrop);
            let _retain = start(Stage::TweenRegistryRetain);
            count(Counter::TweenDrop, 2);
            count(Counter::TweenRegistryEntriesScanned, 7);
            count(Counter::TweenRegistryEntriesScanned, 3);
            count(Counter::TweenRegistryEntriesRemoved, 2);
            count(Counter::TweenRegistryRetainSkipped, 1);
        }
        let snapshot = take();
        assert_eq!(snapshot.tween_drop_count, 2);
        assert_eq!(snapshot.tween_registry_entries_scanned, 10);
        assert_eq!(snapshot.tween_registry_entries_removed, 2);
        assert_eq!(snapshot.tween_registry_retain_skipped_count, 1);
        assert_eq!(snapshot.detail_calls[16], 1);
        assert_eq!(snapshot.detail_calls[17], 1);
        assert!(snapshot.tween_drop_ms >= snapshot.tween_registry_retain_ms);
        let effect = mue_core::frame_timing::Stage::EffectDestroy as usize;
        assert_eq!(snapshot.core_cleanup.detail_calls[effect], 1);
        let empty = take();
        assert_eq!(empty.tween_drop_ms, 0.0);
        assert_eq!(empty.tween_registry_retain_ms, 0.0);
        assert_eq!(empty.tween_drop_count, 0);
        assert_eq!(empty.tween_registry_entries_scanned, 0);
        assert_eq!(empty.tween_registry_entries_removed, 0);
        assert_eq!(empty.tween_registry_retain_skipped_count, 0);
        assert_eq!(
            empty.core_cleanup,
            mue_core::frame_timing::Snapshot::default()
        );
        assert_eq!(
            mue_core::frame_timing::take(),
            mue_core::frame_timing::Snapshot::default()
        );
    }

    #[test]
    fn recursive_detail_records_sum_outer_union_calls_and_unwind_cleanup() {
        let _ = take();
        {
            let _outer = start(Stage::Mount);
            {
                let _inner = start(Stage::Mount);
                count(Counter::NodeMounted, 1);
            }
        }
        let snapshot = take();
        let index = DETAIL_NAMES
            .iter()
            .position(|name| *name == "mount")
            .unwrap();
        assert_eq!(snapshot.detail_calls[index], 2);
        assert!(snapshot.mount_ms >= snapshot.detail_outer_ms[index]);
        assert!(snapshot.detail_outer_ms[index] > 0.0);
        let _ = std::panic::catch_unwind(|| {
            let _timing = start(Stage::Mount);
            panic!("test unwind");
        });
        assert_eq!(take().detail_calls[index], 1);
        let empty = take();
        assert_eq!(empty.detail_outer_ms, [0.0; DETAIL_NAMES.len()]);
        assert_eq!(empty.detail_calls, [0; DETAIL_NAMES.len()]);
    }
}
