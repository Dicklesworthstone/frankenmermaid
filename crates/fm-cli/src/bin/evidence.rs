use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const LEDGER_SUBDIR: &str = "evidence/ledger";
const REPORT_PATH: &str = "evidence/ledger/README.md";
const DEFAULT_BUNDLE_RETENTION_DAYS: u32 = 90;
const BUNDLE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_OVERRIDE_POLICY_PATH: &str = ".ci/quality-gates.toml";
const DEFAULT_OVERRIDE_LEDGER_PATH: &str = ".ci/release-gate-overrides.toml";
const DEFAULT_RELEASE_SIGNOFF_SPEC_PATH: &str = ".ci/release-signoff.toml";
const DEFAULT_PERF_BASELINE_PATH: &str = ".ci/perf-baseline.json";
const DEFAULT_PERF_SLO_PATH: &str = ".ci/slo.yaml";
const OVERRIDE_SCHEMA_VERSION: u32 = 1;
const RELEASE_SIGNOFF_SCHEMA_VERSION: u32 = 1;
const PERF_BASELINE_SCHEMA_VERSION: u32 = 1;
const PERF_SLO_SCHEMA_VERSION: u32 = 1;
const PERF_BOOTSTRAP_ITERATIONS: usize = 200;
const MIN_OVERRIDE_REASON_LEN: usize = 16;
const KNOWN_RELEASE_GATES: &[&str] = &[
    "core-check",
    "golden-checksum-guard",
    "property-test-guard",
    "invariant-proof-guard",
    "determinism-guard",
    "cross-platform-determinism-native",
    "cross-platform-determinism-wasm",
    "cross-platform-determinism-compare",
    "performance-regression-guard",
    "degradation-guard",
    "evidence-ledger-guard",
    "decision-contract-guard",
    "release-gate-override-guard",
    "demo-evidence-guard",
    "wasm-build",
    "coverage",
];

#[derive(Debug, Parser)]
#[command(
    name = "evidence",
    version,
    about = "Manage alien-concept evidence ledger entries"
)]
struct Cli {
    /// Project root containing `evidence/` and `.beads/`.
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a seeded evidence ledger entry.
    Add(AddArgs),
    /// Update fields on an existing evidence ledger entry.
    Update(Box<UpdateArgs>),
    /// Generate a markdown summary and optionally check alien-cs bead coverage.
    Report(ReportArgs),
    /// Create a self-contained release evidence bundle with integrity metadata.
    Bundle(BundleArgs),
    /// Verify a previously generated release evidence bundle.
    VerifyBundle(VerifyBundleArgs),
    /// Validate release-gate emergency overrides and emit active scope summary.
    VerifyOverrides(VerifyOverridesArgs),
    /// Generate a release signoff checklist and E2E validation matrix artifact.
    ReleaseSignoff(ReleaseSignoffArgs),
    /// Summarize repeated performance samples into benchmark evidence artifacts.
    PerfReport(PerfReportArgs),
}

#[derive(Debug, Args)]
struct AddArgs {
    /// Concept id, or `all` to materialize every built-in seed.
    concept_id: String,

    /// Overwrite an existing entry if present.
    #[arg(long, default_value_t = false)]
    force: bool,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    concept_id: String,

    #[arg(long)]
    hypothesis: Option<String>,

    #[arg(long)]
    predicted_improvement: Option<String>,

    #[arg(long)]
    predicted_risk: Option<String>,

    #[arg(long)]
    baseline_date: Option<String>,

    #[arg(long)]
    baseline_commit: Option<String>,

    #[arg(long = "baseline-metric", value_parser = parse_metric_assignment)]
    baseline_metrics: Vec<(String, f64)>,

    #[arg(long)]
    implementation_commit: Option<String>,

    #[arg(long = "add-bead")]
    add_beads: Vec<String>,

    #[arg(long)]
    post_date: Option<String>,

    #[arg(long)]
    post_commit: Option<String>,

    #[arg(long = "post-metric", value_parser = parse_metric_assignment)]
    post_metrics: Vec<(String, f64)>,

    #[arg(long, value_enum)]
    decision: Option<DecisionStatus>,

    #[arg(long)]
    decision_rationale: Option<String>,

    #[arg(long)]
    decision_date: Option<String>,

    #[arg(long)]
    note: Vec<String>,
}

#[derive(Debug, Args)]
struct ReportArgs {
    /// Write the generated markdown to this path instead of the default ledger report.
    #[arg(long)]
    out: Option<PathBuf>,

    /// Check `.beads/issues.jsonl` for closed alien-cs beads without ledger coverage.
    #[arg(long, default_value_t = false)]
    check_beads: bool,

    /// Exit non-zero when `--check-beads` finds uncovered alien-cs beads.
    #[arg(long, default_value_t = false)]
    fail_on_missing_beads: bool,

    /// Override the bead jsonl path used by `--check-beads`.
    #[arg(long)]
    beads_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct BundleArgs {
    /// Output directory that will receive a versioned bundle subdirectory.
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// Semantic bundle version. Defaults to the workspace package version.
    #[arg(long)]
    bundle_version: Option<String>,

    /// Release ref or commit label used in the bundle name.
    #[arg(long)]
    release_ref: Option<String>,

    /// Artifact retention policy in days.
    #[arg(long, default_value_t = DEFAULT_BUNDLE_RETENTION_DAYS)]
    retention_days: u32,

    /// Extra artifacts to include in the bundle, typically CI logs and proof outputs.
    #[arg(long = "artifact")]
    artifacts: Vec<PathBuf>,
}

#[derive(Debug, Args)]
struct VerifyBundleArgs {
    /// Path to the generated bundle manifest.
    #[arg(long)]
    manifest: PathBuf,

    /// Require these file classes to be present in the bundle.
    #[arg(long = "require-kind", value_enum)]
    require_kinds: Vec<EvidenceFileKind>,

    /// Minimum acceptable retention period in days.
    #[arg(long, default_value_t = 30)]
    min_retention_days: u32,

    /// Maximum acceptable retention period in days.
    #[arg(long, default_value_t = 365)]
    max_retention_days: u32,
}

#[derive(Debug, Args)]
struct VerifyOverridesArgs {
    /// Path to the quality-gate policy file.
    #[arg(long)]
    policy_path: Option<PathBuf>,

    /// Path to the override ledger file.
    #[arg(long)]
    overrides_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReleaseSignoffArgs {
    /// Path to the checked-in release signoff spec.
    #[arg(long)]
    spec_path: Option<PathBuf>,

    /// Path to a previously generated release-gate override summary JSON.
    #[arg(long)]
    override_summary: Option<PathBuf>,

    /// Path to the combined demo evidence summary JSON.
    #[arg(long)]
    demo_evidence_summary: Option<PathBuf>,

    /// Gate results in the form `gate=success|failure|cancelled|skipped`.
    #[arg(long = "gate-result", value_parser = parse_gate_result_assignment)]
    gate_results: Vec<(String, GateResult)>,

    /// Directory that will receive `summary.json` and `README.md`.
    #[arg(long, default_value = "artifacts/evidence/signoff")]
    out_dir: PathBuf,
}

#[derive(Debug, Args)]
struct PerfReportArgs {
    /// Path to a log file containing benchmark JSON lines.
    #[arg(long)]
    input: PathBuf,

    /// Directory that will receive `summary.json`, `env.json`, and `corpus_manifest.json`.
    #[arg(long)]
    out_dir: PathBuf,

    /// Optional baseline file used for regression comparison.
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Optional SLO policy used for latency/throughput enforcement.
    #[arg(long)]
    slo_policy: Option<PathBuf>,

    /// Warning threshold for p99 regressions vs baseline.
    #[arg(long, default_value_t = 5.0)]
    warn_threshold_pct: f64,

    /// Failure threshold for p99 regressions vs baseline.
    #[arg(long, default_value_t = 10.0)]
    fail_threshold_pct: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Tier {
    A,
    B,
    C,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DecisionStatus {
    Pending,
    Adopt,
    Reject,
    Defer,
    Hybrid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum EvidenceFileKind {
    LedgerReport,
    LedgerEntry,
    DecisionContract,
    EvidenceReference,
    Policy,
    CiArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceEntry {
    concept_id: String,
    concept_name: String,
    graveyard_section: String,
    graveyard_score: f64,
    tier: Tier,
    contract_path: String,
    hypothesis: String,
    predicted_improvement: String,
    predicted_risk: String,
    baseline: MeasurementStage,
    implementation: ImplementationStage,
    post_measurement: MeasurementStage,
    decision: DecisionStage,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseEvidenceBundle {
    schema_version: u32,
    bundle_name: String,
    bundle_version: String,
    release_ref: String,
    retention_days: u32,
    generated_by: String,
    generated_from_root: String,
    summary: ReleaseEvidenceSummary,
    files: Vec<ReleaseEvidenceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseEvidenceSummary {
    tracked_concepts: usize,
    coverage_warning_count: usize,
    file_count: usize,
    decision_contract_count: usize,
    ledger_entry_count: usize,
    ci_artifact_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseEvidenceFile {
    kind: EvidenceFileKind,
    source_path: String,
    bundled_path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Deserialize)]
struct QualityGatePolicyFile {
    #[serde(default)]
    release_gate_overrides: Option<ReleaseGateOverridePolicy>,
    #[allow(dead_code)]
    #[serde(default)]
    release_signoff: Option<ReleaseSignoffPolicy>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseGateOverridePolicy {
    enabled: bool,
    policy_id: String,
    allowed_approvers: Vec<String>,
    max_override_days: u16,
    require_retro_bead: bool,
    require_fix_bead: bool,
    overrides_path: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ReleaseSignoffPolicy {
    enabled: bool,
    spec_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ReleaseGateOverrideLedger {
    #[serde(default = "default_override_schema_version")]
    schema_version: u32,
    #[serde(default)]
    overrides: Vec<ReleaseGateOverrideRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseGateOverrideRecord {
    id: String,
    approver: String,
    created_by: String,
    created_at: String,
    reason: String,
    scope: Vec<String>,
    expires_at: String,
    retro_bead: Option<String>,
    fix_bead: Option<String>,
    exception_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseGateOverrideSummary {
    enabled: bool,
    policy_id: String,
    overrides_path: String,
    active_override_count: usize,
    active_gates: Vec<String>,
    overrides: Vec<ReleaseGateOverrideStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseGateOverrideStatus {
    id: String,
    approver: String,
    scope: Vec<String>,
    expires_at: String,
    active: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum GateResult {
    Success,
    Failure,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseSignoffSpec {
    schema_version: u32,
    #[serde(default)]
    checklist: Vec<ReleaseChecklistSpec>,
    #[serde(default)]
    validation_matrix: Vec<ReleaseMatrixSpec>,
    #[serde(default)]
    risks: Vec<ReleaseRiskSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseChecklistSpec {
    id: String,
    title: String,
    owner: String,
    source: String,
    criterion: String,
    playbook: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseMatrixSpec {
    id: String,
    title: String,
    owner: String,
    source: String,
    surface: String,
    host_kind: String,
    criterion: String,
    playbook: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseRiskSpec {
    id: String,
    title: String,
    owner: String,
    trigger: String,
    mitigation_playbook: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReleaseSignoffReport {
    schema_version: u32,
    generated_at: String,
    spec_path: String,
    overall_pass: bool,
    gate_summary: ReleaseGateStatusSummary,
    checklist: Vec<ReleaseChecklistResult>,
    validation_matrix: Vec<ReleaseValidationMatrixResult>,
    risks: Vec<ReleaseRiskResult>,
}

#[derive(Debug, Clone, Serialize)]
struct ReleaseGateStatusSummary {
    passed_gates: Vec<String>,
    overridden_failing_gates: Vec<String>,
    uncovered_failing_gates: Vec<String>,
    skipped_gates: Vec<String>,
    active_override_gates: Vec<String>,
    release_blocking_pass: bool,
    gate_results: BTreeMap<String, GateResult>,
}

#[derive(Debug, Clone, Serialize)]
struct ReleaseChecklistResult {
    id: String,
    title: String,
    owner: String,
    source: String,
    criterion: String,
    playbook: String,
    pass: bool,
    evidence_path: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReleaseValidationMatrixResult {
    id: String,
    title: String,
    owner: String,
    source: String,
    surface: String,
    host_kind: String,
    criterion: String,
    playbook: String,
    pass: bool,
    evidence_path: String,
    scenario_count: usize,
    profile_count: usize,
    repeat_count: usize,
    total: usize,
    stable_output: usize,
    stable_normalized: usize,
    replay_manifest_path: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReleaseRiskResult {
    id: String,
    title: String,
    owner: String,
    trigger: String,
    mitigation_playbook: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoEvidenceSummary {
    schema_version: u32,
    static_summary: String,
    react_summary: String,
    #[serde(rename = "static")]
    r#static: DemoEvidenceCounts,
    react: DemoEvidenceCounts,
    replay_bundles: DemoReplayBundles,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoEvidenceCounts {
    total: usize,
    stable_output: usize,
    stable_normalized: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoReplayBundles {
    static_manifest: String,
    react_manifest: String,
}

#[derive(Debug, Clone, Deserialize)]
struct E2eSummaryArtifact {
    surface: String,
    host_kind: String,
    repeat: usize,
    #[serde(default)]
    profiles: Vec<String>,
    #[serde(default)]
    scenarios: Vec<String>,
    #[serde(default)]
    replay_bundle: Option<E2eReplayBundle>,
}

#[derive(Debug, Clone, Deserialize)]
struct E2eReplayBundle {
    manifest_path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PerfBaselineFile {
    #[allow(dead_code)]
    schema_version: u32,
    benchmarks: BTreeMap<String, PerfBaselineEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct PerfBaselineEntry {
    p99_ns: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct PerfSloPolicyFile {
    schema_version: u32,
    benchmarks: BTreeMap<String, PerfSloEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct PerfSloEntry {
    #[serde(default)]
    max_p50_ns: Option<u64>,
    #[serde(default)]
    max_p95_ns: Option<u64>,
    #[serde(default)]
    max_p99_ns: Option<u64>,
    #[serde(default)]
    min_median_ops_per_sec: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct PerfLogRecord {
    benchmark: String,
    #[serde(default)]
    nodes: Option<usize>,
    #[serde(default)]
    edges: Option<usize>,
    #[serde(default)]
    ns: Option<u64>,
    #[serde(default)]
    sugiyama_ns: Option<u64>,
    #[serde(default)]
    force_ns: Option<u64>,
    #[serde(default)]
    tree_ns: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct PerfSummaryReport {
    schema_version: u32,
    generated_at: String,
    input_path: String,
    input_sha256: String,
    baseline_path: Option<String>,
    slo_policy_path: Option<String>,
    warn_threshold_pct: f64,
    fail_threshold_pct: f64,
    release_blocking_pass: bool,
    benchmark_count: usize,
    failed_benchmark_count: usize,
    benchmarks: Vec<PerfBenchmarkSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct PerfBenchmarkSummary {
    benchmark: String,
    nodes: Option<usize>,
    edges: Option<usize>,
    sample_count: usize,
    min_ns: u64,
    max_ns: u64,
    mean_ns: f64,
    median_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    median_ops_per_sec: f64,
    median_ci_low_ns: u64,
    median_ci_high_ns: u64,
    p99_ci_low_ns: u64,
    p99_ci_high_ns: u64,
    baseline_p99_ns: Option<u64>,
    regression_vs_baseline_pct: Option<f64>,
    baseline_gate_status: PerfGateStatus,
    slo_gate_status: PerfGateStatus,
    slo: Option<PerfSloEvaluation>,
    gate_status: PerfGateStatus,
}

#[derive(Debug, Clone, Serialize)]
struct PerfSloEvaluation {
    max_p50_ns: Option<u64>,
    max_p95_ns: Option<u64>,
    max_p99_ns: Option<u64>,
    min_median_ops_per_sec: Option<f64>,
    violations: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PerfGateStatus {
    Pass,
    Warn,
    Fail,
    NoBaseline,
}

#[derive(Debug, Clone, Serialize)]
struct PerfEnvironmentFingerprint {
    rustc_version: String,
    cargo_version: String,
    uname: String,
    arch: String,
    os: String,
    cpu_count: usize,
    cpu_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PerfCorpusManifest {
    source: String,
    input_sha256: String,
    benchmarks: Vec<PerfCorpusEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct PerfCorpusEntry {
    benchmark: String,
    nodes: Option<usize>,
    edges: Option<usize>,
    sample_count: usize,
}

#[derive(Debug, Clone)]
struct PerfSampleBucket {
    nodes: Option<usize>,
    edges: Option<usize>,
    samples_ns: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MeasurementStage {
    date: Option<String>,
    commit: Option<String>,
    metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ImplementationStage {
    beads: Vec<String>,
    commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DecisionStage {
    status: DecisionStatus,
    rationale: String,
    date: Option<String>,
}

impl Default for DecisionStage {
    fn default() -> Self {
        Self {
            status: DecisionStatus::Pending,
            rationale: "Not yet evaluated.".to_string(),
            date: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SeedSpec {
    concept_id: &'static str,
    concept_name: &'static str,
    graveyard_section: &'static str,
    graveyard_score: f64,
    tier: Tier,
    contract_path: &'static str,
    hypothesis: &'static str,
    predicted_improvement: &'static str,
    predicted_risk: &'static str,
}

const SEEDS: &[SeedSpec] = &[
    SeedSpec {
        concept_id: "egraph-crossing-min",
        concept_name: "E-Graphs for Crossing Minimization",
        graveyard_section: "§6.6",
        graveyard_score: 3.0,
        tier: Tier::B,
        contract_path: "evidence/contracts/e-graphs-crossing-minimization.md",
        hypothesis: "Equality saturation can outperform the fixed barycenter sweep on dense crossing-heavy graphs.",
        predicted_improvement: "15-30% lower crossing count on dense layered graphs.",
        predicted_risk: "Combinatorial explosion and wasm-unfriendly dependency/runtime costs.",
    },
    SeedSpec {
        concept_id: "swiss-tables-node-edge-maps",
        concept_name: "Swiss Tables for Node/Edge Maps",
        graveyard_section: "§7.7",
        graveyard_score: 3.0,
        tier: Tier::B,
        contract_path: "evidence/contracts/swiss-tables-node-edge-maps.md",
        hypothesis: "Deterministic Swiss-table maps can speed layout hot paths without changing outputs.",
        predicted_improvement: "20% lookup throughput uplift and 5%+ end-to-end layout speedup.",
        predicted_risk: "Determinism or wasm constraints could erase the practical gain.",
    },
    SeedSpec {
        concept_id: "conformal-geometric-algebra",
        concept_name: "Conformal Geometric Algebra",
        graveyard_section: "§12.11",
        graveyard_score: 2.0,
        tier: Tier::C,
        contract_path: "evidence/contracts/conformal-geometric-algebra.md",
        hypothesis: "CGA could simplify geometric transformations and unlock curved/radial layout work.",
        predicted_improvement: "Cleaner geometric composition with limited performance regression.",
        predicted_risk: "Readability, maintenance cost, and wasm portability could be unacceptable.",
    },
    SeedSpec {
        concept_id: "constraint-programming-layout",
        concept_name: "Constraint Programming for Layout",
        graveyard_section: "§9.7",
        graveyard_score: 3.0,
        tier: Tier::B,
        contract_path: "evidence/contracts/constraint-programming-layout.md",
        hypothesis: "Constraint-aware layout can enforce explicit user intent that the current pipeline ignores.",
        predicted_improvement: "Full or near-full satisfaction of SameRank, MinLength, Pin, and OrderInRank constraints.",
        predicted_risk: "Solver complexity and conflicting constraints may hurt latency and ergonomics.",
    },
    SeedSpec {
        concept_id: "bidirectional-lenses",
        concept_name: "Bidirectional Lenses for Diagram/Text Sync",
        graveyard_section: "§6.2",
        graveyard_score: 2.5,
        tier: Tier::B,
        contract_path: "evidence/contracts/bidirectional-lenses.md",
        hypothesis: "Lens-style get/put machinery can enable disciplined source<->diagram round-tripping.",
        predicted_improvement: "Round-trip editing without losing author intent in supported edits.",
        predicted_risk: "Source preservation and lens laws may be too costly for the current parser architecture.",
    },
    SeedSpec {
        concept_id: "incremental-subgraph-relayout",
        concept_name: "Incremental Subgraph Re-Layout",
        graveyard_section: "§6.1",
        graveyard_score: 3.0,
        tier: Tier::A,
        contract_path: "evidence/contracts/incremental-subgraph-relayout.md",
        hypothesis: "Incremental relayout can cut edit latency dramatically for local changes.",
        predicted_improvement: "<10ms local updates on large diagrams with correctness parity against full relayout.",
        predicted_risk: "Invalidation boundaries and correctness proofs may be hard to keep deterministic.",
    },
    SeedSpec {
        concept_id: "fnx-deterministic-decision-contract",
        concept_name: "FNX Deterministic Decision Contract",
        graveyard_section: "FNX Phase 1",
        graveyard_score: 3.0,
        tier: Tier::A,
        contract_path: "evidence/contracts/fnx-deterministic-decision-contract.md",
        hypothesis: "A contract-first FNX decision policy can preserve deterministic behavior while allowing advisory graph analysis to improve diagnostics.",
        predicted_improvement: "Stable, inspectable FNX usage with explicit advisory-vs-authoritative boundaries and deterministic fallback behavior.",
        predicted_risk: "Ambiguous precedence or under-specified fallbacks could make diagnostics misleading and future FNX adoption nondeterministic.",
    },
];

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = canonicalize_root(&cli.root)?;
    match cli.command {
        Command::Add(args) => add_command(&root, &args),
        Command::Update(args) => update_command(&root, *args),
        Command::Report(args) => report_command(&root, args),
        Command::Bundle(args) => bundle_command(&root, args),
        Command::VerifyBundle(args) => verify_bundle_command(&root, args),
        Command::VerifyOverrides(args) => verify_overrides_command(&root, args),
        Command::ReleaseSignoff(args) => release_signoff_command(&root, args),
        Command::PerfReport(args) => perf_report_command(&root, args),
    }
}

fn canonicalize_root(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        path.canonicalize()
            .with_context(|| format!("failed to canonicalize root {}", path.display()))
    } else {
        bail!("root path does not exist: {}", path.display());
    }
}

fn add_command(root: &Path, args: &AddArgs) -> Result<()> {
    ensure_ledger_dir(root)?;
    if args.concept_id == "all" {
        for seed in SEEDS {
            write_entry(root, &seed_entry(seed), args.force)?;
        }
        return Ok(());
    }

    let seed = lookup_seed(&args.concept_id)?;
    write_entry(root, &seed_entry(seed), args.force)
}

fn update_command(root: &Path, args: UpdateArgs) -> Result<()> {
    ensure_ledger_dir(root)?;
    let entry_path = ledger_entry_path(root, &args.concept_id);
    let mut entry = load_entry(&entry_path)
        .with_context(|| format!("failed to load entry for concept {}", args.concept_id))?;

    if let Some(value) = args.hypothesis {
        entry.hypothesis = value;
    }
    if let Some(value) = args.predicted_improvement {
        entry.predicted_improvement = value;
    }
    if let Some(value) = args.predicted_risk {
        entry.predicted_risk = value;
    }
    if let Some(value) = args.baseline_date {
        entry.baseline.date = Some(value);
    }
    if let Some(value) = args.baseline_commit {
        entry.baseline.commit = Some(value);
    }
    for (key, value) in args.baseline_metrics {
        entry.baseline.metrics.insert(key, value);
    }
    if let Some(value) = args.implementation_commit {
        entry.implementation.commit = Some(value);
    }
    for bead in args.add_beads {
        if !entry
            .implementation
            .beads
            .iter()
            .any(|existing| existing == &bead)
        {
            entry.implementation.beads.push(bead);
        }
    }
    entry.implementation.beads.sort();
    if let Some(value) = args.post_date {
        entry.post_measurement.date = Some(value);
    }
    if let Some(value) = args.post_commit {
        entry.post_measurement.commit = Some(value);
    }
    for (key, value) in args.post_metrics {
        entry.post_measurement.metrics.insert(key, value);
    }
    if let Some(value) = args.decision {
        entry.decision.status = value;
    }
    if let Some(value) = args.decision_rationale {
        entry.decision.rationale = value;
    }
    if let Some(value) = args.decision_date {
        entry.decision.date = Some(value);
    }
    entry.notes.extend(args.note);

    save_entry(&entry_path, &entry)
}

fn report_command(root: &Path, args: ReportArgs) -> Result<()> {
    ensure_ledger_dir(root)?;
    let entries = load_all_entries(root)?;
    let mut warnings = Vec::new();
    if args.check_beads {
        let beads_path = args
            .beads_path
            .unwrap_or_else(|| root.join(".beads/issues.jsonl"));
        warnings = find_uncovered_closed_alien_beads(&entries, &beads_path)?;
    }

    let markdown = render_report(&entries, &warnings);
    let out_path = args.out.unwrap_or_else(|| root.join(REPORT_PATH));
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report parent {}", parent.display()))?;
    }
    fs::write(&out_path, &markdown)
        .with_context(|| format!("failed to write report {}", out_path.display()))?;
    print!("{markdown}");

    if args.fail_on_missing_beads && !warnings.is_empty() {
        bail!(
            "found {} alien-cs bead(s) without ledger coverage",
            warnings.len()
        );
    }
    Ok(())
}

fn bundle_command(root: &Path, args: BundleArgs) -> Result<()> {
    ensure_ledger_dir(root)?;
    let retention_days = args.retention_days;
    if !(30..=365).contains(&retention_days) {
        bail!("retention_days must be between 30 and 365");
    }

    let entries = load_all_entries(root)?;
    let warnings = default_bead_warnings(root, &entries)?;
    let report = render_report(&entries, &warnings);
    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report parent {}", parent.display()))?;
    }
    fs::write(&report_path, &report)
        .with_context(|| format!("failed to write report {}", report_path.display()))?;

    let bundle_version = args
        .bundle_version
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let release_ref = args
        .release_ref
        .or_else(discover_release_ref)
        .unwrap_or_else(|| "workspace".to_string());
    let bundle_name = format!(
        "frankenmermaid-evidence-v{}-{}",
        sanitize_bundle_component(&bundle_version),
        sanitize_bundle_component(&release_ref)
    );
    let out_dir = args.out_dir.map_or_else(
        || root.join("artifacts/release-evidence"),
        |path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        },
    );
    let bundle_dir = out_dir.join(&bundle_name);
    if bundle_dir.exists() {
        bail!(
            "bundle output already exists: {} (choose a different --out-dir or --release-ref)",
            bundle_dir.display()
        );
    }
    fs::create_dir_all(bundle_dir.join("files"))
        .with_context(|| format!("failed to create {}", bundle_dir.join("files").display()))?;

    let source_files = collect_bundle_sources(root, &args.artifacts)?;
    let mut files = Vec::new();
    for (kind, relative_path) in source_files {
        let source_path = root.join(&relative_path);
        let bundled_relative = Path::new("files").join(&relative_path);
        let bundled_path = bundle_dir.join(&bundled_relative);
        if let Some(parent) = bundled_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(&source_path, &bundled_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source_path.display(),
                bundled_path.display()
            )
        })?;
        let bytes = fs::metadata(&bundled_path)
            .with_context(|| format!("failed to stat {}", bundled_path.display()))?
            .len();
        files.push(ReleaseEvidenceFile {
            kind,
            source_path: path_to_unix_string(&relative_path),
            bundled_path: path_to_unix_string(&bundled_relative),
            sha256: sha256_file(&bundled_path)?,
            bytes,
        });
    }
    files.sort_by(|left, right| left.source_path.cmp(&right.source_path));

    let summary = ReleaseEvidenceSummary {
        tracked_concepts: entries.len(),
        coverage_warning_count: warnings.len(),
        file_count: files.len(),
        decision_contract_count: files
            .iter()
            .filter(|file| file.kind == EvidenceFileKind::DecisionContract)
            .count(),
        ledger_entry_count: files
            .iter()
            .filter(|file| file.kind == EvidenceFileKind::LedgerEntry)
            .count(),
        ci_artifact_count: files
            .iter()
            .filter(|file| file.kind == EvidenceFileKind::CiArtifact)
            .count(),
    };

    let bundle = ReleaseEvidenceBundle {
        schema_version: BUNDLE_SCHEMA_VERSION,
        bundle_name,
        bundle_version,
        release_ref,
        retention_days,
        generated_by: "cargo run -p fm-cli --bin evidence -- bundle".to_string(),
        generated_from_root: ".".to_string(),
        summary,
        files,
    };

    let manifest_path = bundle_dir.join("manifest.json");
    let manifest_json =
        serde_json::to_string_pretty(&bundle).context("failed to serialize bundle manifest")?;
    fs::write(&manifest_path, manifest_json)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let bundle_readme = render_bundle_readme(&bundle);
    let readme_path = bundle_dir.join("README.md");
    fs::write(&readme_path, bundle_readme)
        .with_context(|| format!("failed to write {}", readme_path.display()))?;

    println!("{}", bundle_dir.display());
    Ok(())
}

fn verify_bundle_command(root: &Path, args: VerifyBundleArgs) -> Result<()> {
    if args.min_retention_days > args.max_retention_days {
        bail!("min_retention_days cannot exceed max_retention_days");
    }

    let manifest_path = if args.manifest.is_absolute() {
        args.manifest
    } else {
        root.join(args.manifest)
    };
    let bundle_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("manifest path has no parent: {}", manifest_path.display()))?;
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read manifest {}", manifest_path.display()))?;
    let bundle: ReleaseEvidenceBundle = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse manifest {}", manifest_path.display()))?;

    if bundle.schema_version != BUNDLE_SCHEMA_VERSION {
        bail!("unexpected bundle schema version {}", bundle.schema_version);
    }
    if !(args.min_retention_days..=args.max_retention_days).contains(&bundle.retention_days) {
        bail!(
            "bundle retention_days {} is outside the accepted range {}..={}",
            bundle.retention_days,
            args.min_retention_days,
            args.max_retention_days
        );
    }

    let mut seen_sources = BTreeSet::new();
    let present_kinds: BTreeSet<EvidenceFileKind> =
        bundle.files.iter().map(|file| file.kind).collect();
    for required_kind in &args.require_kinds {
        if !present_kinds.contains(required_kind) {
            bail!("bundle is missing required file kind {required_kind:?}");
        }
    }

    for file in &bundle.files {
        if !seen_sources.insert(file.source_path.clone()) {
            bail!("duplicate source path in manifest: {}", file.source_path);
        }
        let bundled_path = bundle_dir.join(&file.bundled_path);
        if !bundled_path.exists() {
            bail!("bundled file missing: {}", bundled_path.display());
        }
        let actual_sha = sha256_file(&bundled_path)?;
        if actual_sha != file.sha256 {
            bail!(
                "sha256 mismatch for {}: manifest={}, actual={}",
                bundled_path.display(),
                file.sha256,
                actual_sha
            );
        }
        let actual_bytes = fs::metadata(&bundled_path)
            .with_context(|| format!("failed to stat {}", bundled_path.display()))?
            .len();
        if actual_bytes != file.bytes {
            bail!(
                "byte-size mismatch for {}: manifest={}, actual={}",
                bundled_path.display(),
                file.bytes,
                actual_bytes
            );
        }
    }

    verify_bundle_links(bundle_dir)?;
    Ok(())
}

fn verify_overrides_command(root: &Path, args: VerifyOverridesArgs) -> Result<()> {
    let policy_path = args.policy_path.map_or_else(
        || root.join(DEFAULT_OVERRIDE_POLICY_PATH),
        |path| absolutize_under_root(root, path),
    );
    let policy = load_override_policy(&policy_path)?;

    if !policy.enabled {
        let summary = ReleaseGateOverrideSummary {
            enabled: false,
            policy_id: policy.policy_id,
            overrides_path: path_to_unix_string(Path::new(DEFAULT_OVERRIDE_LEDGER_PATH)),
            active_override_count: 0,
            active_gates: Vec::new(),
            overrides: Vec::new(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).context("serialize override summary")?
        );
        return Ok(());
    }

    let overrides_path = args
        .overrides_path
        .map(|path| absolutize_under_root(root, path))
        .or_else(|| {
            policy
                .overrides_path
                .as_ref()
                .map(|path| absolutize_under_root(root, PathBuf::from(path)))
        })
        .unwrap_or_else(|| root.join(DEFAULT_OVERRIDE_LEDGER_PATH));
    let ledger = load_override_ledger(&overrides_path)?;
    let now = current_time()?;
    let summary = validate_override_ledger(&policy, &ledger, &overrides_path, now)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).context("serialize override summary")?
    );
    Ok(())
}

fn release_signoff_command(root: &Path, args: ReleaseSignoffArgs) -> Result<()> {
    let spec_path = root.join(
        args.spec_path
            .unwrap_or_else(|| PathBuf::from(DEFAULT_RELEASE_SIGNOFF_SPEC_PATH)),
    );
    let spec = load_release_signoff_spec(&spec_path)?;

    let override_summary_path = root.join(args.override_summary.unwrap_or_else(|| {
        PathBuf::from("artifacts/evidence/policies/release-gate-override-summary.json")
    }));
    let override_summary = load_override_summary(&override_summary_path)?;

    let demo_summary_path = root
        .join(args.demo_evidence_summary.unwrap_or_else(|| {
            PathBuf::from("artifacts/evidence/demo/demo-evidence-summary.json")
        }));
    let demo_summary = load_demo_evidence_summary(&demo_summary_path)?;

    let gate_summary = summarize_gate_results(&args.gate_results, &override_summary)?;
    let checklist = evaluate_release_checklist(
        root,
        &spec,
        &gate_summary,
        &override_summary_path,
        &override_summary,
        &demo_summary_path,
        &demo_summary,
    )?;
    let validation_matrix =
        evaluate_release_matrix(root, &spec, &demo_summary_path, &demo_summary)?;
    let risks = spec
        .risks
        .iter()
        .map(|risk| ReleaseRiskResult {
            id: risk.id.clone(),
            title: risk.title.clone(),
            owner: risk.owner.clone(),
            trigger: risk.trigger.clone(),
            mitigation_playbook: risk.mitigation_playbook.clone(),
        })
        .collect::<Vec<_>>();

    let overall_pass = checklist.iter().all(|item| item.pass)
        && validation_matrix.iter().all(|row| row.pass)
        && gate_summary.release_blocking_pass;

    let report = ReleaseSignoffReport {
        schema_version: RELEASE_SIGNOFF_SCHEMA_VERSION,
        generated_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .context("format generated_at")?,
        spec_path: path_to_unix_string(spec_path.strip_prefix(root).unwrap_or(&spec_path)),
        overall_pass,
        gate_summary,
        checklist,
        validation_matrix,
        risks,
    };

    let out_dir = root.join(args.out_dir);
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let summary_path = out_dir.join("summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&report).context("serialize release signoff summary")?,
    )
    .with_context(|| format!("failed to write {}", summary_path.display()))?;
    let readme_path = out_dir.join("README.md");
    fs::write(&readme_path, render_release_signoff_readme(&report))
        .with_context(|| format!("failed to write {}", readme_path.display()))?;

    if !report.overall_pass {
        bail!(
            "release signoff failed: uncovered failing gates={}, failing checklist items={}, failing matrix rows={}",
            report.gate_summary.uncovered_failing_gates.join(", "),
            report.checklist.iter().filter(|item| !item.pass).count(),
            report
                .validation_matrix
                .iter()
                .filter(|row| !row.pass)
                .count()
        );
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("serialize release signoff stdout")?
    );
    Ok(())
}

fn perf_report_command(root: &Path, args: PerfReportArgs) -> Result<()> {
    if !(0.0..=1000.0).contains(&args.warn_threshold_pct) {
        bail!("warn_threshold_pct must be between 0 and 1000");
    }
    if !(0.0..=1000.0).contains(&args.fail_threshold_pct) {
        bail!("fail_threshold_pct must be between 0 and 1000");
    }
    if args.fail_threshold_pct < args.warn_threshold_pct {
        bail!("fail_threshold_pct must be >= warn_threshold_pct");
    }

    let input_path = resolve_from_root(root, &args.input);
    let input_raw = fs::read_to_string(&input_path)
        .with_context(|| format!("failed to read benchmark log {}", input_path.display()))?;
    let input_sha256 = sha256_hex(input_raw.as_bytes());
    let buckets = parse_perf_log(&input_raw);
    if buckets.is_empty() {
        bail!(
            "no benchmark JSON lines found in input log {}",
            input_path.display()
        );
    }

    let baseline_path = args
        .baseline
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PERF_BASELINE_PATH));
    let baseline_path = resolve_from_root(root, &baseline_path);
    let baseline = if baseline_path.exists() {
        Some(load_perf_baseline(&baseline_path)?)
    } else {
        None
    };
    let slo_path = args
        .slo_policy
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PERF_SLO_PATH));
    let slo_path = resolve_from_root(root, &slo_path);
    let slo_policy = if slo_path.exists() {
        Some(load_perf_slo_policy(&slo_path)?)
    } else {
        None
    };

    let mut release_blocking_pass = true;
    let mut failed_benchmark_count = 0;
    let mut summaries = Vec::new();
    for (benchmark, bucket) in &buckets {
        let baseline_entry = baseline
            .as_ref()
            .and_then(|file| file.benchmarks.get(benchmark))
            .cloned();
        let slo_entry = slo_policy
            .as_ref()
            .and_then(|file| file.benchmarks.get(benchmark))
            .cloned();
        let summary = summarize_perf_bucket(
            benchmark,
            bucket,
            baseline_entry,
            slo_entry,
            args.warn_threshold_pct,
            args.fail_threshold_pct,
        )?;
        if summary.gate_status == PerfGateStatus::Fail {
            release_blocking_pass = false;
            failed_benchmark_count += 1;
        }
        summaries.push(summary);
    }

    let report = PerfSummaryReport {
        schema_version: PERF_BASELINE_SCHEMA_VERSION,
        generated_at: current_time()?.format(&Rfc3339)?,
        input_path: input_path.display().to_string(),
        input_sha256: input_sha256.clone(),
        baseline_path: baseline
            .as_ref()
            .map(|_| baseline_path.display().to_string()),
        slo_policy_path: slo_policy.as_ref().map(|_| slo_path.display().to_string()),
        warn_threshold_pct: args.warn_threshold_pct,
        fail_threshold_pct: args.fail_threshold_pct,
        release_blocking_pass,
        benchmark_count: summaries.len(),
        failed_benchmark_count,
        benchmarks: summaries,
    };

    let out_dir = resolve_from_root(root, &args.out_dir);
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create perf out_dir {}", out_dir.display()))?;
    write_json(out_dir.join("summary.json"), &report)?;
    write_json(out_dir.join("env.json"), &capture_perf_environment())?;
    write_json(
        out_dir.join("corpus_manifest.json"),
        &build_perf_corpus_manifest(&buckets, &input_sha256),
    )?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.release_blocking_pass {
        bail!("performance regression exceeded fail threshold");
    }
    Ok(())
}

fn load_release_signoff_spec(path: &Path) -> Result<ReleaseSignoffSpec> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read release signoff spec {}", path.display()))?;
    let spec: ReleaseSignoffSpec = toml::from_str(&raw)
        .with_context(|| format!("failed to parse release signoff spec {}", path.display()))?;
    if spec.schema_version != RELEASE_SIGNOFF_SCHEMA_VERSION {
        bail!(
            "unexpected release signoff spec schema version {}",
            spec.schema_version
        );
    }
    Ok(spec)
}

fn load_override_summary(path: &Path) -> Result<ReleaseGateOverrideSummary> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read override summary {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse override summary {}", path.display()))
}

fn load_demo_evidence_summary(path: &Path) -> Result<DemoEvidenceSummary> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read demo evidence summary {}", path.display()))?;
    let summary: DemoEvidenceSummary = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse demo evidence summary {}", path.display()))?;
    if summary.schema_version != 1 {
        bail!(
            "unexpected demo evidence summary schema version {}",
            summary.schema_version
        );
    }
    Ok(summary)
}

fn summarize_gate_results(
    gate_results: &[(String, GateResult)],
    override_summary: &ReleaseGateOverrideSummary,
) -> Result<ReleaseGateStatusSummary> {
    let known_gates: BTreeSet<&str> = KNOWN_RELEASE_GATES.iter().copied().collect();
    let active_override_gates: BTreeSet<String> =
        override_summary.active_gates.iter().cloned().collect();
    let mut results = BTreeMap::new();
    let mut passed = Vec::new();
    let mut overridden = Vec::new();
    let mut uncovered = Vec::new();
    let mut skipped = Vec::new();

    for (gate, result) in gate_results {
        if !known_gates.contains(gate.as_str()) {
            bail!("release signoff received unknown gate result {gate}");
        }
        if results.insert(gate.clone(), *result).is_some() {
            bail!("duplicate release signoff gate result {gate}");
        }
    }

    for (gate, result) in &results {
        match result {
            GateResult::Success => passed.push(gate.clone()),
            GateResult::Failure | GateResult::Cancelled => {
                if active_override_gates.contains(gate) {
                    overridden.push(gate.clone());
                } else {
                    uncovered.push(gate.clone());
                }
            }
            GateResult::Skipped => skipped.push(gate.clone()),
        }
    }

    Ok(ReleaseGateStatusSummary {
        passed_gates: passed,
        overridden_failing_gates: overridden,
        uncovered_failing_gates: uncovered.clone(),
        skipped_gates: skipped,
        active_override_gates: active_override_gates.into_iter().collect(),
        release_blocking_pass: uncovered.is_empty(),
        gate_results: results,
    })
}

fn evaluate_release_checklist(
    _root: &Path,
    spec: &ReleaseSignoffSpec,
    gate_summary: &ReleaseGateStatusSummary,
    override_summary_path: &Path,
    override_summary: &ReleaseGateOverrideSummary,
    demo_summary_path: &Path,
    demo_summary: &DemoEvidenceSummary,
) -> Result<Vec<ReleaseChecklistResult>> {
    let mut results = Vec::new();
    for item in &spec.checklist {
        let (pass, evidence_path, detail) = match item.source.as_str() {
            "gate_summary" => (
                gate_summary.release_blocking_pass,
                path_to_unix_string(
                    override_summary_path
                        .parent()
                        .unwrap_or(override_summary_path),
                ),
                if gate_summary.release_blocking_pass {
                    format!(
                        "{} gates passed, {} overridden, {} skipped",
                        gate_summary.passed_gates.len(),
                        gate_summary.overridden_failing_gates.len(),
                        gate_summary.skipped_gates.len()
                    )
                } else {
                    format!(
                        "uncovered failing gates: {}",
                        gate_summary.uncovered_failing_gates.join(", ")
                    )
                },
            ),
            "override_summary" => (
                override_summary.enabled,
                path_to_unix_string(override_summary_path),
                format!(
                    "{} active overrides across {} gates",
                    override_summary.active_override_count,
                    override_summary.active_gates.len()
                ),
            ),
            "demo_evidence" => {
                let pass = demo_summary.r#static.total > 0
                    && demo_summary.react.total > 0
                    && demo_summary.r#static.stable_normalized == demo_summary.r#static.total
                    && demo_summary.react.stable_normalized == demo_summary.react.total;
                (
                    pass,
                    path_to_unix_string(demo_summary_path),
                    format!(
                        "static normalized stability: {}/{}; react normalized stability: {}/{}",
                        demo_summary.r#static.stable_normalized,
                        demo_summary.r#static.total,
                        demo_summary.react.stable_normalized,
                        demo_summary.react.total
                    ),
                )
            }
            other => bail!("unknown release checklist source {other}"),
        };
        results.push(ReleaseChecklistResult {
            id: item.id.clone(),
            title: item.title.clone(),
            owner: item.owner.clone(),
            source: item.source.clone(),
            criterion: item.criterion.clone(),
            playbook: item.playbook.clone(),
            pass,
            evidence_path,
            detail,
        });
    }
    Ok(results)
}

fn evaluate_release_matrix(
    root: &Path,
    spec: &ReleaseSignoffSpec,
    demo_summary_path: &Path,
    demo_summary: &DemoEvidenceSummary,
) -> Result<Vec<ReleaseValidationMatrixResult>> {
    let static_summary = load_e2e_summary(root, &demo_summary.static_summary)?;
    let react_summary = load_e2e_summary(root, &demo_summary.react_summary)?;
    let static_manifest = resolve_report_path(root, &demo_summary.replay_bundles.static_manifest);
    let react_manifest = resolve_report_path(root, &demo_summary.replay_bundles.react_manifest);

    let mut rows = Vec::new();
    for item in &spec.validation_matrix {
        let (artifact, counts, manifest_path) = match item.source.as_str() {
            "demo_static" => (&static_summary, &demo_summary.r#static, &static_manifest),
            "demo_react" => (&react_summary, &demo_summary.react, &react_manifest),
            other => bail!("unknown release validation matrix source {other}"),
        };
        if artifact.surface != item.surface {
            bail!(
                "validation matrix {} expected surface {} but found {}",
                item.id,
                item.surface,
                artifact.surface
            );
        }
        if artifact.host_kind != item.host_kind {
            bail!(
                "validation matrix {} expected host_kind {} but found {}",
                item.id,
                item.host_kind,
                artifact.host_kind
            );
        }
        let replay_manifest = artifact.replay_bundle.as_ref().map_or_else(
            || manifest_path.clone(),
            |bundle| resolve_report_path(root, &bundle.manifest_path),
        );
        let pass = counts.total > 0
            && counts.stable_normalized == counts.total
            && replay_manifest.exists();
        rows.push(ReleaseValidationMatrixResult {
            id: item.id.clone(),
            title: item.title.clone(),
            owner: item.owner.clone(),
            source: item.source.clone(),
            surface: artifact.surface.clone(),
            host_kind: artifact.host_kind.clone(),
            criterion: item.criterion.clone(),
            playbook: item.playbook.clone(),
            pass,
            evidence_path: path_to_unix_string(demo_summary_path),
            scenario_count: artifact.scenarios.len(),
            profile_count: artifact.profiles.len(),
            repeat_count: artifact.repeat,
            total: counts.total,
            stable_output: counts.stable_output,
            stable_normalized: counts.stable_normalized,
            replay_manifest_path: path_to_unix_string(
                replay_manifest
                    .strip_prefix(root)
                    .unwrap_or(&replay_manifest),
            ),
            detail: format!(
                "{} scenarios x {} profiles; normalized stability {}/{}",
                artifact.scenarios.len(),
                artifact.profiles.len(),
                counts.stable_normalized,
                counts.total
            ),
        });
    }
    Ok(rows)
}

fn load_e2e_summary(root: &Path, path: &str) -> Result<E2eSummaryArtifact> {
    let resolved = resolve_report_path(root, path);
    let raw = fs::read_to_string(&resolved)
        .with_context(|| format!("failed to read {}", resolved.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", resolved.display()))
}

fn resolve_report_path(root: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    }
}

fn render_release_signoff_readme(report: &ReleaseSignoffReport) -> String {
    let mut lines = vec![
        "# Release Signoff Checklist and Validation Matrix".to_string(),
        String::new(),
        format!("Overall pass: `{}`", report.overall_pass),
        format!("Spec: `{}`", report.spec_path),
        String::new(),
        "## Checklist".to_string(),
        String::new(),
        "| Item | Owner | Status | Evidence | Detail |".to_string(),
        "|------|-------|--------|----------|--------|".to_string(),
    ];
    for item in &report.checklist {
        lines.push(format!(
            "| {} | {} | {} | `{}` | {} |",
            item.title,
            item.owner,
            if item.pass { "Pass" } else { "Fail" },
            item.evidence_path,
            item.detail
        ));
        lines.push(format!("Criterion: {}", item.criterion));
        lines.push(format!("Playbook: {}", item.playbook));
    }
    lines.push(String::new());
    lines.push("## Validation Matrix".to_string());
    lines.push(String::new());
    lines.push("| Surface | Owner | Status | Groups | Replay Manifest |".to_string());
    lines.push("|---------|-------|--------|--------|-----------------|".to_string());
    for row in &report.validation_matrix {
        lines.push(format!(
            "| {} ({}) | {} | {} | {}/{} normalized | `{}` |",
            row.surface,
            row.host_kind,
            row.owner,
            if row.pass { "Pass" } else { "Fail" },
            row.stable_normalized,
            row.total,
            row.replay_manifest_path
        ));
        lines.push(format!("Criterion: {}", row.criterion));
        lines.push(format!("Playbook: {}", row.playbook));
    }
    lines.push(String::new());
    lines.push("## Risks".to_string());
    lines.push(String::new());
    for risk in &report.risks {
        lines.push(format!(
            "- {} ({}) — Trigger: {}. Playbook: {}",
            risk.title, risk.owner, risk.trigger, risk.mitigation_playbook
        ));
    }
    lines.push(String::new());
    lines.push("## Gate Summary".to_string());
    lines.push(String::new());
    lines.push(format!(
        "- Passed gates: {}",
        report.gate_summary.passed_gates.join(", ")
    ));
    lines.push(format!(
        "- Overridden failing gates: {}",
        if report.gate_summary.overridden_failing_gates.is_empty() {
            "none".to_string()
        } else {
            report.gate_summary.overridden_failing_gates.join(", ")
        }
    ));
    lines.push(format!(
        "- Uncovered failing gates: {}",
        if report.gate_summary.uncovered_failing_gates.is_empty() {
            "none".to_string()
        } else {
            report.gate_summary.uncovered_failing_gates.join(", ")
        }
    ));
    lines.join("\n")
}

fn render_report(entries: &[EvidenceEntry], warnings: &[AlienBeadGap]) -> String {
    let mut lines = vec![
        "# Evidence Ledger Report".to_string(),
        String::new(),
        format!("Tracked concepts: {}", entries.len()),
        String::new(),
        "| Concept | Section | Tier | Decision | Implementation Beads | Contract |".to_string(),
        "| --- | --- | --- | --- | --- | --- |".to_string(),
    ];

    for entry in entries {
        let beads = if entry.implementation.beads.is_empty() {
            "none".to_string()
        } else {
            entry.implementation.beads.join(", ")
        };
        lines.push(format!(
            "| {} | {} | {:?} | {:?} | {} | `{}` |",
            entry.concept_name,
            entry.graveyard_section,
            entry.tier,
            entry.decision.status,
            beads,
            entry.contract_path
        ));
    }

    if !warnings.is_empty() {
        lines.push(String::new());
        lines.push("## Coverage Warnings".to_string());
        lines.push(String::new());
        for gap in warnings {
            lines.push(format!(
                "- `{}` ({}) is closed and labeled `alien-cs` but is not referenced by any ledger entry.",
                gap.id, gap.title
            ));
        }
    }

    lines.push(String::new());
    lines.push("## Notes".to_string());
    lines.push(String::new());
    lines.push("- Generated by `cargo run --bin evidence -- report`.".to_string());
    lines.push("- Entries live under `evidence/ledger/*.toml`.".to_string());
    lines.join("\n")
}

fn render_bundle_readme(bundle: &ReleaseEvidenceBundle) -> String {
    let mut lines = vec![
        "# Release Evidence Bundle".to_string(),
        String::new(),
        format!("Bundle: `{}`", bundle.bundle_name),
        format!("Version: `{}`", bundle.bundle_version),
        format!("Release ref: `{}`", bundle.release_ref),
        format!("Retention: {} days", bundle.retention_days),
        String::new(),
        "## Included Files".to_string(),
        String::new(),
        "| Kind | Source | Bundled Path | SHA-256 |".to_string(),
        "| --- | --- | --- | --- |".to_string(),
    ];

    for file in &bundle.files {
        lines.push(format!(
            "| {:?} | `{}` | [{}]({}) | `{}` |",
            file.kind, file.source_path, file.bundled_path, file.bundled_path, file.sha256
        ));
    }

    lines.push(String::new());
    lines.push("## Notes".to_string());
    lines.push(String::new());
    lines.push("- `manifest.json` is the integrity and retention source of truth.".to_string());
    lines.push(
        "- Relative links are bundle-local so uploaded release artifacts stay navigable."
            .to_string(),
    );
    lines.join("\n")
}

fn ensure_ledger_dir(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join(LEDGER_SUBDIR))
        .with_context(|| format!("failed to create {}", root.join(LEDGER_SUBDIR).display()))
}

fn default_bead_warnings(root: &Path, entries: &[EvidenceEntry]) -> Result<Vec<AlienBeadGap>> {
    let beads_path = root.join(".beads/issues.jsonl");
    if beads_path.exists() {
        find_uncovered_closed_alien_beads(entries, &beads_path)
    } else {
        Ok(Vec::new())
    }
}

fn collect_bundle_sources(
    root: &Path,
    artifacts: &[PathBuf],
) -> Result<Vec<(EvidenceFileKind, PathBuf)>> {
    let mut files = Vec::new();
    let report_path = PathBuf::from(REPORT_PATH);
    if !root.join(&report_path).exists() {
        bail!(
            "required report file missing: {}",
            root.join(&report_path).display()
        );
    }
    files.push((EvidenceFileKind::LedgerReport, report_path));

    let ledger_dir = root.join(LEDGER_SUBDIR);
    for relative in collect_dir_entries(root, &ledger_dir, "toml")? {
        files.push((EvidenceFileKind::LedgerEntry, relative));
    }
    let contracts_dir = root.join("evidence/contracts");
    for relative in collect_dir_entries(root, &contracts_dir, "md")? {
        files.push((EvidenceFileKind::DecisionContract, relative));
    }
    for relative in [
        PathBuf::from("evidence/TEMPLATE.md"),
        PathBuf::from("evidence/capability_matrix.json"),
        PathBuf::from("evidence/capability_scenario_matrix.json"),
        PathBuf::from("evidence/demo_resilience_fixture_suite.json"),
        PathBuf::from("evidence/demo_strategy.md"),
        PathBuf::from("evidence/pattern_inventory.md"),
        PathBuf::from(".ci/quality-gates.toml"),
        PathBuf::from(".ci/release-gate-overrides.toml"),
        PathBuf::from(".ci/release-signoff.toml"),
        PathBuf::from(".ci/slo.yaml"),
    ] {
        let source = root.join(&relative);
        if source.exists() {
            let kind = if matches!(
                relative.as_path(),
                path if path == Path::new(".ci/quality-gates.toml")
                    || path == Path::new(".ci/release-gate-overrides.toml")
                    || path == Path::new(".ci/release-signoff.toml")
                    || path == Path::new(".ci/slo.yaml")
            ) {
                EvidenceFileKind::Policy
            } else {
                EvidenceFileKind::EvidenceReference
            };
            files.push((kind, relative));
        }
    }

    for artifact in artifacts {
        let absolute = if artifact.is_absolute() {
            artifact.clone()
        } else {
            root.join(artifact)
        };
        if !absolute.exists() {
            bail!("artifact path does not exist: {}", absolute.display());
        }
        let relative = absolute.strip_prefix(root).with_context(|| {
            format!(
                "artifact must live under project root {}: {}",
                root.display(),
                absolute.display()
            )
        })?;
        files.push((EvidenceFileKind::CiArtifact, relative.to_path_buf()));
    }

    files.sort_by(|left, right| left.1.cmp(&right.1));
    files.dedup_by(|left, right| left.1 == right.1);
    if !files
        .iter()
        .any(|(kind, _)| *kind == EvidenceFileKind::DecisionContract)
    {
        bail!("bundle requires at least one decision contract");
    }
    if !files
        .iter()
        .any(|(kind, _)| *kind == EvidenceFileKind::LedgerEntry)
    {
        bail!("bundle requires at least one ledger entry");
    }
    Ok(files)
}

fn collect_dir_entries(root: &Path, dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        bail!("required directory missing: {}", dir.display());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry
            .with_context(|| format!("failed to read directory entry under {}", dir.display()))?
            .path();
        if path.extension().is_some_and(|ext| ext == extension) {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to relativize {}", path.display()))?;
            paths.push(relative.to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

fn discover_release_ref() -> Option<String> {
    if let Ok(value) = std::env::var("GITHUB_REF_NAME")
        && !value.trim().is_empty()
    {
        return Some(value);
    }
    if let Ok(value) = std::env::var("GITHUB_SHA")
        && !value.trim().is_empty()
    {
        return Some(value.chars().take(12).collect());
    }

    let output = ProcessCommand::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn sanitize_bundle_component(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn path_to_unix_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn verify_bundle_links(bundle_dir: &Path) -> Result<()> {
    let readme_path = bundle_dir.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    for token in readme.split('(').skip(1) {
        let Some((target, _)) = token.split_once(')') else {
            continue;
        };
        if target.is_empty() || target.starts_with("http") || target.starts_with('#') {
            continue;
        }
        let linked_path = bundle_dir.join(target);
        if !linked_path.exists() {
            bail!(
                "bundle README link target missing: {}",
                linked_path.display()
            );
        }
    }
    Ok(())
}

fn load_override_policy(path: &Path) -> Result<ReleaseGateOverridePolicy> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read policy file {}", path.display()))?;
    let parsed: QualityGatePolicyFile = toml::from_str(&raw)
        .with_context(|| format!("failed to parse policy file {}", path.display()))?;
    parsed.release_gate_overrides.ok_or_else(|| {
        anyhow!(
            "missing [release_gate_overrides] section in {}",
            path.display()
        )
    })
}

fn load_override_ledger(path: &Path) -> Result<ReleaseGateOverrideLedger> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read override ledger {}", path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("failed to parse override ledger {}", path.display()))
}

fn validate_override_ledger(
    policy: &ReleaseGateOverridePolicy,
    ledger: &ReleaseGateOverrideLedger,
    overrides_path: &Path,
    now: OffsetDateTime,
) -> Result<ReleaseGateOverrideSummary> {
    if ledger.schema_version != OVERRIDE_SCHEMA_VERSION {
        bail!(
            "unexpected override ledger schema version {}",
            ledger.schema_version
        );
    }
    if policy.policy_id.trim().is_empty() {
        bail!("release_gate_overrides.policy_id must not be empty");
    }
    if policy.allowed_approvers.is_empty() {
        bail!("release_gate_overrides.allowed_approvers must not be empty");
    }
    if policy.max_override_days == 0 {
        bail!("release_gate_overrides.max_override_days must be > 0");
    }

    let allowed_approvers: BTreeSet<&str> = policy
        .allowed_approvers
        .iter()
        .map(String::as_str)
        .collect();
    let known_gates: BTreeSet<&str> = KNOWN_RELEASE_GATES.iter().copied().collect();
    let mut seen_ids = BTreeSet::new();
    let mut active_gates = BTreeSet::new();
    let mut statuses = Vec::new();

    for record in &ledger.overrides {
        if record.id.trim().is_empty() {
            bail!("override id must not be empty");
        }
        if !seen_ids.insert(record.id.as_str()) {
            bail!("duplicate override id {}", record.id);
        }
        if !allowed_approvers.contains(record.approver.as_str()) {
            bail!(
                "override {} approver {} is not authorized",
                record.id,
                record.approver
            );
        }
        if record.created_by.trim().is_empty() {
            bail!("override {} created_by must not be empty", record.id);
        }
        if record.reason.trim().len() < MIN_OVERRIDE_REASON_LEN {
            bail!(
                "override {} reason must be at least {} characters",
                record.id,
                MIN_OVERRIDE_REASON_LEN
            );
        }
        if record.scope.is_empty() {
            bail!("override {} scope must list at least one gate", record.id);
        }
        let created_at = parse_rfc3339(&record.created_at, "created_at", &record.id)?;
        let expires_at = parse_rfc3339(&record.expires_at, "expires_at", &record.id)?;
        if expires_at <= created_at {
            bail!("override {} expires_at must be after created_at", record.id);
        }
        let duration = expires_at - created_at;
        if duration.whole_days() > i64::from(policy.max_override_days) {
            bail!(
                "override {} exceeds max_override_days {}",
                record.id,
                policy.max_override_days
            );
        }
        if expires_at <= now {
            bail!(
                "override {} has expired at {}",
                record.id,
                record.expires_at
            );
        }
        if policy.require_fix_bead
            && !record
                .fix_bead
                .as_ref()
                .is_some_and(|bead| bead.starts_with("bd-"))
        {
            bail!("override {} requires a fix_bead", record.id);
        }
        if policy.require_retro_bead
            && !record
                .retro_bead
                .as_ref()
                .is_some_and(|bead| bead.starts_with("bd-"))
        {
            bail!("override {} requires a retro_bead", record.id);
        }

        let mut deduped_scope = BTreeSet::new();
        for gate in &record.scope {
            if !known_gates.contains(gate.as_str()) {
                bail!("override {} references unknown gate {}", record.id, gate);
            }
            if !deduped_scope.insert(gate.as_str()) {
                bail!("override {} repeats gate {}", record.id, gate);
            }
            active_gates.insert(gate.clone());
        }

        statuses.push(ReleaseGateOverrideStatus {
            id: record.id.clone(),
            approver: record.approver.clone(),
            scope: record.scope.clone(),
            expires_at: record.expires_at.clone(),
            active: true,
        });
    }

    Ok(ReleaseGateOverrideSummary {
        enabled: policy.enabled,
        policy_id: policy.policy_id.clone(),
        overrides_path: path_to_unix_string(overrides_path),
        active_override_count: statuses.len(),
        active_gates: active_gates.into_iter().collect(),
        overrides: statuses,
    })
}

fn default_override_schema_version() -> u32 {
    OVERRIDE_SCHEMA_VERSION
}

fn load_perf_baseline(path: &Path) -> Result<PerfBaselineFile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read perf baseline {}", path.display()))?;
    let baseline: PerfBaselineFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse perf baseline {}", path.display()))?;
    if baseline.schema_version != PERF_BASELINE_SCHEMA_VERSION {
        bail!(
            "unexpected perf baseline schema version {}",
            baseline.schema_version
        );
    }
    Ok(baseline)
}

fn load_perf_slo_policy(path: &Path) -> Result<PerfSloPolicyFile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read perf slo policy {}", path.display()))?;
    let policy: PerfSloPolicyFile = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse perf slo policy {}", path.display()))?;
    if policy.schema_version != PERF_SLO_SCHEMA_VERSION {
        bail!(
            "unexpected perf slo schema version {}",
            policy.schema_version
        );
    }
    Ok(policy)
}

fn parse_perf_log(raw: &str) -> BTreeMap<String, PerfSampleBucket> {
    let mut buckets = BTreeMap::<String, PerfSampleBucket>::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') || !trimmed.contains("\"benchmark\"") {
            continue;
        }
        let record: PerfLogRecord = match serde_json::from_str(trimmed) {
            Ok(record) => record,
            Err(_) => continue,
        };
        if let Some(ns) = record.ns {
            append_perf_sample(
                &mut buckets,
                record.benchmark,
                record.nodes,
                record.edges,
                ns,
            );
            continue;
        }
        for (suffix, value) in [
            ("sugiyama", record.sugiyama_ns),
            ("force", record.force_ns),
            ("tree", record.tree_ns),
        ] {
            if let Some(ns) = value {
                append_perf_sample(
                    &mut buckets,
                    format!("{}.{}", record.benchmark, suffix),
                    record.nodes,
                    record.edges,
                    ns,
                );
            }
        }
    }
    buckets
}

fn append_perf_sample(
    buckets: &mut BTreeMap<String, PerfSampleBucket>,
    benchmark: String,
    nodes: Option<usize>,
    edges: Option<usize>,
    ns: u64,
) {
    let bucket = buckets
        .entry(benchmark)
        .or_insert_with(|| PerfSampleBucket {
            nodes,
            edges,
            samples_ns: Vec::new(),
        });
    bucket.nodes = bucket.nodes.or(nodes);
    bucket.edges = bucket.edges.or(edges);
    bucket.samples_ns.push(ns);
}

fn summarize_perf_bucket(
    benchmark: &str,
    bucket: &PerfSampleBucket,
    baseline: Option<PerfBaselineEntry>,
    slo: Option<PerfSloEntry>,
    warn_threshold_pct: f64,
    fail_threshold_pct: f64,
) -> Result<PerfBenchmarkSummary> {
    if bucket.samples_ns.is_empty() {
        bail!("benchmark bucket {benchmark} had zero samples");
    }
    let mut sorted = bucket.samples_ns.clone();
    sorted.sort_unstable();
    let mean_ns = sorted.iter().map(|value| *value as f64).sum::<f64>() / sorted.len() as f64;
    let median_ns = percentile_u64(&sorted, 0.50);
    let p95_ns = percentile_u64(&sorted, 0.95);
    let p99_ns = percentile_u64(&sorted, 0.99);
    let median_ops_per_sec = ops_per_sec_from_ns(median_ns);
    let (median_ci_low_ns, median_ci_high_ns) = bootstrap_interval(&sorted, 0.50);
    let (p99_ci_low_ns, p99_ci_high_ns) = bootstrap_interval(&sorted, 0.99);

    let (baseline_p99_ns, regression_vs_baseline_pct, baseline_gate_status) =
        if let Some(baseline) = baseline {
            let pct = if baseline.p99_ns == 0 {
                0.0
            } else {
                ((p99_ns as f64 - baseline.p99_ns as f64) / baseline.p99_ns as f64) * 100.0
            };
            let status = if pct > fail_threshold_pct {
                PerfGateStatus::Fail
            } else if pct > warn_threshold_pct {
                PerfGateStatus::Warn
            } else {
                PerfGateStatus::Pass
            };
            (Some(baseline.p99_ns), Some(pct), status)
        } else {
            (None, None, PerfGateStatus::NoBaseline)
        };
    let (slo_gate_status, slo) = evaluate_slo(benchmark, slo, median_ns, p95_ns, p99_ns);
    let gate_status = combine_gate_statuses(baseline_gate_status, slo_gate_status);

    Ok(PerfBenchmarkSummary {
        benchmark: benchmark.to_string(),
        nodes: bucket.nodes,
        edges: bucket.edges,
        sample_count: sorted.len(),
        min_ns: *sorted.first().expect("sorted non-empty"),
        max_ns: *sorted.last().expect("sorted non-empty"),
        mean_ns,
        median_ns,
        p95_ns,
        p99_ns,
        median_ops_per_sec,
        median_ci_low_ns,
        median_ci_high_ns,
        p99_ci_low_ns,
        p99_ci_high_ns,
        baseline_p99_ns,
        regression_vs_baseline_pct,
        baseline_gate_status,
        slo_gate_status,
        slo,
        gate_status,
    })
}

fn evaluate_slo(
    benchmark: &str,
    slo: Option<PerfSloEntry>,
    median_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
) -> (PerfGateStatus, Option<PerfSloEvaluation>) {
    let Some(slo) = slo else {
        return (PerfGateStatus::NoBaseline, None);
    };
    let mut violations = Vec::new();
    if let Some(max_p50_ns) = slo.max_p50_ns
        && median_ns > max_p50_ns
    {
        violations.push(format!(
            "{benchmark} median_ns={median_ns} exceeded max_p50_ns={max_p50_ns}"
        ));
    }
    if let Some(max_p95_ns) = slo.max_p95_ns
        && p95_ns > max_p95_ns
    {
        violations.push(format!(
            "{benchmark} p95_ns={p95_ns} exceeded max_p95_ns={max_p95_ns}"
        ));
    }
    if let Some(max_p99_ns) = slo.max_p99_ns
        && p99_ns > max_p99_ns
    {
        violations.push(format!(
            "{benchmark} p99_ns={p99_ns} exceeded max_p99_ns={max_p99_ns}"
        ));
    }
    let median_ops_per_sec = ops_per_sec_from_ns(median_ns);
    if let Some(min_median_ops_per_sec) = slo.min_median_ops_per_sec
        && median_ops_per_sec < min_median_ops_per_sec
    {
        violations.push(format!(
            "{benchmark} median_ops_per_sec={median_ops_per_sec:.2} fell below min_median_ops_per_sec={min_median_ops_per_sec:.2}"
        ));
    }
    let gate_status = if violations.is_empty() {
        PerfGateStatus::Pass
    } else {
        PerfGateStatus::Fail
    };
    (
        gate_status,
        Some(PerfSloEvaluation {
            max_p50_ns: slo.max_p50_ns,
            max_p95_ns: slo.max_p95_ns,
            max_p99_ns: slo.max_p99_ns,
            min_median_ops_per_sec: slo.min_median_ops_per_sec,
            violations,
        }),
    )
}

fn combine_gate_statuses(left: PerfGateStatus, right: PerfGateStatus) -> PerfGateStatus {
    use PerfGateStatus::{Fail, NoBaseline, Pass, Warn};

    match (left, right) {
        (Fail, _) | (_, Fail) => Fail,
        (Warn, _) | (_, Warn) => Warn,
        (Pass | NoBaseline, Pass) | (Pass, NoBaseline) => Pass,
        (NoBaseline, NoBaseline) => NoBaseline,
    }
}

fn ops_per_sec_from_ns(ns: u64) -> f64 {
    if ns == 0 {
        return f64::INFINITY;
    }
    1_000_000_000.0 / ns as f64
}

fn percentile_u64(sorted: &[u64], quantile: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let clamped = quantile.clamp(0.0, 1.0);
    let index = ((sorted.len() - 1) as f64 * clamped).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn bootstrap_interval(sorted: &[u64], quantile: f64) -> (u64, u64) {
    if sorted.len() <= 1 {
        let value = *sorted.first().unwrap_or(&0);
        return (value, value);
    }
    let mut rng = Lcg64::new(
        0x9e37_79b9_7f4a_7c15_u64 ^ (sorted.len() as u64).wrapping_mul(0x517c_c1b7_2722_0a95),
    );
    let mut estimates = Vec::with_capacity(PERF_BOOTSTRAP_ITERATIONS);
    let mut sample = Vec::with_capacity(sorted.len());
    for _ in 0..PERF_BOOTSTRAP_ITERATIONS {
        sample.clear();
        for _ in 0..sorted.len() {
            sample.push(sorted[rng.next_usize(sorted.len())]);
        }
        sample.sort_unstable();
        estimates.push(percentile_u64(&sample, quantile));
    }
    estimates.sort_unstable();
    (
        percentile_u64(&estimates, 0.025),
        percentile_u64(&estimates, 0.975),
    )
}

fn build_perf_corpus_manifest(
    buckets: &BTreeMap<String, PerfSampleBucket>,
    input_sha256: &str,
) -> PerfCorpusManifest {
    let benchmarks = buckets
        .iter()
        .map(|(benchmark, bucket)| PerfCorpusEntry {
            benchmark: benchmark.clone(),
            nodes: bucket.nodes,
            edges: bucket.edges,
            sample_count: bucket.samples_ns.len(),
        })
        .collect();
    PerfCorpusManifest {
        source: "fm-layout performance benchmark log".to_string(),
        input_sha256: input_sha256.to_string(),
        benchmarks,
    }
}

fn capture_perf_environment() -> PerfEnvironmentFingerprint {
    PerfEnvironmentFingerprint {
        rustc_version: shell_command_output("rustc", &["--version"])
            .unwrap_or_else(|| "unknown".to_string()),
        cargo_version: shell_command_output("cargo", &["--version"])
            .unwrap_or_else(|| "unknown".to_string()),
        uname: shell_command_output("uname", &["-a"]).unwrap_or_else(|| "unknown".to_string()),
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
        cpu_count: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        cpu_model: cpu_model_name(),
    }
}

fn shell_command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn cpu_model_name() -> Option<String> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok()?;
    cpuinfo.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() == "model name" {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn resolve_from_root(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(value)?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

struct Lcg64 {
    state: u64,
}

impl Lcg64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            return 0;
        }
        (self.next_u64() % upper as u64) as usize
    }
}

fn parse_rfc3339(value: &str, field_name: &str, override_id: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).with_context(|| {
        format!("override {override_id} has invalid {field_name} timestamp {value}")
    })
}

fn current_time() -> Result<OffsetDateTime> {
    if let Ok(value) = std::env::var("EVIDENCE_OVERRIDE_NOW")
        && !value.trim().is_empty()
    {
        return OffsetDateTime::parse(&value, &Rfc3339)
            .with_context(|| format!("invalid EVIDENCE_OVERRIDE_NOW timestamp {value}"));
    }
    Ok(OffsetDateTime::now_utc())
}

fn absolutize_under_root(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn ledger_entry_path(root: &Path, concept_id: &str) -> PathBuf {
    root.join(LEDGER_SUBDIR).join(format!("{concept_id}.toml"))
}

fn write_entry(root: &Path, entry: &EvidenceEntry, force: bool) -> Result<()> {
    let path = ledger_entry_path(root, &entry.concept_id);
    if path.exists() && !force {
        bail!(
            "entry already exists: {} (use --force to overwrite)",
            path.display()
        );
    }
    save_entry(&path, entry)
}

fn save_entry(path: &Path, entry: &EvidenceEntry) -> Result<()> {
    let toml = toml::to_string_pretty(entry).context("failed to serialize entry to TOML")?;
    fs::write(path, toml).with_context(|| format!("failed to write {}", path.display()))
}

fn load_entry(path: &Path) -> Result<EvidenceEntry> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn load_all_entries(root: &Path) -> Result<Vec<EvidenceEntry>> {
    let dir = root.join(LEDGER_SUBDIR);
    let mut entries = Vec::new();
    if !dir.exists() {
        return Ok(entries);
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();

    for path in paths {
        entries.push(load_entry(&path)?);
    }
    Ok(entries)
}

fn lookup_seed(concept_id: &str) -> Result<&'static SeedSpec> {
    SEEDS
        .iter()
        .find(|seed| seed.concept_id == concept_id)
        .ok_or_else(|| anyhow!("unknown concept id `{concept_id}`"))
}

fn seed_entry(seed: &SeedSpec) -> EvidenceEntry {
    EvidenceEntry {
        concept_id: seed.concept_id.to_string(),
        concept_name: seed.concept_name.to_string(),
        graveyard_section: seed.graveyard_section.to_string(),
        graveyard_score: seed.graveyard_score,
        tier: seed.tier,
        contract_path: seed.contract_path.to_string(),
        hypothesis: seed.hypothesis.to_string(),
        predicted_improvement: seed.predicted_improvement.to_string(),
        predicted_risk: seed.predicted_risk.to_string(),
        baseline: MeasurementStage::default(),
        implementation: ImplementationStage::default(),
        post_measurement: MeasurementStage::default(),
        decision: DecisionStage::default(),
        notes: vec!["Seeded from the corresponding decision contract.".to_string()],
    }
}

#[derive(Debug, Deserialize)]
struct BeadRecord {
    id: String,
    title: String,
    status: String,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug)]
struct AlienBeadGap {
    id: String,
    title: String,
}

fn find_uncovered_closed_alien_beads(
    entries: &[EvidenceEntry],
    beads_path: &Path,
) -> Result<Vec<AlienBeadGap>> {
    let raw = fs::read_to_string(beads_path)
        .with_context(|| format!("failed to read beads file {}", beads_path.display()))?;
    let covered: BTreeSet<&str> = entries
        .iter()
        .flat_map(|entry| entry.implementation.beads.iter().map(String::as_str))
        .collect();

    let mut gaps = Vec::new();
    for (line_no, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: BeadRecord = serde_json::from_str(line).with_context(|| {
            format!(
                "failed to parse bead record at {}:{}",
                beads_path.display(),
                line_no + 1
            )
        })?;
        if record.status == "closed"
            && record.labels.iter().any(|label| label == "alien-cs")
            && !covered.contains(record.id.as_str())
        {
            gaps.push(AlienBeadGap {
                id: record.id,
                title: record.title,
            });
        }
    }
    Ok(gaps)
}

fn parse_metric_assignment(input: &str) -> Result<(String, f64), String> {
    let (key, value) = input
        .split_once('=')
        .ok_or_else(|| "metric must be key=value".to_string())?;
    let metric = value
        .parse::<f64>()
        .map_err(|_| format!("metric value must be numeric: {input}"))?;
    Ok((key.trim().to_string(), metric))
}

fn parse_gate_result_assignment(input: &str) -> Result<(String, GateResult), String> {
    let (gate, value) = input
        .split_once('=')
        .ok_or_else(|| "gate result must be gate=result".to_string())?;
    let parsed = match value.trim() {
        "success" => GateResult::Success,
        "failure" => GateResult::Failure,
        "cancelled" => GateResult::Cancelled,
        "skipped" => GateResult::Skipped,
        other => {
            return Err(format!(
                "gate result must be one of success|failure|cancelled|skipped: {other}"
            ));
        }
    };
    Ok((gate.trim().to_string(), parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metric_assignment_accepts_numeric_values() {
        let (key, value) = parse_metric_assignment("crossing_count=42").expect("metric");
        assert_eq!(key, "crossing_count");
        assert_eq!(value, 42.0);
    }

    #[test]
    fn parse_gate_result_assignment_accepts_known_statuses() {
        let (gate, result) =
            parse_gate_result_assignment("core-check=success").expect("gate result");
        assert_eq!(gate, "core-check");
        assert_eq!(result, GateResult::Success);
    }

    #[test]
    fn seed_entry_starts_pending() {
        let entry = seed_entry(&SEEDS[0]);
        assert_eq!(entry.decision.status, DecisionStatus::Pending);
        assert!(entry.implementation.beads.is_empty());
    }

    #[test]
    fn render_report_includes_warning_section_when_needed() {
        let entry = seed_entry(&SEEDS[0]);
        let report = render_report(
            &[entry],
            &[AlienBeadGap {
                id: "bd-test".to_string(),
                title: "Alien task".to_string(),
            }],
        );
        assert!(report.contains("Coverage Warnings"));
        assert!(report.contains("bd-test"));
    }

    #[test]
    fn validate_override_ledger_accepts_authorized_active_scope() {
        let policy = ReleaseGateOverridePolicy {
            enabled: true,
            policy_id: "fm.release-gate.override@v1".to_string(),
            allowed_approvers: vec!["Dicklesworthstone".to_string()],
            max_override_days: 14,
            require_retro_bead: true,
            require_fix_bead: true,
            overrides_path: Some(".ci/release-gate-overrides.toml".to_string()),
        };
        let ledger = ReleaseGateOverrideLedger {
            schema_version: OVERRIDE_SCHEMA_VERSION,
            overrides: vec![ReleaseGateOverrideRecord {
                id: "ovr-1".to_string(),
                approver: "Dicklesworthstone".to_string(),
                created_by: "BlackShore".to_string(),
                created_at: "2026-03-31T10:00:00Z".to_string(),
                reason: "Emergency release needs a temporary deterministic gate bypass."
                    .to_string(),
                scope: vec!["coverage".to_string()],
                expires_at: "2026-04-01T10:00:00Z".to_string(),
                retro_bead: Some("bd-retro.1".to_string()),
                fix_bead: Some("bd-fix.1".to_string()),
                exception_key: None,
            }],
        };

        let summary = validate_override_ledger(
            &policy,
            &ledger,
            Path::new(".ci/release-gate-overrides.toml"),
            OffsetDateTime::parse("2026-03-31T12:00:00Z", &Rfc3339).expect("timestamp"),
        )
        .expect("override summary");
        assert_eq!(summary.active_override_count, 1);
        assert_eq!(summary.active_gates, vec!["coverage".to_string()]);
    }

    #[test]
    fn validate_override_ledger_rejects_unknown_gate() {
        let policy = ReleaseGateOverridePolicy {
            enabled: true,
            policy_id: "fm.release-gate.override@v1".to_string(),
            allowed_approvers: vec!["Dicklesworthstone".to_string()],
            max_override_days: 14,
            require_retro_bead: true,
            require_fix_bead: true,
            overrides_path: Some(".ci/release-gate-overrides.toml".to_string()),
        };
        let ledger = ReleaseGateOverrideLedger {
            schema_version: OVERRIDE_SCHEMA_VERSION,
            overrides: vec![ReleaseGateOverrideRecord {
                id: "ovr-bad".to_string(),
                approver: "Dicklesworthstone".to_string(),
                created_by: "BlackShore".to_string(),
                created_at: "2026-03-31T10:00:00Z".to_string(),
                reason: "Emergency release override with an invalid scope entry.".to_string(),
                scope: vec!["not-a-real-gate".to_string()],
                expires_at: "2026-04-01T10:00:00Z".to_string(),
                retro_bead: Some("bd-retro.2".to_string()),
                fix_bead: Some("bd-fix.2".to_string()),
                exception_key: None,
            }],
        };

        let error = validate_override_ledger(
            &policy,
            &ledger,
            Path::new(".ci/release-gate-overrides.toml"),
            OffsetDateTime::parse("2026-03-31T12:00:00Z", &Rfc3339).expect("timestamp"),
        )
        .expect_err("unknown gate should fail");
        assert!(error.to_string().contains("unknown gate"));
    }
}
