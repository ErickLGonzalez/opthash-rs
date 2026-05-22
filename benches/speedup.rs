mod common;

use std::collections::HashMap as StdHashMap;
use std::hint::black_box;
use std::path::Path;
use std::time::Duration;

use common::{
    LATENCY_SIZES, VALUE_XOR_MIX_ALT, build_elastic_map, build_funnel_map, build_hashbrown_map,
    build_std_map, key_at, make_pairs, size_label,
};
use criterion::{
    BatchSize, Criterion, Throughput, criterion_group, criterion_main, profiler::Profiler,
};
use hashbrown::HashMap as HashbrownMap;
use opthash::{ElasticHashMap, FunnelHashMap};
use pprof::{ProfilerGuard, flamegraph::Options as FlamegraphOptions};

struct FlamegraphProfiler {
    frequency: i32,
    active: Option<ProfilerGuard<'static>>,
}

impl FlamegraphProfiler {
    fn new() -> Self {
        Self {
            frequency: 997,
            active: None,
        }
    }
}

impl Profiler for FlamegraphProfiler {
    fn start_profiling(&mut self, _benchmark_id: &str, _benchmark_dir: &Path) {
        self.active = Some(ProfilerGuard::new(self.frequency).unwrap());
    }

    fn stop_profiling(&mut self, _benchmark_id: &str, benchmark_dir: &Path) {
        if let Some(guard) = self.active.take() {
            let report = guard.report().build().unwrap();
            let mut opts = FlamegraphOptions::default();
            opts.deterministic = true;
            std::fs::create_dir_all(benchmark_dir).unwrap();
            let path = benchmark_dir.join("flamegraph.svg");
            let file = std::fs::File::create(&path).unwrap();
            report.flamegraph_with_options(file, &mut opts).unwrap();
        }
    }
}

/// Items inserted per iteration of `insert_throughput`.
const INSERT_COUNT: usize = 10_000;
/// Pre-populated map size for the lookup throughput suite.
const LOOKUP_MAP_SIZE: usize = 20_000;
/// `get_hit_throughput` lookups per iteration.
const HIT_LOOKUP_COUNT: usize = 100_000;
/// `get_miss_throughput` lookups per iteration.
const MISS_LOOKUP_COUNT: usize = 20_000;
/// Map size for `tiny_lookup_throughput` — fits comfortably in L1.
const TINY_MAP_SIZE: usize = 32;
/// `tiny_lookup_throughput` lookups per iteration.
const TINY_LOOKUP_COUNT: usize = 20_000;
/// Pre-populated map size for `delete_heavy_throughput`.
const DELETE_MAP_SIZE: usize = 12_000;
/// Delete + reinsert ops per iteration of `delete_heavy_throughput`.
const DELETE_OP_COUNT: usize = 6_000;
/// Inserts per iteration of `resize_heavy_throughput`; triggers multiple resizes.
const RESIZE_INSERT_COUNT: usize = 8_000;
/// Pre-populated map size for `mixed_throughput`.
const MIXED_MAP_SIZE: usize = 20_000;
/// Mixed-op (insert/get/delete) count per iteration of `mixed_throughput`.
const MIXED_OP_COUNT: usize = 100_000;

/// Emits per-impl `bench_function` blocks
macro_rules! bench_all_impls {
    ($group:expr, $batch:expr, $std_setup:expr, $hb_setup:expr, $el_setup:expr, $fn_setup:expr, $body:expr $(,)?) => {{
        let group = &mut $group;
        group.bench_function("std", |b| b.iter_batched_ref($std_setup, $body, $batch));
        group.bench_function("hashbrown", |b| {
            b.iter_batched_ref($hb_setup, $body, $batch)
        });
        group.bench_function("elastic", |b| b.iter_batched_ref($el_setup, $body, $batch));
        group.bench_function("funnel", |b| b.iter_batched_ref($fn_setup, $body, $batch));
    }};
}

fn bench_insert_throughput(c: &mut Criterion) {
    let pairs = make_pairs(INSERT_COUNT);
    let mut group = c.benchmark_group("insert_throughput");
    group.throughput(Throughput::Elements(INSERT_COUNT as u64));

    bench_all_impls!(
        group,
        BatchSize::PerIteration,
        || StdHashMap::<u64, u64>::with_capacity(INSERT_COUNT * 2),
        || HashbrownMap::<u64, u64>::with_capacity(INSERT_COUNT * 2),
        || ElasticHashMap::<u64, u64>::with_capacity(INSERT_COUNT * 2),
        || FunnelHashMap::<u64, u64>::with_capacity(INSERT_COUNT * 2),
        |map| {
            for &(key, value) in &pairs {
                map.insert(black_box(key), black_box(value));
            }
            black_box(map.len())
        },
    );

    group.finish();
}

fn bench_lookup_workload(c: &mut Criterion, name: &str, pairs: &[(u64, u64)], query_keys: &[u64]) {
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Elements(query_keys.len() as u64));

    bench_all_impls!(
        group,
        BatchSize::LargeInput,
        || build_std_map(pairs),
        || build_hashbrown_map(pairs),
        || build_elastic_map(pairs),
        || build_funnel_map(pairs),
        |map| {
            for key in query_keys {
                black_box(map.get(black_box(key)));
            }
        },
    );

    group.finish();
}

fn bench_get_hit_throughput(c: &mut Criterion) {
    let pairs = make_pairs(LOOKUP_MAP_SIZE);
    let query_keys: Vec<u64> = (0..HIT_LOOKUP_COUNT)
        .map(|idx| pairs[idx % LOOKUP_MAP_SIZE].0)
        .collect();
    bench_lookup_workload(c, "get_hit_throughput", &pairs, &query_keys);
}

fn bench_get_miss_throughput(c: &mut Criterion) {
    let pairs = make_pairs(LOOKUP_MAP_SIZE);
    let query_keys: Vec<u64> = (0..MISS_LOOKUP_COUNT)
        .map(|idx| key_at(idx + LOOKUP_MAP_SIZE + 10_000_000))
        .collect();
    bench_lookup_workload(c, "get_miss_throughput", &pairs, &query_keys);
}

fn bench_tiny_lookup_throughput(c: &mut Criterion) {
    let pairs = make_pairs(TINY_MAP_SIZE);
    let query_keys: Vec<u64> = (0..TINY_LOOKUP_COUNT)
        .map(|idx| {
            if idx % 2 == 0 {
                pairs[idx % TINY_MAP_SIZE].0
            } else {
                key_at(idx + 5_000_000)
            }
        })
        .collect();
    bench_lookup_workload(c, "tiny_lookup_throughput", &pairs, &query_keys);
}

fn bench_delete_heavy_throughput(c: &mut Criterion) {
    let initial_pairs = make_pairs(DELETE_MAP_SIZE);
    let replacement_pairs: Vec<(u64, u64)> = (0..DELETE_OP_COUNT)
        .map(|idx| {
            let key = key_at(idx + 20_000_000);
            (key, key ^ VALUE_XOR_MIX_ALT)
        })
        .collect();

    let mut group = c.benchmark_group("delete_heavy_throughput");
    group.throughput(Throughput::Elements((DELETE_OP_COUNT * 2) as u64));

    bench_all_impls!(
        group,
        BatchSize::PerIteration,
        || build_std_map(&initial_pairs),
        || build_hashbrown_map(&initial_pairs),
        || build_elastic_map(&initial_pairs),
        || build_funnel_map(&initial_pairs),
        |map| {
            for idx in 0..DELETE_OP_COUNT {
                black_box(map.remove(black_box(&initial_pairs[idx].0)));
                let (key, value) = replacement_pairs[idx];
                black_box(map.insert(black_box(key), black_box(value)));
            }
        },
    );

    group.finish();
}

fn bench_mixed_throughput(c: &mut Criterion) {
    let pairs = make_pairs(MIXED_MAP_SIZE);
    let ops: Vec<(usize, bool)> = (0..MIXED_OP_COUNT)
        .map(|i| {
            let idx = ((i as u32).wrapping_mul(2_654_435_761) as usize) % MIXED_MAP_SIZE;
            (idx, i & 1 == 0)
        })
        .collect();

    let mut group = c.benchmark_group("mixed_throughput");
    group.throughput(Throughput::Elements(MIXED_OP_COUNT as u64));

    bench_all_impls!(
        group,
        BatchSize::LargeInput,
        || build_std_map(&pairs),
        || build_hashbrown_map(&pairs),
        || build_elastic_map(&pairs),
        || build_funnel_map(&pairs),
        |map| {
            for &(idx, is_read) in &ops {
                let key = pairs[idx].0;
                if is_read {
                    black_box(map.get(black_box(&key)));
                } else {
                    black_box(map.insert(black_box(key), black_box(idx as u64)));
                }
            }
        },
    );

    group.finish();
}

fn bench_resize_heavy_throughput(c: &mut Criterion) {
    let pairs = make_pairs(RESIZE_INSERT_COUNT);
    let mut group = c.benchmark_group("resize_heavy_throughput");
    group.throughput(Throughput::Elements(RESIZE_INSERT_COUNT as u64));

    bench_all_impls!(
        group,
        BatchSize::PerIteration,
        StdHashMap::<u64, u64>::new,
        HashbrownMap::<u64, u64>::new,
        ElasticHashMap::<u64, u64>::new,
        FunnelHashMap::<u64, u64>::new,
        |map| {
            for &(key, value) in &pairs {
                black_box(map.insert(black_box(key), black_box(value)));
            }
            black_box(map.len())
        },
    );

    group.finish();
}

fn bench_get_hit_latency(c: &mut Criterion) {
    for &size in LATENCY_SIZES {
        let pairs = make_pairs(size);
        let query_keys: Vec<u64> = (0..size).map(|idx| pairs[idx].0).collect();

        let label = size_label(size);
        let mut group = c.benchmark_group(format!("get_hit_latency_{label}"));

        macro_rules! latency_arm {
            ($name:literal, $build:expr) => {
                group.bench_function($name, |b| {
                    let map = $build(&pairs);
                    let mut i = 0;
                    b.iter(|| {
                        let key = &query_keys[i % size];
                        i = i.wrapping_add(1);
                        black_box(map.get(black_box(key)))
                    });
                });
            };
        }

        latency_arm!("std", build_std_map);
        latency_arm!("hashbrown", build_hashbrown_map);
        latency_arm!("elastic", build_elastic_map);
        latency_arm!("funnel", build_funnel_map);

        group.finish();
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .with_profiler(FlamegraphProfiler::new())
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    targets =
        bench_insert_throughput,
        bench_get_hit_throughput,
        bench_get_miss_throughput,
        bench_tiny_lookup_throughput,
        bench_mixed_throughput,
        bench_delete_heavy_throughput,
        bench_resize_heavy_throughput,
        bench_get_hit_latency
);
criterion_main!(benches);
