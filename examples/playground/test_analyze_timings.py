"""Synthetic tests; no captured logs, build steps or third-party dependencies."""

import contextlib
import csv
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("analyze_timings.py")
spec = importlib.util.spec_from_file_location("analyze_timings", SCRIPT)
analyzer = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = analyzer
spec.loader.exec_module(analyzer)

OLD_HEADER = "frame,page,frame_interval_ms,example_update_ms,clear_ms,app_frame_ms,next_frame_wait_ms,page_after,transition_before,transition_after,bookkeeping_ms,transition_ms,timers_ms,font_resolve_ms,layout_ms,taffy_ms,layout_sync_ms,render_ms,pointer_ms,wheel_ms,text_content_ms,text_measure_ms,text_render_shape_ms,text_glyph_draw_ms,keyed_update_ms,keyed_build_ms,keyed_drop_ms,pages_build_ms,theme_resolve_ms,destination_layout_ms,destination_typography_ms,destination_graphics_ms,destination_motion_ms,destination_scrolling_ms,destination_input_ms,destination_map_gestures_ms,destination_theme_ms,destination_style_declarations_ms,destination_font_licenses_ms,backend_begin_frame_ms,backend_end_frame_ms,backend_render_passes_ms,backend_ui_draw_ms,backend_draw_submit_ms,backend_commit_ms,next_frame_unattributed_ms,layout_pass_count,measure_call_count,text_content_update_count,text_measure_call_count,text_render_call_count,glyph_count,node_mounted_count,keyed_node_created_count,keyed_node_removed_count,pages_created_count,atlas_rasterized,atlas_evicted,atlas_uploads,atlas_uploaded_pixels,atlas_rejected_oversized,atlas_rejected_exhausted,atlas_rejected_raster,atlas_pages".split(",")


class AnalyzerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def make_csv(self, rows, header=None, name="run.csv"):
        header = header or OLD_HEADER
        path = self.root / name
        with path.open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, header)
            writer.writeheader()
            for overrides in rows:
                row = dict.fromkeys(header, 0)
                row.update(page="Layout", page_after="Layout", transition_before="None", transition_after="None", frame_interval_ms=16)
                row = {key: value for key, value in row.items() if key in header}
                row.update(overrides)
                writer.writerow(row)
        return path

    def test_grouping_retains_slow_and_does_not_look_ahead(self):
        path = self.make_csv([
            {"frame": i, "frame_interval_ms": 90 if i == 2 else 16} for i in range(4)
        ] + [
            {"frame": 4, "page_after": "Typography"},
            {"frame": 5, "page": "Typography", "page_after": "Typography"},
            {"frame": 6, "page": "Typography", "page_after": "Typography", "frame_interval_ms": 150},
            {"frame": 7, "page": "Typography", "page_after": "Typography", "transition_after": "Slide"},
            {"frame": 8, "page": "Typography", "page_after": "Typography", "transition_before": "Slide", "transition_after": "Slide"},
            {"frame": 9, "page": "Typography", "page_after": "Typography", "transition_before": "Slide", "transition_after": "Slide"},
        ])
        data = analyzer.read_capture(path)
        groups, kinds = analyzer.group_samples(data, 2, 1)
        self.assertEqual([s.frame for s in groups[("Layout", "None")]], [1, 2, 3])
        self.assertEqual([s.frame for s in groups[("Typography", "None")]], [6])
        self.assertEqual([s.frame for s in groups[("Typography", "Slide")]], [9])
        self.assertEqual(len(kinds["page-change"]), 1)
        self.assertEqual(len(kinds["mode-change"]), 1)
        self.assertEqual(len(kinds["initialization"]), 1)
        report = analyzer.render_capture(data, 2, 1, 50)
        self.assertIn("2 / 10 valid frames", report)
        self.assertIn("150", report)

    def test_percentiles(self):
        self.assertEqual(analyzer.percentile([], 50), None)
        self.assertEqual(analyzer.percentile([7], 99), 7)
        self.assertEqual(analyzer.percentile([0, 10], 50), 5)
        self.assertAlmostEqual(analyzer.percentile([0, 10], 95), 9.5)
        self.assertAlmostEqual(analyzer.percentile([0, 10], 99), 9.9)

    def test_old_schema_and_appended_fields(self):
        self.assertEqual(len(OLD_HEADER), 64)
        old = analyzer.read_capture(self.make_csv([{"frame": 0}]))
        self.assertNotIn("detail_work_outer_ms", old.metrics)
        header = list(reversed(OLD_HEADER + ["detail_work_ms", "detail_work_outer_ms", "detail_work_calls", "backend_future_ms"]))
        path = self.make_csv([{"frame": 0, "detail_work_outer_ms": 12, "detail_work_calls": 3}], header, "new.csv")
        data = analyzer.read_capture(path)
        self.assertEqual(data.samples[0].values["detail_work_calls"], 3)
        report = analyzer.render_capture(data, 0, 0, 50)
        self.assertIn("detail_work_outer_ms | ms | 1", report)
        self.assertIn("detail_work_calls | count | 1", report)
        self.assertIn("backend_future_ms", report)
        self.assertIn("Hotspot drill-down", report)
        self.assertIn("No image/render/cleanup/tween detail columns", report)

    def test_hotspot_drilldown_uses_available_columns_and_populations(self):
        header = OLD_HEADER + ["texture_from_rgba8_ms", "scene_node_draw_ms", "scope_children_ms", "tween_drop_count"]
        path = self.make_csv([
            {"frame": 0, "page": "Graphics", "page_after": "Graphics", "transition_before": "None", "transition_after": "Slide", "texture_from_rgba8_ms": 4, "tween_drop_count": 1},
            {"frame": 1, "page": "Graphics", "page_after": "Graphics", "transition_before": "Slide", "transition_after": "Slide", "texture_from_rgba8_ms": 0, "tween_drop_count": 0},
            {"frame": 2, "page": "Typography", "page_after": "Typography", "transition_before": "Slide", "transition_after": "Slide", "scene_node_draw_ms": 5, "scope_children_ms": 2},
        ], header, "hotspots.csv")
        capture = analyzer.read_capture(path)
        report = analyzer.render_capture(capture, 0, 0, 50)
        self.assertIn("#### mode-change", report)
        self.assertIn("settled: Typography / Slide", report)
        self.assertIn("texture_from_rgba8_ms | ms | 1 | 4", report)
        self.assertIn("scene_node_draw_ms | ms | 1 | 5", report)
        self.assertIn("scope_children_ms | ms | 1 | 2", report)

    def test_no_feature_zero_metrics_and_header_only(self):
        capture = analyzer.read_capture(self.make_csv([{"frame": 0}]))
        self.assertEqual(analyzer.statistics_for(capture.samples, "layout_ms"), (1, 0, 0, 0, 0, 0, 0))
        capture = analyzer.read_capture(self.make_csv([], name="empty.csv"))
        self.assertEqual(capture.samples, [])
        self.assertIn("Valid rows: 0", analyzer.render_capture(capture, 35, 6, 50))

    def test_partial_tail_and_malformed_interior(self):
        path = self.make_csv([{"frame": 0}])
        with path.open("a") as stream:
            stream.write("1,Layout,16")
        capture = analyzer.read_capture(path)
        self.assertEqual(len(capture.samples), 1)
        self.assertIn("partial final row", capture.excluded[0])
        with path.open("a") as stream:
            stream.write("\n2,Layout\n")
        with self.assertRaisesRegex(ValueError, "short row before end"):
            analyzer.read_capture(path)

    def test_extra_cell_rejected_even_at_end(self):
        path = self.make_csv([{"frame": 0}])
        with path.open("a") as stream:
            stream.write(",".join(["0"] * 65) + "\n")
        with self.assertRaisesRegex(ValueError, "extra cells"):
            analyzer.read_capture(path)

    def test_bad_numeric_and_frame_values(self):
        for column, value in [("layout_ms", "nan"), ("backend_end_frame_ms", "inf"), ("atlas_pages", "bad"), ("frame", "1.5")]:
            with self.subTest(column=column, value=value):
                path = self.make_csv([{column: value, **({"frame": 0} if column != "frame" else {})}])
                with self.assertRaises(ValueError):
                    analyzer.read_capture(path)
        for frames in ([1, 1], [2, 1]):
            path = self.make_csv([{"frame": frame} for frame in frames])
            with self.assertRaisesRegex(ValueError, "duplicate/decreasing"):
                analyzer.read_capture(path)
        capture = analyzer.read_capture(self.make_csv([{"frame": 1}, {"frame": 4}]))
        self.assertEqual(capture.gaps, [(2, 3)])

    def test_duplicate_and_missing_header(self):
        path = self.root / "bad.csv"
        for text in ("frame,page,frame_interval_ms,frame\n", "frame,page\n", ""):
            path.write_text(text)
            with self.assertRaises(ValueError):
                analyzer.read_capture(path)

    def test_cli_output_exclusive_and_source_preserved(self):
        path = self.make_csv([{"frame": 0}])
        before = path.read_bytes()
        output = self.root / "report.md"
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(analyzer.main([str(path), "--output", str(output)]), 0)
            first = output.read_bytes()
            self.assertEqual(analyzer.main([str(path), "--output", str(output)]), 2)
            self.assertEqual(output.read_bytes(), first)
            self.assertEqual(analyzer.main([str(path), "--output", str(output), "--force"]), 0)
            self.assertEqual(analyzer.main([str(path), "--output", str(path), "--force"]), 2)
        self.assertEqual(path.read_bytes(), before)

    def test_multiple_files_stdout(self):
        first = self.make_csv([], name="first.csv")
        second = self.make_csv([], name="second.csv")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(analyzer.main([str(first), str(second)]), 0)
        self.assertEqual(out.getvalue().count("## File:"), 2)


if __name__ == "__main__":
    unittest.main()
