# AGENTS.md

Run all of these after every refactor. Check benchmark results in `target/criterion/` for performance regressions.

## Commands

```bash
cargo fmt                                   # Format Rust code
cargo clippy --all-features -- -W clippy::pedantic   # Lint with pedantic warnings
cargo test                                  # Run all tests
scripts/bench.sh                            # Run all benchmarks (noise-controlled — see Benchmarks)
uvx ruff format                             # Format Python code (scripts/, tests/)
pre-commit run --all-files                  # Run formatters on the whole tree
```

One-time setup (after cloning):

```bash
uv tool install pre-commit
pre-commit install
```

## Benchmarks

Criterion suite comparing `ElasticHashMap`, `FunnelHashMap`, `std::HashMap`, `hashbrown::HashMap` (SwissTable + foldhash — absolute ceiling).

**Always use `scripts/bench.sh` for results you'll act on.** Raw `cargo bench` is unpinned — wall-clock noise can swing ±10% and flip the sign of real ±5% changes. Use raw cargo only for smoke runs / single-filter iteration.

```bash
scripts/bench.sh                            # baseline → "ref"
# … apply change …
BASELINE=ref scripts/bench.sh               # compare vs ref
```

- Wraps `cargo bench` with `taskset -c $CORE` + `setarch -R` (no privileges). `sudo` adds governor=performance, turbo off, SCHED_FIFO/99; drops back to `$SUDO_USER` for cargo.
- `BENCH=all` (default) runs `speedup` then `latency`; set `BENCH=speedup|latency` for single-target.
- Re-pin `ref` when env changes (sudo vs not, core pin) — baselines are wall-clock.
- Pass through flags: `BASELINE=ref scripts/bench.sh -- --measurement-time 10`. Criterion name filter: `scripts/bench.sh -- "get_hit_latency"`.
- `latency` bench writes histograms to `target/latency/` and ignores `--baseline`.

**Read results from JSON, not stdout** (stdout truncates + mixes runs):

- `target/criterion/<group>/<variant>/new/estimates.json` — absolute ns (`mean.point_estimate`)
- `target/criterion/<group>/<variant>/change/estimates.json` — fractional change vs prev (e.g. +0.05 = 5% slower)

Example: `target/criterion/get_hit_throughput/elastic/change/estimates.json`

### Tail-latency harness

- **`cargo bench --bench latency`** — HDR get-hit latency distribution (p50…p99999/max), fixed config: 10M × 4 maps × 1M samples × 10K warmup.
- Output: `target/latency/<map>/<size>/<op>.json` — percentiles + histogram buckets + `clock_overhead_ns`.

### Python-side benchmarks

`benches/python/throughput.py` — pytest-benchmark suite comparing `dict`, `ElasticHashMap`, and `FunnelHashMap` from Python across insert / get_hit / get_miss / mixed / delete workloads at N = 10K. Each opthash op crosses the GIL → `HashedAny::hash()` → Python bytecode.

```bash
pytest benches/python/throughput.py --benchmark-json=.benchmarks/python.json

uv run --group charts python scripts/generate_python_chart.py
```

### Charts

- `uv run --group charts scripts/generate_speedup_chart.py` — throughput speedup bar chart
- `uv run --group charts scripts/generate_latency_chart.py` — Criterion mean-latency line (`target/criterion/get_hit_latency_<size>`; sizes from `LATENCY_SIZES` in `benches/common.rs`) + HDR get-hit tail CDF @ 10M (`target/latency/`).
- `uv run --group charts scripts/generate_all_charts.py` — regenerate everything
- `uv run --group charts scripts/generate_python_chart.py` — Python-side dict-vs-opthash speedup (reads `.benchmarks/python.json`)

Charts are saved in `assets/`. Shared plotting helpers (`IMPLEMENTATIONS`, loaders, axis styling) live in `scripts/plot_common.py`. The tail plotter subtracts `clock_overhead_ns` so percentiles reflect per-op latency, not per-(op + `Instant::now()`).

## Project structure

- `src/elastic.rs` — `ElasticHashMap` (tests inline)
- `src/funnel.rs` — `FunnelHashMap` (tests inline)
- `src/common/` — shared internals: control-byte SIMD ops, layout math, config

## Worktree naming

When spawning a worktree, name its branch after the work (e.g. `feat/std-parity-mut-iters`) and pass the same name to `git worktree add`.

## Refactoring guidelines

- Low-level helpers used by both the library and benchmarks live in `src/common/` (bitmask, simd, layout, math). Benches pull fixtures from `benches/common.rs`. Don't duplicate primitives across `src/` and `benches/`.
- Prefer layout and locality wins before adding more metadata.
- Keep hot metadata contiguous. If fields are read together, store them together.
- Avoid metadata that is expensive to maintain on every insert or delete unless benchmarks prove it wins overall.
- Cache routing state that is reused in hot paths. Do not recompute it per probe.
- Preserve SIMD-friendly control-byte scans: contiguous groups, cheap bitmask iteration, and early rejection before touching payloads.
- Reject optimizations that improve only microbenchmarks but regress the public `throughput` suite.
- Profile hot functions before and after changes. In this repo, focus on `find_slot_indices_with_hash` / `find_in_level_by_probe` (elastic), `find_slot_location_with_hash` / `find_in_level_bucket` (funnel), `group_probe_params`, `choose_slot_for_new_key`, and the resize paths.
- Use `target/criterion/` as the final gate. If the relevant benchmark regresses, the optimization does not stay.
