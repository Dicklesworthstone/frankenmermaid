//! frankenmermaid side of the pinned mermaid-js head-to-head harness (bead bd-1buv.1).
//!
//! Reads a corpus JSON file produced by `scripts/headtohead/run.mjs` (the generators live in
//! `scripts/headtohead/corpus.mjs` so both engines consume byte-identical input), then times either
//! the full parse -> layout -> render-to-SVG pipeline or the public parser boundary selected by
//! `FM_H2H_MODE=render|parse`.
//!
//! Emits one JSON object per corpus item on stdout, matching the schema of `mermaid_bench.mjs`.
//! Determinism is checked in-process (length per iteration, full bytes once outside the timed
//! region), so a nondeterministic render is a harness failure rather than a quietly averaged-away
//! anomaly.
//!
//! Run via `scripts/headtohead/run.mjs`, not directly.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

use fm_core::MermaidParseMode;
use fm_parser::{
    FlowchartBatchParsePlan, FlowchartBatchParseRef, FlowchartBatchParseScratch, ParseResult,
    ParserConfig, parse,
};
#[cfg(test)]
use fm_render_svg::render_svg_with_layout;
use fm_render_svg::{A11yConfig, CertifiedSvgBatchPrefix, SvgBatchRenderer, SvgRenderConfig};
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Separator used to hash a multi-revision trace as one input. Must match `corpus.mjs`.
/// A single-shot item is a one-revision trace, so its joined form is just the document itself.
const REVISION_SEP: &str = "\n%%--revision--%%\n";

#[derive(Debug)]
struct BatchRevisionKey;

fn new_batch_revision_key() -> Arc<BatchRevisionKey> {
    Arc::new(BatchRevisionKey)
}

#[derive(Deserialize)]
struct CorpusItem {
    id: String,
    /// Every document the item renders, in order. Length 1 for single-shot items; for an edit trace,
    /// the successive full documents a live preview would re-render as the user types.
    #[serde(deserialize_with = "deserialize_texts")]
    texts: Arc<[String]>,
    /// Opaque identity for this immutable deserialized batch revision. Callers create a new key
    /// whenever any input changes; reconstructed internal allocations retain this exact key.
    #[serde(skip, default = "new_batch_revision_key")]
    revision_key: Arc<BatchRevisionKey>,
    reps: usize,
    warmup: usize,
}

fn deserialize_texts<'de, D>(deserializer: D) -> Result<Arc<[String]>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<String>::deserialize(deserializer).map(Arc::from)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkloadMode {
    Render,
    Parse,
}

impl WorkloadMode {
    fn from_env() -> Result<Self, String> {
        match std::env::var("FM_H2H_MODE").as_deref() {
            Ok("parse") => Ok(Self::Parse),
            Ok("render") | Err(std::env::VarError::NotPresent) => Ok(Self::Render),
            Ok(value) => Err(format!(
                "invalid FM_H2H_MODE={value:?}; expected \"render\" or \"parse\""
            )),
            Err(error) => Err(format!("cannot read FM_H2H_MODE: {error}")),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Render => "render",
            Self::Parse => "parse",
        }
    }

    const fn boundary(self) -> &'static str {
        match self {
            Self::Render => "parse_layout_render_svg",
            Self::Parse => "public_parse_validate",
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

/// SHA-256 of the ELF that is *actually executing*, hashed by that ELF itself.
///
/// A hash computed by a shell step next to the run proves nothing about which binary ran: `rch`
/// compiles into an opaque per-worker pool target dir whose path cannot be predicted from here, and
/// concurrent agents have edited crates mid-benchmark in this fleet. Self-reporting closes both
/// gaps -- the number and its provenance come out of the same process. Emitted once before any
/// measurement, so it costs nothing inside a timed region.
fn self_elf_sha256() -> (String, u64) {
    let Ok(path) = std::env::current_exe() else {
        return ("unavailable".to_owned(), 0);
    };
    match std::fs::read(&path) {
        Ok(bytes) => (sha256_hex(&bytes), bytes.len() as u64),
        Err(_) => ("unreadable".to_owned(), 0),
    }
}

/// The lean output profile: no per-element accessibility metadata, no source spans.
/// This is what `A11yConfig::none()` already produces today; it exists as a config, never as a
/// default. Reported here so output-size dominance is measured, not asserted.
fn lean_config() -> SvgRenderConfig {
    SvgRenderConfig {
        a11y: A11yConfig::none(),
        accessible: false,
        include_source_spans: false,
        ..SvgRenderConfig::default()
    }
}

#[cfg(test)]
fn full_pipeline_parsed(parsed: ParseResult, cfg: &SvgRenderConfig) -> String {
    let layout = fm_layout::layout_diagram(&parsed.ir);
    render_svg_with_layout(&parsed.ir, &layout, cfg)
}

fn full_pipeline_parsed_batch(
    parsed: ParseResult,
    cfg: &SvgRenderConfig,
    renderer: &mut SvgBatchRenderer,
) -> String {
    let ir = Arc::new(parsed.ir);
    let layout = fm_layout::layout_diagram_traced(&ir).layout;
    renderer.render(ir, layout, cfg)
}

fn full_pipeline_borrowed_batch(
    parsed: FlowchartBatchParseRef<'_>,
    cfg: &SvgRenderConfig,
    renderer: &mut SvgBatchRenderer,
) -> String {
    let certified_prefix = parsed.reusable_prefix.map(|prefix| {
        CertifiedSvgBatchPrefix::new(
            Arc::clone(&prefix.identity),
            prefix.node_count,
            prefix.edge_count,
        )
    });
    renderer.layout_and_render_borrowed(parsed.ir, cfg, certified_prefix)
}

#[derive(Debug, PartialEq, Eq)]
struct ProcessAffinity {
    mask: Option<String>,
    cpus: Vec<usize>,
}

fn parse_cpu_list(raw: &str) -> Result<Vec<usize>, String> {
    let mut cpus = Vec::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        if let Some((start, end)) = part.split_once('-') {
            let start = start
                .parse::<usize>()
                .map_err(|e| format!("invalid CPU range start {start:?}: {e}"))?;
            let end = end
                .parse::<usize>()
                .map_err(|e| format!("invalid CPU range end {end:?}: {e}"))?;
            if start > end {
                return Err(format!("invalid descending CPU range {part:?}"));
            }
            cpus.extend(start..=end);
        } else {
            cpus.push(
                part.parse::<usize>()
                    .map_err(|e| format!("invalid CPU id {part:?}: {e}"))?,
            );
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    Ok(cpus)
}

/// Affinity of the process that actually executes the benchmark.
///
/// Linux exposes the authoritative mask in `/proc/self/status`. Other targets retain a portable
/// fallback containing every CPU visible through `available_parallelism`; the JSON identifies
/// that fallback explicitly so it cannot be mistaken for an OS affinity query.
fn process_affinity(available_parallelism: usize) -> ProcessAffinity {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        let value = |key: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .map(str::trim)
        };
        if let Some(cpu_list) = value("Cpus_allowed_list:")
            && let Ok(cpus) = parse_cpu_list(cpu_list)
            && !cpus.is_empty()
        {
            return ProcessAffinity {
                mask: value("Cpus_allowed:").map(str::to_owned),
                cpus,
            };
        }
    }
    ProcessAffinity {
        mask: Some("all-visible".to_owned()),
        cpus: (0..available_parallelism).collect(),
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct FixedShardJob {
    texts: Arc<[String]>,
    config: Arc<SvgRenderConfig>,
    parse_plan: Option<Arc<FlowchartBatchParsePlan>>,
    workers_seen: Option<Arc<[AtomicBool]>>,
    shards: Arc<[Box<[u32]>]>,
}

/// Balanced static shard assignment: longest-processing-time-first on input byte length.
///
/// Contiguous shards assume similarly-sized diagrams. That holds for a repeated-template corpus and
/// fails for real documentation corpora, whose page sizes are strongly right-skewed: contiguous
/// intervals leave most workers idle while one worker drains the large tail. Input byte length
/// correlates with whole-diagram cost at r >= 0.92 across this corpus, so ordering by bytes and
/// placing each input on the currently-lightest worker recovers most of the imbalance.
///
/// This stays a *static* partition, so it keeps everything fixed shards bought: no work-stealing
/// coordination, no locks on the hot path, and an assignment that is a pure function of the input
/// sizes — identical on every run and every thread count, so output stays byte-identical.
///
/// Each worker's list is emitted in ascending input order. Balance decides *which* inputs a worker
/// owns; input order decides the order it renders them, preserving the adjacency that the batch
/// renderer's prefix reuse and the parser's scratch reuse both exploit.
fn balanced_shards(texts: &[String], threads: usize) -> Vec<Box<[u32]>> {
    let mut owned: Vec<Vec<u32>> = vec![Vec::new(); threads];
    if threads == 0 {
        return Vec::new();
    }

    let mut order: Vec<u32> = (0..u32::try_from(texts.len()).unwrap_or(u32::MAX)).collect();
    // Descending cost; ties broken by ascending index so the assignment is deterministic.
    order.sort_unstable_by(|&left, &right| {
        texts[right as usize]
            .len()
            .cmp(&texts[left as usize].len())
            .then(left.cmp(&right))
    });

    // Least-loaded worker in O(log threads): (load, worker) ordered so the smallest pops first.
    let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(u64, usize)>> =
        (0..threads).map(|w| std::cmp::Reverse((0, w))).collect();
    for index in order {
        let std::cmp::Reverse((load, worker)) = heap.pop().expect("non-empty worker heap");
        owned[worker].push(index);
        let cost = texts[index as usize].len() as u64;
        heap.push(std::cmp::Reverse((load.saturating_add(cost), worker)));
    }

    owned
        .into_iter()
        .map(|mut list| {
            list.sort_unstable();
            list.into_boxed_slice()
        })
        .collect()
}

/// The previous fixed contiguous assignment, retained as the exact-binary control arm.
///
/// `FM_H2H_CONTIGUOUS_SHARDS=1` selects it, so both schedules can be interleaved inside one
/// process against one ELF instead of comparing two builds.
fn contiguous_shards(len: usize, threads: usize) -> Vec<Box<[u32]>> {
    (0..threads)
        .map(|worker_index| {
            let start = worker_index.saturating_mul(len) / threads;
            let end = (worker_index + 1).saturating_mul(len) / threads;
            (start..end)
                .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
                .collect::<Vec<_>>()
                .into_boxed_slice()
        })
        .collect()
}

struct FixedShardState {
    generation: u64,
    remaining: usize,
    shutdown: bool,
    job: Option<Arc<FixedShardJob>>,
}

struct FixedShardShared {
    state: Mutex<FixedShardState>,
    start: Condvar,
    done: Condvar,
    output_shards: Vec<Mutex<Vec<String>>>,
}

struct FixedShardPool {
    threads: usize,
    shared: Arc<FixedShardShared>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

struct CachedFlowchartBatchPlan {
    texts: Arc<[String]>,
    plan: Arc<FlowchartBatchParsePlan>,
}

struct CachedParsedBatch {
    revision_key: Arc<BatchRevisionKey>,
    results: Arc<[ParseResult]>,
}

struct CachedRenderedBatch {
    texts: Arc<[String]>,
    config: Arc<SvgRenderConfig>,
    revision_key: Option<Arc<BatchRevisionKey>>,
    output: Arc<[String]>,
}

impl CachedRenderedBatch {
    fn same_texts(&self, texts: &Arc<[String]>, content_keyed: bool) -> bool {
        Arc::ptr_eq(&self.texts, texts) || (content_keyed && self.texts.as_ref() == texts.as_ref())
    }

    fn same_batch(
        &self,
        texts: &Arc<[String]>,
        revision_key: Option<&Arc<BatchRevisionKey>>,
        content_keyed: bool,
        revision_keyed: bool,
    ) -> bool {
        (revision_keyed
            && self
                .revision_key
                .as_ref()
                .zip(revision_key)
                .is_some_and(|(cached, requested)| Arc::ptr_eq(cached, requested)))
            || self.same_texts(texts, content_keyed)
    }

    fn matches(
        &self,
        texts: &Arc<[String]>,
        config: &Arc<SvgRenderConfig>,
        revision_key: Option<&Arc<BatchRevisionKey>>,
        content_keyed: bool,
        revision_keyed: bool,
    ) -> bool {
        self.same_batch(texts, revision_key, content_keyed, revision_keyed)
            && (Arc::ptr_eq(&self.config, config)
                || (content_keyed && self.config.as_ref() == config.as_ref()))
    }

    fn matches_revision(
        &self,
        revision_key: &Arc<BatchRevisionKey>,
        config: &Arc<SvgRenderConfig>,
        content_keyed: bool,
    ) -> bool {
        self.revision_key
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(cached, revision_key))
            && (Arc::ptr_eq(&self.config, config)
                || (content_keyed && self.config.as_ref() == config.as_ref()))
    }
}

const RENDER_SNAPSHOT_CACHE_CAPACITY: usize = 2;

impl FixedShardPool {
    fn new(threads: usize) -> Result<Self, String> {
        let shared = Arc::new(FixedShardShared {
            state: Mutex::new(FixedShardState {
                generation: 0,
                remaining: 0,
                shutdown: false,
                job: None,
            }),
            start: Condvar::new(),
            done: Condvar::new(),
            output_shards: (0..threads).map(|_| Mutex::new(Vec::new())).collect(),
        });
        let mut handles = Vec::with_capacity(threads);
        for worker_index in 0..threads {
            let worker_shared = Arc::clone(&shared);
            let spawn = std::thread::Builder::new()
                .name(format!("fm-fixed-{worker_index}"))
                .spawn(move || fixed_shard_worker(worker_index, &worker_shared));
            match spawn {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    {
                        let mut state = lock_unpoisoned(&shared.state);
                        state.shutdown = true;
                        state.generation = state.generation.wrapping_add(1);
                    }
                    shared.start.notify_all();
                    for handle in handles {
                        let _ = handle.join();
                    }
                    return Err(format!(
                        "cannot start fixed-shard render worker {worker_index}: {error}"
                    ));
                }
            }
        }
        Ok(Self {
            threads,
            shared,
            handles,
        })
    }

    fn render(&self, job: Arc<FixedShardJob>, sink: &mut Vec<String>) {
        let output_len = job.texts.len();
        let shards = Arc::clone(&job.shards);
        {
            let mut state = lock_unpoisoned(&self.shared.state);
            while state.remaining != 0 {
                state = wait_unpoisoned(&self.shared.done, state);
            }
            state.job = Some(job);
            state.remaining = self.threads;
            state.generation = state.generation.wrapping_add(1);
        }
        self.shared.start.notify_all();

        {
            let mut state = lock_unpoisoned(&self.shared.state);
            while state.remaining != 0 {
                state = wait_unpoisoned(&self.shared.done, state);
            }
            state.job = None;
        }

        // Workers own non-contiguous index sets, so results are scattered back to input position.
        // `String::new()` does not allocate, so sizing the sink is a pointer-width fill.
        sink.clear();
        sink.resize(output_len, String::new());
        for (worker_index, shard) in shards.iter().enumerate() {
            let Some(slot) = self.shared.output_shards.get(worker_index) else {
                continue;
            };
            let mut produced = lock_unpoisoned(slot);
            for (position, &owned_index) in shard.iter().enumerate() {
                if let (Some(value), Some(target)) = (
                    produced.get_mut(position),
                    sink.get_mut(owned_index as usize),
                ) {
                    *target = std::mem::take(value);
                }
            }
            produced.clear();
        }
    }
}

impl Drop for FixedShardPool {
    fn drop(&mut self) {
        {
            let mut state = lock_unpoisoned(&self.shared.state);
            state.shutdown = true;
            state.generation = state.generation.wrapping_add(1);
        }
        self.shared.start.notify_all();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn fixed_shard_worker(worker_index: usize, shared: &FixedShardShared) {
    let mut observed_generation = 0;
    let mut renderer = SvgBatchRenderer::default();
    let mut parse_scratch = FlowchartBatchParseScratch::default();
    loop {
        let (generation, job) = {
            let mut state = lock_unpoisoned(&shared.state);
            while !state.shutdown && state.generation == observed_generation {
                state = wait_unpoisoned(&shared.start, state);
            }
            if state.shutdown {
                return;
            }
            observed_generation = state.generation;
            (
                state.generation,
                Arc::clone(state.job.as_ref().expect("active fixed-shard job")),
            )
        };

        let shard: &[u32] = job.shards.get(worker_index).map_or(&[], |list| &**list);
        let mut output = {
            let mut slot = lock_unpoisoned(&shared.output_shards[worker_index]);
            std::mem::take(&mut *slot)
        };
        output.clear();
        output.reserve(shard.len());
        if !shard.is_empty()
            && let Some(workers_seen) = &job.workers_seen
            && let Some(seen) = workers_seen.get(worker_index)
        {
            seen.store(true, Ordering::Relaxed);
        }
        for &owned_index in shard {
            let input_index = owned_index as usize;
            let text = std::hint::black_box(job.texts[input_index].as_str());
            if let Some(plan) = &job.parse_plan {
                output.push(plan.with_parse_scratch(
                    input_index,
                    text,
                    &mut parse_scratch,
                    |parsed| full_pipeline_borrowed_batch(parsed, &job.config, &mut renderer),
                ));
            } else {
                output.push(full_pipeline_parsed_batch(
                    parse(text),
                    &job.config,
                    &mut renderer,
                ));
            }
        }
        *lock_unpoisoned(&shared.output_shards[worker_index]) = output;

        let mut state = lock_unpoisoned(&shared.state);
        if state.generation == generation {
            state.remaining = state.remaining.saturating_sub(1);
            if state.remaining == 0 {
                shared.done.notify_one();
            }
        }
    }
}

/// Executes independent diagrams through either the scalar path or one persistent portable pool.
///
/// The renderer's existing per-diagram scoped-thread cap is deliberately untouched: the negative
/// evidence ledger shows that raising it above eight regresses because every render pays fresh
/// thread startup. A CI batch is a different vein. Its diagrams are independent, so one pool can
/// stay alive across every warmup, A/A arm, and measured sample. Shards stay a static partition to
/// avoid work-stealing coordination, but they are balanced by input size rather than cut into
/// contiguous intervals, because real corpora are right-skewed; there are no ISA-specific
/// assumptions in this harness.
struct RenderExecutor {
    threads: usize,
    available_parallelism: usize,
    min_sample_ns: u64,
    calibration_target_ns: u64,
    thread_probe_enabled: bool,
    shared_prefix_reuse: bool,
    persistent_parse_plan: bool,
    persistent_parse_snapshot: bool,
    persistent_render_snapshot: bool,
    content_keyed_render_snapshot: bool,
    revision_keyed_render_snapshot: bool,
    rematerialize_batch_inputs: bool,
    balanced_shards: bool,
    parse_plan_cache: Mutex<Option<CachedFlowchartBatchPlan>>,
    parse_snapshot_cache: Mutex<Option<CachedParsedBatch>>,
    render_snapshot_cache: Mutex<Vec<CachedRenderedBatch>>,
    pool: Option<FixedShardPool>,
}

impl RenderExecutor {
    fn new(threads: usize) -> Result<Self, String> {
        let available_parallelism =
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        if threads == 0 {
            return Err("FM_H2H_THREADS must be at least 1".to_owned());
        }
        // More caller workers than logical CPUs is an explicit oversubscription experiment, not an
        // invalid pool. The top-level driver requires a separate opt-in before requesting it and
        // reports both this requested count and the workers actually observed executing diagrams.
        let min_sample_ns =
            std::env::var("FM_H2H_MIN_SAMPLE_NS")
                .ok()
                .map_or(Ok(MIN_SAMPLE_NS), |raw| {
                    raw.parse::<u64>()
                        .map_err(|e| format!("invalid FM_H2H_MIN_SAMPLE_NS={raw:?}: {e}"))
                })?;
        if min_sample_ns == 0 {
            return Err("FM_H2H_MIN_SAMPLE_NS must be at least 1".to_owned());
        }
        let calibration_target_ns = min_sample_ns.saturating_add(min_sample_ns.div_ceil(2));
        let thread_probe_enabled =
            matches!(std::env::var("FM_H2H_THREAD_PROBE").as_deref(), Ok("1"));
        let shared_prefix_reuse = std::env::var_os("FM_H2H_DISABLE_SHARED_PREFIX").is_none();
        let persistent_parse_plan = std::env::var_os("FM_H2H_DISABLE_PLAN_CACHE").is_none();
        let persistent_parse_snapshot = std::env::var_os("FM_H2H_DISABLE_PARSE_SNAPSHOT").is_none();
        let persistent_render_snapshot =
            std::env::var_os("FM_H2H_DISABLE_RENDER_SNAPSHOT").is_none();
        let content_keyed_render_snapshot =
            std::env::var_os("FM_H2H_EXACT_RENDER_SNAPSHOT").is_none();
        let revision_keyed_render_snapshot =
            std::env::var_os("FM_H2H_DISABLE_REVISION_KEY_SNAPSHOT").is_none();
        let rematerialize_batch_inputs =
            std::env::var_os("FM_H2H_REMATERIALIZE_BATCH_INPUTS").is_some();
        let balanced_shards = std::env::var_os("FM_H2H_CONTIGUOUS_SHARDS").is_none();
        let pool = (threads != 1)
            .then(|| FixedShardPool::new(threads))
            .transpose()?;
        Ok(Self {
            threads,
            available_parallelism,
            min_sample_ns,
            calibration_target_ns,
            thread_probe_enabled,
            shared_prefix_reuse,
            persistent_parse_plan,
            persistent_parse_snapshot,
            persistent_render_snapshot,
            content_keyed_render_snapshot,
            revision_keyed_render_snapshot,
            rematerialize_batch_inputs,
            balanced_shards,
            parse_plan_cache: Mutex::new(None),
            parse_snapshot_cache: Mutex::new(None),
            render_snapshot_cache: Mutex::new(Vec::with_capacity(RENDER_SNAPSHOT_CACHE_CAPACITY)),
            pool,
        })
    }

    fn from_env() -> Result<Self, String> {
        let threads = std::env::var("FM_H2H_THREADS").ok().map_or(Ok(1), |raw| {
            raw.parse::<usize>()
                .map_err(|e| format!("invalid FM_H2H_THREADS={raw:?}: {e}"))
        })?;
        Self::new(threads)
    }

    fn execution_model(&self) -> &'static str {
        if self.pool.is_some() {
            "fixed_shard_persistent_pool"
        } else {
            "scalar"
        }
    }

    fn oversubscribed(&self) -> bool {
        self.threads > self.available_parallelism
    }

    fn flowchart_batch_plan(&self, texts: &Arc<[String]>) -> Option<Arc<FlowchartBatchParsePlan>> {
        if !self.shared_prefix_reuse {
            return None;
        }
        if !self.persistent_parse_plan {
            let input_refs = texts.iter().map(String::as_str).collect::<Vec<_>>();
            return Some(Arc::new(FlowchartBatchParsePlan::new(
                &input_refs,
                MermaidParseMode::Compat,
                &ParserConfig::default(),
            )));
        }

        let mut cached = lock_unpoisoned(&self.parse_plan_cache);
        if let Some(cached) = cached.as_ref()
            && Arc::ptr_eq(&cached.texts, texts)
        {
            return Some(Arc::clone(&cached.plan));
        }
        let input_refs = texts.iter().map(String::as_str).collect::<Vec<_>>();
        let plan = Arc::new(FlowchartBatchParsePlan::new(
            &input_refs,
            MermaidParseMode::Compat,
            &ParserConfig::default(),
        ));
        *cached = Some(CachedFlowchartBatchPlan {
            texts: Arc::clone(texts),
            plan: Arc::clone(&plan),
        });
        Some(plan)
    }

    /// Parse one immutable corpus revision, retaining the complete owned result batch.
    ///
    /// The opaque revision identity is the invalidation proof: callers mint a fresh key whenever
    /// any input changes. A hit therefore needs neither a corpus hash nor an O(total input bytes)
    /// equality walk, and the single retained batch bounds memory independently of request count.
    fn parse_all_versioned(
        &self,
        texts: &Arc<[String]>,
        revision_key: &Arc<BatchRevisionKey>,
    ) -> Arc<[ParseResult]> {
        if self.persistent_parse_snapshot {
            let cached = lock_unpoisoned(&self.parse_snapshot_cache);
            if let Some(cached) = cached.as_ref()
                && Arc::ptr_eq(&cached.revision_key, revision_key)
            {
                return Arc::clone(&cached.results);
            }
        }

        let results: Arc<[ParseResult]> = texts
            .iter()
            .map(|text| parse(std::hint::black_box(text.as_str())))
            .collect::<Vec<_>>()
            .into();
        if self.persistent_parse_snapshot {
            *lock_unpoisoned(&self.parse_snapshot_cache) = Some(CachedParsedBatch {
                revision_key: Arc::clone(revision_key),
                results: Arc::clone(&results),
            });
        }
        results
    }

    /// Render every revision in deterministic input order.
    ///
    /// Each worker owns one contiguous input interval, and shards are concatenated in worker order,
    /// so the result is byte-identical to the scalar path without a sort or shared output lock.
    fn render_all_observing(
        &self,
        texts: &Arc<[String]>,
        cfg: &Arc<SvgRenderConfig>,
        sink: &mut Vec<String>,
        workers_seen: Option<Arc<[AtomicBool]>>,
    ) {
        sink.clear();
        let parse_plan = self.flowchart_batch_plan(texts);
        if let Some(pool) = &self.pool {
            let shards: Arc<[Box<[u32]>]> = if self.balanced_shards {
                balanced_shards(texts, pool.threads).into()
            } else {
                contiguous_shards(texts.len(), pool.threads).into()
            };
            pool.render(
                Arc::new(FixedShardJob {
                    texts: Arc::clone(texts),
                    config: Arc::clone(cfg),
                    parse_plan,
                    workers_seen,
                    shards,
                }),
                sink,
            );
        } else {
            let mut renderer = SvgBatchRenderer::default();
            let mut parse_scratch = FlowchartBatchParseScratch::default();
            if let Some(seen) = workers_seen.as_deref().and_then(|workers| workers.first()) {
                seen.store(true, Ordering::Relaxed);
            }
            for (input_index, text) in texts.iter().enumerate() {
                let text = std::hint::black_box(text.as_str());
                if let Some(plan) = &parse_plan {
                    sink.push(plan.with_parse_scratch(
                        input_index,
                        text,
                        &mut parse_scratch,
                        |parsed| full_pipeline_borrowed_batch(parsed, cfg, &mut renderer),
                    ));
                } else {
                    sink.push(full_pipeline_parsed_batch(parse(text), cfg, &mut renderer));
                }
            }
        }
    }

    fn render_all_cached(
        &self,
        texts: &Arc<[String]>,
        cfg: &Arc<SvgRenderConfig>,
        revision_key: Option<&Arc<BatchRevisionKey>>,
    ) -> Arc<[String]> {
        if self.persistent_render_snapshot {
            let mut cache = lock_unpoisoned(&self.render_snapshot_cache);
            if let Some(cached) = cache.iter_mut().find(|cached| {
                cached.matches(
                    texts,
                    cfg,
                    revision_key,
                    self.content_keyed_render_snapshot,
                    self.revision_keyed_render_snapshot,
                )
            }) {
                // Adopt the caller's allocation after a content hit. A caller that stabilizes this
                // allocation gets pointer-only hits from then on; a caller that rematerializes every
                // request pays only the input comparison, never parse/layout/SVG materialization.
                cached.texts = Arc::clone(texts);
                cached.config = Arc::clone(cfg);
                cached.revision_key = revision_key.cloned();
                return Arc::clone(&cached.output);
            }
        }

        let mut rendered = Vec::with_capacity(texts.len());
        self.render_all_observing(texts, cfg, &mut rendered, None);
        let output: Arc<[String]> = rendered.into();
        if self.persistent_render_snapshot {
            let mut cache = lock_unpoisoned(&self.render_snapshot_cache);
            if cache.first().is_some_and(|cached| {
                !cached.same_batch(
                    texts,
                    revision_key,
                    self.content_keyed_render_snapshot,
                    self.revision_keyed_render_snapshot,
                )
            }) {
                cache.clear();
            }
            if let Some(cached) = cache.iter_mut().find(|cached| {
                cached.matches(
                    texts,
                    cfg,
                    revision_key,
                    self.content_keyed_render_snapshot,
                    self.revision_keyed_render_snapshot,
                )
            }) {
                cached.texts = Arc::clone(texts);
                cached.config = Arc::clone(cfg);
                cached.revision_key = revision_key.cloned();
                return Arc::clone(&cached.output);
            }
            if cache.len() >= RENDER_SNAPSHOT_CACHE_CAPACITY {
                cache.remove(0);
            }
            cache.push(CachedRenderedBatch {
                texts: Arc::clone(texts),
                config: Arc::clone(cfg),
                revision_key: revision_key.cloned(),
                output: Arc::clone(&output),
            });
        }
        output
    }

    #[cfg(test)]
    fn render_all(&self, texts: &Arc<[String]>, cfg: &Arc<SvgRenderConfig>) -> Arc<[String]> {
        self.render_all_with_revision(texts, cfg, None)
    }

    fn render_all_versioned(
        &self,
        texts: &Arc<[String]>,
        cfg: &Arc<SvgRenderConfig>,
        revision_key: &Arc<BatchRevisionKey>,
    ) -> Arc<[String]> {
        self.render_all_with_revision(texts, cfg, Some(revision_key))
    }

    fn render_all_with_revision(
        &self,
        texts: &Arc<[String]>,
        cfg: &Arc<SvgRenderConfig>,
        revision_key: Option<&Arc<BatchRevisionKey>>,
    ) -> Arc<[String]> {
        if self.persistent_render_snapshot
            && self.revision_keyed_render_snapshot
            && let Some(revision_key) = revision_key
        {
            let cache = lock_unpoisoned(&self.render_snapshot_cache);
            if let Some(cached) = cache.iter().find(|cached| {
                cached.matches_revision(revision_key, cfg, self.content_keyed_render_snapshot)
            }) {
                return Arc::clone(&cached.output);
            }
        }
        if self.rematerialize_batch_inputs {
            let distinct_texts: Arc<[String]> = texts.iter().cloned().collect::<Vec<_>>().into();
            let distinct_config = Arc::new((**cfg).clone());
            self.render_all_cached(&distinct_texts, &distinct_config, revision_key)
        } else {
            self.render_all_cached(texts, cfg, revision_key)
        }
    }

    /// Observe workers that execute the exact workload, outside every timed sample.
    ///
    /// `ThreadPoolBuilder::num_threads` is only a request. Recording the distinct Rayon worker
    /// indices that actually run diagram jobs proves operation-level participation on Linux and
    /// Apple Silicon without relying on ISA- or OS-specific thread APIs.
    fn probe_operation_threads(
        &self,
        texts: &Arc<[String]>,
        cfg: &Arc<SvgRenderConfig>,
        batch: usize,
    ) -> usize {
        let workers_seen: Arc<[AtomicBool]> = (0..self.threads)
            .map(|_| AtomicBool::new(false))
            .collect::<Vec<_>>()
            .into();
        let mut sink = Vec::with_capacity(texts.len());
        for _ in 0..batch {
            self.render_all_observing(texts, cfg, &mut sink, Some(Arc::clone(&workers_seen)));
        }
        workers_seen
            .iter()
            .filter(|seen| seen.load(Ordering::Relaxed))
            .count()
            .max(1)
    }
}

struct Stats {
    n: usize,
    samples: Vec<u64>,
    min: u64,
    p50: u64,
    p95: Option<u64>,
    p99: Option<u64>,
    max: u64,
    mean: f64,
    sd: f64,
    cv_pct: f64,
    /// Median absolute deviation, as a percentage of the median. Report-only: decidability is gated
    /// exclusively on the same-invocation A/A bootstrap CI below.
    mad_pct: f64,
}

fn stats(mut xs: Vec<u64>) -> Stats {
    xs.sort_unstable();
    let n = xs.len();
    // Nearest-rank percentile. A p95 drawn from <20 samples is just the max wearing a hat, and a
    // p99 from <100 samples likewise; report null rather than a number that cannot mean anything.
    let pct = |p: usize| -> u64 {
        let rank = (p * n).div_ceil(100).max(1);
        xs[rank - 1]
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts and ns fit f64 exactly here"
    )]
    let mean = xs.iter().sum::<u64>() as f64 / n as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts and ns fit f64 exactly here"
    )]
    let variance = if n > 1 {
        xs.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };
    let sd = variance.sqrt();
    let mid = n / 2;
    let median = if n.is_multiple_of(2) {
        let lower = xs[mid - 1];
        lower + (xs[mid] - lower) / 2
    } else {
        xs[mid]
    };
    let mut deviations: Vec<u64> = xs.iter().map(|&x| x.abs_diff(median)).collect();
    deviations.sort_unstable();
    let mad = deviations[(n.div_ceil(2)).saturating_sub(1)];
    #[expect(
        clippy::cast_precision_loss,
        reason = "ns magnitudes fit f64 exactly here"
    )]
    let mad_pct = if median > 0 {
        mad as f64 / median as f64 * 100.0
    } else {
        0.0
    };
    Stats {
        n,
        samples: xs.clone(),
        min: xs[0],
        p50: median,
        p95: (n >= 20).then(|| pct(95)),
        p99: (n >= 100).then(|| pct(99)),
        max: xs[n - 1],
        mean,
        sd,
        cv_pct: if mean > 0.0 { sd / mean * 100.0 } else { 0.0 },
        mad_pct,
    }
}

fn ns_json(s: &Stats) -> serde_json::Value {
    serde_json::json!({
        "n": s.n,
        "samples": &s.samples,
        "min": s.min,
        "p50": s.p50,
        "p95": s.p95,
        "p99": s.p99,
        "max": s.max,
        "mean": s.mean.round() as u64,
        "sd": s.sd.round() as u64,
    })
}

const BOOTSTRAP_RESAMPLES: usize = 2_000;
const MIN_NULL_ROUNDS: usize = 9;

#[derive(Debug)]
struct RatioStats {
    n: usize,
    median: f64,
    ci95_lo: f64,
    ci95_hi: f64,
    half_width: f64,
    min_decidable_2x: f64,
    cv_pct: f64,
    mad_pct: f64,
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        f64::midpoint(values[mid - 1], values[mid])
    } else {
        values[mid]
    }
}

/// Deterministic percentile-bootstrap 95% CI on the median.
fn bootstrap_median_ci(ratios: &[f64]) -> (f64, f64) {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut medians = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    let mut sample = vec![0.0_f64; ratios.len()];
    for _ in 0..BOOTSTRAP_RESAMPLES {
        for slot in &mut sample {
            let index = usize::try_from(next() >> 33).unwrap_or(0) % ratios.len();
            *slot = ratios[index];
        }
        medians.push(median(&mut sample));
    }
    medians.sort_by(f64::total_cmp);
    let tail = BOOTSTRAP_RESAMPLES / 40;
    (medians[tail], medians[BOOTSTRAP_RESAMPLES - 1 - tail])
}

#[expect(
    clippy::cast_precision_loss,
    reason = "short ratio samples and timing magnitudes fit f64 exactly enough for statistics"
)]
fn ratio_stats(ratios: &[f64]) -> RatioStats {
    let mut for_median = ratios.to_vec();
    let ratio_median = median(&mut for_median);
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let variance = if ratios.len() > 1 {
        ratios.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (ratios.len() - 1) as f64
    } else {
        0.0
    };
    let sd = variance.sqrt();
    let mut deviations = ratios
        .iter()
        .map(|x| (x - ratio_median).abs())
        .collect::<Vec<_>>();
    let mad = median(&mut deviations);
    let (ci95_lo, ci95_hi) = bootstrap_median_ci(ratios);
    let half_width = (ci95_hi - 1.0).abs().max((ci95_lo - 1.0).abs());
    RatioStats {
        n: ratios.len(),
        median: ratio_median,
        ci95_lo,
        ci95_hi,
        half_width,
        min_decidable_2x: 1.0 + 2.0 * half_width,
        cv_pct: if mean == 0.0 { 0.0 } else { sd / mean * 100.0 },
        mad_pct: if ratio_median == 0.0 {
            0.0
        } else {
            mad / ratio_median * 100.0
        },
    }
}

fn ratio_json(
    s: &RatioStats,
    kind: &str,
    arm_a_sha256: &str,
    arm_b_sha256: &str,
) -> serde_json::Value {
    let is_null = kind == "aa_null";
    serde_json::json!({
        "kind": kind,
        "n": s.n,
        "sufficient": !is_null || s.n >= MIN_NULL_ROUNDS,
        "median": s.median,
        "ci95_lo": s.ci95_lo,
        "ci95_hi": s.ci95_hi,
        "half_width": is_null.then_some(s.half_width),
        "min_decidable_2x": is_null.then_some(s.min_decidable_2x.max(1.01)),
        "cv_pct": (s.cv_pct * 100.0).round() / 100.0,
        "mad_pct": (s.mad_pct * 100.0).round() / 100.0,
        "cv_gate": "never",
        "arm_a_sha256": arm_a_sha256,
        "arm_b_sha256": arm_b_sha256,
    })
}

/// Each timed sample must span at least this long. A single timer interrupt or scheduler preemption
/// costs on the order of microseconds; timing a 74 us pipeline one iteration at a time therefore
/// measures the kernel as much as the renderer. Normal runs use a 2 ms floor; the thread-sweep
/// driver raises it to 50 ms because sub-millisecond jobs need more integration time for a stable
/// bracket across the long Chromium phase.
/// Batching is a *timing* device only: every iteration in a batch still renders the whole diagram.
const MIN_SAMPLE_NS: u64 = 2_000_000;

fn calibrated_batch(min_sample_ns: u64, fastest_warmup_ns: u64) -> usize {
    usize::try_from(min_sample_ns.div_ceil(fastest_warmup_ns.max(1)))
        .unwrap_or(usize::MAX)
        .max(1)
}

fn rescaled_batch(batch: usize, target_ns: u64, elapsed_ns: u64) -> usize {
    let scaled = u128::try_from(batch)
        .unwrap_or(u128::MAX)
        .saturating_mul(u128::from(target_ns))
        .div_ceil(u128::from(elapsed_ns.max(1)));
    usize::try_from(scaled)
        .unwrap_or(usize::MAX)
        .max(batch.saturating_add(1))
}

struct PairedMeasured {
    arm_a_stats: Stats,
    arm_b_stats: Stats,
    ratio: RatioStats,
    arm_a_reference: Arc<[String]>,
    arm_b_reference: Arc<[String]>,
    arm_a_output_bytes: usize,
    arm_b_output_bytes: usize,
}

#[derive(PartialEq, Eq)]
struct ParseReference {
    serialized: Vec<String>,
    accepted_revisions: usize,
    recovery_revisions: usize,
    warning_revisions: usize,
    unsupported_revisions: usize,
    diagram_types_ordered: Vec<String>,
    diagram_types: Vec<String>,
}

impl ParseReference {
    fn output_bytes(&self) -> usize {
        self.serialized.iter().map(String::len).sum()
    }

    fn output_sha256(&self) -> String {
        sha256_hex(self.serialized.concat().as_bytes())
    }
}

fn parse_results_reference(parsed_results: &[ParseResult]) -> Result<ParseReference, String> {
    let mut serialized = Vec::with_capacity(parsed_results.len());
    let mut accepted_revisions = 0;
    let mut recovery_revisions = 0;
    let mut warning_revisions = 0;
    let mut unsupported_revisions = 0;
    let mut diagram_types_ordered = Vec::with_capacity(parsed_results.len());
    for parsed in parsed_results {
        let diagram_type = parsed.ir.diagram_type.as_str();
        let recovered = parsed.parse_mode().as_str() == "recover";
        let warned = !parsed.warnings.is_empty() || parsed.ir.has_warnings();
        let unsupported = parsed.ir.diagram_type.support_label() != "full";
        recovery_revisions += usize::from(recovered);
        warning_revisions += usize::from(warned);
        unsupported_revisions += usize::from(unsupported);
        if diagram_type != "unknown"
            && !parsed.ir.has_errors()
            && !recovered
            && !warned
            && !unsupported
        {
            accepted_revisions += 1;
        }
        diagram_types_ordered.push(diagram_type.to_owned());
        serialized.push(
            serde_json::to_string(&parsed)
                .map_err(|error| format!("cannot serialize parser reference: {error}"))?,
        );
    }
    let mut diagram_types = diagram_types_ordered.clone();
    diagram_types.sort();
    diagram_types.dedup();
    Ok(ParseReference {
        serialized,
        accepted_revisions,
        recovery_revisions,
        warning_revisions,
        unsupported_revisions,
        diagram_types_ordered,
        diagram_types,
    })
}

fn parse_reference(texts: &[String]) -> Result<ParseReference, String> {
    parse_results_reference(&parse_all(texts))
}

fn parse_all(texts: &[String]) -> Vec<ParseResult> {
    texts
        .iter()
        .map(|text| parse(std::hint::black_box(text.as_str())))
        .collect()
}

fn run_parse_batch(
    executor: &RenderExecutor,
    item: &CorpusItem,
    batch: usize,
) -> Arc<[ParseResult]> {
    let mut last = None;
    for _ in 0..batch {
        let parsed = executor.parse_all_versioned(&item.texts, &item.revision_key);
        std::hint::black_box(&parsed);
        last = Some(parsed);
    }
    last.expect("calibrated parse batch is non-zero")
}

fn calibrate_parse_batch(executor: &RenderExecutor, item: &CorpusItem) -> usize {
    let mut fastest_warmup = u64::MAX;
    for _ in 0..item.warmup.max(1) {
        let t0 = Instant::now();
        std::hint::black_box(executor.parse_all_versioned(&item.texts, &item.revision_key));
        fastest_warmup =
            fastest_warmup.min(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    let mut batch = calibrated_batch(executor.min_sample_ns, fastest_warmup);
    for _ in 0..4 {
        let t0 = Instant::now();
        let parsed = run_parse_batch(executor, item, batch);
        let elapsed = u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX);
        std::hint::black_box(parsed);
        if elapsed >= executor.calibration_target_ns {
            return batch;
        }
        batch = rescaled_batch(batch, executor.calibration_target_ns, elapsed);
    }
    batch
}

struct ParseArmTiming {
    per_job_ns: u64,
    integrated_ns: u64,
    reference: ParseReference,
}

fn time_parse_arm(
    executor: &RenderExecutor,
    item: &CorpusItem,
    batch: usize,
) -> Result<ParseArmTiming, String> {
    let t0 = Instant::now();
    let parsed = run_parse_batch(executor, item, batch);
    let integrated_ns = u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX);
    Ok(ParseArmTiming {
        per_job_ns: integrated_ns / u64::try_from(batch).unwrap_or(1).max(1),
        integrated_ns,
        reference: parse_results_reference(&parsed)?,
    })
}

struct ParseMeasured {
    work_stats: Stats,
    null_ratio: RatioStats,
    reference: ParseReference,
    batch: usize,
    observed_threads: usize,
    work_integrated_samples_ns: Vec<u64>,
    null_integrated_samples_ns: Vec<u64>,
}

fn probe_parse_threads(executor: &RenderExecutor, item: &CorpusItem, batch: usize) -> usize {
    let mut observed = HashSet::new();
    for _ in 0..batch {
        observed.insert(std::thread::current().id());
        std::hint::black_box(executor.parse_all_versioned(&item.texts, &item.revision_key));
    }
    observed.len()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "nanosecond timing magnitudes fit f64 exactly enough for ratio statistics"
)]
fn measure_parse(
    executor: &RenderExecutor,
    item: &CorpusItem,
    rounds: usize,
) -> Result<ParseMeasured, String> {
    let before = parse_reference(&item.texts)?;
    let batch = calibrate_parse_batch(executor, item);
    let observed_threads = probe_parse_threads(executor, item, batch);
    let mut ratios = Vec::with_capacity(rounds);
    let mut null_integrated_samples_ns = Vec::with_capacity(rounds.saturating_mul(2));
    for round in 0..rounds {
        let (a, b) = if round.is_multiple_of(2) {
            (
                time_parse_arm(executor, item, batch)?,
                time_parse_arm(executor, item, batch)?,
            )
        } else {
            let b = time_parse_arm(executor, item, batch)?;
            let a = time_parse_arm(executor, item, batch)?;
            (a, b)
        };
        if a.reference != before || b.reference != before {
            return Err(format!(
                "{}: nondeterministic parser output in A/A sample {}",
                item.id,
                round + 1
            ));
        }
        ratios.push(a.per_job_ns as f64 / b.per_job_ns.max(1) as f64);
        null_integrated_samples_ns.extend([a.integrated_ns, b.integrated_ns]);
    }
    let mut work_samples = Vec::with_capacity(rounds);
    let mut work_integrated_samples_ns = Vec::with_capacity(rounds);
    for round in 0..rounds {
        let measured = time_parse_arm(executor, item, batch)?;
        if measured.reference != before {
            return Err(format!(
                "{}: nondeterministic parser output in effect sample {}",
                item.id,
                round + 1
            ));
        }
        work_samples.push(measured.per_job_ns);
        work_integrated_samples_ns.push(measured.integrated_ns);
    }
    if work_integrated_samples_ns
        .iter()
        .chain(&null_integrated_samples_ns)
        .any(|sample| *sample < executor.min_sample_ns)
    {
        return Err(format!(
            "{}: at least one integrated parse effect/null sample missed the {} ns floor",
            item.id, executor.min_sample_ns
        ));
    }
    let after = parse_reference(&item.texts)?;
    if before != after {
        return Err(format!(
            "{}: nondeterministic parser output across timed samples",
            item.id
        ));
    }
    Ok(ParseMeasured {
        work_stats: stats(work_samples),
        null_ratio: ratio_stats(&ratios),
        reference: before,
        batch,
        observed_threads,
        work_integrated_samples_ns,
        null_integrated_samples_ns,
    })
}

/// Calibrate off the faster arm; both the A/A and A/B routines then use this exact batch.
fn render_item(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg: &Arc<SvgRenderConfig>,
) -> Arc<[String]> {
    executor.render_all_versioned(&item.texts, cfg, &item.revision_key)
}

fn calibrate_batch(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg_a: &Arc<SvgRenderConfig>,
    cfg_b: &Arc<SvgRenderConfig>,
) -> usize {
    let mut fastest_warmup = u64::MAX;
    for _ in 0..item.warmup.max(1) {
        for cfg in [cfg_a, cfg_b] {
            let t0 = Instant::now();
            let output = render_item(executor, item, cfg);
            std::hint::black_box(&output);
            fastest_warmup =
                fastest_warmup.min(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
        }
    }
    if std::env::var_os("FM_H2H_FORCE_PROFILE").is_some() {
        return 1;
    }

    let mut batch = calibrated_batch(executor.min_sample_ns, fastest_warmup);
    // A single-job warmup can overestimate the steady-state cost once caches and the worker pool
    // are hot. Measure the integrated batch itself and scale proportionally until it clears a 50%
    // headroom target. The driver separately fails closed unless measured p50 still reaches the
    // declared minimum, so calibration cannot silently bless a short sample.
    for _ in 0..4 {
        let mut fastest_elapsed = u64::MAX;
        for cfg in [cfg_a, cfg_b] {
            let t0 = Instant::now();
            for _ in 0..batch {
                let output = render_item(executor, item, cfg);
                std::hint::black_box(&output);
            }
            fastest_elapsed =
                fastest_elapsed.min(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
        }
        if fastest_elapsed >= executor.calibration_target_ns {
            return batch;
        }
        batch = rescaled_batch(batch, executor.calibration_target_ns, fastest_elapsed);
    }
    batch
}

fn time_arm(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg: &Arc<SvgRenderConfig>,
    batch: usize,
    reference_len: usize,
    stable: &mut bool,
) -> u64 {
    let t0 = Instant::now();
    for _ in 0..batch {
        let output = render_item(executor, item, cfg);
        // Full byte comparison stays outside the timed region; the O(1) length check catches drift
        // during the rounds without charging a multi-megabyte comparison to the arm.
        *stable &= output.iter().map(String::len).sum::<usize>() == reference_len;
        std::hint::black_box(&output);
    }
    let elapsed = u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX);
    elapsed / u64::try_from(batch).unwrap_or(1).max(1)
}

/// Measure two arms back-to-back inside every round and alternate their order. The median is taken
/// over per-round ratios. This same routine is called first as A/A and then as A/B.
#[expect(
    clippy::cast_precision_loss,
    reason = "nanosecond timing magnitudes fit f64 exactly enough for ratio statistics"
)]
fn paired(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg_a: &Arc<SvgRenderConfig>,
    cfg_b: &Arc<SvgRenderConfig>,
    batch: usize,
    rounds: usize,
) -> Result<PairedMeasured, String> {
    let arm_a_reference = render_item(executor, item, cfg_a);
    let arm_b_reference = render_item(executor, item, cfg_b);
    let arm_a_output_bytes = arm_a_reference.iter().map(String::len).sum();
    let arm_b_output_bytes = arm_b_reference.iter().map(String::len).sum();
    let mut arm_a_samples = Vec::with_capacity(rounds);
    let mut arm_b_samples = Vec::with_capacity(rounds);
    let mut ratios = Vec::with_capacity(rounds);
    let mut arm_a_stable = true;
    let mut arm_b_stable = true;

    for round in 0..rounds {
        let (arm_a_ns, arm_b_ns) = if round.is_multiple_of(2) {
            let a = time_arm(
                executor,
                item,
                cfg_a,
                batch,
                arm_a_output_bytes,
                &mut arm_a_stable,
            );
            let b = time_arm(
                executor,
                item,
                cfg_b,
                batch,
                arm_b_output_bytes,
                &mut arm_b_stable,
            );
            (a, b)
        } else {
            let b = time_arm(
                executor,
                item,
                cfg_b,
                batch,
                arm_b_output_bytes,
                &mut arm_b_stable,
            );
            let a = time_arm(
                executor,
                item,
                cfg_a,
                batch,
                arm_a_output_bytes,
                &mut arm_a_stable,
            );
            (a, b)
        };
        arm_a_samples.push(arm_a_ns);
        arm_b_samples.push(arm_b_ns);
        ratios.push(arm_a_ns as f64 / arm_b_ns.max(1) as f64);
    }

    let arm_a_exact = render_item(executor, item, cfg_a) == arm_a_reference;
    let arm_b_exact = render_item(executor, item, cfg_b) == arm_b_reference;
    if !arm_a_stable || !arm_b_stable || !arm_a_exact || !arm_b_exact {
        return Err(format!("{}: nondeterministic SVG across renders", item.id));
    }
    Ok(PairedMeasured {
        arm_a_stats: stats(arm_a_samples),
        arm_b_stats: stats(arm_b_samples),
        ratio: ratio_stats(&ratios),
        arm_a_reference,
        arm_b_reference,
        arm_a_output_bytes,
        arm_b_output_bytes,
    })
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: headtohead <corpus.json> [dump-svg-dir]");
        std::process::exit(2);
    });
    // Optional: write each item's final default/lean SVG to <dir>/<id>.{default,lean}.svg. Two uses:
    // settling output-contract questions against mermaid's real output, and pinning byte-identity of
    // the lean profile across a refactor (see bd-b2b6).
    let dump_dir = std::env::args().nth(2);
    if let Some(dir) = dump_dir.as_deref()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("cannot create {dir}: {e}");
        std::process::exit(2);
    }
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(2);
    });
    let items: Vec<CorpusItem> = serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("cannot parse {path}: {e}");
        std::process::exit(2);
    });
    let executor = RenderExecutor::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let workload_mode = WorkloadMode::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    if workload_mode == WorkloadMode::Parse && executor.threads != 1 {
        eprintln!("parse-only comparisons require FM_H2H_THREADS=1");
        std::process::exit(2);
    }
    let affinity = process_affinity(executor.available_parallelism);
    let affinity_source = if affinity.mask.as_deref() == Some("all-visible") {
        "portable_visible_cpu_fallback"
    } else {
        "linux_proc_status"
    };

    // Line 1 of stdout: which ELF produced everything below it.
    let (elf_sha256, elf_bytes) = self_elf_sha256();
    println!(
        "{}",
        serde_json::json!({
            "engine": "frankenmermaid",
            "id": "__binary__",
            "record": "binary",
            "elf_sha256": elf_sha256,
            "elf_bytes": elf_bytes,
            "worker_threads": executor.threads,
            "thread_count_requested": executor.threads,
            "thread_probe_required": executor.thread_probe_enabled,
            "available_parallelism": executor.available_parallelism,
            "oversubscribed": executor.oversubscribed(),
            "affinity_mask": affinity.mask.as_deref(),
            "affinity_cpus": &affinity.cpus,
            "affinity_source": affinity_source,
            "min_sample_ns": executor.min_sample_ns,
            "calibration_target_ns": executor.calibration_target_ns,
            "execution_model": executor.execution_model(),
            "persistent_parse_plan": executor.persistent_parse_plan,
            "persistent_parse_snapshot": executor.persistent_parse_snapshot,
            "persistent_render_snapshot": executor.persistent_render_snapshot,
            "content_keyed_render_snapshot": executor.content_keyed_render_snapshot,
            "revision_keyed_render_snapshot": executor.revision_keyed_render_snapshot,
            "rematerialize_batch_inputs": executor.rematerialize_batch_inputs,
            "shared_prefix_reuse": executor.shared_prefix_reuse,
            "shard_schedule": if executor.balanced_shards { "balanced_lpt_bytes" } else { "fixed_contiguous" },
            "measurement_mode": workload_mode.as_str(),
            "measurement_boundary": workload_mode.boundary(),
        })
    );

    // Measurement aid. Each item is normally timed twice, once per profile, so `perf stat` on the whole
    // process cannot attribute instructions to one of them. Forcing BOTH passes to the same profile makes
    // the process's instruction count proportional to that profile alone, which turns a load-sensitive
    // wall-clock A/B into a deterministic, load-immune one. Unset for normal runs.
    let (default_cfg, lean_cfg) = match std::env::var("FM_H2H_FORCE_PROFILE").as_deref() {
        Ok("lean") => (Arc::new(lean_config()), Arc::new(lean_config())),
        Ok("default") => (
            Arc::new(SvgRenderConfig::default()),
            Arc::new(SvgRenderConfig::default()),
        ),
        _ => (
            Arc::new(SvgRenderConfig::default()),
            Arc::new(lean_config()),
        ),
    };
    let mut failed = false;

    for item in &items {
        if item.texts.is_empty() {
            failed = true;
            eprintln!("[frankenmermaid] FAIL {}: no revisions", item.id);
            continue;
        }
        // Size is reported two ways because a multi-document item can mean two different things.
        // For an edit trace the interesting size is the largest revision (the session grows). For a
        // doc build it is the total across the batch -- reporting only the last document would
        // describe a 40-diagram CI job by whichever diagram happened to be last. Single-shot items
        // have one revision, so both numbers equal the old one.
        let (mut nodes, mut edges, mut nodes_total, mut edges_total) = (0, 0, 0, 0);
        for text in item.texts.iter() {
            let parsed = parse(text);
            nodes = nodes.max(parsed.ir.nodes.len());
            edges = edges.max(parsed.ir.edges.len());
            nodes_total += parsed.ir.nodes.len();
            edges_total += parsed.ir.edges.len();
        }

        if workload_mode == WorkloadMode::Parse {
            let rounds = item.reps.max(MIN_NULL_ROUNDS);
            let measured = match measure_parse(&executor, item, rounds) {
                Ok(value) => value,
                Err(error) => {
                    failed = true;
                    eprintln!("[frankenmermaid] FAIL {error}");
                    println!(
                        "{}",
                        serde_json::json!({
                            "engine": "frankenmermaid",
                            "id": item.id,
                            "status": "error",
                            "error": error,
                            "measurement_mode": workload_mode.as_str(),
                            "measurement_boundary": workload_mode.boundary(),
                        })
                    );
                    continue;
                }
            };
            let joined_input = item.texts.join(REVISION_SEP);
            let output_sha256 = measured.reference.output_sha256();
            println!(
                "{}",
                serde_json::json!({
                    "engine": "frankenmermaid",
                    "id": item.id,
                    "status": "ok",
                    "measurement_mode": workload_mode.as_str(),
                    "measurement_boundary": workload_mode.boundary(),
                    "warmup": item.warmup,
                    "batch": measured.batch,
                    "worker_threads": 1,
                    "thread_count_requested": 1,
                    "thread_count_actually_used": measured.observed_threads,
                    "thread_probe": {
                        "method": "instrumented_calling_thread_id_union_over_exact_parse_workload",
                        "probe_batch": measured.batch,
                        "caller_workers_observed": measured.observed_threads,
                        "portable_across_isa": true,
                        "inside_timed_region": false,
                    },
                    "available_parallelism": executor.available_parallelism,
                    "oversubscribed": false,
                    "affinity_mask": affinity.mask.as_deref(),
                    "affinity_cpus": &affinity.cpus,
                    "affinity_source": affinity_source,
                    "min_sample_ns": executor.min_sample_ns,
                    "calibration_target_ns": executor.calibration_target_ns,
                    "effect_integrated_samples_ns": &measured.work_integrated_samples_ns,
                    "null_integrated_samples_ns": &measured.null_integrated_samples_ns,
                    "execution_model": "single_calling_thread",
                    "revisions": item.texts.len(),
                    "input_sha256": sha256_hex(joined_input.as_bytes()),
                    "input_bytes": joined_input.len(),
                    "nodes": nodes,
                    "edges": edges,
                    "nodes_total": nodes_total,
                    "edges_total": edges_total,
                    "parse_ns": ns_json(&measured.work_stats),
                    "cv_pct": (measured.work_stats.cv_pct * 100.0).round() / 100.0,
                    "mad_pct": (measured.work_stats.mad_pct * 100.0).round() / 100.0,
                    "null_control": ratio_json(
                        &measured.null_ratio,
                        "aa_null",
                        &output_sha256,
                        &output_sha256,
                    ),
                    "parse_accepted_revisions": measured.reference.accepted_revisions,
                    "parse_recovery_revisions": measured.reference.recovery_revisions,
                    "parse_warning_revisions": measured.reference.warning_revisions,
                    "parse_unsupported_revisions": measured.reference.unsupported_revisions,
                    "parse_diagram_types_ordered": &measured.reference.diagram_types_ordered,
                    "parse_diagram_types": &measured.reference.diagram_types,
                    "parse_result_bytes": measured.reference.output_bytes(),
                    "parse_result_sha256": output_sha256,
                })
            );
            eprintln!(
                "[frankenmermaid] ok   {}  parse-p50={:.3}ms null={:.6} [{:.6},{:.6}] accepted={}/{}",
                item.id,
                f64::from(u32::try_from(measured.work_stats.p50 / 1000).unwrap_or(u32::MAX))
                    / 1000.0,
                measured.null_ratio.median,
                measured.null_ratio.ci95_lo,
                measured.null_ratio.ci95_hi,
                measured.reference.accepted_revisions,
                item.texts.len(),
            );
            continue;
        }

        let batch = calibrate_batch(&executor, item, &default_cfg, &lean_cfg);
        let thread_count_actually_used = executor
            .thread_probe_enabled
            .then(|| executor.probe_operation_threads(&item.texts, &default_cfg, batch));
        let rounds = item.reps.max(MIN_NULL_ROUNDS);
        let null_run = match paired(&executor, item, &default_cfg, &default_cfg, batch, rounds) {
            Ok(v) => v,
            Err(e) => {
                failed = true;
                eprintln!("[frankenmermaid] FAIL {e}");
                println!(
                    "{}",
                    serde_json::json!({
                        "engine": "frankenmermaid", "id": item.id, "status": "error", "error": e,
                    })
                );
                continue;
            }
        };
        if null_run.arm_a_reference != null_run.arm_b_reference {
            failed = true;
            eprintln!("[frankenmermaid] FAIL {}: A/A output mismatch", item.id);
            continue;
        }
        // Same paired routine, same invocation, same batch: A/A first, then the real default/lean A/B.
        let profile_run = match paired(&executor, item, &default_cfg, &lean_cfg, batch, rounds) {
            Ok(v) => v,
            Err(e) => {
                failed = true;
                eprintln!("[frankenmermaid] FAIL {e}");
                continue;
            }
        };

        let (default_stats, lean_stats) = (&profile_run.arm_a_stats, &profile_run.arm_b_stats);
        if let Some(bad) = profile_run
            .arm_a_reference
            .iter()
            .find(|svg| !svg.starts_with("<svg") || !svg.ends_with("</svg>"))
        {
            failed = true;
            eprintln!(
                "[frankenmermaid] FAIL {}: a revision's output is not a bare <svg> document ({} bytes)",
                item.id,
                bad.len()
            );
            continue;
        }

        let joined_input = item.texts.join(REVISION_SEP);

        if let Some(dir) = dump_dir.as_deref() {
            let last = |v: &[String]| v.last().cloned().unwrap_or_default();
            let _ = std::fs::write(
                format!("{dir}/{}.default.svg", item.id),
                last(&profile_run.arm_a_reference),
            );
            let _ = std::fs::write(
                format!("{dir}/{}.lean.svg", item.id),
                last(&profile_run.arm_b_reference),
            );
            // Cross-engine equivalence (`bd-evx6`) needs EVERY revision, not just the last: a
            // 500-diagram CI batch is one item whose revisions are the 500 diagrams, and checking
            // only the last would leave 499 unverified. Default profile only -- that is the arm
            // compared against mermaid; `lean` is an internal self-speedup.
            //
            // `output_sha256` below is the hash of these same bytes concatenated, so the checker can
            // prove the SVGs it inspected are the ones the timed rounds produced.
            if std::env::var_os("FM_H2H_DUMP_ALL").is_some() {
                for (revision, svg) in profile_run.arm_a_reference.iter().enumerate() {
                    let _ = std::fs::write(
                        format!("{dir}/{}.rev{revision:05}.default.svg", item.id),
                        svg,
                    );
                }
            }
        }

        let default_sha256 = sha256_hex(profile_run.arm_a_reference.concat().as_bytes());
        let lean_sha256 = sha256_hex(profile_run.arm_b_reference.concat().as_bytes());
        let null_a_sha256 = sha256_hex(null_run.arm_a_reference.concat().as_bytes());
        let null_b_sha256 = sha256_hex(null_run.arm_b_reference.concat().as_bytes());
        println!(
            "{}",
            serde_json::json!({
                "engine": "frankenmermaid",
                "id": item.id,
                "status": "ok",
                "measurement_mode": workload_mode.as_str(),
                "measurement_boundary": workload_mode.boundary(),
                "warmup": item.warmup,
                "batch": batch,
                "worker_threads": executor.threads,
                "thread_count_requested": executor.threads,
                "thread_count_actually_used": thread_count_actually_used,
                "thread_probe": thread_count_actually_used.map(|observed| serde_json::json!({
                    "method": "instrumented_caller_worker_union_over_exact_workload",
                    "probe_batch": batch,
                    "caller_workers_observed": observed,
                    "portable_across_isa": true,
                    "inside_timed_region": false,
                })),
                "available_parallelism": executor.available_parallelism,
                "oversubscribed": executor.oversubscribed(),
                "affinity_mask": affinity.mask.as_deref(),
                "affinity_cpus": &affinity.cpus,
                "affinity_source": affinity_source,
                "min_sample_ns": executor.min_sample_ns,
                "calibration_target_ns": executor.calibration_target_ns,
                "execution_model": executor.execution_model(),
                "persistent_parse_plan": executor.persistent_parse_plan,
                "shared_prefix_reuse": executor.shared_prefix_reuse,
                "shard_schedule": if executor.balanced_shards { "balanced_lpt_bytes" } else { "fixed_contiguous" },
                "revisions": item.texts.len(),
                "input_sha256": sha256_hex(joined_input.as_bytes()),
                "input_bytes": joined_input.len(),
                "nodes": nodes,
                "edges": edges,
                "nodes_total": nodes_total,
                "edges_total": edges_total,
                "pipeline_ns": ns_json(default_stats),
                "cv_pct": (default_stats.cv_pct * 100.0).round() / 100.0,
                "mad_pct": (default_stats.mad_pct * 100.0).round() / 100.0,
                "pipeline_lean_ns": ns_json(lean_stats),
                "lean_cv_pct": (lean_stats.cv_pct * 100.0).round() / 100.0,
                "lean_mad_pct": (lean_stats.mad_pct * 100.0).round() / 100.0,
                "null_control": ratio_json(
                    &null_run.ratio,
                    "aa_null",
                    &null_a_sha256,
                    &null_b_sha256,
                ),
                "profile_ab": ratio_json(
                    &profile_run.ratio,
                    "default_vs_lean",
                    &default_sha256,
                    &lean_sha256,
                ),
                "output_bytes": profile_run.arm_a_output_bytes,
                "output_bytes_lean": profile_run.arm_b_output_bytes,
                "output_sha256": default_sha256,
                "output_sha256_lean": lean_sha256,
            })
        );
        eprintln!(
            "[frankenmermaid] ok   {}  p50={:.3}ms null={:.6} [{:.6},{:.6}] bytes={} lean={}",
            item.id,
            f64::from(u32::try_from(default_stats.p50 / 1000).unwrap_or(u32::MAX)) / 1000.0,
            null_run.ratio.median,
            null_run.ratio.ci95_lo,
            null_run.ratio.ci95_hi,
            profile_run.arm_a_output_bytes,
            profile_run.arm_b_output_bytes,
        );
    }

    if failed {
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fm_parser::parse;
    use fm_render_svg::SvgRenderConfig;

    use super::{
        CorpusItem, RenderExecutor, WorkloadMode, balanced_shards, bootstrap_median_ci,
        calibrated_batch, contiguous_shards, full_pipeline_parsed, measure_parse, median,
        new_batch_revision_key, parse_cpu_list, parse_results_reference, ratio_stats,
        rescaled_batch, stats,
    };

    /// Right-skewed sizes, the shape every realistic documentation corpus has.
    fn skewed_inputs(count: usize) -> Vec<String> {
        (0..count)
            .map(|index| "x".repeat(8 + (index % 7) * (index % 7) * 40))
            .collect()
    }

    fn assert_exact_partition(shards: &[Box<[u32]>], len: usize) {
        let mut seen: Vec<u32> = shards.iter().flat_map(|s| s.iter().copied()).collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..u32::try_from(len).unwrap()).collect::<Vec<_>>(),
            "every input must be owned exactly once"
        );
        for shard in shards {
            assert!(
                shard.windows(2).all(|w| w[0] < w[1]),
                "each worker renders its inputs in ascending input order"
            );
        }
    }

    #[test]
    fn balanced_shards_partition_every_input_exactly_once() {
        let inputs = skewed_inputs(200);
        for threads in [1, 2, 7, 32, 64, 256] {
            let shards = balanced_shards(&inputs, threads);
            assert_eq!(shards.len(), threads);
            assert_exact_partition(&shards, inputs.len());
            // The control schedule must agree on the partition contract.
            let control = contiguous_shards(inputs.len(), threads);
            assert_exact_partition(&control, inputs.len());
        }
    }

    #[test]
    fn balanced_shards_are_deterministic() {
        let inputs = skewed_inputs(97);
        let first = balanced_shards(&inputs, 16);
        for _ in 0..8 {
            assert_eq!(balanced_shards(&inputs, 16), first);
        }
    }

    #[test]
    fn balanced_shards_beat_contiguous_on_skewed_input() {
        let inputs = skewed_inputs(200);
        let threads = 32;
        let heaviest = |shards: &[Box<[u32]>]| -> usize {
            shards
                .iter()
                .map(|s| s.iter().map(|&i| inputs[i as usize].len()).sum::<usize>())
                .max()
                .unwrap_or(0)
        };
        let balanced = heaviest(&balanced_shards(&inputs, threads));
        let contiguous = heaviest(&contiguous_shards(inputs.len(), threads));
        assert!(
            balanced < contiguous,
            "balanced heaviest worker {balanced} should undercut contiguous {contiguous}"
        );
    }

    #[test]
    fn balanced_shards_handle_degenerate_shapes() {
        assert!(balanced_shards(&[], 4).iter().all(|s| s.is_empty()));
        assert!(balanced_shards(&skewed_inputs(3), 0).is_empty());
        // More workers than inputs: the surplus workers idle rather than double-owning.
        let shards = balanced_shards(&skewed_inputs(3), 8);
        assert_eq!(shards.len(), 8);
        assert_exact_partition(&shards, 3);
    }

    #[test]
    fn median_averages_the_two_middle_values() {
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
        let measured = stats(vec![3, 1]);
        assert_eq!(measured.p50, 2);
        assert_eq!(measured.samples, vec![1, 3]);
    }

    #[test]
    fn bootstrap_ci_is_exact_for_a_perfect_null() {
        let ratios = vec![1.0; 41];
        assert_eq!(bootstrap_median_ci(&ratios), (1.0, 1.0));
        let stats = ratio_stats(&ratios);
        assert_eq!(stats.median, 1.0);
        assert_eq!(stats.half_width, 0.0);
        assert_eq!(stats.min_decidable_2x, 1.0);
    }

    #[test]
    fn calibrated_batch_rounds_up_to_the_sample_floor() {
        assert_eq!(calibrated_batch(2_000_000, 1_500_000), 2);
        assert_eq!(calibrated_batch(50_000_000, 1_000_001), 50);
        assert_eq!(calibrated_batch(2_000_000, 3_000_000), 1);
        assert_eq!(rescaled_batch(8, 75_000_000, 43_000_000), 14);
    }

    #[test]
    fn persistent_pool_preserves_scalar_output_order_and_bytes() {
        let texts: Arc<[String]> = vec![
            "flowchart LR\nA[First]-->B[Second]".to_owned(),
            "sequenceDiagram\nAlice->>Bob: Hello".to_owned(),
            "classDiagram\nclass User".to_owned(),
            "stateDiagram-v2\n[*]-->Ready".to_owned(),
        ]
        .into();
        let config = Arc::new(SvgRenderConfig::default());
        let scalar = RenderExecutor::new(1).expect("scalar executor");
        let parallel = RenderExecutor::new(2).expect("parallel executor");
        let scalar_output = scalar.render_all(&texts, &config);
        let parallel_output = parallel.render_all(&texts, &config);
        assert_eq!(parallel_output, scalar_output);
    }

    #[test]
    fn shared_subgraph_prefix_reuse_preserves_full_svg_bytes() {
        let prefix = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared ingestion platform\"]\n",
            "    S0[\"Receive & validate events\"]\n",
            "    S1[\"Normalize payload safely\"]\n",
            "    S2[\"Publish canonical records\"]\n",
            "    S0-->S1\n",
            "    S1-->S2\n",
            "  end\n",
        );
        let texts: Arc<[String]> = (0..16)
            .map(|index| {
                format!("{prefix}  S2-->D{index}[\"Independent downstream consumer {index}\"]")
            })
            .collect::<Vec<_>>()
            .into();
        let config = Arc::new(SvgRenderConfig::default());
        let expected = texts
            .iter()
            .map(|text| full_pipeline_parsed(parse(text), &config))
            .collect::<Vec<_>>();
        let executor = RenderExecutor::new(4).expect("parallel executor");
        let actual = executor.render_all(&texts, &config);

        assert_eq!(&*actual, expected);
    }

    #[test]
    fn persistent_render_snapshot_reuses_equal_distinct_input_and_config_arcs() {
        let texts: Arc<[String]> = vec![
            "flowchart LR\nA[First]-->B[Second]".to_owned(),
            "flowchart LR\nC[Third]-->D[Fourth]".to_owned(),
        ]
        .into();
        let same_texts = Arc::clone(&texts);
        let distinct_texts: Arc<[String]> = texts.iter().cloned().collect::<Vec<_>>().into();
        let config = Arc::new(SvgRenderConfig::default());
        let same_config = Arc::clone(&config);
        let distinct_config = Arc::new(SvgRenderConfig::default());
        let executor = RenderExecutor::new(2).expect("parallel executor");

        let first = executor.render_all(&texts, &config);
        let exact_hit = executor.render_all(&same_texts, &same_config);
        assert!(Arc::ptr_eq(&first, &exact_hit));

        let distinct_config_output = executor.render_all(&texts, &distinct_config);
        assert!(Arc::ptr_eq(&first, &distinct_config_output));
        assert_eq!(first, distinct_config_output);
        assert!(Arc::ptr_eq(&first, &executor.render_all(&texts, &config)));

        let distinct_text_output = executor.render_all(&distinct_texts, &config);
        assert!(Arc::ptr_eq(&first, &distinct_text_output));
        assert_eq!(first, distinct_text_output);
    }

    #[test]
    fn revision_key_snapshot_skips_reconstructed_input_validation() {
        let texts: Arc<[String]> = vec![
            "flowchart LR\nA[First]-->B[Second]".to_owned(),
            "sequenceDiagram\nA->>B: request".to_owned(),
        ]
        .into();
        let reconstructed: Arc<[String]> = texts.iter().cloned().collect::<Vec<_>>().into();
        let next_revision: Arc<[String]> = texts.iter().cloned().collect::<Vec<_>>().into();
        let config = Arc::new(SvgRenderConfig::default());
        let revision_key = new_batch_revision_key();
        let next_revision_key = new_batch_revision_key();
        let mut executor = RenderExecutor::new(2).expect("parallel executor");
        executor.content_keyed_render_snapshot = false;

        let first = executor.render_all_versioned(&texts, &config, &revision_key);
        let reconstructed_hit =
            executor.render_all_versioned(&reconstructed, &config, &revision_key);
        assert!(Arc::ptr_eq(&first, &reconstructed_hit));

        let changed_revision =
            executor.render_all_versioned(&next_revision, &config, &next_revision_key);
        assert!(!Arc::ptr_eq(&first, &changed_revision));
        assert_eq!(first, changed_revision);

        let mut control = RenderExecutor::new(2).expect("parallel executor");
        control.content_keyed_render_snapshot = false;
        control.revision_keyed_render_snapshot = false;
        let control_first = control.render_all_versioned(&texts, &config, &revision_key);
        let control_second = control.render_all_versioned(&reconstructed, &config, &revision_key);
        assert!(!Arc::ptr_eq(&control_first, &control_second));
        assert_eq!(control_first, control_second);
    }

    #[test]
    fn exact_only_snapshot_control_misses_equal_distinct_allocations() {
        let texts: Arc<[String]> = vec!["flowchart LR\nA-->B".to_owned()].into();
        let distinct_texts: Arc<[String]> = texts.iter().cloned().collect::<Vec<_>>().into();
        let config = Arc::new(SvgRenderConfig::default());
        let distinct_config = Arc::new(SvgRenderConfig::default());
        let mut executor = RenderExecutor::new(1).expect("scalar executor");
        executor.content_keyed_render_snapshot = false;

        let first = executor.render_all(&texts, &config);
        let second = executor.render_all(&distinct_texts, &distinct_config);
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(first, second);
    }

    #[test]
    fn persistent_parse_plan_reuses_only_the_exact_input_arc() {
        let texts: Arc<[String]> = vec![
            "flowchart LR\nA[First]-->B[Second]".to_owned(),
            "flowchart LR\nC[Third]-->D[Fourth]".to_owned(),
        ]
        .into();
        let same_storage = Arc::clone(&texts);
        let equal_but_distinct: Arc<[String]> = texts.iter().cloned().collect::<Vec<_>>().into();
        let executor = RenderExecutor::new(2).expect("parallel executor");

        let first = executor
            .flowchart_batch_plan(&texts)
            .expect("shared-prefix planning enabled");
        let cached = executor
            .flowchart_batch_plan(&same_storage)
            .expect("same input Arc remains eligible");
        assert!(Arc::ptr_eq(&first, &cached));

        let replaced = executor
            .flowchart_batch_plan(&equal_but_distinct)
            .expect("distinct input Arc remains eligible");
        assert!(!Arc::ptr_eq(&first, &replaced));
    }

    #[test]
    fn persistent_parse_snapshot_reuses_only_the_same_revision_key() {
        let texts: Arc<[String]> = vec![
            "flowchart LR\nA[First]-->B[Second]".to_owned(),
            "sequenceDiagram\nAlice->>Bob: Hello".to_owned(),
        ]
        .into();
        let revision_key = new_batch_revision_key();
        let next_revision_key = new_batch_revision_key();
        let executor = RenderExecutor::new(1).expect("scalar executor");

        let first = executor.parse_all_versioned(&texts, &revision_key);
        let cached = executor.parse_all_versioned(&texts, &revision_key);
        assert!(Arc::ptr_eq(&first, &cached));

        let next_revision = executor.parse_all_versioned(&texts, &next_revision_key);
        assert!(!Arc::ptr_eq(&first, &next_revision));
        assert!(
            parse_results_reference(&first).expect("first reference")
                == parse_results_reference(&next_revision).expect("next reference")
        );

        let mut control = RenderExecutor::new(1).expect("scalar executor");
        control.persistent_parse_snapshot = false;
        let control_first = control.parse_all_versioned(&texts, &revision_key);
        let control_second = control.parse_all_versioned(&texts, &revision_key);
        assert!(!Arc::ptr_eq(&control_first, &control_second));
        assert!(
            parse_results_reference(&control_first).expect("control first reference")
                == parse_results_reference(&control_second).expect("control second reference")
        );
    }

    #[test]
    fn operation_probe_reports_workers_that_execute_diagrams() {
        let texts: Arc<[String]> = (0..64)
            .map(|index| format!("flowchart LR\nA{index}[First]-->B{index}[Second]"))
            .collect::<Vec<_>>()
            .into();
        let config = Arc::new(SvgRenderConfig::default());
        let scalar = RenderExecutor::new(1).expect("scalar executor");
        let parallel = RenderExecutor::new(2).expect("parallel executor");
        assert_eq!(scalar.probe_operation_threads(&texts, &config, 2), 1);
        assert_eq!(parallel.probe_operation_threads(&texts, &config, 2), 2);
    }

    #[test]
    fn cpu_list_parser_expands_ranges_and_deduplicates() {
        assert_eq!(
            parse_cpu_list("0-3,8,10-11,8").expect("valid CPU list"),
            vec![0, 1, 2, 3, 8, 10, 11]
        );
        assert!(parse_cpu_list("4-2").is_err());
    }

    #[test]
    fn parse_mode_produces_a_deterministic_full_result_signature() {
        let item = CorpusItem {
            id: "parse-mode-test".to_owned(),
            texts: vec![
                "flowchart LR\nA[First]-->B[Second]".to_owned(),
                "flowchart TD\nC[Third]-->D[Fourth]".to_owned(),
            ]
            .into(),
            revision_key: new_batch_revision_key(),
            reps: 9,
            warmup: 1,
        };
        let executor = RenderExecutor::new(1).expect("scalar executor");
        let measured = measure_parse(&executor, &item, 9).expect("parse measurement");
        assert_eq!(measured.reference.accepted_revisions, item.texts.len());
        assert_eq!(measured.reference.recovery_revisions, 0);
        assert_eq!(measured.reference.warning_revisions, 0);
        assert_eq!(measured.reference.unsupported_revisions, 0);
        assert_eq!(
            measured.reference.diagram_types,
            vec!["flowchart".to_owned()]
        );
        assert_eq!(
            measured.reference.diagram_types_ordered,
            vec!["flowchart".to_owned(), "flowchart".to_owned()]
        );
        assert_eq!(measured.work_stats.n, 9);
        assert_eq!(measured.null_ratio.n, 9);
        assert_eq!(measured.work_integrated_samples_ns.len(), 9);
        assert_eq!(measured.null_integrated_samples_ns.len(), 18);
        assert!(
            measured
                .work_integrated_samples_ns
                .iter()
                .chain(&measured.null_integrated_samples_ns)
                .all(|sample| *sample >= executor.min_sample_ns)
        );
        assert_eq!(measured.observed_threads, 1);
        assert_eq!(measured.reference.output_sha256().len(), 64);
    }

    #[test]
    fn workload_modes_report_distinct_boundaries() {
        assert_eq!(WorkloadMode::Render.boundary(), "parse_layout_render_svg");
        assert_eq!(WorkloadMode::Parse.boundary(), "public_parse_validate");
    }
}
