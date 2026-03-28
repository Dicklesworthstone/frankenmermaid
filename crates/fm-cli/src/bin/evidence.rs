use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const LEDGER_SUBDIR: &str = "evidence/ledger";
const REPORT_PATH: &str = "evidence/ledger/README.md";

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
];

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = canonicalize_root(&cli.root)?;
    match cli.command {
        Command::Add(args) => add_command(&root, &args),
        Command::Update(args) => update_command(&root, *args),
        Command::Report(args) => report_command(&root, args),
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

fn ensure_ledger_dir(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join(LEDGER_SUBDIR))
        .with_context(|| format!("failed to create {}", root.join(LEDGER_SUBDIR).display()))
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
}
