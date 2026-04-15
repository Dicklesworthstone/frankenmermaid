//! Real-World Mermaid Corpus Ingestion Pipeline (bd-2xl.15)
//!
//! Processes real-world Mermaid snippets from public repositories for
//! compatibility tracking and regression detection.
//!
//! Features:
//! - Corpus loading from curated JSONL files
//! - Deduplication via content hashing
//! - Diagram type detection and normalization
//! - Compatibility checking with structured evidence
//! - Trend analysis and failure signature extraction

use fm_core::evidence;
use fm_layout::layout_diagram;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

// ============================================================================
// Corpus Entry Types
// ============================================================================

/// A corpus entry representing a real-world Mermaid diagram.
#[derive(Debug, Clone)]
pub struct CorpusEntry {
    /// Unique identifier (content hash).
    pub id: String,
    /// Original source (e.g., "github:owner/repo/path").
    pub source: String,
    /// License of the source repository.
    pub license: String,
    /// Raw Mermaid input.
    pub input: String,
    /// Detected diagram type.
    pub diagram_type: String,
    /// Syntax profile (simple, moderate, complex).
    pub complexity: Complexity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Complexity {
    Simple,
    Moderate,
    Complex,
}

impl Complexity {
    fn from_input(input: &str) -> Self {
        let node_estimate = input.matches("-->").count()
            + input.matches("---").count()
            + input.matches("->>").count();
        let has_subgraph = input.contains("subgraph");
        let has_fragments =
            input.contains("alt ") || input.contains("loop ") || input.contains("opt ");

        if has_subgraph || has_fragments || node_estimate > 20 {
            Complexity::Complex
        } else if node_estimate > 5 {
            Complexity::Moderate
        } else {
            Complexity::Simple
        }
    }
}

impl std::fmt::Display for Complexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Complexity::Simple => write!(f, "simple"),
            Complexity::Moderate => write!(f, "moderate"),
            Complexity::Complex => write!(f, "complex"),
        }
    }
}

// ============================================================================
// Compatibility Result
// ============================================================================

#[derive(Debug, Clone)]
pub struct CompatResult {
    pub id: String,
    pub diagram_type: String,
    pub complexity: Complexity,
    pub parse_success: bool,
    pub parse_warnings: usize,
    pub layout_success: bool,
    pub render_success: bool,
    pub failure_signature: Option<String>,
    pub elapsed_us: u64,
}

impl CompatResult {
    pub fn overall_success(&self) -> bool {
        self.parse_success && self.layout_success && self.render_success
    }
}

// ============================================================================
// Corpus Loader
// ============================================================================

fn corpus_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

/// Load corpus entries from the curated corpus directory.
/// Format: JSONL files with {source, license, input} per line.
/// Always includes synthetic corpus even if no JSONL files exist.
pub fn load_corpus() -> Vec<CorpusEntry> {
    let dir = corpus_dir();
    let mut entries = Vec::new();
    let mut seen_hashes: BTreeSet<String> = BTreeSet::new();

    // Start with synthetic corpus to ensure we always have test data
    entries.extend(synthetic_corpus());
    for entry in &entries {
        seen_hashes.insert(entry.id.clone());
    }

    if !dir.exists() {
        return entries;
    }

    let Ok(files) = fs::read_dir(&dir) else {
        return entries;
    };

    for entry in files.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "jsonl")
            && let Ok(content) = fs::read_to_string(&path)
        {
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    let input = json["input"].as_str().unwrap_or("").to_string();
                    let source = json["source"].as_str().unwrap_or("unknown").to_string();
                    let license = json["license"].as_str().unwrap_or("unknown").to_string();

                    // Deduplicate by content hash
                    let hash = evidence::fnv1a_hex(input.as_bytes());
                    if seen_hashes.contains(&hash) {
                        continue;
                    }
                    seen_hashes.insert(hash.clone());

                    // Detect diagram type
                    let parsed = parse(&input);
                    let diagram_type = format!("{:?}", parsed.ir.diagram_type).to_lowercase();
                    let complexity = Complexity::from_input(&input);

                    entries.push(CorpusEntry {
                        id: hash,
                        source,
                        license,
                        input,
                        diagram_type,
                        complexity,
                    });
                }
            }
        }
    }

    entries
}

/// Embedded synthetic corpus for testing when no JSONL files exist.
fn synthetic_corpus() -> Vec<CorpusEntry> {
    let inputs = vec![
        // Simple flowcharts
        ("flowchart LR\n    A --> B --> C", "synthetic/simple/flow1"),
        (
            "flowchart TD\n    Start --> Process --> End",
            "synthetic/simple/flow2",
        ),
        (
            "graph LR\n    A[Input] --> B{Decision}\n    B -->|Yes| C[Output]",
            "synthetic/simple/flow3",
        ),
        // Complex flowcharts
        (
            r#"flowchart TB
    subgraph Production
        A[Load Balancer] --> B[Web Server 1]
        A --> C[Web Server 2]
        B --> D[(Database)]
        C --> D
    end
    subgraph Monitoring
        E[Prometheus] --> A
        E --> B
        E --> C
    end"#,
            "synthetic/complex/infra",
        ),
        // Sequence diagrams
        (
            "sequenceDiagram\n    Alice->>Bob: Hello\n    Bob-->>Alice: Hi",
            "synthetic/simple/seq1",
        ),
        (
            r#"sequenceDiagram
    participant C as Client
    participant S as Server
    participant DB as Database
    C->>S: Request
    activate S
    S->>DB: Query
    activate DB
    DB-->>S: Results
    deactivate DB
    S-->>C: Response
    deactivate S"#,
            "synthetic/moderate/seq2",
        ),
        // Class diagrams
        (
            "classDiagram\n    Animal <|-- Dog\n    Animal <|-- Cat",
            "synthetic/simple/class1",
        ),
        // State diagrams
        (
            "stateDiagram-v2\n    [*] --> Active\n    Active --> [*]",
            "synthetic/simple/state1",
        ),
        // ER diagrams
        (
            "erDiagram\n    CUSTOMER ||--o{ ORDER : places\n    ORDER ||--|{ LINE-ITEM : contains",
            "synthetic/simple/er1",
        ),
        // Mind maps
        (
            "mindmap\n    root((Central))\n        Topic 1\n            Subtopic A\n        Topic 2",
            "synthetic/simple/mindmap1",
        ),
        // Gantt charts
        (
            "gantt\n    title Project Plan\n    section Phase 1\n    Task 1 :a1, 2024-01-01, 30d",
            "synthetic/simple/gantt1",
        ),
        // Pie charts
        (
            "pie\n    title Market Share\n    \"A\" : 40\n    \"B\" : 30\n    \"C\" : 30",
            "synthetic/simple/pie1",
        ),
    ];

    inputs
        .into_iter()
        .map(|(input, source)| {
            let hash = evidence::fnv1a_hex(input.as_bytes());
            let parsed = parse(input);
            let diagram_type = format!("{:?}", parsed.ir.diagram_type).to_lowercase();
            let complexity = Complexity::from_input(input);

            CorpusEntry {
                id: hash,
                source: source.to_string(),
                license: "synthetic".to_string(),
                input: input.to_string(),
                diagram_type,
                complexity,
            }
        })
        .collect()
}

// ============================================================================
// Compatibility Checker
// ============================================================================

/// Run compatibility check on a single corpus entry.
pub fn check_compatibility(entry: &CorpusEntry) -> CompatResult {
    let start = std::time::Instant::now();

    // Parse - parser is best-effort, so "success" means we got meaningful IR
    let parsed = parse(&entry.input);
    // Parse is considered successful if we have nodes or it's a valid diagram type
    let has_content = !parsed.ir.nodes.is_empty() || !parsed.ir.edges.is_empty();
    let parse_success = has_content || parsed.confidence > 0.5;
    let parse_warnings = parsed.warnings.len();
    let mut failure_signature = None;

    if !parse_success {
        failure_signature = Some(format!(
            "parse:no_content (nodes={} edges={} confidence={:.2})",
            parsed.ir.nodes.len(),
            parsed.ir.edges.len(),
            parsed.confidence
        ));
    } else if !parsed.warnings.is_empty() {
        // Record first warning as context, but don't fail
        failure_signature = parsed.warnings.first().map(|w| format!("warn:{}", w));
    }

    // Layout
    let layout_result = std::panic::catch_unwind(|| layout_diagram(&parsed.ir));
    let layout_success = layout_result.is_ok();
    let layout = layout_result.ok();

    if !layout_success && failure_signature.is_none() {
        failure_signature = Some("layout:panic".to_string());
    }

    // Render
    let render_success = if let Some(ref layout) = layout {
        let svg_config = SvgRenderConfig::default();
        let render_result =
            std::panic::catch_unwind(|| render_svg_with_layout(&parsed.ir, layout, &svg_config));
        match render_result {
            Ok(svg) => svg.contains("<svg"),
            Err(_) => {
                if failure_signature.is_none() {
                    failure_signature = Some("render:panic".to_string());
                }
                false
            }
        }
    } else {
        false
    };

    let elapsed_us = start.elapsed().as_micros() as u64;

    CompatResult {
        id: entry.id.clone(),
        diagram_type: entry.diagram_type.clone(),
        complexity: entry.complexity,
        parse_success,
        parse_warnings,
        layout_success,
        render_success,
        failure_signature,
        elapsed_us,
    }
}

// ============================================================================
// Trend Analysis
// ============================================================================

#[derive(Debug, Default)]
pub struct TrendReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub warning_rate: f64,
    pub by_diagram_type: BTreeMap<String, TypeStats>,
    pub by_complexity: BTreeMap<String, TypeStats>,
    pub top_failure_signatures: Vec<(String, usize)>,
    pub high_value_failures: Vec<HighValueFailure>,
}

#[derive(Debug, Default, Clone)]
pub struct TypeStats {
    pub total: usize,
    pub passed: usize,
    pub warnings: usize,
    pub avg_time_us: f64,
}

#[derive(Debug, Clone)]
pub struct HighValueFailure {
    pub id: String,
    pub diagram_type: String,
    pub complexity: Complexity,
    pub signature: String,
    pub input_preview: String,
}

/// Generate trend report from compatibility results.
pub fn analyze_trends(results: &[CompatResult], corpus: &[CorpusEntry]) -> TrendReport {
    let mut report = TrendReport::default();

    // Build corpus lookup
    let corpus_map: HashMap<_, _> = corpus.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut by_type: BTreeMap<String, Vec<&CompatResult>> = BTreeMap::new();
    let mut by_complexity: BTreeMap<String, Vec<&CompatResult>> = BTreeMap::new();
    let mut failure_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_warnings = 0usize;

    for result in results {
        report.total += 1;
        if result.overall_success() {
            report.passed += 1;
        } else {
            report.failed += 1;
        }

        total_warnings += result.parse_warnings;

        by_type
            .entry(result.diagram_type.clone())
            .or_default()
            .push(result);
        by_complexity
            .entry(result.complexity.to_string())
            .or_default()
            .push(result);

        if let Some(ref sig) = result.failure_signature {
            *failure_counts.entry(sig.clone()).or_default() += 1;

            // Collect high-value failures for regression fixtures
            if let Some(entry) = corpus_map.get(result.id.as_str()) {
                report.high_value_failures.push(HighValueFailure {
                    id: result.id.clone(),
                    diagram_type: result.diagram_type.clone(),
                    complexity: result.complexity,
                    signature: sig.clone(),
                    input_preview: entry.input.chars().take(100).collect(),
                });
            }
        }
    }

    report.pass_rate = if report.total > 0 {
        report.passed as f64 / report.total as f64 * 100.0
    } else {
        100.0
    };

    report.warning_rate = if report.total > 0 {
        total_warnings as f64 / report.total as f64
    } else {
        0.0
    };

    // Compute per-type stats
    for (dtype, results) in by_type {
        let total = results.len();
        let passed = results.iter().filter(|r| r.overall_success()).count();
        let warnings: usize = results.iter().map(|r| r.parse_warnings).sum();
        let total_time: u64 = results.iter().map(|r| r.elapsed_us).sum();
        let avg_time = if total > 0 {
            total_time as f64 / total as f64
        } else {
            0.0
        };

        report.by_diagram_type.insert(
            dtype,
            TypeStats {
                total,
                passed,
                warnings,
                avg_time_us: avg_time,
            },
        );
    }

    for (complexity, results) in by_complexity {
        let total = results.len();
        let passed = results.iter().filter(|r| r.overall_success()).count();
        let warnings: usize = results.iter().map(|r| r.parse_warnings).sum();
        let total_time: u64 = results.iter().map(|r| r.elapsed_us).sum();
        let avg_time = if total > 0 {
            total_time as f64 / total as f64
        } else {
            0.0
        };

        report.by_complexity.insert(
            complexity,
            TypeStats {
                total,
                passed,
                warnings,
                avg_time_us: avg_time,
            },
        );
    }

    // Top failure signatures
    let mut sigs: Vec<_> = failure_counts.into_iter().collect();
    sigs.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    report.top_failure_signatures = sigs.into_iter().take(10).collect();

    // Limit high-value failures to most impactful
    report.high_value_failures.truncate(20);

    report
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn corpus_loads_and_deduplicates() {
    let corpus = load_corpus();
    // Synthetic corpus should always be present
    assert!(!corpus.is_empty(), "Corpus should not be empty");

    // Check deduplication works (no duplicate IDs)
    let ids: BTreeSet<_> = corpus.iter().map(|e| &e.id).collect();
    assert_eq!(ids.len(), corpus.len(), "Corpus should have unique IDs");
}

#[test]
fn compatibility_check_runs_on_all_entries() {
    let corpus = load_corpus();
    let results: Vec<_> = corpus.iter().map(check_compatibility).collect();

    assert_eq!(results.len(), corpus.len());

    // Log evidence
    for result in &results {
        let entry = serde_json::json!({
            "scenario_id": result.id,
            "diagram_type": result.diagram_type,
            "complexity": result.complexity.to_string(),
            "parse_success": result.parse_success,
            "parse_warnings": result.parse_warnings,
            "layout_success": result.layout_success,
            "render_success": result.render_success,
            "overall_success": result.overall_success(),
            "failure_signature": result.failure_signature,
            "elapsed_us": result.elapsed_us,
            "surface": "corpus-ingestion",
        });
        println!("{}", serde_json::to_string(&entry).unwrap());
    }
}

#[test]
fn trend_report_generates_correctly() {
    let corpus = load_corpus();
    let results: Vec<_> = corpus.iter().map(check_compatibility).collect();
    let report = analyze_trends(&results, &corpus);

    // Basic sanity checks
    assert_eq!(report.total, corpus.len());
    assert_eq!(report.passed + report.failed, report.total);
    assert!(report.pass_rate >= 0.0 && report.pass_rate <= 100.0);

    // Log trend report
    let entry = serde_json::json!({
        "total": report.total,
        "passed": report.passed,
        "failed": report.failed,
        "pass_rate": report.pass_rate,
        "warning_rate": report.warning_rate,
        "by_diagram_type": report.by_diagram_type.keys().collect::<Vec<_>>(),
        "by_complexity": report.by_complexity.keys().collect::<Vec<_>>(),
        "top_failure_signatures": report.top_failure_signatures,
        "high_value_failure_count": report.high_value_failures.len(),
        "surface": "corpus-ingestion-trend",
    });
    println!("{}", serde_json::to_string(&entry).unwrap());
}

#[test]
fn synthetic_corpus_has_coverage() {
    let corpus = synthetic_corpus();

    // Should have multiple diagram types
    let types: BTreeSet<_> = corpus.iter().map(|e| &e.diagram_type).collect();
    assert!(
        types.len() >= 5,
        "Synthetic corpus should cover multiple diagram types"
    );

    // Should have multiple complexity levels
    let complexities: BTreeSet<_> = corpus.iter().map(|e| e.complexity).collect();
    assert!(
        complexities.len() >= 2,
        "Synthetic corpus should have varying complexity"
    );
}

#[test]
fn corpus_evidence_log_emitted() {
    let corpus = load_corpus();
    let results: Vec<_> = corpus.iter().map(check_compatibility).collect();
    let report = analyze_trends(&results, &corpus);

    let evidence = serde_json::json!({
        "gate_id": "corpus_ingestion",
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        "corpus_size": corpus.len(),
        "pass_rate": report.pass_rate,
        "warning_rate": report.warning_rate,
        "diagram_types_covered": report.by_diagram_type.len(),
        "failure_signatures": report.top_failure_signatures.len(),
        "high_value_failures": report.high_value_failures.len(),
        "pass_fail_reason": if report.pass_rate >= 80.0 { "pass" } else { "needs_attention" },
        "surface": "corpus-ingestion-gate",
    });
    println!("{}", serde_json::to_string(&evidence).unwrap());
}
