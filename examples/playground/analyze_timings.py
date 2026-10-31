#!/usr/bin/env python3
"""Offline Markdown statistics for playground-frame-times CSVs (stdlib only).

Examples:
  python3 examples/playground/analyze_timings.py playground-frame-times-*.csv
  python3 examples/playground/analyze_timings.py run.csv --output report.md
"""

import argparse
import csv
import math
import statistics
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from pathlib import Path


LABELS = {"page", "page_after", "transition_before", "transition_after"}
REQUIRED = {"frame", "page", "frame_interval_ms"}


@dataclass
class Sample:
    frame: int
    line: int
    labels: dict
    values: dict
    category: str = ""

    @property
    def before(self):
        return (self.labels["page"], self.labels.get("transition_before", "(unavailable)"))

    @property
    def after(self):
        return (self.labels.get("page_after", self.labels["page"]),
                self.labels.get("transition_after", self.before[1]))

    @property
    def changes(self):
        changes = []
        if self.before[0] != self.after[0]:
            changes.append("page-change")
        if self.before[1] != self.after[1]:
            changes.append("mode-change")
        return changes


@dataclass
class Capture:
    path: Path
    metrics: list
    samples: list
    gaps: list
    excluded: list


def percentile(ordered, percent):
    """Linear interpolation at (n-1)*p; input must be sorted."""
    if not ordered:
        return None
    position = (len(ordered) - 1) * percent / 100
    low = math.floor(position)
    high = math.ceil(position)
    weight = position - low
    return ordered[low] * (1 - weight) + ordered[high] * weight


def statistics_for(samples, metric):
    ordered = sorted(s.values[metric] for s in samples)
    if not ordered:
        return (0,) + (None,) * 6
    return (len(ordered), statistics.fmean(ordered), percentile(ordered, 50),
            percentile(ordered, 95), percentile(ordered, 99), ordered[-1], sum(ordered))


def read_capture(path):
    """Read by column name. Only an actually short final record is excluded."""
    path = Path(path)
    samples, gaps, excluded = [], [], []
    with path.open(newline="", encoding="utf-8-sig") as stream:
        reader = csv.DictReader(stream, strict=True)
        try:
            header = reader.fieldnames
            if not header:
                raise ValueError("missing CSV header")
            if any(not name.strip() or name != name.strip() for name in header):
                raise ValueError("empty or whitespace-padded column name")
            if len(set(header)) != len(header):
                raise ValueError("duplicate column name")
            missing = REQUIRED - set(header)
            if missing:
                raise ValueError("missing required columns: " + ", ".join(sorted(missing)))
            # Every non-label field is numeric, including future appended metrics.
            metrics = [name for name in header if name not in LABELS and name != "frame"]
            previous = None
            pending_short = None
            for raw in reader:
                line = reader.line_num
                if pending_short is not None:
                    raise ValueError(f"line {pending_short}: short row before end of file")
                if None in raw:
                    raise ValueError(f"line {line}: expected {len(header)} cells, got extra cells")
                if any(value is None for value in raw.values()):
                    pending_short = line
                    continue
                try:
                    frame_decimal = Decimal(raw["frame"])
                except InvalidOperation as exc:
                    raise ValueError(f"line {line}: invalid frame {raw['frame']!r}") from exc
                if not frame_decimal.is_finite() or frame_decimal != frame_decimal.to_integral_value():
                    raise ValueError(f"line {line}: frame must be a finite integer")
                frame = int(frame_decimal)
                if frame < 0:
                    raise ValueError(f"line {line}: frame must be non-negative")
                if previous is not None:
                    if frame <= previous:
                        raise ValueError(f"line {line}: duplicate/decreasing frame {frame} after {previous}")
                    if frame != previous + 1:
                        gaps.append((previous + 1, frame - 1))
                values = {}
                for name in metrics:
                    try:
                        value = float(raw[name])
                    except ValueError as exc:
                        raise ValueError(f"line {line}: {name} is not numeric: {raw[name]!r}") from exc
                    if not math.isfinite(value):
                        raise ValueError(f"line {line}: {name} must be finite")
                    values[name] = value
                labels = {name: raw[name] for name in LABELS if name in raw}
                if any(not value.strip() for value in labels.values()):
                    raise ValueError(f"line {line}: empty page/transition label")
                samples.append(Sample(frame, line, labels, values))
                previous = frame
            if pending_short is not None:
                excluded.append(f"partial final row at line {pending_short} (too few cells)")
        except (ValueError, csv.Error) as exc:
            raise ValueError(f"{path}: {exc}") from exc
    return Capture(path, metrics, samples, gaps, excluded)


def group_samples(capture, settle_frames, warmup_frames):
    """Classify causally: never exclude a sample because of a later event."""
    groups = defaultdict(list)
    categories = defaultdict(list)
    last_change = None
    previous_after = None
    for sample in capture.samples:
        # Changes between records are also boundaries (e.g. a gap in recording).
        discontinuity = previous_after is not None and sample.before != previous_after
        if sample.changes or discontinuity:
            last_change = sample.frame
        if sample.changes:
            sample.category = "+".join(sample.changes)
            for kind in sample.changes:
                categories[kind].append(sample)
        elif sample.frame < warmup_frames:
            sample.category = "initialization"
        elif last_change is not None and sample.frame - last_change < settle_frames:
            sample.category = "settling"
        else:
            sample.category = "settled"
            groups[sample.before].append(sample)
        categories["all"].append(sample)
        if not sample.changes:
            categories[sample.category].append(sample)
        previous_after = sample.after
    return groups, categories


def cell(value):
    return str(value).replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def number(value):
    if value is None:
        return "—"
    return f"{value:.6g}"


def table(headers, rows):
    lines = ["| " + " | ".join(headers) + " |", "| " + " | ".join("---" for _ in headers) + " |"]
    lines.extend("| " + " | ".join(cell(v) for v in row) + " |" for row in rows)
    return "\n".join(lines)


def metric_kind(name):
    if name.endswith("_ms"):
        return "ms"
    if name == "atlas_pages":
        return "gauge (pages)"
    if name.startswith("atlas_"):
        return "counter delta"
    if name.endswith("_bytes"):
        return "bytes"
    if "quad_draw_" in name or name.startswith("tween_registry_entries_"):
        return "count"
    if name.endswith(("_count", "_calls")):
        return "count"
    return "numeric"


def metric_table(samples, metrics):
    return table(["Metric", "Kind", "n", "Mean", "p50", "p95", "p99", "Max", "Total"],
                 ([name, metric_kind(name)] + [number(v) for v in statistics_for(samples, name)]
                  for name in metrics))


def top_times(sample, metrics):
    # Prefer same-stage outer time in rankings; keep inclusive sums in tables.
    available = set(metrics)
    times = sorted((name for name in metrics if name.endswith("_ms") and
                    (name[:-3] + "_outer_ms") not in available),
                   key=lambda name: (-sample.values[name], name))
    return "; ".join(f"{name}={number(sample.values[name])}" for name in times[:8])


def keyed_nodes(sample, metrics):
    names = [name for name in metrics if name.startswith("keyed_") or
             name in ("node_mounted_count", "pages_created_count")]
    return "; ".join(f"{name}={number(sample.values[name])}" for name in names) or "unavailable"


def hotspot_metrics(metrics):
    prefixes = ("image_", "texture_", "scene_node_", "scene_camera_", "scene_bookkeeping_",
                "quad_draw_", "scope_", "cleanup_", "signal_", "effect_", "tween_")
    exact = {"texture_from_rgba8_ms", "texture_readback_ms", "texture_readback_alloc_ms",
             "texture_readback_gpu_ms", "texture_atlas_build_ms", "texture_atlas_cache_ms",
             "tween_drop_ms", "tween_registry_retain_ms", "backend_scene_update_ms",
             "resource_image_ms", "detail_resource_image_outer_ms"}
    def base(name):
        for prefix in ("app_", "backend_", "core_", "detail_"):
            if name.startswith(prefix):
                return name[len(prefix):]
        return name
    return [name for name in metrics if name in exact or base(name).startswith(prefixes)]


def slow_distribution(samples, threshold):
    # Exact per-frame interval bins; no exclusion from settled groups.
    boundaries = sorted({0.0, threshold, 100.0, 250.0, 500.0, 1000.0})
    counts = Counter()
    for sample in samples:
        value = sample.values["frame_interval_ms"]
        bucket = next((i for i, end in enumerate(boundaries) if value < end), len(boundaries))
        counts[bucket] += 1
    result = []
    for i in range(len(boundaries) + 1):
        low = "-infinity" if i == 0 else number(boundaries[i - 1])
        high = "infinity" if i == len(boundaries) else number(boundaries[i])
        result.append([f"[{low}, {high})", counts[i]])
    return result


def render_capture(capture, settle_frames, warmup_frames, slow_ms):
    groups, categories = group_samples(capture, settle_frames, warmup_frames)
    slow = sorted((s for s in capture.samples if s.values["frame_interval_ms"] >= slow_ms),
                  key=lambda s: (-s.values["frame_interval_ms"], s.frame))
    parts = [f"## File: {cell(capture.path)}", "",
             f"Valid rows: {len(capture.samples)}; numeric measurement columns: {len(capture.metrics)}.", "",
             "Excluded: " + ("; ".join(capture.excluded) or "none") + ".", "",
             "Missing frame indices: " + (", ".join(f"{a}–{b}" for a, b in capture.gaps) or "none") + ".", "",
             "### Row populations", "",
             table(["Population", "Rows"], [[name, len(categories[name])] for name in
                   ["initialization", "page-change", "mode-change", "settling", "settled"]]), "",
             "A row changing both page and mode appears in both event populations; other populations are disjoint.", "",
             "### Settled page/transition comparison", ""]
    comparison = []
    for (page, mode), samples in sorted(groups.items()):
        stat = statistics_for(samples, "frame_interval_ms")
        comparison.append([page, mode, stat[0], sum(s.values["frame_interval_ms"] >= slow_ms for s in samples)] +
                          [number(v) for v in stat[1:]])
    parts += [table(["Page", "Transition", "n", "Slow frames", "Mean ms", "p50 ms", "p95 ms", "p99 ms", "Max ms", "Total ms"], comparison), ""]
    for (page, mode), samples in sorted(groups.items()):
        parts += [f"#### Settled: {cell(page)} / {cell(mode)}", "", metric_table(samples, capture.metrics), ""]
    selected = hotspot_metrics(capture.metrics)
    parts += ["### Hotspot drill-down", "",
              "Image/render/cleanup/tween spans remain nested. Missing optional columns are unavailable, not zero.", ""]
    if selected:
        populations = [("mode-change", categories["mode-change"])]
        populations += [(f"settled: {page} / {mode}", samples)
                        for (page, mode), samples in sorted(groups.items()) if page in ("Layout", "Typography")]
        for name, samples in populations:
            parts += [f"#### {cell(name)}", "", metric_table(samples, selected), ""]
    else:
        parts += ["No image/render/cleanup/tween detail columns are present in this capture.", ""]
    for kind in ("initialization", "page-change", "mode-change", "settling"):
        parts += [f"### {kind.capitalize()} statistics", "", metric_table(categories[kind], capture.metrics), ""]
    parts += ["### Event details (all page and mode changes)", "",
              table(["Frame", "Line", "Kind", "Page before", "Page after", "Mode before", "Mode after", "Top time metrics (ms; inclusive)", "Keyed/node metrics"],
                    ([s.frame, s.line, s.category, *[s.before[0], s.after[0], s.before[1], s.after[1]],
                      top_times(s, capture.metrics), keyed_nodes(s, capture.metrics)]
                     for s in capture.samples if s.changes)), "",
              f"### Slow frames (frame_interval_ms >= {number(slow_ms)} ms)", "",
              f"{len(slow)} / {len(capture.samples)} valid frames. Ranked by frame interval; all slow frames are listed.", "",
              table(["Rank", "Frame", "Line", "Population", "Page before → after", "Mode before → after", "Interval ms", "Top time metrics (ms; inclusive)", "Keyed/node metrics"],
                    ([i, s.frame, s.line, s.category, s.before[0] + " → " + s.after[0],
                      s.before[1] + " → " + s.after[1], number(s.values["frame_interval_ms"]),
                      top_times(s, capture.metrics), keyed_nodes(s, capture.metrics)] for i, s in enumerate(slow, 1))), "",
              "### Frame interval distribution", "",
              table(["Interval bin (ms)", "Frames"], slow_distribution(capture.samples, slow_ms)), "",
              "### All valid rows (including events and initialization)", "", metric_table(capture.samples, capture.metrics), ""]
    return "\n".join(parts)


def nonnegative_int(text):
    value = int(text)
    if value < 0:
        raise argparse.ArgumentTypeError("must be non-negative")
    return value


def nonnegative_float(text):
    value = float(text)
    if not math.isfinite(value) or value < 0:
        raise argparse.ArgumentTypeError("must be finite and non-negative")
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("files", nargs="+", type=Path, help="one or more timing CSV files")
    parser.add_argument("--output", type=Path, help="Markdown output path (default: stdout)")
    parser.add_argument("--force", action="store_true", help="allow overwriting --output (never an input file)")
    parser.add_argument("--settle-frames", type=nonnegative_int, default=35)
    parser.add_argument("--warmup-frames", type=nonnegative_int, default=6)
    parser.add_argument("--slow-ms", type=nonnegative_float, default=50)
    args = parser.parse_args(argv)
    try:
        if args.output and any(args.output.resolve() == path.resolve() or
                               (args.output.exists() and path.exists() and args.output.samefile(path))
                               for path in args.files):
            raise ValueError("output must not replace an input CSV")
        captures = [read_capture(path) for path in args.files]
        intro = """# Playground frame timing statistics

Percentiles use linear interpolation at (n−1)×p, including single-sample groups.
Warmup excludes frame indices below {warmup}; initialization is independent of measurement duration.
Settled groups require at least {settle} frame-index steps since the most recent page/mode change.
A change row itself is always an event; later changes never remove earlier samples.
Gaps are reported, not filled. Slow means frame_interval_ms >= {slow} ms; slow frames remain in settled groups.

Metric totals sum one column over samples, never sum columns/stages. Times are CPU wall-clock spans,
not GPU time. Inclusive relationships: app_frame_ms encloses application stages; layout_ms includes
Taffy/measure work and synchronization; text_measure_ms includes shaping. render_ms includes text render
and glyph draw work. keyed_update_ms includes keyed_build_ms/keyed_drop_ms; recursive keyed work and
Pages/destination creation can overlap. next_frame_wait_ms includes backend begin/end; backend_end_frame_ms
includes render passes, UI draw, draw submission and commit. next_frame_unattributed_ms is a residual,
not GPU/VSync time. detail_*_ms is nested instrumentation; detail_*_outer_ms is an enclosing wall-clock
span and detail_*_calls counts invocations. Do not add detail spans to their enclosing parent stages.
Counter/call columns are distinct from milliseconds; atlas_* are per-row deltas except atlas_pages,
a gauge whose total is merely the sum of sampled page counts. Missing optional columns mean unavailable,
not zero; present all-zero columns remain reported (including feature-disabled captures).
""".format(warmup=args.warmup_frames, settle=args.settle_frames, slow=number(args.slow_ms))
        output = intro + "\n" + "\n".join(render_capture(capture, args.settle_frames, args.warmup_frames, args.slow_ms)
                                              for capture in captures)
        if args.output:
            with args.output.open("w" if args.force else "x", encoding="utf-8", newline="\n") as stream:
                stream.write(output)
        else:
            sys.stdout.write(output)
    except (OSError, ValueError, UnicodeError) as exc:
        print(f"analyze_timings: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
