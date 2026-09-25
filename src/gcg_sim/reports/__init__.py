"""Benchmark reports: results.json (versioned JSON Schema), summary.md, games.ndjson,
replays, and timing.json."""

from gcg_sim.reports.results import build_results, load_schema, validate_results
from gcg_sim.reports.stats import wilson
from gcg_sim.reports.writer import ReportPaths, write_reports

__all__ = [
    "ReportPaths",
    "build_results",
    "load_schema",
    "validate_results",
    "wilson",
    "write_reports",
]
