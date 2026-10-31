//! Native per-frame CSV output. See ../TIMING.md for column semantics.
use mue_components::PagesTransition;
use mue_macroquad::{font::GlyphAtlasStats, frame_timing::Snapshot};
use std::{
    fs::File,
    io::{self, BufWriter, Write},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const HEADER: &str = "frame,page,frame_interval_ms,example_update_ms,clear_ms,app_frame_ms,next_frame_wait_ms,page_after,transition_before,transition_after,bookkeeping_ms,transition_ms,timers_ms,font_resolve_ms,layout_ms,taffy_ms,layout_sync_ms,render_ms,pointer_ms,wheel_ms,text_content_ms,text_measure_ms,text_render_shape_ms,text_glyph_draw_ms,keyed_update_ms,keyed_build_ms,keyed_drop_ms,pages_build_ms,theme_resolve_ms,destination_layout_ms,destination_typography_ms,destination_graphics_ms,destination_motion_ms,destination_scrolling_ms,destination_input_ms,destination_map_gestures_ms,destination_theme_ms,destination_style_declarations_ms,destination_font_licenses_ms,backend_begin_frame_ms,backend_end_frame_ms,backend_render_passes_ms,backend_ui_draw_ms,backend_draw_submit_ms,backend_commit_ms,next_frame_unattributed_ms,layout_pass_count,measure_call_count,text_content_update_count,text_measure_call_count,text_render_call_count,glyph_count,node_mounted_count,keyed_node_created_count,keyed_node_removed_count,pages_created_count,atlas_rasterized,atlas_evicted,atlas_uploads,atlas_uploaded_pixels,atlas_rejected_oversized,atlas_rejected_exhausted,atlas_rejected_raster,atlas_pages,detail_prepare_ms,detail_prepare_outer_ms,detail_prepare_calls,detail_seal_ms,detail_seal_outer_ms,detail_seal_calls,detail_mount_ms,detail_mount_outer_ms,detail_mount_calls,detail_transition_bind_ms,detail_transition_bind_outer_ms,detail_transition_bind_calls,detail_paint_bind_ms,detail_paint_bind_outer_ms,detail_paint_bind_calls,detail_children_mount_ms,detail_children_mount_outer_ms,detail_children_mount_calls,detail_node_register_ms,detail_node_register_outer_ms,detail_node_register_calls,detail_dispose_total_ms,detail_dispose_total_outer_ms,detail_dispose_total_calls,detail_scope_dispose_ms,detail_scope_dispose_outer_ms,detail_scope_dispose_calls,detail_layout_create_ms,detail_layout_create_outer_ms,detail_layout_create_calls,detail_hook_build_ms,detail_hook_build_outer_ms,detail_hook_build_calls,detail_theme_projection_ms,detail_theme_projection_outer_ms,detail_theme_projection_calls,detail_resource_image_ms,detail_resource_image_outer_ms,detail_resource_image_calls,detail_resource_material_ms,detail_resource_material_outer_ms,detail_resource_material_calls,detail_resource_material_flush_ms,detail_resource_material_flush_outer_ms,detail_resource_material_flush_calls,detail_resource_shape_ms,detail_resource_shape_outer_ms,detail_resource_shape_calls,backend_scene_update_ms,backend_projection_ms,backend_gpu_query_ms,backend_input_reset_ms,backend_texture_gc_ms,backend_touch_cleanup_ms,backend_end_unattributed_ms,app_image_decode_ms,app_image_upload_ms,app_image_load_count,app_image_decoded_bytes,backend_image_decode_ms,backend_image_upload_ms,backend_image_load_count,backend_image_decoded_bytes";

// Name/value pairs share one source so appended CSV columns cannot drift.
fn backend_times(s: macroquad::frame_timing::Snapshot) -> [(&'static str, f64); 24] {
    [
        ("texture_from_rgba8_ms", s.texture_from_rgba8_ms),
        ("texture_new_ms", s.texture_new_ms),
        ("texture_store_ms", s.texture_store_ms),
        ("texture_filter_ms", s.texture_filter_ms),
        ("texture_batcher_add_ms", s.texture_batcher_add_ms),
        ("texture_readback_ms", s.texture_readback_ms),
        ("texture_readback_alloc_ms", s.texture_readback_alloc_ms),
        ("texture_readback_gpu_ms", s.texture_readback_gpu_ms),
        ("texture_atlas_build_ms", s.texture_atlas_build_ms),
        ("texture_atlas_cache_ms", s.texture_atlas_cache_ms),
        ("scene_node_logic_ms", s.scene_node_logic_ms),
        ("scene_node_draw_ms", s.scene_node_draw_ms),
        ("scene_camera_set_ms", s.scene_camera_set_ms),
        ("scene_camera_pop_ms", s.scene_camera_pop_ms),
        ("scene_bookkeeping_ms", s.scene_bookkeeping_ms),
        ("quad_draw_ms", s.quad_draw_ms),
        ("quad_draw_buffer_create_ms", s.quad_draw_buffer_create_ms),
        ("quad_draw_pass_ms", s.quad_draw_pass_ms),
        ("quad_draw_upload_ms", s.quad_draw_upload_ms),
        ("quad_draw_bindings_ms", s.quad_draw_bindings_ms),
        ("quad_draw_uniforms_ms", s.quad_draw_uniforms_ms),
        ("quad_draw_backend_ms", s.quad_draw_backend_ms),
        ("quad_draw_snapshot_ms", s.quad_draw_snapshot_ms),
        ("quad_draw_capture_ms", s.quad_draw_capture_ms),
    ]
}
fn backend_counts(s: macroquad::frame_timing::Snapshot) -> [(&'static str, u64); 13] {
    [
        ("texture_from_rgba8_count", s.texture_from_rgba8_count),
        ("texture_from_rgba8_bytes", s.texture_from_rgba8_bytes),
        ("texture_readback_count", s.texture_readback_count),
        ("texture_readback_bytes", s.texture_readback_bytes),
        ("quad_draw_invocations", s.quad_draw_invocations),
        ("quad_draw_calls", s.quad_draw_calls),
        ("quad_draw_vertices", s.quad_draw_vertices),
        ("quad_draw_indices", s.quad_draw_indices),
        ("quad_draw_buffer_uploads", s.quad_draw_buffer_uploads),
        (
            "quad_draw_buffer_upload_bytes",
            s.quad_draw_buffer_upload_bytes,
        ),
        ("quad_draw_buffers_created", s.quad_draw_buffers_created),
        ("quad_draw_render_passes", s.quad_draw_render_passes),
        ("quad_draw_screen_snapshots", s.quad_draw_screen_snapshots),
    ]
}
fn header() -> String {
    let mut result = HEADER.to_owned();
    for name in ["tween_drop", "tween_registry_retain"] {
        result.push_str(&format!(
            ",detail_{name}_ms,detail_{name}_outer_ms,detail_{name}_calls"
        ));
    }
    for name in [
        "tween_drop_count",
        "tween_registry_entries_scanned",
        "tween_registry_entries_removed",
        "tween_registry_retain_skipped_count",
    ] {
        result.push_str(&format!(",{name}"));
    }
    for name in mue_core::frame_timing::DETAIL_NAMES {
        result.push_str(&format!(
            ",core_{name}_ms,core_{name}_outer_ms,core_{name}_calls"
        ));
    }
    for prefix in ["app", "backend"] {
        for (name, _) in backend_times(Default::default()) {
            result.push_str(&format!(",{prefix}_{name}"));
        }
        for (name, _) in backend_counts(Default::default()) {
            result.push_str(&format!(",{prefix}_{name}"));
        }
    }
    result
}

pub(super) struct Frame {
    pub index: u64,
    pub page: &'static str,
    pub page_after: &'static str,
    pub transition_before: PagesTransition,
    pub transition_after: PagesTransition,
    pub interval_ms: f64,
    pub example_update_ms: f64,
    pub clear_ms: f64,
    pub app_frame_ms: f64,
    pub next_frame_wait_ms: f64,
    pub bookkeeping_ms: f64,
    pub mue: Snapshot,
    pub app_backend: macroquad::frame_timing::Snapshot,
    pub backend: macroquad::frame_timing::Snapshot,
    pub atlas_before: GlyphAtlasStats,
    pub atlas_after: GlyphAtlasStats,
}

pub(super) struct Logger {
    writer: Option<BufWriter<File>>,
}
impl Logger {
    pub fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = format!(
            "playground-frame-times-{timestamp}-{}.csv",
            std::process::id()
        );
        let path = std::env::current_dir()
            .map(|dir| dir.join(&name))
            .unwrap_or_else(|_| name.into());
        let create = || -> io::Result<BufWriter<File>> {
            let file = File::options().write(true).create_new(true).open(&path)?;
            let mut writer = BufWriter::with_capacity(256 * 1024, file);
            writeln!(writer, "{}", header())?;
            writer.flush()?;
            Ok(writer)
        };
        let writer = match create() {
            Ok(writer) => {
                eprintln!("Writing playground frame timings to {}", path.display());
                Some(writer)
            }
            Err(error) => {
                eprintln!(
                    "Cannot create timing log {}: {error}; timing output disabled",
                    path.display()
                );
                None
            }
        };
        Self { writer }
    }
    pub fn record(&mut self, frame: &Frame) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let result = write_frame(writer, frame).and_then(|_| {
            if (frame.index + 1) % 120 == 0 {
                writer.flush()
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            eprintln!("Cannot write timing log: {error}; timing output disabled");
            self.writer = None;
        }
    }
}
impl Drop for Logger {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            if let Err(error) = writer.flush() {
                eprintln!("Cannot flush timing log: {error}");
            }
        }
    }
}
fn transition_name(value: PagesTransition) -> &'static str {
    match value {
        PagesTransition::None => "None",
        PagesTransition::Slide => "Slide",
        PagesTransition::SlideVertical => "Vertical",
    }
}
fn write_frame(writer: &mut impl Write, f: &Frame) -> io::Result<()> {
    write!(
        writer,
        "{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{:.6}",
        f.index,
        f.page,
        f.interval_ms,
        f.example_update_ms,
        f.clear_ms,
        f.app_frame_ms,
        f.next_frame_wait_ms,
        f.page_after,
        transition_name(f.transition_before),
        transition_name(f.transition_after),
        f.bookkeeping_ms
    )?;
    for value in [
        f.mue.transition_ms,
        f.mue.timers_ms,
        f.mue.font_resolve_ms,
        f.mue.layout_ms,
        f.mue.taffy_ms,
        f.mue.layout_sync_ms,
        f.mue.render_ms,
        f.mue.pointer_ms,
        f.mue.wheel_ms,
        f.mue.text_content_ms,
        f.mue.text_measure_ms,
        f.mue.text_render_shape_ms,
        f.mue.text_glyph_draw_ms,
        f.mue.keyed_update_ms,
        f.mue.keyed_build_ms,
        f.mue.keyed_drop_ms,
        f.mue.pages_build_ms,
        f.mue.theme_resolve_ms,
    ] {
        write!(writer, ",{value:.6}")?;
    }
    for value in f.mue.destination_ms {
        write!(writer, ",{value:.6}")?;
    }
    for value in [
        f.backend.begin_frame_ms,
        f.backend.end_frame_ms,
        f.backend.render_passes_ms,
        f.backend.ui_draw_ms,
        f.backend.draw_submit_ms,
        f.backend.commit_ms,
    ] {
        write!(writer, ",{value:.6}")?;
    }
    // Residual includes event-loop/presentation waits and uninstrumented work.
    // It is neither GPU time nor an exact VSync measurement.
    write!(
        writer,
        ",{:.6}",
        f.next_frame_wait_ms - f.backend.begin_frame_ms - f.backend.end_frame_ms
    )?;
    for value in [
        f.mue.layout_pass_count,
        f.mue.measure_call_count,
        f.mue.text_content_update_count,
        f.mue.text_measure_call_count,
        f.mue.text_render_call_count,
        f.mue.glyph_count,
        f.mue.node_mounted_count,
        f.mue.keyed_node_created_count,
        f.mue.keyed_node_removed_count,
        f.mue.pages_created_count,
    ] {
        write!(writer, ",{value}")?;
    }
    let before = f.atlas_before;
    let after = f.atlas_after;
    for value in [
        after.rasterized.saturating_sub(before.rasterized),
        after.evicted.saturating_sub(before.evicted),
        after.uploads.saturating_sub(before.uploads),
        after.uploaded_pixels.saturating_sub(before.uploaded_pixels),
        after
            .rejected_oversized
            .saturating_sub(before.rejected_oversized),
        after
            .rejected_exhausted
            .saturating_sub(before.rejected_exhausted),
        after.rejected_raster.saturating_sub(before.rejected_raster),
        after.pages as u64,
    ] {
        write!(writer, ",{value}")?;
    }
    for (index, value) in [
        f.mue.prepare_ms,
        f.mue.seal_ms,
        f.mue.mount_ms,
        f.mue.transition_bind_ms,
        f.mue.paint_bind_ms,
        f.mue.children_mount_ms,
        f.mue.node_register_ms,
        f.mue.dispose_total_ms,
        f.mue.scope_dispose_ms,
        f.mue.layout_create_ms,
        f.mue.hook_build_ms,
        f.mue.theme_projection_ms,
        f.mue.resource_image_ms,
        f.mue.resource_material_ms,
        f.mue.resource_material_flush_ms,
        f.mue.resource_shape_ms,
    ]
    .into_iter()
    .enumerate()
    {
        write!(
            writer,
            ",{value:.6},{:.6},{}",
            f.mue.detail_outer_ms[index], f.mue.detail_calls[index]
        )?;
    }
    for value in [
        f.backend.scene_update_ms,
        f.backend.projection_ms,
        f.backend.gpu_query_ms,
        f.backend.input_reset_ms,
        f.backend.texture_gc_ms,
        f.backend.touch_cleanup_ms,
    ] {
        write!(writer, ",{value:.6}")?;
    }
    let covered = f.backend.render_passes_ms
        + f.backend.ui_draw_ms
        + f.backend.draw_submit_ms
        + f.backend.commit_ms
        + f.backend.scene_update_ms
        + f.backend.projection_ms
        + f.backend.gpu_query_ms
        + f.backend.input_reset_ms
        + f.backend.texture_gc_ms
        + f.backend.touch_cleanup_ms;
    write!(writer, ",{:.6}", f.backend.end_frame_ms - covered)?;
    for snapshot in [f.app_backend, f.backend] {
        write!(
            writer,
            ",{:.6},{:.6},{},{}",
            snapshot.image_decode_ms,
            snapshot.image_upload_ms,
            snapshot.image_load_count,
            snapshot.image_decoded_bytes
        )?;
    }
    for (index, value) in [
        (16, f.mue.tween_drop_ms),
        (17, f.mue.tween_registry_retain_ms),
    ] {
        write!(
            writer,
            ",{value:.6},{:.6},{}",
            f.mue.detail_outer_ms[index], f.mue.detail_calls[index]
        )?;
    }
    for value in [
        f.mue.tween_drop_count,
        f.mue.tween_registry_entries_scanned,
        f.mue.tween_registry_entries_removed,
        f.mue.tween_registry_retain_skipped_count,
    ] {
        write!(writer, ",{value}")?;
    }
    for index in 0..mue_core::frame_timing::DETAIL_NAMES.len() {
        let s = f.mue.core_cleanup;
        write!(
            writer,
            ",{:.6},{:.6},{}",
            s.detail_total_ms[index], s.detail_outer_ms[index], s.detail_calls[index]
        )?;
    }
    for s in [f.app_backend, f.backend] {
        for (_, value) in backend_times(s) {
            write!(writer, ",{value:.6}")?;
        }
        for (_, value) in backend_counts(s) {
            write!(writer, ",{value}")?;
        }
    }
    writeln!(writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_header_and_sample_align_and_preserve_measurement_semantics() {
        let mut mue = Snapshot::default();
        mue.destination_ms[1] = 42.0;
        mue.node_mounted_count = 17;
        mue.mount_ms = 8.0;
        mue.detail_outer_ms[2] = 5.0;
        mue.detail_calls[2] = 3;
        mue.tween_drop_ms = 4.5;
        mue.detail_outer_ms[16] = 4.0;
        mue.detail_calls[16] = 9;
        mue.tween_registry_entries_scanned = 12345;
        mue.core_cleanup.detail_total_ms[6] = 7.0;
        mue.core_cleanup.detail_outer_ms[6] = 6.0;
        mue.core_cleanup.detail_calls[6] = 11;
        let frame = Frame {
            index: 7,
            page: "Layout",
            page_after: "Typography",
            transition_before: PagesTransition::Slide,
            transition_after: PagesTransition::None,
            interval_ms: 20.0,
            example_update_ms: 0.1,
            clear_ms: 0.01,
            app_frame_ms: 1.0,
            next_frame_wait_ms: 18.0,
            bookkeeping_ms: 0.89,
            mue,
            app_backend: macroquad::frame_timing::Snapshot {
                image_decode_ms: 2.0,
                image_upload_ms: 1.0,
                image_load_count: 2,
                image_decoded_bytes: 4096,
                texture_new_ms: 1.25,
                quad_draw_calls: 19,
                quad_draw_buffer_upload_bytes: 4321,
                ..Default::default()
            },
            backend: macroquad::frame_timing::Snapshot {
                begin_frame_ms: 1.0,
                end_frame_ms: 3.0,
                texture_gc_ms: 1.5,
                gpu_query_ms: 0.5,
                texture_new_ms: 2.25,
                scene_camera_set_ms: 0.75,
                quad_draw_calls: 23,
                ..Default::default()
            },
            atlas_before: GlyphAtlasStats {
                uploads: 5,
                ..Default::default()
            },
            atlas_after: GlyphAtlasStats {
                uploads: 8,
                pages: 2,
                ..Default::default()
            },
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &frame).unwrap();
        let output = String::from_utf8(bytes).unwrap();
        let full_header = header();
        let columns: Vec<_> = full_header.split(',').collect();
        let cells: Vec<_> = output.trim_end().split(',').collect();
        assert_eq!(columns.len(), cells.len());
        let get = |name| cells[columns.iter().position(|field| *field == name).unwrap()];
        assert_eq!(get("page_after"), "Typography");
        assert_eq!(get("transition_after"), "None");
        assert_eq!(get("destination_typography_ms"), "42.000000");
        assert_eq!(get("next_frame_unattributed_ms"), "14.000000");
        assert_eq!(get("atlas_uploads"), "3");
        assert_eq!(get("node_mounted_count"), "17");
        assert_eq!(HEADER.split(',').count(), 127);
        assert_eq!(columns.len(), 235);
        assert_eq!(
            columns
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            columns.len()
        );
        assert_eq!(get("detail_mount_ms"), "8.000000");
        assert_eq!(get("detail_mount_outer_ms"), "5.000000");
        assert_eq!(get("detail_mount_calls"), "3");
        assert_eq!(get("backend_texture_gc_ms"), "1.500000");
        assert_eq!(get("backend_end_unattributed_ms"), "1.000000");
        assert_eq!(get("app_image_decode_ms"), "2.000000");
        assert_eq!(get("app_image_load_count"), "2");
        assert_eq!(get("app_image_decoded_bytes"), "4096");
        assert_eq!(get("backend_image_load_count"), "0");
        assert_eq!(get("detail_tween_drop_ms"), "4.500000");
        assert_eq!(get("detail_tween_drop_outer_ms"), "4.000000");
        assert_eq!(get("detail_tween_drop_calls"), "9");
        assert_eq!(get("tween_registry_entries_scanned"), "12345");
        assert_eq!(get("core_effect_destroy_ms"), "7.000000");
        assert_eq!(get("core_effect_destroy_outer_ms"), "6.000000");
        assert_eq!(get("core_effect_destroy_calls"), "11");
        assert_eq!(get("app_texture_new_ms"), "1.250000");
        assert_eq!(get("backend_texture_new_ms"), "2.250000");
        assert_eq!(get("backend_scene_camera_set_ms"), "0.750000");
        assert_eq!(get("app_quad_draw_calls"), "19");
        assert_eq!(get("backend_quad_draw_calls"), "23");
        assert_eq!(get("app_quad_draw_buffer_upload_bytes"), "4321");
    }
}
