#![forbid(unsafe_code)]

pub mod art;
pub mod cga;
pub mod constraints;
pub mod epoch;
mod font_metrics;
pub mod leapfrog;
#[cfg(test)]
mod lens_tests;
pub mod quotient_filter;
pub mod succinct;

pub use font_metrics::{
    CharWidthClass, DiagnosticLevel, FontMetrics, FontMetricsConfig, FontMetricsDiagnostic,
    FontPreset, is_east_asian_wide,
};

use std::collections::{BTreeMap, BTreeSet};

pub use franken_kernel::{Budget, Cx, DecisionId, NoCaps, PolicyId, SchemaVersion, TraceId};
pub use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

// ── Fast hash collections for internal graph IDs ─────────────────────
//
// NodeId and EdgeId are sequential integers assigned by the parser. We use
// FxHash (multiply-shift) instead of SipHash since HashDoS is not a concern
// for internally-generated keys. This yields ~7-10x faster hash throughput.

/// A `HashMap` optimised for [`IrNodeId`] keys (FxHash).
pub type NodeMap<V> = FxHashMap<IrNodeId, V>;

/// A `HashSet` optimised for [`IrNodeId`] values (FxHash).
pub type NodeSet = FxHashSet<IrNodeId>;

/// A `HashMap` optimised for `usize` edge-index keys (FxHash).
pub type EdgeMap<V> = FxHashMap<usize, V>;

pub const MERMAID_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);

#[must_use]
pub fn mermaid_layout_guard_policy_id() -> PolicyId {
    PolicyId::new("fm.layout.guard", 1)
}

#[must_use]
pub fn mermaid_root_cx(trace_id: TraceId, budget_ms: u64) -> Cx<'static, NoCaps> {
    Cx::new(trace_id, Budget::new(budget_ms), NoCaps)
}

#[must_use]
pub fn mermaid_trace_id(surface: &str, source: &str) -> TraceId {
    TraceId::from_raw(stable_u128_hash("trace", &[surface, source]))
}

#[must_use]
pub fn mermaid_decision_id(
    trace_id: TraceId,
    policy_id: &PolicyId,
    phase: &str,
    detail: &str,
) -> DecisionId {
    let trace = trace_id.to_string();
    let version = policy_id.version().to_string();
    DecisionId::from_raw(stable_u128_hash(
        "decision",
        &[&trace, policy_id.name(), &version, phase, detail],
    ))
}

fn stable_u128_hash(domain: &str, parts: &[&str]) -> u128 {
    let upper = stable_u64_hash("upper", domain, parts);
    let lower = stable_u64_hash("lower", domain, parts);
    (u128::from(upper) << 64) | u128::from(lower)
}

fn stable_u64_hash(salt: &str, domain: &str, parts: &[&str]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    let mut update = |s: &str| {
        for byte in s.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };

    update(salt);
    update(domain);
    for part in parts {
        update(part);
    }
    hash
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidObservabilityIds {
    pub trace_id: TraceId,
    pub decision_id: DecisionId,
    pub policy_id: PolicyId,
    #[serde(with = "schema_version_semver")]
    pub schema_version: SchemaVersion,
}

impl Default for MermaidObservabilityIds {
    fn default() -> Self {
        Self {
            trace_id: TraceId::from_raw(0),
            decision_id: DecisionId::from_raw(0),
            policy_id: mermaid_layout_guard_policy_id(),
            schema_version: MERMAID_SCHEMA_VERSION,
        }
    }
}

#[must_use]
pub fn mermaid_layout_guard_observability(
    surface: &str,
    source: &str,
    selected_algorithm: &str,
    budget_ms: u64,
) -> (Cx<'static, NoCaps>, MermaidObservabilityIds) {
    let trace_id = mermaid_trace_id(surface, source);
    let cx = mermaid_root_cx(trace_id, budget_ms);
    let policy_id = mermaid_layout_guard_policy_id();
    let cx_trace_id = cx.trace_id();
    let decision_id =
        mermaid_decision_id(cx_trace_id, &policy_id, "layout.guard", selected_algorithm);
    (
        cx,
        MermaidObservabilityIds {
            trace_id: cx_trace_id,
            decision_id,
            policy_id,
            schema_version: MERMAID_SCHEMA_VERSION,
        },
    )
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
    pub byte: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn at_line(line: usize, line_len: usize) -> Self {
        let start = Position {
            line,
            col: 1,
            byte: 0,
        };
        let end = Position {
            line,
            col: line_len.max(1),
            byte: 0,
        };
        Self::new(start, end)
    }

    #[must_use]
    pub const fn is_unknown(self) -> bool {
        self.start.line == 0
            && self.start.col == 0
            && self.start.byte == 0
            && self.end.line == 0
            && self.end.col == 0
            && self.end.byte == 0
    }

    #[must_use]
    pub fn compact_display(self) -> String {
        format!(
            "{}:{}-{}:{}@{}-{}",
            self.start.line,
            self.start.col,
            self.end.line,
            self.end.col,
            self.start.byte,
            self.end.byte
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidErrorCode {
    #[default]
    Parse,
    Validation,
    Unsupported,
}

impl MermaidErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "mermaid/error/parse",
            Self::Validation => "mermaid/error/validation",
            Self::Unsupported => "mermaid/error/unsupported",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq, Eq)]
pub enum MermaidError {
    #[error("{message}")]
    Parse {
        message: String,
        span: Span,
        expected: Vec<String>,
    },
    #[error("{message}")]
    Validation { message: String, span: Span },
    #[error("{message}")]
    Unsupported { message: String, span: Span },
}

impl MermaidError {
    #[must_use]
    pub const fn code(&self) -> MermaidErrorCode {
        match self {
            Self::Parse { .. } => MermaidErrorCode::Parse,
            Self::Validation { .. } => MermaidErrorCode::Validation,
            Self::Unsupported { .. } => MermaidErrorCode::Unsupported,
        }
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Parse { span, .. }
            | Self::Validation { span, .. }
            | Self::Unsupported { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidWarningCode {
    #[default]
    ParseRecovery,
    UnsupportedStyle,
    UnsupportedLink,
    UnsupportedFeature,
}

impl MermaidWarningCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseRecovery => "mermaid/warn/parse-recovery",
            Self::UnsupportedStyle => "mermaid/warn/unsupported-style",
            Self::UnsupportedLink => "mermaid/warn/unsupported-link",
            Self::UnsupportedFeature => "mermaid/warn/unsupported-feature",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidWarning {
    pub code: MermaidWarningCode,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum DiagramType {
    Flowchart,
    Sequence,
    State,
    Gantt,
    Class,
    Er,
    Mindmap,
    Pie,
    GitGraph,
    Journey,
    Requirement,
    Timeline,
    QuadrantChart,
    Sankey,
    XyChart,
    BlockBeta,
    PacketBeta,
    ArchitectureBeta,
    C4Context,
    C4Container,
    C4Component,
    C4Dynamic,
    C4Deployment,
    Kanban,
    #[default]
    Unknown,
}

impl DiagramType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence",
            Self::State => "state",
            Self::Gantt => "gantt",
            Self::Class => "class",
            Self::Er => "er",
            Self::Mindmap => "mindmap",
            Self::Pie => "pie",
            Self::GitGraph => "gitGraph",
            Self::Journey => "journey",
            Self::Requirement => "requirementDiagram",
            Self::Timeline => "timeline",
            Self::QuadrantChart => "quadrantChart",
            Self::Sankey => "sankey",
            Self::XyChart => "xyChart",
            Self::BlockBeta => "block-beta",
            Self::PacketBeta => "packet-beta",
            Self::ArchitectureBeta => "architecture-beta",
            Self::C4Context => "C4Context",
            Self::C4Container => "C4Container",
            Self::C4Component => "C4Component",
            Self::C4Dynamic => "C4Dynamic",
            Self::C4Deployment => "C4Deployment",
            Self::Kanban => "kanban",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn support_level(self) -> MermaidSupportLevel {
        match self {
            Self::Flowchart
            | Self::Class
            | Self::State
            | Self::Er
            | Self::Gantt
            | Self::Journey
            | Self::Mindmap
            | Self::Timeline
            | Self::QuadrantChart
            | Self::Requirement
            | Self::GitGraph
            | Self::BlockBeta
            | Self::PacketBeta
            | Self::Sankey
            | Self::ArchitectureBeta
            | Self::C4Context
            | Self::C4Container
            | Self::C4Component
            | Self::C4Dynamic
            | Self::C4Deployment
            | Self::Pie
            | Self::XyChart
            | Self::Kanban => MermaidSupportLevel::Supported,
            Self::Sequence => MermaidSupportLevel::Partial,
            Self::Unknown => MermaidSupportLevel::Unsupported,
        }
    }

    #[must_use]
    pub const fn support_label(self) -> &'static str {
        match self {
            Self::Flowchart
            | Self::Class
            | Self::State
            | Self::Er
            | Self::Gantt
            | Self::Journey
            | Self::Mindmap
            | Self::Timeline
            | Self::QuadrantChart
            | Self::Requirement
            | Self::GitGraph
            | Self::BlockBeta
            | Self::PacketBeta
            | Self::Sankey
            | Self::ArchitectureBeta
            | Self::C4Context
            | Self::C4Container
            | Self::C4Component
            | Self::C4Dynamic
            | Self::C4Deployment
            | Self::Pie
            | Self::XyChart
            | Self::Kanban => "full",
            Self::Sequence => "partial",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidSupportLevel {
    #[default]
    Supported,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MermaidParseMode {
    Strict,
    #[default]
    Compat,
    Recover,
}

impl MermaidParseMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Compat => "compat",
            Self::Recover => "recover",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Implemented,
    Partial,
    Experimental,
    Planned,
    Unsupported,
}

impl CapabilityStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Partial => "partial",
            Self::Experimental => "experimental",
            Self::Planned => "planned",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityEvidence {
    pub kind: String,
    pub reference: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityClaim {
    pub id: String,
    pub category: String,
    pub title: String,
    pub status: CapabilityStatus,
    pub advertised_in: Vec<String>,
    pub code_paths: Vec<String>,
    pub evidence: Vec<CapabilityEvidence>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityMatrix {
    #[serde(with = "schema_version_semver")]
    pub schema_version: SchemaVersion,
    pub project: String,
    pub status_counts: BTreeMap<String, usize>,
    pub claims: Vec<CapabilityClaim>,
}

#[must_use]
pub fn capability_matrix() -> CapabilityMatrix {
    let mut claims = documented_diagram_type_claims();
    claims.extend(surface_capability_claims());

    let mut status_counts = BTreeMap::new();
    for claim in &claims {
        *status_counts
            .entry(claim.status.as_str().to_string())
            .or_insert(0) += 1;
    }

    CapabilityMatrix {
        schema_version: MERMAID_SCHEMA_VERSION,
        project: String::from("frankenmermaid"),
        status_counts,
        claims,
    }
}

/// Returns the capability matrix as a pretty-printed JSON string.
///
/// # Errors
///
/// Returns a `serde_json::Error` if the internal capability matrix fails to serialize.
pub fn capability_matrix_json_pretty() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&capability_matrix())
}

#[must_use]
pub fn capability_readme_supported_diagram_types_markdown() -> String {
    let mut lines = vec![
        String::from("| Diagram Type | Runtime Status |"),
        String::from("|--------------|----------------|"),
    ];

    for diagram_type in documented_diagram_types() {
        let status = match diagram_type.support_level() {
            MermaidSupportLevel::Supported => CapabilityStatus::Implemented,
            MermaidSupportLevel::Partial => CapabilityStatus::Partial,
            MermaidSupportLevel::Unsupported => CapabilityStatus::Unsupported,
        };
        lines.push(format!(
            "| `{}` | {} |",
            diagram_type.as_str(),
            capability_status_label(status)
        ));
    }

    lines.join("\n")
}

#[must_use]
pub fn capability_readme_surface_markdown() -> String {
    let matrix = capability_matrix();
    let mut lines = vec![
        String::from("| Surface | Status | Evidence |"),
        String::from("|---------|--------|----------|"),
    ];

    for claim in matrix
        .claims
        .iter()
        .filter(|claim| claim.category == "surface")
    {
        lines.push(format!(
            "| {} | {} | {} evidence refs |",
            claim.title,
            capability_status_label(claim.status),
            claim.evidence.len()
        ));
    }

    lines.join("\n")
}

const fn capability_status_label(status: CapabilityStatus) -> &'static str {
    match status {
        CapabilityStatus::Implemented => "Implemented",
        CapabilityStatus::Partial => "Partial",
        CapabilityStatus::Experimental => "Experimental",
        CapabilityStatus::Planned => "Planned",
        CapabilityStatus::Unsupported => "Unsupported",
    }
}

fn documented_diagram_type_claims() -> Vec<CapabilityClaim> {
    documented_diagram_types()
        .iter()
        .map(|diagram_type| CapabilityClaim {
            id: format!("diagram-type/{}", diagram_type.as_str()),
            category: String::from("diagram_type"),
            title: format!("Support {} diagrams", diagram_type.as_str()),
            status: match diagram_type.support_level() {
                MermaidSupportLevel::Supported => CapabilityStatus::Implemented,
                MermaidSupportLevel::Partial => CapabilityStatus::Partial,
                MermaidSupportLevel::Unsupported => CapabilityStatus::Unsupported,
            },
            advertised_in: vec![String::from("README.md#supported-diagram-types")],
            code_paths: vec![
                String::from("crates/fm-core/src/lib.rs::DiagramType"),
                String::from("crates/fm-parser/src/lib.rs::detect_type_with_confidence"),
            ],
            evidence: vec![
                CapabilityEvidence {
                    kind: String::from("code_path"),
                    reference: String::from("crates/fm-core/src/lib.rs::DiagramType::support_level"),
                    note: Some(String::from("Source-of-truth support taxonomy")),
                },
                CapabilityEvidence {
                    kind: String::from("test"),
                    reference: String::from(
                        "crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations",
                    ),
                    note: Some(String::from("Verifies advertised support level mapping")),
                },
            ],
            notes: vec![format!(
                "README advertises this family; current code marks it as {} capability",
                diagram_type.support_label()
            )],
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn surface_capability_claims() -> Vec<CapabilityClaim> {
    vec![
        CapabilityClaim {
            id: String::from("surface/cli-detect"),
            category: String::from("surface"),
            title: String::from("CLI detect command"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#quick-example"),
                String::from("README.md#command-reference"),
            ],
            code_paths: vec![
                String::from("crates/fm-cli/src/main.rs::Command::Detect"),
                String::from("crates/fm-parser/src/lib.rs::detect_type_with_confidence"),
            ],
            evidence: vec![
                CapabilityEvidence {
                    kind: String::from("test"),
                    reference: String::from(
                        "crates/fm-parser/src/lib.rs::tests::detects_flowchart_keyword",
                    ),
                    note: Some(String::from("Smoke coverage for type detection")),
                },
                CapabilityEvidence {
                    kind: String::from("code_path"),
                    reference: String::from("crates/fm-cli/src/main.rs::cmd_detect"),
                    note: None,
                },
            ],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/cli-parse"),
            category: String::from("surface"),
            title: String::from("CLI parse command with IR JSON evidence"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#quick-example"),
                String::from("README.md#command-reference"),
            ],
            code_paths: vec![
                String::from("crates/fm-cli/src/main.rs::Command::Parse"),
                String::from("crates/fm-parser/src/lib.rs::parse_evidence_json"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-parser/src/lib.rs::tests::parse_flowchart_extracts_nodes_edges_and_direction",
                ),
                note: Some(String::from(
                    "Validates parse output contains structural IR",
                )),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/cli-render-svg"),
            category: String::from("surface"),
            title: String::from("CLI SVG rendering"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#quick-example"),
                String::from("README.md#command-reference"),
            ],
            code_paths: vec![
                String::from("crates/fm-cli/src/main.rs::Command::Render"),
                String::from("crates/fm-render-svg/src/lib.rs::render_svg_with_layout"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-render-svg/src/lib.rs::tests::prop_svg_render_is_total_and_counts_match",
                ),
                note: Some(String::from("SVG renderer smoke coverage")),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/cli-render-term"),
            category: String::from("surface"),
            title: String::from("CLI terminal rendering"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#quick-example"),
                String::from("README.md#command-reference"),
            ],
            code_paths: vec![
                String::from("crates/fm-cli/src/main.rs::Command::Render"),
                String::from("crates/fm-render-term/src/lib.rs::render_term_with_config"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-render-term/src/lib.rs::tests::render_term_produces_output",
                ),
                note: Some(String::from("Terminal renderer smoke coverage")),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/cli-validate"),
            category: String::from("surface"),
            title: String::from("CLI validate command with structured diagnostics"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#quick-example"),
                String::from("README.md#command-reference"),
            ],
            code_paths: vec![
                String::from("crates/fm-cli/src/main.rs::Command::Validate"),
                String::from("crates/fm-core/src/lib.rs::StructuredDiagnostic"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-cli/src/main.rs::tests::collect_validation_diagnostics_includes_parse_warnings",
                ),
                note: Some(String::from("Validate path emits structured diagnostics")),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/cli-capabilities"),
            category: String::from("surface"),
            title: String::from("CLI capability matrix command"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#command-reference"),
                String::from("README.md#runtime-capability-metadata"),
            ],
            code_paths: vec![
                String::from("crates/fm-cli/src/main.rs::Command::Capabilities"),
                String::from("crates/fm-cli/src/main.rs::cmd_capabilities"),
                String::from("crates/fm-core/src/lib.rs::capability_matrix"),
            ],
            evidence: vec![
                CapabilityEvidence {
                    kind: String::from("test"),
                    reference: String::from(
                        "crates/fm-core/src/lib.rs::tests::capability_matrix_json_matches_checked_in_artifact",
                    ),
                    note: Some(String::from(
                        "CLI command serializes the checked-in capability artifact",
                    )),
                },
                CapabilityEvidence {
                    kind: String::from("code_path"),
                    reference: String::from("crates/fm-cli/src/main.rs::cmd_capabilities"),
                    note: None,
                },
            ],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/wasm-svg"),
            category: String::from("surface"),
            title: String::from("WASM API renders SVG"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#javascript--wasm-api"),
                String::from("README.md#technical-architecture"),
            ],
            code_paths: vec![
                String::from("crates/fm-wasm/src/lib.rs::render"),
                String::from("crates/fm-wasm/src/lib.rs::render_svg_js"),
                String::from("crates/fm-wasm/src/lib.rs::Diagram::render"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-wasm/src/lib.rs::tests::render_returns_svg_and_type",
                ),
                note: Some(String::from("WASM facade smoke coverage")),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/wasm-capabilities"),
            category: String::from("surface"),
            title: String::from("WASM API exposes capability matrix metadata"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#javascript--wasm-api"),
                String::from("README.md#runtime-capability-metadata"),
            ],
            code_paths: vec![
                String::from("crates/fm-wasm/src/lib.rs::capability_matrix_js"),
                String::from("crates/fm-core/src/lib.rs::capability_matrix"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-wasm/src/lib.rs::tests::capability_matrix_js_returns_matrix_payload",
                ),
                note: Some(String::from(
                    "WASM surface returns the shared capability matrix",
                )),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("surface/canvas"),
            category: String::from("surface"),
            title: String::from("Canvas rendering backend"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#why-use-frankenmermaid"),
                String::from("README.md#technical-architecture"),
            ],
            code_paths: vec![
                String::from("crates/fm-render-canvas/src/lib.rs::render_to_canvas"),
                String::from("crates/fm-wasm/src/lib.rs::Diagram::render"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-render-canvas/src/lib.rs::tests::render_with_mock_context",
                ),
                note: Some(String::from("Canvas backend exercises draw pipeline")),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("layout/deterministic"),
            category: String::from("layout"),
            title: String::from("Deterministic layout output"),
            status: CapabilityStatus::Implemented,
            advertised_in: vec![
                String::from("README.md#design-philosophy"),
                String::from("README.md#faq"),
            ],
            code_paths: vec![
                String::from("crates/fm-layout/src/lib.rs::layout_diagram_traced"),
                String::from("crates/fm-layout/src/lib.rs::crossing_refinement"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-layout/src/lib.rs::tests::traced_layout_is_deterministic",
                ),
                note: Some(String::from(
                    "Checks full traced layout equality across runs",
                )),
            }],
            notes: vec![],
        },
        CapabilityClaim {
            id: String::from("parser/recovery"),
            category: String::from("parser"),
            title: String::from("Best-effort parse with warnings instead of hard failure"),
            status: CapabilityStatus::Partial,
            advertised_in: vec![
                String::from("README.md#tl-dr"),
                String::from("README.md#design-philosophy"),
            ],
            code_paths: vec![
                String::from("crates/fm-parser/src/lib.rs::parse"),
                String::from("crates/fm-core/src/lib.rs::MermaidWarning"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("test"),
                reference: String::from(
                    "crates/fm-parser/src/lib.rs::tests::empty_input_returns_warning",
                ),
                note: Some(String::from(
                    "Current coverage proves warning-based fallback for empty input",
                )),
            }],
            notes: vec![String::from(
                "Recovery exists, but README claims are broader than current automated evidence",
            )],
        },
        CapabilityClaim {
            id: String::from("runtime/guard-report"),
            category: String::from("runtime"),
            title: String::from("Guard and degradation report types exist in shared IR"),
            status: CapabilityStatus::Experimental,
            advertised_in: vec![
                String::from("AGENTS.md#key-design-decisions"),
                String::from("README.md#technical-architecture"),
            ],
            code_paths: vec![
                String::from("crates/fm-core/src/lib.rs::MermaidGuardReport"),
                String::from("crates/fm-core/src/lib.rs::MermaidDegradationPlan"),
            ],
            evidence: vec![CapabilityEvidence {
                kind: String::from("code_path"),
                reference: String::from("crates/fm-core/src/lib.rs::MermaidDiagramMeta"),
                note: Some(String::from(
                    "Types are threaded into IR metadata but not yet fully activated",
                )),
            }],
            notes: vec![String::from(
                "Data model exists; cross-pipeline activation is still an open backlog item",
            )],
        },
    ]
}

const fn documented_diagram_types() -> &'static [DiagramType] {
    const DOCUMENTED: &[DiagramType] = &[
        DiagramType::Flowchart,
        DiagramType::Sequence,
        DiagramType::Class,
        DiagramType::State,
        DiagramType::Er,
        DiagramType::C4Context,
        DiagramType::C4Container,
        DiagramType::C4Component,
        DiagramType::C4Dynamic,
        DiagramType::C4Deployment,
        DiagramType::ArchitectureBeta,
        DiagramType::BlockBeta,
        DiagramType::Gantt,
        DiagramType::Timeline,
        DiagramType::Journey,
        DiagramType::GitGraph,
        DiagramType::Sankey,
        DiagramType::Mindmap,
        DiagramType::Pie,
        DiagramType::QuadrantChart,
        DiagramType::XyChart,
        DiagramType::Requirement,
        DiagramType::PacketBeta,
        DiagramType::Kanban,
    ];
    DOCUMENTED
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum GraphDirection {
    #[default]
    TB,
    TD,
    LR,
    RL,
    BT,
}

impl GraphDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TB => "TB",
            Self::TD => "TD",
            Self::LR => "LR",
            Self::RL => "RL",
            Self::BT => "BT",
        }
    }
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
)]
pub struct IrNodeId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrPortId(pub usize);

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
)]
pub struct IrLabelId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrClusterId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrSubgraphId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IrPortSideHint {
    #[default]
    Auto,
    Horizontal,
    Vertical,
}

impl IrPortSideHint {
    #[must_use]
    pub const fn from_direction(direction: GraphDirection) -> Self {
        match direction {
            GraphDirection::LR | GraphDirection::RL => Self::Horizontal,
            GraphDirection::TB | GraphDirection::TD | GraphDirection::BT => Self::Vertical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrLabel {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IrLabelSegment {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        code: bool,
        strike: bool,
    },
    LineBreak,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum NodeShape {
    #[default]
    Rect,
    Rounded,
    Stadium,
    Subroutine,
    Diamond,
    Hexagon,
    Circle,
    FilledCircle,
    Asymmetric,
    Cylinder,
    Trapezoid,
    DoubleCircle,
    HorizontalBar,
    Note,
    // Extended shapes for FrankenMermaid
    InvTrapezoid,
    Parallelogram,
    InvParallelogram,
    Triangle,
    Pentagon,
    Star,
    Cloud,
    Tag,
    CrossedCircle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum ArrowType {
    #[default]
    Line,
    Arrow,
    OpenArrow,
    HalfArrowTop,
    HalfArrowBottom,
    HalfArrowTopReverse,
    HalfArrowBottomReverse,
    StickArrowTop,
    StickArrowBottom,
    StickArrowTopReverse,
    StickArrowBottomReverse,
    ThickArrow,
    DottedArrow,
    DottedOpenArrow,
    DottedCross,
    HalfArrowTopDotted,
    HalfArrowBottomDotted,
    HalfArrowTopReverseDotted,
    HalfArrowBottomReverseDotted,
    StickArrowTopDotted,
    StickArrowBottomDotted,
    StickArrowTopReverseDotted,
    StickArrowBottomReverseDotted,
    Circle,
    Cross,
    ThickLine,
    DottedLine,
    DoubleArrow,
    DoubleThickArrow,
    DoubleDottedArrow,
}

impl ArrowType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Line => "---",
            Self::Arrow => "-->",
            Self::OpenArrow => "-)",
            Self::HalfArrowTop => "-|\\",
            Self::HalfArrowBottom => "-|/",
            Self::HalfArrowTopReverse => "/|-",
            Self::HalfArrowBottomReverse => "\\|-",
            Self::StickArrowTop => "-\\\\",
            Self::StickArrowBottom => "-//",
            Self::StickArrowTopReverse => "//-",
            Self::StickArrowBottomReverse => "\\\\-",
            Self::ThickArrow => "==>",
            Self::DottedArrow => "-.->",
            Self::DottedOpenArrow => "--)",
            Self::DottedCross => "--x",
            Self::HalfArrowTopDotted => "--|\\",
            Self::HalfArrowBottomDotted => "--|/",
            Self::HalfArrowTopReverseDotted => "/|--",
            Self::HalfArrowBottomReverseDotted => "\\|--",
            Self::StickArrowTopDotted => "--\\\\",
            Self::StickArrowBottomDotted => "--//",
            Self::StickArrowTopReverseDotted => "//--",
            Self::StickArrowBottomReverseDotted => "\\\\--",
            Self::Circle => "--o",
            Self::Cross => "-x",
            Self::ThickLine => "===",
            Self::DottedLine => "-.-",
            Self::DoubleArrow => "<-->",
            Self::DoubleThickArrow => "<==>",
            Self::DoubleDottedArrow => "<-.->",
        }
    }
}

/// Key modifier for ER entity attributes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum IrAttributeKey {
    /// Primary key
    Pk,
    /// Foreign key
    Fk,
    /// Unique key
    Uk,
    /// No key modifier
    #[default]
    None,
}

/// An attribute/member of an ER entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrEntityAttribute {
    /// Data type of the attribute (e.g., "int", "string", "varchar(255)")
    pub data_type: String,
    /// Name of the attribute
    pub name: String,
    /// Key modifier (PK, FK, UK, or None)
    pub key: IrAttributeKey,
    /// Optional comment/description
    pub comment: Option<String>,
}

// ── Class-diagram member types ─────────────────────────────────────────

/// Visibility modifier for a class member.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum ClassVisibility {
    #[default]
    Public,
    Private,
    Protected,
    Package,
}

/// Whether a class member is an attribute (field) or a method (operation).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum ClassMemberKind {
    #[default]
    Attribute,
    Method,
}

/// A single class member (attribute or method).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrClassMember {
    pub visibility: ClassVisibility,
    pub kind: ClassMemberKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub is_static: bool,
    pub is_abstract: bool,
}

/// Stereotype annotation for a class (e.g., `<<interface>>`, `<<abstract>>`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ClassStereotype {
    Interface,
    Abstract,
    Enum,
    Service,
    Custom(String),
}

/// Class-diagram-specific metadata for a node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrClassNodeMeta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<IrClassMember>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<IrClassMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stereotype: Option<ClassStereotype>,
    /// Generic type parameters, e.g. `["T"]` for `List~T~`, `["K","V"]` for `Map~K,V~`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<String>,
}

/// C4-diagram-specific metadata for a node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrC4NodeMeta {
    pub element_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technology: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrMenuLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrNode {
    pub id: String,
    pub label: Option<IrLabelId>,
    pub shape: NodeShape,
    /// Optional icon metadata attached to the node (`::icon(...)`, architecture icons, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub classes: Vec<String>,
    pub href: Option<String>,
    /// JavaScript callback function name from `click nodeId call functionName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<String>,
    /// Tooltip text from `click nodeId "url" "tooltip"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu_links: Vec<IrMenuLink>,
    pub span_primary: Span,
    pub span_all: Vec<Span>,
    pub implicit: bool,
    /// Entity attributes/members (for ER diagrams)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<IrEntityAttribute>,
    /// Class-diagram-specific metadata (attributes, methods, stereotypes)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_meta: Option<IrClassNodeMeta>,
    /// Requirement-diagram metadata (type, id, text, risk, verifymethod)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_meta: Option<IrRequirementNodeMeta>,
    /// C4-diagram-specific metadata (element type, technology, description)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c4_meta: Option<IrC4NodeMeta>,
    /// Parsed inline style from `style nodeId ...` directives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_style: Option<IrInlineStyle>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum IrNodeKind {
    #[default]
    Generic,
    Entity,
    Participant,
    State,
    Task,
    Event,
    Commit,
    Requirement,
    Slice,
    Point,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrPort {
    pub node: IrNodeId,
    pub name: String,
    pub side_hint: IrPortSideHint,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IrEndpoint {
    #[default]
    Unresolved,
    Node(IrNodeId),
    Port(IrPortId),
}

impl IrEndpoint {
    #[must_use]
    pub fn resolved_node_id(self, ports: &[IrPort]) -> Option<IrNodeId> {
        match self {
            Self::Unresolved => None,
            Self::Node(node_id) => Some(node_id),
            Self::Port(port_id) => ports.get(port_id.0).map(|port| port.node),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrEdge {
    pub from: IrEndpoint,
    pub to: IrEndpoint,
    pub arrow: ArrowType,
    pub label: Option<IrLabelId>,
    pub span: Span,
    /// Raw ER cardinality operator (e.g., `"||--o{"`), stored only for ER diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub er_notation: Option<String>,
    /// Source-side cardinality label (e.g., `"1"`, `"0..*"`) for class diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_cardinality: Option<String>,
    /// Target-side cardinality label (e.g., `"*"`, `"1..*"`) for class diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_cardinality: Option<String>,
    /// Guard condition on a state transition (e.g., `[isValid]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    /// Action on a state transition (e.g., `cleanup()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Parsed inline style from `linkStyle N ...` directives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_style: Option<IrInlineStyle>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum IrEdgeKind {
    #[default]
    Generic,
    Relationship,
    Message,
    Timeline,
    Dependency,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrCluster {
    pub id: IrClusterId,
    pub title: Option<IrLabelId>,
    pub members: Vec<IrNodeId>,
    pub grid_span: usize,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrSubgraph {
    pub id: IrSubgraphId,
    pub key: String,
    pub title: Option<IrLabelId>,
    pub parent: Option<IrSubgraphId>,
    pub children: Vec<IrSubgraphId>,
    pub members: Vec<IrNodeId>,
    pub cluster: Option<IrClusterId>,
    pub grid_span: usize,
    pub span: Span,
    /// Per-subgraph direction override (e.g., `direction LR` inside a subgraph block).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<GraphDirection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrGraphNode {
    pub node_id: IrNodeId,
    pub kind: IrNodeKind,
    pub clusters: Vec<IrClusterId>,
    pub subgraphs: Vec<IrSubgraphId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrGraphEdge {
    pub edge_id: usize,
    pub kind: IrEdgeKind,
    pub from: IrEndpoint,
    pub to: IrEndpoint,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrGraphCluster {
    pub cluster_id: IrClusterId,
    pub title: Option<IrLabelId>,
    pub members: Vec<IrNodeId>,
    pub subgraph: Option<IrSubgraphId>,
    pub grid_span: usize,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidGraphIr {
    pub nodes: Vec<IrGraphNode>,
    pub edges: Vec<IrGraphEdge>,
    pub clusters: Vec<IrGraphCluster>,
    pub subgraphs: Vec<IrSubgraph>,
}

impl MermaidGraphIr {
    #[must_use]
    pub fn node(&self, node_id: IrNodeId) -> Option<&IrGraphNode> {
        self.nodes.get(node_id.0)
    }

    #[must_use]
    pub fn edge(&self, edge_id: usize) -> Option<&IrGraphEdge> {
        self.edges.get(edge_id)
    }

    #[must_use]
    pub fn cluster(&self, cluster_id: IrClusterId) -> Option<&IrGraphCluster> {
        self.clusters.get(cluster_id.0)
    }

    #[must_use]
    pub fn subgraph(&self, subgraph_id: IrSubgraphId) -> Option<&IrSubgraph> {
        self.subgraphs.get(subgraph_id.0)
    }

    #[must_use]
    pub fn subgraphs_by_key(&self, key: &str) -> Vec<&IrSubgraph> {
        self.subgraphs
            .iter()
            .filter(|subgraph| subgraph.key == key)
            .collect()
    }

    /// Returns the first matching subgraph for a key.
    ///
    /// Mermaid and DOT can legally contain multiple subgraphs with the same key, so
    /// callers that need exhaustive lookup should use [`Self::subgraphs_by_key`].
    #[must_use]
    pub fn first_subgraph_by_key(&self, key: &str) -> Option<&IrSubgraph> {
        self.subgraphs.iter().find(|subgraph| subgraph.key == key)
    }

    #[must_use]
    pub fn root_subgraphs(&self) -> Vec<&IrSubgraph> {
        self.subgraphs
            .iter()
            .filter(|subgraph| subgraph.parent.is_none())
            .collect()
    }

    #[must_use]
    pub fn leaf_subgraphs(&self) -> Vec<&IrSubgraph> {
        self.subgraphs
            .iter()
            .filter(|subgraph| subgraph.children.is_empty())
            .collect()
    }

    #[must_use]
    pub fn node_clusters(&self, node_id: IrNodeId) -> Vec<&IrGraphCluster> {
        self.node(node_id)
            .map(|node| {
                node.clusters
                    .iter()
                    .filter_map(|&cluster_id| self.cluster(cluster_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn node_subgraphs(&self, node_id: IrNodeId) -> Vec<&IrSubgraph> {
        self.node(node_id)
            .map(|node| {
                node.subgraphs
                    .iter()
                    .filter_map(|&subgraph_id| self.subgraph(subgraph_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns ancestors from the root-most parent down to the immediate parent.
    #[must_use]
    pub fn subgraph_ancestors(&self, subgraph_id: IrSubgraphId) -> Vec<&IrSubgraph> {
        let mut ancestors = Vec::new();
        let mut current = self
            .subgraph(subgraph_id)
            .and_then(|subgraph| subgraph.parent);

        while let Some(parent_id) = current {
            let Some(parent) = self.subgraph(parent_id) else {
                break;
            };
            ancestors.push(parent);
            current = parent.parent;
        }

        ancestors.reverse();
        ancestors
    }

    /// Returns descendant subgraphs in deterministic pre-order traversal.
    #[must_use]
    pub fn subgraph_descendants(&self, subgraph_id: IrSubgraphId) -> Vec<&IrSubgraph> {
        let mut descendants = Vec::new();
        let Some(start_subgraph) = self.subgraph(subgraph_id) else {
            return descendants;
        };

        // Use a stack to hold the children. To maintain deterministic pre-order
        // traversal (left-to-right), we push children in reverse order.
        let mut stack: Vec<IrSubgraphId> = start_subgraph.children.iter().copied().rev().collect();

        while let Some(current_id) = stack.pop() {
            let Some(child) = self.subgraph(current_id) else {
                continue;
            };
            descendants.push(child);
            stack.extend(child.children.iter().copied().rev());
        }

        descendants
    }

    /// Returns unique member nodes from this subgraph and all descendant subgraphs.
    #[must_use]
    pub fn subgraph_members_recursive(&self, subgraph_id: IrSubgraphId) -> Vec<IrNodeId> {
        let mut members = Vec::new();
        let mut stack = vec![subgraph_id];

        while let Some(current_id) = stack.pop() {
            let Some(subgraph) = self.subgraph(current_id) else {
                continue;
            };
            members.extend(subgraph.members.iter().copied());
            // Order does not matter since we sort and dedup at the end.
            stack.extend(subgraph.children.iter().copied());
        }

        members.sort_unstable();
        members.dedup();
        members
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IrConstraint {
    SameRank {
        node_ids: Vec<String>,
        span: Span,
    },
    MinLength {
        from_id: String,
        to_id: String,
        min_len: usize,
        span: Span,
    },
    Pin {
        node_id: String,
        x: f64,
        y: f64,
        span: Span,
    },
    OrderInRank {
        node_ids: Vec<String>,
        span: Span,
    },
}

/// Target of a style reference — what gets styled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IrStyleTarget {
    /// `classDef name fill:#fff,stroke:#000` — defines a reusable class.
    Class(String),
    /// `style nodeId fill:#fff` — applies CSS directly to a node.
    Node(IrNodeId),
    /// `linkStyle 0 stroke:#f00` — applies CSS to an edge by index.
    Link(usize),
    /// `linkStyle default stroke:#f00` — default style for all edges.
    LinkDefault,
}

/// A style reference from a `classDef`, `style`, or `linkStyle` directive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrStyleRef {
    pub target: IrStyleTarget,
    /// Raw CSS property string, e.g. `"fill:#fff,stroke:#000,stroke-width:2px"`.
    pub style: String,
    pub span: Span,
}

// ── Structured style types ──────────────────────────────────────────

/// A parsed `classDef` definition — a named, reusable set of CSS-like style
/// properties.
///
/// Example: `classDef important fill:#f9f,stroke:#333,stroke-width:4px`
/// becomes `IrStyleDef { name: "important", properties: {"fill": "#f9f", ...} }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrStyleDef {
    /// Class name (e.g. `"important"`).
    pub name: String,
    /// Parsed CSS-like properties: key → sanitized value.
    pub properties: BTreeMap<String, String>,
    pub span: Span,
}

/// Parsed inline style as a key-value map of CSS-like properties.
///
/// Stored on individual nodes (from `style nodeId ...`) and edges
/// (from `linkStyle N ...`).  All values are sanitized to prevent SVG
/// injection — see [`sanitize_style_value`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrInlineStyle {
    pub properties: BTreeMap<String, String>,
}

impl IrInlineStyle {
    /// Create from an iterator of (key, value) pairs, sanitizing each value.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            properties: pairs
                .into_iter()
                .filter_map(|(k, v)| {
                    let sanitized = sanitize_style_value(&v)?;
                    Some((k, sanitized))
                })
                .collect(),
        }
    }

    /// True when the map has no properties.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    /// Render as a CSS-style string for the `style` attribute.
    #[must_use]
    pub fn to_css_string(&self) -> String {
        self.properties
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The set of CSS-like properties that are safe to pass through to SVG
/// attributes.  Anything not in this list is silently dropped by
/// [`sanitize_style_value`].
const ALLOWED_STYLE_PROPERTIES: &[&str] = &[
    "fill",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-opacity",
    "fill-opacity",
    "opacity",
    "filter",
    "color",
    "font-size",
    "font-weight",
    "font-family",
    "font-style",
    "text-decoration",
    "background",
    "border-radius",
    "padding",
    "rx",
    "ry",
];

/// Returns `true` if `property` is in the allowed-list.
#[must_use]
pub fn is_allowed_style_property(property: &str) -> bool {
    ALLOWED_STYLE_PROPERTIES.contains(&property)
}

/// Sanitize a CSS-like value for safe inclusion in SVG.
///
/// Rejects values containing `url(`, `javascript:`, event-handler patterns
/// (e.g. `onclick`), and XML injection characters (`<`, `>`).
/// Returns `None` for rejected values.
#[must_use]
pub fn sanitize_style_value(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let trimmed = lower.trim();

    // Reject url() values — can load external resources.
    if trimmed.contains("url(") {
        return None;
    }
    // Reject javascript: protocol.
    if trimmed.contains("javascript:") {
        return None;
    }
    // Reject event-handler attributes smuggled as values.
    if trimmed.contains("onclick")
        || trimmed.contains("onerror")
        || trimmed.contains("onload")
        || trimmed.contains("onmouseover")
    {
        return None;
    }
    // Reject XML/SVG injection characters.
    if value.contains('<') || value.contains('>') {
        return None;
    }
    // Reject expression() (IE legacy XSS vector).
    if trimmed.contains("expression(") {
        return None;
    }

    Some(value.trim().to_owned())
}

/// Parse a raw CSS-like property string (`"fill:#fff,stroke:#000,stroke-width:2px"`)
/// into an [`IrInlineStyle`], returning any rejected properties.
///
/// Handles both comma-separated and semicolon-separated declarations, and
/// respects parenthesised values like `rgb(1,2,3)`. Commas are only treated
/// as declaration delimiters when followed by a `key:` pair.
#[must_use]
pub fn parse_style_string_with_rejections(raw: &str) -> (IrInlineStyle, Vec<String>) {
    let mut properties = BTreeMap::new();
    let mut rejected = BTreeSet::new();
    let mut start = 0_usize;
    let mut paren_depth = 0_usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    let is_comma_delimiter = |raw: &str, index: usize| -> bool {
        let bytes = raw.as_bytes();
        let mut cursor = index.saturating_add(1);
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || bytes[cursor] == b'-'
                || bytes[cursor] == b'_')
        {
            cursor += 1;
        }
        if start == cursor {
            return false;
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        matches!(bytes.get(cursor), Some(b':'))
    };

    let push_declaration =
        |s: &str, props: &mut BTreeMap<String, String>, rejected: &mut BTreeSet<String>| {
            let s = s.trim();
            if s.is_empty() {
                return;
            }
            if let Some(colon_pos) = s.find(':') {
                let key = s[..colon_pos].trim().to_ascii_lowercase();
                let val = s[colon_pos + 1..].trim();
                if key.is_empty() {
                    return;
                }
                if !is_allowed_style_property(&key) {
                    rejected.insert(key);
                    return;
                }
                if let Some(sanitized) = sanitize_style_value(val) {
                    props.insert(key, sanitized);
                } else {
                    rejected.insert(key);
                }
            }
        };

    for (index, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quote.is_some() => escaped = true,
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                }
            }
            '(' if quote.is_none() => paren_depth += 1,
            ')' if quote.is_none() => paren_depth = paren_depth.saturating_sub(1),
            ';' if quote.is_none() && paren_depth == 0 => {
                push_declaration(&raw[start..index], &mut properties, &mut rejected);
                start = index + ch.len_utf8();
            }
            ',' if quote.is_none() && paren_depth == 0 && is_comma_delimiter(raw, index) => {
                push_declaration(&raw[start..index], &mut properties, &mut rejected);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    push_declaration(&raw[start..], &mut properties, &mut rejected);

    (IrInlineStyle { properties }, rejected.into_iter().collect())
}

/// Parse a raw CSS-like property string (`"fill:#fff,stroke:#000,stroke-width:2px"`)
/// into an [`IrInlineStyle`].
///
/// Handles both comma-separated and semicolon-separated declarations, and
/// respects parenthesised values like `rgb(1,2,3)`.
#[must_use]
pub fn parse_style_string(raw: &str) -> IrInlineStyle {
    parse_style_string_with_rejections(raw).0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidGlyphMode {
    #[default]
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidRenderMode {
    #[default]
    Auto,
    CellOnly,
    Braille,
    Block,
    HalfBlock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DiagramPalettePreset {
    #[default]
    Default,
    Corporate,
    Neon,
    Monochrome,
    Pastel,
    HighContrast,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidTier {
    Compact,
    #[default]
    Normal,
    Rich,
    Auto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidWrapMode {
    None,
    Word,
    Char,
    #[default]
    WordChar,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidLinkMode {
    Inline,
    Footnote,
    #[default]
    Off,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidSanitizeMode {
    #[default]
    Strict,
    Lenient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidErrorMode {
    #[default]
    Panel,
    Raw,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MermaidConfig {
    pub enabled: bool,
    pub glyph_mode: MermaidGlyphMode,
    pub render_mode: MermaidRenderMode,
    pub tier_override: MermaidTier,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub route_budget: usize,
    pub layout_iteration_budget: usize,
    pub edge_bundling: bool,
    pub edge_bundle_min_count: usize,
    pub max_label_chars: usize,
    pub max_label_lines: usize,
    pub wrap_mode: MermaidWrapMode,
    pub enable_styles: bool,
    pub enable_init_directives: bool,
    pub enable_links: bool,
    pub link_mode: MermaidLinkMode,
    pub sanitize_mode: MermaidSanitizeMode,
    pub error_mode: MermaidErrorMode,
    pub log_path: Option<String>,
    pub cache_enabled: bool,
    pub capability_profile: Option<String>,
    pub debug_overlay: bool,
    pub palette: DiagramPalettePreset,
    /// Mermaid-style theme name from `mermaid.initialize` / init directives.
    pub theme: Option<String>,
    /// Mermaid-style `themeVariables` overrides.
    pub theme_variables: BTreeMap<String, String>,
    /// Mermaid-style flowchart direction hint (`LR`, `TB`, etc.).
    pub flowchart_direction: Option<GraphDirection>,
    /// Mermaid-style flowchart curve mode (for example, `basis`, `linear`).
    pub flowchart_curve: Option<String>,
    /// Mermaid-style sequence mirror actors toggle.
    pub sequence_mirror_actors: Option<bool>,
    /// Mermaid-style sequence message numbering toggle.
    pub sequence_show_sequence_numbers: Option<bool>,
}

impl Default for MermaidConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            glyph_mode: MermaidGlyphMode::Unicode,
            render_mode: MermaidRenderMode::Braille,
            tier_override: MermaidTier::Auto,
            max_nodes: 200,
            max_edges: 400,
            route_budget: 4_000,
            layout_iteration_budget: 200,
            edge_bundling: false,
            edge_bundle_min_count: 3,
            max_label_chars: 48,
            max_label_lines: 3,
            wrap_mode: MermaidWrapMode::WordChar,
            enable_styles: true,
            enable_init_directives: false,
            enable_links: false,
            link_mode: MermaidLinkMode::Off,
            sanitize_mode: MermaidSanitizeMode::Strict,
            error_mode: MermaidErrorMode::Panel,
            log_path: None,
            cache_enabled: true,
            capability_profile: None,
            debug_overlay: false,
            palette: DiagramPalettePreset::Default,
            theme: None,
            theme_variables: BTreeMap::new(),
            flowchart_direction: None,
            flowchart_curve: None,
            sequence_mirror_actors: None,
            sequence_show_sequence_numbers: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidConfigError {
    pub field: String,
    pub value: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidConfigParse {
    pub config: MermaidConfig,
    pub warnings: Vec<MermaidWarning>,
    pub errors: Vec<MermaidConfigError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidThemeOverrides {
    pub theme: Option<String>,
    pub theme_variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidInitConfig {
    pub theme: Option<String>,
    pub theme_variables: BTreeMap<String, String>,
    pub flowchart_direction: Option<GraphDirection>,
    pub flowchart_curve: Option<String>,
    pub sequence_mirror_actors: Option<bool>,
    pub sequence_show_sequence_numbers: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidInitParse {
    pub config: MermaidInitConfig,
    pub warnings: Vec<MermaidWarning>,
    pub errors: Vec<MermaidError>,
}

#[must_use]
pub fn parse_mermaid_js_config_value(value: &Value) -> MermaidConfigParse {
    let mut parsed = MermaidConfigParse::default();
    let Some(config_obj) = value.as_object() else {
        parsed.errors.push(MermaidConfigError {
            field: "$".to_string(),
            value: value.to_string(),
            message: "Mermaid config root must be a JSON object".to_string(),
        });
        return parsed;
    };

    for (key, raw_value) in config_obj {
        match key.as_str() {
            "theme" => {
                if let Some(theme) = raw_value.as_str() {
                    parsed.config.theme = Some(theme.to_string());
                    parsed.config.palette = palette_from_theme_name(theme);
                } else {
                    push_type_error(
                        &mut parsed,
                        "theme",
                        raw_value,
                        "must be a string (for example, \"default\" or \"dark\")",
                    );
                }
            }
            "themeVariables" => {
                if let Some(theme_vars) = raw_value.as_object() {
                    for (var_key, var_value) in theme_vars {
                        if let Some(value_text) = json_scalar_to_string(var_value) {
                            parsed
                                .config
                                .theme_variables
                                .insert(var_key.clone(), value_text);
                        } else {
                            push_type_error(
                                &mut parsed,
                                &format!("themeVariables.{var_key}"),
                                var_value,
                                "must be a string, number, or boolean",
                            );
                        }
                    }
                } else {
                    push_type_error(
                        &mut parsed,
                        "themeVariables",
                        raw_value,
                        "must be an object",
                    );
                }
            }
            "flowchart" => parse_flowchart_config(raw_value, &mut parsed),
            "sequence" => parse_sequence_config(raw_value, &mut parsed),
            "securityLevel" => {
                if let Some(level) = raw_value.as_str() {
                    match level.to_ascii_lowercase().as_str() {
                        "strict" | "antiscript" => {
                            parsed.config.sanitize_mode = MermaidSanitizeMode::Strict;
                        }
                        "loose" => {
                            parsed.config.sanitize_mode = MermaidSanitizeMode::Lenient;
                        }
                        _ => {
                            push_warning(
                                &mut parsed,
                                format!("Unsupported securityLevel '{level}' ignored"),
                            );
                        }
                    }
                } else {
                    push_type_error(&mut parsed, "securityLevel", raw_value, "must be a string");
                }
            }
            // Common Mermaid key, but currently no equivalent runtime behavior in fm-core.
            "startOnLoad" => {
                if raw_value.is_boolean() {
                    push_warning(
                        &mut parsed,
                        "Config key 'startOnLoad' is accepted but currently ignored".to_string(),
                    );
                } else {
                    push_type_error(&mut parsed, "startOnLoad", raw_value, "must be a boolean");
                }
            }
            other => push_warning(
                &mut parsed,
                format!("Unsupported Mermaid config key '{other}' ignored"),
            ),
        }
    }

    parsed
}

#[must_use]
pub fn to_init_parse(parsed_config: MermaidConfigParse) -> MermaidInitParse {
    let init_config = MermaidInitConfig {
        theme: parsed_config.config.theme.clone(),
        theme_variables: parsed_config.config.theme_variables.clone(),
        flowchart_direction: parsed_config.config.flowchart_direction,
        flowchart_curve: parsed_config.config.flowchart_curve.clone(),
        sequence_mirror_actors: parsed_config.config.sequence_mirror_actors,
        sequence_show_sequence_numbers: parsed_config.config.sequence_show_sequence_numbers,
    };

    let errors = parsed_config
        .errors
        .into_iter()
        .map(|error| MermaidError::Parse {
            message: format!("Config field '{}': {}", error.field, error.message),
            span: Span::default(),
            expected: vec!["a valid Mermaid config value".to_string()],
        })
        .collect();

    MermaidInitParse {
        config: init_config,
        warnings: parsed_config.warnings,
        errors,
    }
}

fn parse_flowchart_config(value: &Value, parsed: &mut MermaidConfigParse) {
    let Some(obj) = value.as_object() else {
        push_type_error(parsed, "flowchart", value, "must be an object");
        return;
    };

    for (key, raw_value) in obj {
        match key.as_str() {
            "direction" | "rankDir" => {
                if let Some(direction_text) = raw_value.as_str() {
                    if let Some(direction) = parse_graph_direction_token(direction_text) {
                        parsed.config.flowchart_direction = Some(direction);
                    } else {
                        push_warning(
                            parsed,
                            format!("Unsupported flowchart direction '{direction_text}' ignored"),
                        );
                    }
                } else {
                    push_type_error(
                        parsed,
                        &format!("flowchart.{key}"),
                        raw_value,
                        "must be a direction string (LR, RL, TB, TD, BT)",
                    );
                }
            }
            "curve" => {
                if let Some(curve) = raw_value.as_str() {
                    parsed.config.flowchart_curve = Some(curve.to_string());
                } else {
                    push_type_error(parsed, "flowchart.curve", raw_value, "must be a string");
                }
            }
            other => push_warning(
                parsed,
                format!("Unsupported flowchart config key '{other}' ignored"),
            ),
        }
    }
}

fn parse_sequence_config(value: &Value, parsed: &mut MermaidConfigParse) {
    let Some(obj) = value.as_object() else {
        push_type_error(parsed, "sequence", value, "must be an object");
        return;
    };

    for (key, raw_value) in obj {
        match key.as_str() {
            "mirrorActors" => {
                if let Some(mirror) = raw_value.as_bool() {
                    parsed.config.sequence_mirror_actors = Some(mirror);
                } else {
                    push_type_error(
                        parsed,
                        "sequence.mirrorActors",
                        raw_value,
                        "must be a boolean",
                    );
                }
            }
            "showSequenceNumbers" => {
                if let Some(show_numbers) = raw_value.as_bool() {
                    parsed.config.sequence_show_sequence_numbers = Some(show_numbers);
                } else {
                    push_type_error(
                        parsed,
                        "sequence.showSequenceNumbers",
                        raw_value,
                        "must be a boolean",
                    );
                }
            }
            other => push_warning(
                parsed,
                format!("Unsupported sequence config key '{other}' ignored"),
            ),
        }
    }
}

fn push_type_error(parsed: &mut MermaidConfigParse, field: &str, value: &Value, message: &str) {
    parsed.errors.push(MermaidConfigError {
        field: field.to_string(),
        value: value.to_string(),
        message: message.to_string(),
    });
}

fn push_warning(parsed: &mut MermaidConfigParse, message: String) {
    parsed.warnings.push(MermaidWarning {
        code: MermaidWarningCode::UnsupportedFeature,
        message,
        span: Span::default(),
    });
}

fn json_scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_graph_direction_token(token: &str) -> Option<GraphDirection> {
    match token.trim().to_ascii_uppercase().as_str() {
        "LR" => Some(GraphDirection::LR),
        "RL" => Some(GraphDirection::RL),
        "TB" => Some(GraphDirection::TB),
        "TD" => Some(GraphDirection::TD),
        "BT" => Some(GraphDirection::BT),
        _ => None,
    }
}

fn palette_from_theme_name(theme: &str) -> DiagramPalettePreset {
    match theme.trim().to_ascii_lowercase().as_str() {
        "corporate" => DiagramPalettePreset::Corporate,
        "neon" => DiagramPalettePreset::Neon,
        "monochrome" => DiagramPalettePreset::Monochrome,
        "pastel" => DiagramPalettePreset::Pastel,
        "highcontrast" | "high-contrast" => DiagramPalettePreset::HighContrast,
        _ => DiagramPalettePreset::Default,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidComplexity {
    pub nodes: usize,
    pub edges: usize,
    pub labels: usize,
    pub clusters: usize,
    pub ports: usize,
    pub style_refs: usize,
    pub score: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidPressureSource {
    #[default]
    Unavailable,
    Native,
    Wasm,
}

impl MermaidPressureSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Native => "native",
            Self::Wasm => "wasm",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidPressureTier {
    #[default]
    Unknown,
    Nominal,
    Elevated,
    High,
    Critical,
}

impl MermaidPressureTier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Nominal => "nominal",
            Self::Elevated => "elevated",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    #[must_use]
    pub const fn from_quantized_score(score_permille: u16, telemetry_available: bool) -> Self {
        if !telemetry_available {
            return Self::Unknown;
        }
        match score_permille {
            0..=349 => Self::Nominal,
            350..=649 => Self::Elevated,
            650..=849 => Self::High,
            _ => Self::Critical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidPressureReport {
    pub source: MermaidPressureSource,
    pub telemetry_available: bool,
    pub conservative_fallback: bool,
    pub tier: MermaidPressureTier,
    pub quantized_score_permille: u16,
    pub cpu_pressure_permille: Option<u16>,
    pub memory_pressure_permille: Option<u16>,
    pub io_pressure_permille: Option<u16>,
    pub available_parallelism: Option<usize>,
    pub rss_mib: Option<u64>,
    pub frame_budget_ms: Option<u16>,
    pub frame_time_ms: Option<u16>,
    pub event_loop_lag_ms: Option<u16>,
    pub worker_saturation_permille: Option<u16>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidNativePressureSignals {
    pub cpu_pressure_permille: Option<u16>,
    pub memory_pressure_permille: Option<u16>,
    pub io_pressure_permille: Option<u16>,
    pub available_parallelism: Option<usize>,
    pub rss_mib: Option<u64>,
}

impl MermaidNativePressureSignals {
    #[must_use]
    pub fn sample() -> Self {
        Self {
            cpu_pressure_permille: env_permille("FM_PRESSURE_CPU_PERMILLE"),
            memory_pressure_permille: env_permille("FM_PRESSURE_MEMORY_PERMILLE"),
            io_pressure_permille: env_permille("FM_PRESSURE_IO_PERMILLE"),
            available_parallelism: env_usize("FM_PRESSURE_AVAILABLE_PARALLELISM")
                .or_else(|| std::thread::available_parallelism().ok().map(usize::from)),
            rss_mib: env_u64("FM_PRESSURE_RSS_MIB").or_else(read_process_rss_mib),
        }
    }

    #[must_use]
    pub fn into_report(self) -> MermaidPressureReport {
        let parallelism_pressure =
            self.available_parallelism
                .map(|parallelism| match parallelism {
                    0..=1 => 900,
                    2 => 700,
                    3..=4 => 450,
                    5..=8 => 250,
                    _ => 100,
                });
        let rss_pressure = self.rss_mib.map(|rss_mib| match rss_mib {
            0..=255 => 120,
            256..=511 => 320,
            512..=1023 => 560,
            1024..=2047 => 760,
            _ => 920,
        });
        let quantized_score_permille = [
            self.cpu_pressure_permille,
            self.memory_pressure_permille,
            self.io_pressure_permille,
            parallelism_pressure,
            rss_pressure,
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
        let telemetry_available = self.cpu_pressure_permille.is_some()
            || self.memory_pressure_permille.is_some()
            || self.io_pressure_permille.is_some()
            || self.available_parallelism.is_some()
            || self.rss_mib.is_some();
        let mut notes = Vec::new();
        if !telemetry_available {
            notes.push(String::from(
                "native telemetry unavailable; pressure tier is unknown and callers should use a conservative policy",
            ));
        }
        if self.available_parallelism.is_none() {
            notes.push(String::from("available parallelism probe unavailable"));
        }
        if self.rss_mib.is_none() {
            notes.push(String::from("rss probe unavailable"));
        }
        MermaidPressureReport {
            source: if telemetry_available {
                MermaidPressureSource::Native
            } else {
                MermaidPressureSource::Unavailable
            },
            telemetry_available,
            conservative_fallback: !telemetry_available,
            tier: MermaidPressureTier::from_quantized_score(
                quantized_score_permille,
                telemetry_available,
            ),
            quantized_score_permille,
            cpu_pressure_permille: self.cpu_pressure_permille,
            memory_pressure_permille: self.memory_pressure_permille,
            io_pressure_permille: self.io_pressure_permille,
            available_parallelism: self.available_parallelism,
            rss_mib: self.rss_mib,
            frame_budget_ms: None,
            frame_time_ms: None,
            event_loop_lag_ms: None,
            worker_saturation_permille: None,
            notes,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidWasmPressureSignals {
    pub frame_budget_ms: Option<u16>,
    pub frame_time_ms: Option<u16>,
    pub event_loop_lag_ms: Option<u16>,
    pub worker_saturation_permille: Option<u16>,
}

impl MermaidWasmPressureSignals {
    #[must_use]
    pub fn into_report(self) -> MermaidPressureReport {
        let frame_pressure = match (self.frame_budget_ms, self.frame_time_ms) {
            (Some(budget), Some(frame_time)) if budget > 0 => {
                let scaled = (u32::from(frame_time) * 1_000) / u32::from(budget);
                Some(scaled.min(1_000) as u16)
            }
            _ => None,
        };
        let lag_pressure = self
            .event_loop_lag_ms
            .map(|lag_ms| (u32::from(lag_ms) * 50).min(1_000) as u16);
        let quantized_score_permille = [
            frame_pressure,
            lag_pressure,
            self.worker_saturation_permille
                .map(|value| value.min(1_000)),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
        let telemetry_available = self.frame_budget_ms.is_some()
            || self.frame_time_ms.is_some()
            || self.event_loop_lag_ms.is_some()
            || self.worker_saturation_permille.is_some();
        let mut notes = Vec::new();
        if !telemetry_available {
            notes.push(String::from(
                "wasm telemetry unavailable; pressure tier is unknown and callers should use a conservative policy",
            ));
        } else if self.frame_budget_ms.is_none() || self.frame_time_ms.is_none() {
            notes.push(String::from(
                "frame budget telemetry incomplete; using event-loop and worker proxies only",
            ));
        }
        MermaidPressureReport {
            source: if telemetry_available {
                MermaidPressureSource::Wasm
            } else {
                MermaidPressureSource::Unavailable
            },
            telemetry_available,
            conservative_fallback: !telemetry_available,
            tier: MermaidPressureTier::from_quantized_score(
                quantized_score_permille,
                telemetry_available,
            ),
            quantized_score_permille,
            cpu_pressure_permille: None,
            memory_pressure_permille: None,
            io_pressure_permille: None,
            available_parallelism: None,
            rss_mib: None,
            frame_budget_ms: self.frame_budget_ms,
            frame_time_ms: self.frame_time_ms,
            event_loop_lag_ms: self.event_loop_lag_ms,
            worker_saturation_permille: self.worker_saturation_permille,
            notes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidStageBudgetLedger {
    pub stage: String,
    pub allocated_ms: u64,
    pub used_ms: u64,
    pub remaining_ms: u64,
    pub exceeded: bool,
}

impl MermaidStageBudgetLedger {
    #[must_use]
    pub fn new(stage: &str, allocated_ms: u64) -> Self {
        Self {
            stage: stage.to_string(),
            allocated_ms,
            used_ms: 0,
            remaining_ms: allocated_ms,
            exceeded: false,
        }
    }

    pub const fn consume(&mut self, used_ms: u64) {
        self.used_ms = used_ms;
        self.remaining_ms = self.allocated_ms.saturating_sub(used_ms);
        self.exceeded = used_ms > self.allocated_ms;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidBudgetEvent {
    pub kind: String,
    pub stage: Option<String>,
    pub allocated_ms: Option<u64>,
    pub used_ms: Option<u64>,
    pub remaining_ms: Option<u64>,
    pub remaining_total_ms: u64,
    pub exceeded: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidBudgetLedger {
    pub arbitration_policy: String,
    pub total_budget_ms: u64,
    pub remaining_total_ms: u64,
    pub exhausted: bool,
    pub pressure_tier: MermaidPressureTier,
    pub parse: MermaidStageBudgetLedger,
    pub layout: MermaidStageBudgetLedger,
    pub render: MermaidStageBudgetLedger,
    pub notes: Vec<String>,
    pub events: Vec<MermaidBudgetEvent>,
}

impl Default for MermaidBudgetLedger {
    fn default() -> Self {
        Self::new(&MermaidNativePressureSignals::default().into_report())
    }
}

impl MermaidBudgetLedger {
    #[must_use]
    pub fn new(pressure: &MermaidPressureReport) -> Self {
        let total_budget_ms: u64 = match pressure.tier {
            MermaidPressureTier::Unknown | MermaidPressureTier::High => 120,
            MermaidPressureTier::Nominal => 250,
            MermaidPressureTier::Elevated => 180,
            MermaidPressureTier::Critical => 80,
        };
        let parse_budget_ms = total_budget_ms.div_ceil(5);
        let render_budget_ms = total_budget_ms.div_ceil(5);
        let layout_budget_ms = total_budget_ms
            .saturating_sub(parse_budget_ms)
            .saturating_sub(render_budget_ms);
        let mut notes = Vec::new();
        if pressure.conservative_fallback {
            notes.push(String::from(
                "telemetry unavailable; broker used conservative global budget defaults",
            ));
        }
        let mut ledger = Self {
            arbitration_policy: String::from("parse_first_then_layout_heavy_then_render_tail"),
            total_budget_ms,
            remaining_total_ms: total_budget_ms,
            exhausted: false,
            pressure_tier: pressure.tier,
            parse: MermaidStageBudgetLedger::new("parse", parse_budget_ms),
            layout: MermaidStageBudgetLedger::new("layout", layout_budget_ms),
            render: MermaidStageBudgetLedger::new("render", render_budget_ms),
            notes,
            events: Vec::new(),
        };
        ledger.push_stage_event(
            "allocate",
            "parse",
            ledger.parse.allocated_ms,
            ledger.parse.used_ms,
            ledger.parse.remaining_ms,
            ledger.parse.exceeded,
            None,
        );
        ledger.push_stage_event(
            "allocate",
            "layout",
            ledger.layout.allocated_ms,
            ledger.layout.used_ms,
            ledger.layout.remaining_ms,
            ledger.layout.exceeded,
            None,
        );
        ledger.push_stage_event(
            "allocate",
            "render",
            ledger.render.allocated_ms,
            ledger.render.used_ms,
            ledger.render.remaining_ms,
            ledger.render.exceeded,
            None,
        );
        if pressure.conservative_fallback {
            ledger.events.push(MermaidBudgetEvent {
                kind: String::from("policy_note"),
                stage: None,
                allocated_ms: None,
                used_ms: None,
                remaining_ms: None,
                remaining_total_ms: ledger.snapshot_remaining_total_ms(),
                exceeded: false,
                note: Some(String::from(
                    "telemetry unavailable; broker used conservative global budget defaults",
                )),
            });
        }
        ledger
    }

    pub fn record_parse(&mut self, used_ms: u64) {
        self.parse.consume(used_ms);
        self.push_stage_event(
            "consume",
            "parse",
            self.parse.allocated_ms,
            self.parse.used_ms,
            self.parse.remaining_ms,
            self.parse.exceeded,
            None,
        );
        self.rebalance_after_parse();
        self.finish_stage_accounting();
    }

    pub fn record_layout(&mut self, used_ms: u64) {
        self.layout.consume(used_ms);
        self.push_stage_event(
            "consume",
            "layout",
            self.layout.allocated_ms,
            self.layout.used_ms,
            self.layout.remaining_ms,
            self.layout.exceeded,
            None,
        );
        self.finish_stage_accounting();
    }

    pub fn record_render(&mut self, used_ms: u64) {
        self.render.consume(used_ms);
        self.push_stage_event(
            "consume",
            "render",
            self.render.allocated_ms,
            self.render.used_ms,
            self.render.remaining_ms,
            self.render.exceeded,
            None,
        );
        self.finish_stage_accounting();
    }

    #[must_use]
    pub fn layout_time_budget_ms(&self) -> usize {
        usize::try_from(self.layout.allocated_ms.max(1)).unwrap_or(usize::MAX)
    }

    #[must_use]
    pub fn layout_iteration_budget(&self, default_budget: usize) -> usize {
        scale_budget(default_budget, self.layout.allocated_ms, 250)
    }

    #[must_use]
    pub fn route_budget(&self, default_budget: usize) -> usize {
        scale_budget(default_budget, self.layout.allocated_ms, 250)
    }

    #[must_use]
    pub const fn should_simplify_render(&self) -> bool {
        matches!(
            self.pressure_tier,
            MermaidPressureTier::High | MermaidPressureTier::Critical
        ) || self.render.allocated_ms <= 24
    }

    fn rebalance_after_parse(&mut self) {
        let remaining_total = self.total_budget_ms.saturating_sub(self.parse.used_ms);
        let render_budget_ms = remaining_total.div_ceil(4);
        let layout_budget_ms = remaining_total.saturating_sub(render_budget_ms);
        self.layout.allocated_ms = layout_budget_ms;
        self.layout.remaining_ms = layout_budget_ms;
        self.render.allocated_ms = render_budget_ms;
        self.render.remaining_ms = render_budget_ms;
        self.push_stage_event(
            "rebalance",
            "layout",
            self.layout.allocated_ms,
            self.layout.used_ms,
            self.layout.remaining_ms,
            self.layout.exceeded,
            Some(String::from(
                "layout share increased after parse arbitration",
            )),
        );
        self.push_stage_event(
            "rebalance",
            "render",
            self.render.allocated_ms,
            self.render.used_ms,
            self.render.remaining_ms,
            self.render.exceeded,
            Some(String::from(
                "render tail budget recalculated after parse arbitration",
            )),
        );
    }

    fn finish_stage_accounting(&mut self) {
        let used_total = self
            .parse
            .used_ms
            .saturating_add(self.layout.used_ms)
            .saturating_add(self.render.used_ms);
        self.remaining_total_ms = self.total_budget_ms.saturating_sub(used_total);
        self.exhausted = used_total > self.total_budget_ms;
        self.events.push(MermaidBudgetEvent {
            kind: String::from("accounting"),
            stage: None,
            allocated_ms: None,
            used_ms: Some(used_total),
            remaining_ms: None,
            remaining_total_ms: self.snapshot_remaining_total_ms(),
            exceeded: self.exhausted,
            note: Some(if self.exhausted {
                String::from("global budget exhausted")
            } else {
                String::from("global budget accounting updated")
            }),
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn push_stage_event(
        &mut self,
        kind: &str,
        stage: &str,
        allocated_ms: u64,
        used_ms: u64,
        remaining_ms: u64,
        exceeded: bool,
        note: Option<String>,
    ) {
        self.events.push(MermaidBudgetEvent {
            kind: kind.to_string(),
            stage: Some(stage.to_string()),
            allocated_ms: Some(allocated_ms),
            used_ms: Some(used_ms),
            remaining_ms: Some(remaining_ms),
            remaining_total_ms: self.snapshot_remaining_total_ms(),
            exceeded,
            note,
        });
    }

    const fn snapshot_remaining_total_ms(&self) -> u64 {
        self.total_budget_ms
            .saturating_sub(self.current_used_total_ms())
    }

    const fn current_used_total_ms(&self) -> u64 {
        self.parse
            .used_ms
            .saturating_add(self.layout.used_ms)
            .saturating_add(self.render.used_ms)
    }
}

fn scale_budget(default_budget: usize, allocated_ms: u64, baseline_ms: u64) -> usize {
    let numerator = (default_budget as u128)
        .saturating_mul(u128::from(allocated_ms.max(1)))
        .div_ceil(u128::from(baseline_ms.max(1)));
    numerator.max(1).min(usize::MAX as u128) as usize
}

fn env_permille(key: &str) -> Option<u16> {
    env_u16(key).map(|value| value.min(1_000))
}

fn env_u16(key: &str) -> Option<u16> {
    std::env::var(key).ok()?.trim().parse().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse().ok()
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.trim().parse().ok()
}

fn read_process_rss_mib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kib = rest
                .split_whitespace()
                .find_map(|token| token.parse::<u64>().ok())?;
            return Some(kib.div_ceil(1024));
        }
    }
    None
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidFidelity {
    Rich,
    #[default]
    Normal,
    Compact,
    Outline,
}

/// User-selectable quality mode that biases degradation thresholds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidQualityMode {
    /// Full quality, no degradation even under pressure.
    QualityFirst,
    /// Default: degrade when budgets are exceeded.
    #[default]
    Auto,
    /// Trade quality for speed when budget pressure is Elevated or higher.
    Balanced,
    /// Aggressively degrade to minimize latency.
    LatencyFirst,
}

impl MermaidQualityMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QualityFirst => "quality-first",
            Self::Auto => "auto",
            Self::Balanced => "balanced",
            Self::LatencyFirst => "latency-first",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MermaidDegradationPlan {
    pub target_fidelity: MermaidFidelity,
    pub hide_labels: bool,
    pub collapse_clusters: bool,
    pub simplify_routing: bool,
    pub reduce_decoration: bool,
    pub force_glyph_mode: Option<MermaidGlyphMode>,
}

impl MermaidDegradationPlan {
    /// Returns true if any degradation operator is active.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        !matches!(
            self.target_fidelity,
            MermaidFidelity::Normal | MermaidFidelity::Rich
        ) || self.hide_labels
            || self.collapse_clusters
            || self.simplify_routing
            || self.reduce_decoration
            || self.force_glyph_mode.is_some()
    }

    /// Produce a human-readable explanation of what degradation was applied and why.
    #[must_use]
    pub fn explain(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.is_degraded() {
            lines.push(String::from(
                "No degradation applied; rendering at full quality.",
            ));
            return lines;
        }
        lines.push(String::from("Degraded rendering active:"));
        if self.reduce_decoration {
            lines.push(String::from(
                "  - Decoration reduced: shadows, gradients, and glow effects disabled.",
            ));
        }
        if self.simplify_routing {
            lines.push(String::from(
                "  - Edge routing simplified to reduce layout computation.",
            ));
        }
        if self.force_glyph_mode.is_some() {
            lines.push(String::from(
                "  - Terminal output forced to ASCII glyphs for faster rendering.",
            ));
        }
        match self.target_fidelity {
            MermaidFidelity::Compact => {
                lines.push(String::from(
                    "  - Fidelity downgraded to Compact (reduced detail).",
                ));
            }
            MermaidFidelity::Outline => {
                lines.push(String::from(
                    "  - Fidelity downgraded to Outline (skeleton only).",
                ));
            }
            _ => {}
        }
        if self.collapse_clusters {
            lines.push(String::from("  - Cluster/subgraph boundaries collapsed."));
        }
        if self.hide_labels {
            lines.push(String::from("  - Non-essential labels hidden."));
        }
        lines.push(String::from(
            "Remediation: reduce diagram complexity, lower pressure (close competing processes), or use --quality-mode=quality-first to force full quality.",
        ));
        lines
    }
}

/// A named degradation operator that can be applied to reduce rendering cost.
///
/// Operators are ordered by cost-effectiveness: cheap quality reductions first,
/// expensive structural changes last. The ordering is deterministic and stable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DegradationOperator {
    /// Disable gradient fills and drop shadows.
    ReduceDecoration,
    /// Switch edge routing from orthogonal/spline to simplified straight lines.
    SimplifyRouting,
    /// Force ASCII glyph mode in terminal rendering.
    ForceAsciiGlyphs,
    /// Drop fidelity from Rich/Normal to Compact.
    DowngradeToCompact,
    /// Collapse cluster/subgraph boundaries (render as flat).
    CollapseClusters,
    /// Elide non-essential labels (keep only node IDs).
    HideLabels,
    /// Drop fidelity to Outline (skeleton only).
    DowngradeToOutline,
}

impl DegradationOperator {
    /// Canonical ordering: lower index = applied first (cheapest quality loss).
    /// This ordering is the tie-break rule for determinism.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        match self {
            Self::ReduceDecoration => 0,
            Self::SimplifyRouting => 1,
            Self::ForceAsciiGlyphs => 2,
            Self::DowngradeToCompact => 3,
            Self::CollapseClusters => 4,
            Self::HideLabels => 5,
            Self::DowngradeToOutline => 6,
        }
    }

    /// All operators in canonical order (cheapest first).
    #[must_use]
    pub const fn all_in_order() -> [Self; 7] {
        [
            Self::ReduceDecoration,
            Self::SimplifyRouting,
            Self::ForceAsciiGlyphs,
            Self::DowngradeToCompact,
            Self::CollapseClusters,
            Self::HideLabels,
            Self::DowngradeToOutline,
        ]
    }

    /// Apply this operator to a mutable degradation plan.
    pub const fn apply(self, plan: &mut MermaidDegradationPlan) {
        match self {
            Self::ReduceDecoration => plan.reduce_decoration = true,
            Self::SimplifyRouting => plan.simplify_routing = true,
            Self::ForceAsciiGlyphs => {
                plan.force_glyph_mode = Some(MermaidGlyphMode::Ascii);
            }
            Self::DowngradeToCompact => {
                if matches!(
                    plan.target_fidelity,
                    MermaidFidelity::Rich | MermaidFidelity::Normal
                ) {
                    plan.target_fidelity = MermaidFidelity::Compact;
                }
            }
            Self::CollapseClusters => plan.collapse_clusters = true,
            Self::HideLabels => plan.hide_labels = true,
            Self::DowngradeToOutline => {
                plan.target_fidelity = MermaidFidelity::Outline;
            }
        }
    }
}

/// Input signals that drive degradation operator selection.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct DegradationContext {
    pub pressure_tier: MermaidPressureTier,
    pub route_budget_exceeded: bool,
    pub layout_budget_exceeded: bool,
    pub time_budget_exceeded: bool,
    pub node_limit_exceeded: bool,
    pub edge_limit_exceeded: bool,
}

/// Deterministically select and apply degradation operators based on context.
///
/// The selection algorithm applies operators in canonical order (cheapest first)
/// until all budget/limit violations are addressed. The result is always the same
/// for the same input — no randomness, no timing dependence.
#[must_use]
pub fn compute_degradation_plan(ctx: &DegradationContext) -> MermaidDegradationPlan {
    let mut plan = MermaidDegradationPlan::default();
    #[allow(clippy::collection_is_never_read)]
    let mut applied = Vec::new();

    let any_budget_exceeded =
        ctx.route_budget_exceeded || ctx.layout_budget_exceeded || ctx.time_budget_exceeded;

    // Walk operators in canonical order; apply each if its trigger condition is met.
    for op in DegradationOperator::all_in_order() {
        let should_apply = match op {
            DegradationOperator::ReduceDecoration => {
                any_budget_exceeded
                    || matches!(
                        ctx.pressure_tier,
                        MermaidPressureTier::High | MermaidPressureTier::Critical
                    )
            }
            DegradationOperator::SimplifyRouting => ctx.route_budget_exceeded,
            DegradationOperator::ForceAsciiGlyphs | DegradationOperator::DowngradeToCompact => {
                any_budget_exceeded
            }
            DegradationOperator::CollapseClusters => ctx.node_limit_exceeded && any_budget_exceeded,
            DegradationOperator::HideLabels => ctx.node_limit_exceeded && ctx.time_budget_exceeded,
            DegradationOperator::DowngradeToOutline => {
                matches!(ctx.pressure_tier, MermaidPressureTier::Critical)
                    && ctx.time_budget_exceeded
            }
        };
        if should_apply {
            op.apply(&mut plan);
            applied.push(op);
        }
    }
    plan
}

/// Compute the degradation plan and return the list of applied operators for audit.
#[must_use]
pub fn compute_degradation_plan_with_trace(
    ctx: &DegradationContext,
) -> (MermaidDegradationPlan, Vec<DegradationOperator>) {
    let mut plan = MermaidDegradationPlan::default();
    let mut applied = Vec::new();

    let any_budget_exceeded =
        ctx.route_budget_exceeded || ctx.layout_budget_exceeded || ctx.time_budget_exceeded;

    for op in DegradationOperator::all_in_order() {
        let should_apply = match op {
            DegradationOperator::ReduceDecoration => {
                any_budget_exceeded
                    || matches!(
                        ctx.pressure_tier,
                        MermaidPressureTier::High | MermaidPressureTier::Critical
                    )
            }
            DegradationOperator::SimplifyRouting => ctx.route_budget_exceeded,
            DegradationOperator::ForceAsciiGlyphs | DegradationOperator::DowngradeToCompact => {
                any_budget_exceeded
            }
            DegradationOperator::CollapseClusters => ctx.node_limit_exceeded && any_budget_exceeded,
            DegradationOperator::HideLabels => ctx.node_limit_exceeded && ctx.time_budget_exceeded,
            DegradationOperator::DowngradeToOutline => {
                matches!(ctx.pressure_tier, MermaidPressureTier::Critical)
                    && ctx.time_budget_exceeded
            }
        };
        if should_apply {
            op.apply(&mut plan);
            applied.push(op);
        }
    }
    (plan, applied)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MermaidGuardReport {
    pub complexity: MermaidComplexity,
    pub label_chars_over: usize,
    pub label_lines_over: usize,
    pub node_limit_exceeded: bool,
    pub edge_limit_exceeded: bool,
    pub label_limit_exceeded: bool,
    pub route_budget_exceeded: bool,
    pub layout_budget_exceeded: bool,
    pub limits_exceeded: bool,
    pub budget_exceeded: bool,
    pub route_ops_estimate: usize,
    pub layout_iterations_estimate: usize,
    pub layout_time_estimate_ms: usize,
    pub layout_requested_algorithm: Option<String>,
    pub layout_selected_algorithm: Option<String>,
    pub guard_reason: Option<String>,
    pub observability: MermaidObservabilityIds,
    pub pressure: MermaidPressureReport,
    pub budget_broker: MermaidBudgetLedger,
    pub degradation: MermaidDegradationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidLayoutDecisionAlternative {
    pub algorithm: String,
    pub selected: bool,
    pub available_for_diagram: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidDecisionWeight {
    pub key: String,
    pub value_permille: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidLayoutDecisionRecord {
    pub kind: String,
    pub trace_id: TraceId,
    pub decision_id: DecisionId,
    pub policy_id: PolicyId,
    #[serde(with = "schema_version_semver")]
    pub schema_version: SchemaVersion,
    pub requested_algorithm: String,
    pub selected_algorithm: String,
    pub capability_unavailable: bool,
    pub decision_mode: String,
    pub dispatch_reason: String,
    pub guard_reason: String,
    pub fallback_applied: bool,
    pub confidence_permille: u16,
    pub selected_expected_loss_permille: u32,
    pub node_count: usize,
    pub edge_count: usize,
    pub crossing_count: usize,
    pub reversed_edges: usize,
    pub estimated_layout_time_ms: usize,
    pub estimated_layout_iterations: usize,
    pub estimated_route_ops: usize,
    pub pressure_source: MermaidPressureSource,
    pub pressure_tier: MermaidPressureTier,
    pub budget_total_ms: u64,
    pub budget_exhausted: bool,
    pub state_posterior: Vec<MermaidDecisionWeight>,
    pub expected_loss: Vec<MermaidDecisionWeight>,
    pub alternatives: Vec<MermaidLayoutDecisionAlternative>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidLayoutDecisionLedger {
    pub entries: Vec<MermaidLayoutDecisionRecord>,
}

impl MermaidLayoutDecisionLedger {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    ///
    /// # Errors
    ///
    /// Returns a `serde_json::Error` if any entry cannot be serialized.
    pub fn to_jsonl(&self) -> serde_json::Result<String> {
        let mut lines = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            lines.push(serde_json::to_string(entry)?);
        }
        Ok(lines.join("\n"))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MermaidFallbackAction {
    #[default]
    Ignore,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidFallbackPolicy {
    pub unsupported_diagram: MermaidFallbackAction,
    pub unsupported_directive: MermaidFallbackAction,
    pub unsupported_style: MermaidFallbackAction,
    pub unsupported_link: MermaidFallbackAction,
    pub unsupported_feature: MermaidFallbackAction,
}

impl Default for MermaidFallbackPolicy {
    fn default() -> Self {
        Self {
            unsupported_diagram: MermaidFallbackAction::Error,
            unsupported_directive: MermaidFallbackAction::Warn,
            unsupported_style: MermaidFallbackAction::Warn,
            unsupported_link: MermaidFallbackAction::Warn,
            unsupported_feature: MermaidFallbackAction::Warn,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidValidation {
    pub warnings: Vec<MermaidWarning>,
    pub errors: Vec<MermaidError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidCompatibilityReport {
    pub diagram_support: MermaidSupportLevel,
    pub warnings: Vec<MermaidWarning>,
    pub fatal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidDiagramMeta {
    pub diagram_type: DiagramType,
    pub direction: GraphDirection,
    pub support_level: MermaidSupportLevel,
    pub parse_mode: MermaidParseMode,
    pub block_beta_columns: Option<usize>,
    pub init: MermaidInitParse,
    pub theme_overrides: MermaidThemeOverrides,
    pub c4_show_legend: bool,
    pub guard: MermaidGuardReport,
    /// Visible diagram title from front matter `title:` or inline `title ...` directives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Accessibility title from `accTitle: ...` directive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acc_title: Option<String>,
    /// Accessibility description from `accDescr: ...` or `accDescr { ... }` directive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acc_descr: Option<String>,
}

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    /// Informational hint (e.g., "consider using...")
    Hint,
    /// Something that works but could be improved
    #[default]
    Info,
    /// Potential issue that was auto-recovered
    Warning,
    /// Serious issue that may affect output quality
    Error,
}

impl DiagnosticSeverity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub const fn emoji(self) -> &'static str {
        match self {
            Self::Hint => "💡",
            Self::Info => "ℹ️",
            Self::Warning => "⚠️",
            Self::Error => "❌",
        }
    }
}

/// Category of diagnostic for filtering and grouping.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DiagnosticCategory {
    /// Lexer/tokenization issues
    Lexer,
    /// Parser/syntax issues
    #[default]
    Parser,
    /// Semantic/validation issues
    Semantic,
    /// Recovery action was taken
    Recovery,
    /// Intent inference was performed
    Inference,
    /// Compatibility with mermaid-js
    Compatibility,
}

impl DiagnosticCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lexer => "lexer",
            Self::Parser => "parser",
            Self::Semantic => "semantic",
            Self::Recovery => "recovery",
            Self::Inference => "inference",
            Self::Compatibility => "compatibility",
        }
    }
}

/// A diagnostic message with rich context for error reporting and recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Diagnostic {
    /// Severity of the diagnostic
    pub severity: DiagnosticSeverity,
    /// Category for filtering/grouping
    pub category: DiagnosticCategory,
    /// Human-readable message
    pub message: String,
    /// Source location where the issue occurred
    pub span: Option<Span>,
    /// Suggested fix or action
    pub suggestion: Option<String>,
    /// What was expected (for parse errors)
    pub expected: Vec<String>,
    /// What was found (for parse errors)
    pub found: Option<String>,
    /// Related diagnostics (e.g., "also defined here")
    pub related: Vec<RelatedDiagnostic>,
    /// Machine-readable rule identifier (e.g., "classdef-not-applied")
    pub rule_id: Option<String>,
}

/// A related diagnostic location (e.g., "also defined at...")
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RelatedDiagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    /// Create a new diagnostic with the given severity and message.
    #[must_use]
    pub fn new(severity: DiagnosticSeverity, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
            ..Default::default()
        }
    }

    /// Create an error diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Error, message)
    }

    /// Create a warning diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Warning, message)
    }

    /// Create an info diagnostic.
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Info, message)
    }

    /// Create a hint diagnostic.
    #[must_use]
    pub fn hint(message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Hint, message)
    }

    /// Set the category.
    #[must_use]
    pub const fn with_category(mut self, category: DiagnosticCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the source span.
    #[must_use]
    pub const fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Set the suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Set what was expected.
    #[must_use]
    pub fn with_expected(mut self, expected: Vec<String>) -> Self {
        self.expected = expected;
        self
    }

    /// Set what was found.
    #[must_use]
    pub fn with_found(mut self, found: impl Into<String>) -> Self {
        self.found = Some(found.into());
        self
    }

    /// Add a related diagnostic.
    #[must_use]
    pub fn with_related(mut self, message: impl Into<String>, span: Span) -> Self {
        self.related.push(RelatedDiagnostic {
            message: message.into(),
            span,
        });
        self
    }

    /// Set the machine-readable rule identifier.
    #[must_use]
    pub fn with_rule_id(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    /// Check if this is an error-level diagnostic.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::Error)
    }

    /// Check if this is a warning-level diagnostic.
    #[must_use]
    pub const fn is_warning(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::Warning)
    }
}

/// Stable, machine-readable diagnostics payload schema for automation surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StructuredDiagnostic {
    pub error_code: String,
    pub severity: String,
    pub message: String,
    pub span: Option<Span>,
    pub source_line: Option<usize>,
    pub source_column: Option<usize>,
    pub rule_id: Option<String>,
    pub confidence: Option<f32>,
    pub remediation_hint: Option<String>,
}

impl StructuredDiagnostic {
    #[must_use]
    pub fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        let (source_line, source_column) = diagnostic.span.map_or((None, None), |span| {
            (Some(span.start.line), Some(span.start.col))
        });

        Self {
            error_code: format!("mermaid/diag/{}", diagnostic.category.as_str()),
            severity: diagnostic.severity.as_str().to_string(),
            message: diagnostic.message.clone(),
            span: diagnostic.span,
            source_line,
            source_column,
            rule_id: None,
            confidence: None,
            remediation_hint: diagnostic.suggestion.clone(),
        }
    }

    #[must_use]
    pub fn from_warning(warning: &MermaidWarning) -> Self {
        Self {
            error_code: warning.code.as_str().to_string(),
            severity: DiagnosticSeverity::Warning.as_str().to_string(),
            message: warning.message.clone(),
            span: Some(warning.span),
            source_line: Some(warning.span.start.line),
            source_column: Some(warning.span.start.col),
            rule_id: None,
            confidence: None,
            remediation_hint: None,
        }
    }

    #[must_use]
    pub fn from_error(error: &MermaidError) -> Self {
        let span = error.span();
        let remediation_hint = match error {
            MermaidError::Parse { expected, .. } if !expected.is_empty() => {
                Some(format!("Expected one of: {}", expected.join(", ")))
            }
            _ => None,
        };

        Self {
            error_code: error.code().as_str().to_string(),
            severity: DiagnosticSeverity::Error.as_str().to_string(),
            message: error.to_string(),
            span: Some(span),
            source_line: Some(span.start.line),
            source_column: Some(span.start.col),
            rule_id: None,
            confidence: None,
            remediation_hint,
        }
    }

    #[must_use]
    pub fn with_rule_id(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    #[must_use]
    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }

    #[must_use]
    pub fn with_remediation_hint(mut self, remediation_hint: impl Into<String>) -> Self {
        self.remediation_hint = Some(remediation_hint.into());
        self
    }

    #[must_use]
    pub fn severity_rank(&self) -> u8 {
        match self.severity.as_str() {
            "hint" => 1,
            "info" => 2,
            "warning" => 3,
            "error" => 4,
            _ => 0,
        }
    }
}

// ── Sequence-diagram metadata ──────────────────────────────────────────

/// Position of a note relative to participant lifelines.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NotePosition {
    LeftOf,
    RightOf,
    Over,
}

/// Kind of interaction fragment (combined fragment) in a sequence diagram.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FragmentKind {
    Loop,
    Alt,
    Opt,
    Par,
    Critical,
    Break,
    Rect,
}

/// Lifecycle event kind for participant creation/destruction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum LifecycleEventKind {
    Create,
    Destroy,
}

/// An activation bar on a participant lifeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrActivation {
    pub participant: IrNodeId,
    pub start_edge: usize,
    pub end_edge: usize,
    pub depth: usize,
}

/// A note attached to one or more participant lifelines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrSequenceNote {
    pub position: NotePosition,
    pub participants: Vec<IrNodeId>,
    pub text: String,
    /// Index into `ir.edges` indicating which message this note appears after.
    /// Used for vertical positioning in the layout.
    #[serde(default)]
    pub after_edge: usize,
}

/// One alternative section inside an `Alt` fragment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FragmentAlternative {
    pub label: String,
    pub start_edge: usize,
    pub end_edge: usize,
}

/// A combined-fragment (interaction operand) spanning a range of messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrSequenceFragment {
    pub kind: FragmentKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub start_edge: usize,
    pub end_edge: usize,
    pub alternatives: Vec<FragmentAlternative>,
    pub children: Vec<usize>,
}

/// A named group of participants (rendered as a box around lifelines).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrParticipantGroup {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub participants: Vec<IrNodeId>,
}

/// A participant creation or destruction event at a specific message index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrLifecycleEvent {
    pub kind: LifecycleEventKind,
    pub participant: IrNodeId,
    pub at_edge: usize,
}

/// A named section in a Gantt chart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrGanttSection {
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum GanttTaskType {
    #[default]
    Normal,
    Critical,
    Done,
    Active,
    Milestone,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GanttDate {
    Absolute(String),
    AfterTask(String),
    DurationDays(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GanttExclude {
    Weekends,
    Dates(Vec<String>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GanttTickInterval {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

/// A single Gantt task with parsed scheduling metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrGanttTask {
    pub node: IrNodeId,
    pub section_idx: usize,
    pub meta: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<GanttDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<GanttDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
    #[serde(default, skip_serializing_if = "is_default_gantt_task_type")]
    pub task_type: GanttTaskType,
}

/// Gantt-diagram-specific metadata that extends the generic IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrGanttMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_interval: Option<GanttTickInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub today_marker_style: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inclusive_end_dates: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekday_start: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<GanttExclude>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<IrGanttSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<IrGanttTask>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_gantt_task_type(value: &GanttTaskType) -> bool {
    matches!(value, GanttTaskType::Normal)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IrXySeriesKind {
    #[default]
    Bar,
    Line,
    Area,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrXyAxis {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrXySeries {
    pub kind: IrXySeriesKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<IrNodeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrXyChartMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub x_axis: IrXyAxis,
    #[serde(default)]
    pub y_axis: IrXyAxis,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<IrXySeries>,
}

/// Sequence-diagram-specific metadata that extends the generic IR.
///
/// Captures all advanced sequence constructs (activations, notes, fragments,
/// participant groups, lifecycle events, autonumbering) that do not fit in the
/// generic node/edge model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrSequenceMeta {
    pub autonumber: bool,
    #[serde(
        default = "default_sequence_autonumber_start",
        skip_serializing_if = "is_default_sequence_autonumber_start"
    )]
    pub autonumber_start: u32,
    #[serde(
        default = "default_sequence_autonumber_increment",
        skip_serializing_if = "is_default_sequence_autonumber_increment"
    )]
    pub autonumber_increment: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_footbox: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activations: Vec<IrActivation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<IrSequenceNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fragments: Vec<IrSequenceFragment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participant_groups: Vec<IrParticipantGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifecycle_events: Vec<IrLifecycleEvent>,
}

const fn default_sequence_autonumber_start() -> u32 {
    1
}

const fn default_sequence_autonumber_increment() -> u32 {
    1
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_sequence_autonumber_start(value: &u32) -> bool {
    *value == default_sequence_autonumber_start()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_sequence_autonumber_increment(value: &u32) -> bool {
    *value == default_sequence_autonumber_increment()
}

impl Default for IrSequenceMeta {
    fn default() -> Self {
        Self {
            autonumber: false,
            autonumber_start: default_sequence_autonumber_start(),
            autonumber_increment: default_sequence_autonumber_increment(),
            hide_footbox: false,
            activations: Vec::new(),
            notes: Vec::new(),
            fragments: Vec::new(),
            participant_groups: Vec::new(),
            lifecycle_events: Vec::new(),
        }
    }
}

impl IrSequenceMeta {
    #[must_use]
    pub fn autonumber_value(&self, edge_index: usize) -> Option<u64> {
        if !self.autonumber {
            return None;
        }

        Some(
            u64::from(self.autonumber_start)
                + (edge_index as u64) * u64::from(self.autonumber_increment),
        )
    }
}

/// A single slice in a pie chart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IrPieSlice {
    pub label: String,
    pub value: f32,
}

/// Pie-chart-specific metadata that extends the generic IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrPieMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show_data: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<IrPieSlice>,
}

/// Requirement-diagram-specific metadata for a node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrRequirementNodeMeta {
    /// The requirement category (e.g., "requirement", "functionalRequirement").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_type: Option<String>,
    /// Unique identifier from the `id:` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<String>,
    /// Human-readable text from the `text:` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Risk level from the `risk:` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    /// Verification method from the `verifymethod:` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_method: Option<String>,
}

/// A data point in a quadrant chart with normalized [0, 1] coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IrQuadrantPoint {
    pub label: String,
    pub x: f32,
    pub y: f32,
}

/// Quadrant-chart-specific metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IrQuadrantMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis_left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis_right: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_axis_bottom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_axis_top: Option<String>,
    /// Labels for the four quadrants: [Q1 top-right, Q2 top-left, Q3 bottom-left, Q4 bottom-right].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quadrant_labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<IrQuadrantPoint>,
}

// ── State diagram note ────────────────────────────────────────────────

/// A note attached to a state node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrStateNote {
    /// Target state name the note is attached to.
    pub target: String,
    /// Position relative to the state: `"right"` or `"left"`.
    pub position: String,
    /// Note text content (may be multi-line).
    pub text: String,
    pub span: Span,
}

// ── Main IR container ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MermaidDiagramIr {
    pub diagram_type: DiagramType,
    pub direction: GraphDirection,
    pub nodes: Vec<IrNode>,
    pub edges: Vec<IrEdge>,
    pub ports: Vec<IrPort>,
    pub clusters: Vec<IrCluster>,
    pub graph: MermaidGraphIr,
    pub labels: Vec<IrLabel>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub label_markup: BTreeMap<IrLabelId, Vec<IrLabelSegment>>,
    pub constraints: Vec<IrConstraint>,
    /// Style references from `classDef`, `style`, and `linkStyle` directives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_refs: Vec<IrStyleRef>,
    /// Parsed `classDef` definitions (structured key-value properties).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_defs: Vec<IrStyleDef>,
    pub meta: MermaidDiagramMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_meta: Option<IrSequenceMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gantt_meta: Option<IrGanttMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xy_chart_meta: Option<IrXyChartMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pie_meta: Option<IrPieMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quadrant_meta: Option<IrQuadrantMeta>,
    /// Notes attached to state diagram nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_notes: Vec<IrStateNote>,
    pub diagnostics: Vec<Diagnostic>,
}

impl MermaidDiagramIr {
    #[must_use]
    pub fn empty(diagram_type: DiagramType) -> Self {
        Self {
            diagram_type,
            direction: GraphDirection::TB,
            nodes: Vec::new(),
            edges: Vec::new(),
            ports: Vec::new(),
            clusters: Vec::new(),
            graph: MermaidGraphIr::default(),
            labels: Vec::new(),
            label_markup: BTreeMap::new(),
            constraints: Vec::new(),
            style_refs: Vec::new(),
            style_defs: Vec::new(),
            meta: MermaidDiagramMeta {
                diagram_type,
                direction: GraphDirection::TB,
                support_level: diagram_type.support_level(),
                parse_mode: MermaidParseMode::Compat,
                block_beta_columns: None,
                init: MermaidInitParse::default(),
                theme_overrides: MermaidThemeOverrides::default(),
                c4_show_legend: false,
                guard: MermaidGuardReport::default(),
                title: None,
                acc_title: None,
                acc_descr: None,
            },
            sequence_meta: None,
            gantt_meta: None,
            xy_chart_meta: None,
            pie_meta: None,
            quadrant_meta: None,
            state_notes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Pre-size internal vectors for the estimated graph cardinalities.
    ///
    /// Call this after constructing an empty IR if you know the approximate
    /// number of nodes, edges, and labels. Avoids repeated `Vec` reallocation
    /// during incremental graph construction.
    pub fn reserve_capacity(&mut self, nodes: usize, edges: usize, labels: usize) {
        self.nodes.reserve(nodes);
        self.edges.reserve(edges);
        self.labels.reserve(labels);
        self.graph.nodes.reserve(nodes);
        self.graph.edges.reserve(edges);
    }

    /// Add a diagnostic to this IR.
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Add multiple diagnostics.
    pub fn add_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    /// Check if there are any error-level diagnostics.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Check if there are any warning-level diagnostics.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_warning)
    }

    /// Count diagnostics by severity.
    #[must_use]
    pub fn diagnostic_counts(&self) -> DiagnosticCounts {
        let mut counts = DiagnosticCounts::default();
        for diag in &self.diagnostics {
            match diag.severity {
                DiagnosticSeverity::Hint => counts.hints += 1,
                DiagnosticSeverity::Info => counts.infos += 1,
                DiagnosticSeverity::Warning => counts.warnings += 1,
                DiagnosticSeverity::Error => counts.errors += 1,
            }
        }
        counts
    }

    /// Get diagnostics filtered by severity.
    #[must_use]
    pub fn diagnostics_by_severity(&self, severity: DiagnosticSeverity) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .collect()
    }

    /// Get diagnostics filtered by category.
    #[must_use]
    pub fn diagnostics_by_category(&self, category: DiagnosticCategory) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.category == category)
            .collect()
    }

    /// Find a node by ID, returning its index.
    #[must_use]
    pub fn find_node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Find a node by ID.
    #[must_use]
    pub fn find_node(&self, id: &str) -> Option<&IrNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    #[must_use]
    pub fn node(&self, node_id: IrNodeId) -> Option<&IrNode> {
        self.nodes.get(node_id.0)
    }

    #[must_use]
    pub fn graph_node(&self, node_id: IrNodeId) -> Option<&IrGraphNode> {
        self.graph.node(node_id)
    }

    #[must_use]
    pub fn graph_subgraph(&self, subgraph_id: IrSubgraphId) -> Option<&IrSubgraph> {
        self.graph.subgraph(subgraph_id)
    }

    #[must_use]
    pub fn resolve_endpoint_node(&self, endpoint: IrEndpoint) -> Option<IrNodeId> {
        endpoint.resolved_node_id(&self.ports)
    }

    #[must_use]
    pub fn graph_incident_edges(&self, node_id: IrNodeId) -> Vec<&IrGraphEdge> {
        self.graph
            .edges
            .iter()
            .filter(|edge| {
                self.resolve_endpoint_node(edge.from) == Some(node_id)
                    || self.resolve_endpoint_node(edge.to) == Some(node_id)
            })
            .collect()
    }

    #[must_use]
    pub fn graph_outgoing_edges(&self, node_id: IrNodeId) -> Vec<&IrGraphEdge> {
        self.graph
            .edges
            .iter()
            .filter(|edge| self.resolve_endpoint_node(edge.from) == Some(node_id))
            .collect()
    }

    #[must_use]
    pub fn graph_incoming_edges(&self, node_id: IrNodeId) -> Vec<&IrGraphEdge> {
        self.graph
            .edges
            .iter()
            .filter(|edge| self.resolve_endpoint_node(edge.to) == Some(node_id))
            .collect()
    }

    #[must_use]
    pub fn graph_neighbors(&self, node_id: IrNodeId) -> Vec<IrNodeId> {
        let mut neighbors = Vec::new();

        for edge in self.graph_incident_edges(node_id) {
            let Some(from) = self.resolve_endpoint_node(edge.from) else {
                continue;
            };
            let Some(to) = self.resolve_endpoint_node(edge.to) else {
                continue;
            };

            let candidate = if from == node_id { to } else { from };
            if !neighbors.contains(&candidate) {
                neighbors.push(candidate);
            }
        }

        neighbors
    }

    /// Populate [`style_defs`](Self::style_defs) and per-node/edge
    /// [`inline_style`](IrNode::inline_style) fields from the raw
    /// [`style_refs`](Self::style_refs).
    ///
    /// Call this after parsing is complete.  It is idempotent — calling it
    /// twice produces the same result.
    pub fn populate_structured_styles(&mut self) {
        // 1. Build classDef definitions.
        let mut defs: BTreeMap<String, IrStyleDef> = BTreeMap::new();
        for sr in &self.style_refs {
            if let IrStyleTarget::Class(ref name) = sr.target {
                let parsed = parse_style_string(&sr.style);
                defs.entry(name.clone())
                    .and_modify(|existing| {
                        existing.properties.extend(parsed.properties.clone());
                    })
                    .or_insert_with(|| IrStyleDef {
                        name: name.clone(),
                        properties: parsed.properties,
                        span: sr.span,
                    });
            }
        }
        self.style_defs = defs.into_values().collect();

        // 2. Apply per-node styles (cascade: classDef → style directive).
        for (node_idx, node) in self.nodes.iter_mut().enumerate() {
            let node_id = IrNodeId(node_idx);
            let mut merged = BTreeMap::new();

            // Layer 1: classDef properties via node.classes.
            for class_name in &node.classes {
                if let Some(def) = self.style_defs.iter().find(|d| d.name == *class_name) {
                    merged.extend(def.properties.clone());
                }
            }

            // Layer 2: direct `style nodeId` overrides.
            for sr in &self.style_refs {
                if let IrStyleTarget::Node(target_id) = sr.target
                    && target_id == node_id
                {
                    let parsed = parse_style_string(&sr.style);
                    merged.extend(parsed.properties);
                }
            }

            if !merged.is_empty() {
                node.inline_style = Some(IrInlineStyle { properties: merged });
            }
        }

        // 3. Apply per-edge styles (cascade: linkStyle default → linkStyle N).
        let mut default_link_style = BTreeMap::new();
        for sr in &self.style_refs {
            if sr.target == IrStyleTarget::LinkDefault {
                let parsed = parse_style_string(&sr.style);
                default_link_style.extend(parsed.properties);
            }
        }

        for (edge_idx, edge) in self.edges.iter_mut().enumerate() {
            let mut merged = default_link_style.clone();

            for sr in &self.style_refs {
                if let IrStyleTarget::Link(link_idx) = sr.target
                    && link_idx == edge_idx
                {
                    let parsed = parse_style_string(&sr.style);
                    merged.extend(parsed.properties);
                }
            }

            if !merged.is_empty() {
                edge.inline_style = Some(IrInlineStyle { properties: merged });
            }
        }
    }

    #[must_use]
    pub fn source_map(&self) -> MermaidSourceMap {
        let mut entries = Vec::new();

        for (index, node) in self.nodes.iter().enumerate() {
            if node.span_primary.is_unknown() {
                continue;
            }

            entries.push(MermaidSourceMapEntry {
                kind: MermaidSourceMapKind::Node,
                index,
                element_id: mermaid_node_element_id(&node.id, index),
                source_id: (!node.id.is_empty()).then(|| node.id.clone()),
                span: node.span_primary,
            });
        }

        for (index, edge) in self.edges.iter().enumerate() {
            if edge.span.is_unknown() {
                continue;
            }

            entries.push(MermaidSourceMapEntry {
                kind: MermaidSourceMapKind::Edge,
                index,
                element_id: mermaid_edge_element_id(index),
                source_id: None,
                span: edge.span,
            });
        }

        for (index, cluster) in self.clusters.iter().enumerate() {
            if cluster.span.is_unknown() {
                continue;
            }

            entries.push(MermaidSourceMapEntry {
                kind: MermaidSourceMapKind::Cluster,
                index,
                element_id: mermaid_cluster_element_id(index),
                source_id: Some(cluster.id.0.to_string()),
                span: cluster.span,
            });
        }

        MermaidSourceMap {
            diagram_type: self.diagram_type,
            entries,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MermaidSourceMapKind {
    Node,
    Edge,
    Cluster,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidSourceMapEntry {
    pub kind: MermaidSourceMapKind,
    pub index: usize,
    pub element_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MermaidSourceMap {
    pub diagram_type: DiagramType,
    pub entries: Vec<MermaidSourceMapEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MermaidTextRange {
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MermaidLensBinding {
    pub kind: MermaidSourceMapKind,
    pub index: usize,
    pub element_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub span: Span,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_range: Option<MermaidTextRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MermaidLensEdit {
    pub element_id: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MermaidLensEditResult {
    pub element_id: String,
    pub replaced_range: MermaidTextRange,
    pub previous_snippet: String,
    pub replacement: String,
    pub updated_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Error)]
pub enum MermaidLensError {
    #[error("source element '{0}' was not found in the source map")]
    ElementNotFound(String),
    #[error("source span for element '{0}' could not be resolved against the current source text")]
    UnresolvedSpan(String),
}

#[must_use]
pub fn build_lens_bindings(source: &str, source_map: &MermaidSourceMap) -> Vec<MermaidLensBinding> {
    source_map
        .entries
        .iter()
        .map(|entry| {
            let text_range = resolve_span_text_range(source, entry.span);
            let snippet = text_range
                .as_ref()
                .and_then(|range| source.get(range.start_byte..range.end_byte))
                .map(str::to_string);
            MermaidLensBinding {
                kind: entry.kind,
                index: entry.index,
                element_id: entry.element_id.clone(),
                source_id: entry.source_id.clone(),
                span: entry.span,
                text_range,
                snippet,
            }
        })
        .collect()
}

pub fn apply_lens_edit(
    source: &str,
    source_map: &MermaidSourceMap,
    edit: &MermaidLensEdit,
) -> Result<MermaidLensEditResult, MermaidLensError> {
    let binding = build_lens_bindings(source, source_map)
        .into_iter()
        .find(|binding| binding.element_id == edit.element_id)
        .ok_or_else(|| MermaidLensError::ElementNotFound(edit.element_id.clone()))?;
    let replaced_range = binding
        .text_range
        .ok_or_else(|| MermaidLensError::UnresolvedSpan(edit.element_id.clone()))?;
    let previous_snippet = binding
        .snippet
        .clone()
        .ok_or_else(|| MermaidLensError::UnresolvedSpan(edit.element_id.clone()))?;

    let mut updated_source = source.to_string();
    updated_source.replace_range(
        replaced_range.start_byte..replaced_range.end_byte,
        &edit.replacement,
    );

    Ok(MermaidLensEditResult {
        element_id: binding.element_id,
        replaced_range,
        previous_snippet,
        replacement: edit.replacement.clone(),
        updated_source,
    })
}

#[must_use]
pub fn resolve_span_text_range(source: &str, span: Span) -> Option<MermaidTextRange> {
    if span.is_unknown() {
        return None;
    }

    if span.end.byte > span.start.byte {
        return Some(MermaidTextRange {
            start_byte: span.start.byte,
            end_byte: span.end.byte,
        });
    }

    let line_starts = source_line_starts(source);
    let start_byte =
        byte_index_for_line_col(source, &line_starts, span.start.line, span.start.col)?;
    let end_col_exclusive = span.end.col.saturating_add(1);
    let end_byte = byte_index_for_line_col(source, &line_starts, span.end.line, end_col_exclusive)?;
    (end_byte >= start_byte).then_some(MermaidTextRange {
        start_byte,
        end_byte,
    })
}

fn source_line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, ch) in source.char_indices() {
        if ch == '\n' {
            starts.push(idx + ch.len_utf8());
        }
    }
    starts
}

fn byte_index_for_line_col(
    source: &str,
    line_starts: &[usize],
    line: usize,
    col: usize,
) -> Option<usize> {
    if line == 0 || col == 0 {
        return None;
    }

    let line_start = *line_starts.get(line - 1)?;
    let mut line_end = line_starts.get(line).copied().unwrap_or(source.len());
    if source.as_bytes().get(line_end.wrapping_sub(1)) == Some(&b'\n') {
        line_end -= 1;
    }
    if source.as_bytes().get(line_end.wrapping_sub(1)) == Some(&b'\r') {
        line_end -= 1;
    }

    let line_slice = source.get(line_start..line_end)?;
    let mut current_col = 1;
    for (offset, _) in line_slice.char_indices() {
        if current_col == col {
            return Some(line_start + offset);
        }
        current_col += 1;
    }

    (current_col == col).then_some(line_end)
}

fn sanitize_render_element_fragment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_was_dash = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !out.is_empty() {
            out.push('-');
            last_was_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

#[must_use]
pub fn mermaid_node_element_id(node_id: &str, index: usize) -> String {
    mermaid_node_element_id_with_variant(node_id, index, None)
}

#[must_use]
pub fn mermaid_node_element_id_with_variant(
    node_id: &str,
    index: usize,
    variant: Option<&str>,
) -> String {
    let fragment = sanitize_render_element_fragment(node_id);
    let mut id = if fragment.is_empty() {
        format!("fm-node-{index}")
    } else {
        format!("fm-node-{fragment}-{index}")
    };

    if let Some(variant) = variant
        .map(sanitize_render_element_fragment)
        .filter(|v| !v.is_empty())
    {
        id.push('-');
        id.push_str(&variant);
    }

    id
}

#[must_use]
pub fn mermaid_edge_element_id(index: usize) -> String {
    format!("fm-edge-{index}")
}

#[must_use]
pub fn mermaid_cluster_element_id(index: usize) -> String {
    format!("fm-cluster-{index}")
}

/// Counts of diagnostics by severity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticCounts {
    pub hints: usize,
    pub infos: usize,
    pub warnings: usize,
    pub errors: usize,
}

impl DiagnosticCounts {
    /// Total count of all diagnostics.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.hints + self.infos + self.warnings + self.errors
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MermaidIrParse {
    pub ir: MermaidDiagramIr,
    pub warnings: Vec<MermaidWarning>,
    pub errors: Vec<MermaidError>,
}

mod schema_version_semver {
    use serde::{self, Deserialize, Deserializer, Serializer};

    use crate::SchemaVersion;

    pub fn serialize<S>(value: &SchemaVersion, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SchemaVersion, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value
            .parse()
            .map_err(|_| serde::de::Error::custom(format!("invalid schema version: {value}")))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use std::collections::BTreeMap;

    use super::{
        ArrowType, DegradationContext, DegradationOperator, Diagnostic, DiagnosticCategory,
        DiagnosticSeverity, DiagramPalettePreset, DiagramType, EdgeMap, FragmentAlternative,
        FragmentKind, GanttDate, GanttExclude, GanttTaskType, GanttTickInterval, GraphDirection,
        IrActivation, IrAttributeKey, IrCluster, IrClusterId, IrEdge, IrEdgeKind, IrEndpoint,
        IrEntityAttribute, IrGanttMeta, IrGanttSection, IrGanttTask, IrGraphCluster, IrGraphEdge,
        IrGraphNode, IrInlineStyle, IrLabel, IrLabelId, IrLifecycleEvent, IrNode, IrNodeId,
        IrNodeKind, IrParticipantGroup, IrPort, IrPortId, IrPortSideHint, IrSequenceFragment,
        IrSequenceMeta, IrSequenceNote, IrStyleDef, IrStyleRef, IrStyleTarget, IrSubgraph,
        IrSubgraphId, IrXyAxis, IrXyChartMeta, IrXySeries, IrXySeriesKind, LifecycleEventKind,
        MERMAID_SCHEMA_VERSION, MermaidBudgetLedger, MermaidConfig, MermaidDecisionWeight,
        MermaidDegradationPlan, MermaidDiagramIr, MermaidError, MermaidErrorCode,
        MermaidFallbackAction, MermaidFallbackPolicy, MermaidFidelity, MermaidGlyphMode,
        MermaidGuardReport, MermaidLayoutDecisionAlternative, MermaidLayoutDecisionLedger,
        MermaidLayoutDecisionRecord, MermaidLensEdit, MermaidNativePressureSignals,
        MermaidPressureReport, MermaidPressureTier, MermaidQualityMode, MermaidSanitizeMode,
        MermaidSourceMap, MermaidSourceMapEntry, MermaidSourceMapKind, MermaidSupportLevel,
        MermaidWarningCode, MermaidWasmPressureSignals, NodeMap, NodeSet, NodeShape, NotePosition,
        Position, Span, StructuredDiagnostic, apply_lens_edit, build_lens_bindings,
        capability_matrix, capability_matrix_json_pretty,
        capability_readme_supported_diagram_types_markdown, capability_readme_surface_markdown,
        documented_diagram_types, is_allowed_style_property, mermaid_layout_guard_observability,
        parse_mermaid_js_config_value, parse_style_string, parse_style_string_with_rejections,
        resolve_span_text_range, sanitize_style_value, scale_budget, to_init_parse,
    };

    fn sample_span(line: usize, start_col: usize, end_col: usize) -> Span {
        Span::new(
            Position {
                line,
                col: start_col,
                byte: 0,
            },
            Position {
                line,
                col: end_col,
                byte: 0,
            },
        )
    }

    #[test]
    fn creates_empty_ir() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        assert_eq!(ir.direction, GraphDirection::TB);
        assert_eq!(ir.nodes.len(), 0);
        assert_eq!(ir.edges.len(), 0);
        assert!(ir.graph.nodes.is_empty());
        assert!(ir.graph.edges.is_empty());
        assert!(ir.graph.clusters.is_empty());
        assert!(ir.graph.subgraphs.is_empty());
        assert_eq!(ir.diagnostics.len(), 0);
    }

    #[test]
    fn arrow_type_string_mapping_is_stable() {
        assert_eq!(ArrowType::DottedArrow.as_str(), "-.->");
    }

    #[test]
    fn diagnostic_builder_pattern() {
        let diag = Diagnostic::error("Test error")
            .with_category(DiagnosticCategory::Parser)
            .with_span(Span::default())
            .with_suggestion("Try this instead")
            .with_expected(vec!["foo".to_string(), "bar".to_string()])
            .with_found("baz");

        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert_eq!(diag.category, DiagnosticCategory::Parser);
        assert_eq!(diag.message, "Test error");
        assert!(diag.span.is_some());
        assert_eq!(diag.suggestion, Some("Try this instead".to_string()));
        assert_eq!(diag.expected, vec!["foo", "bar"]);
        assert_eq!(diag.found, Some("baz".to_string()));
    }

    #[test]
    fn diagnostic_severity_levels() {
        assert!(Diagnostic::error("e").is_error());
        assert!(!Diagnostic::error("e").is_warning());
        assert!(Diagnostic::warning("w").is_warning());
        assert!(!Diagnostic::warning("w").is_error());
    }

    #[test]
    fn layout_decision_ledger_serializes_to_jsonl() {
        let (_cx, observability) = mermaid_layout_guard_observability(
            "cli.validate",
            "flowchart LR\nA-->B",
            "sugiyama",
            42,
        );
        let ledger = MermaidLayoutDecisionLedger {
            entries: vec![MermaidLayoutDecisionRecord {
                kind: "layout_decision".to_string(),
                trace_id: observability.trace_id,
                decision_id: observability.decision_id,
                policy_id: observability.policy_id,
                schema_version: observability.schema_version,
                requested_algorithm: "auto".to_string(),
                selected_algorithm: "sugiyama".to_string(),
                capability_unavailable: false,
                decision_mode: "expected_loss_general_graph_v1".to_string(),
                dispatch_reason: "auto_selected_for_flowchart".to_string(),
                guard_reason: "within_budget".to_string(),
                fallback_applied: false,
                confidence_permille: 820,
                selected_expected_loss_permille: 140,
                node_count: 2,
                edge_count: 1,
                crossing_count: 0,
                reversed_edges: 0,
                estimated_layout_time_ms: 12,
                estimated_layout_iterations: 3,
                estimated_route_ops: 8,
                pressure_source: super::MermaidPressureSource::Native,
                pressure_tier: MermaidPressureTier::Nominal,
                budget_total_ms: 250,
                budget_exhausted: false,
                state_posterior: vec![
                    MermaidDecisionWeight {
                        key: "tree_like".to_string(),
                        value_permille: 120,
                    },
                    MermaidDecisionWeight {
                        key: "dense_graph".to_string(),
                        value_permille: 80,
                    },
                    MermaidDecisionWeight {
                        key: "layered_general".to_string(),
                        value_permille: 800,
                    },
                ],
                expected_loss: vec![
                    MermaidDecisionWeight {
                        key: "sugiyama".to_string(),
                        value_permille: 140,
                    },
                    MermaidDecisionWeight {
                        key: "tree".to_string(),
                        value_permille: 500,
                    },
                    MermaidDecisionWeight {
                        key: "force".to_string(),
                        value_permille: 620,
                    },
                ],
                alternatives: vec![MermaidLayoutDecisionAlternative {
                    algorithm: "sugiyama".to_string(),
                    selected: true,
                    available_for_diagram: true,
                    note: Some("selected via auto_selected_for_flowchart".to_string()),
                }],
                notes: vec!["auto_selected_for_flowchart".to_string()],
            }],
        };

        let jsonl = ledger.to_jsonl().expect("ledger should serialize");
        assert!(jsonl.contains("\"kind\":\"layout_decision\""));
        assert!(jsonl.contains("\"selected_algorithm\":\"sugiyama\""));
        assert!(jsonl.contains("\"confidence_permille\":820"));
        assert!(!ledger.is_empty());
    }

    #[test]
    fn ir_diagnostic_helpers() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        assert!(!ir.has_errors());
        assert!(!ir.has_warnings());

        ir.add_diagnostic(Diagnostic::warning("a warning"));
        assert!(!ir.has_errors());
        assert!(ir.has_warnings());

        ir.add_diagnostic(Diagnostic::error("an error"));
        assert!(ir.has_errors());

        let counts = ir.diagnostic_counts();
        assert_eq!(counts.warnings, 1);
        assert_eq!(counts.errors, 1);
        assert_eq!(counts.total(), 2);
    }

    #[test]
    fn ir_diagnostic_filtering() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.add_diagnostic(Diagnostic::warning("w1").with_category(DiagnosticCategory::Parser));
        ir.add_diagnostic(Diagnostic::warning("w2").with_category(DiagnosticCategory::Semantic));
        ir.add_diagnostic(Diagnostic::error("e1").with_category(DiagnosticCategory::Parser));

        let parser_diags = ir.diagnostics_by_category(DiagnosticCategory::Parser);
        assert_eq!(parser_diags.len(), 2);

        let warnings = ir.diagnostics_by_severity(DiagnosticSeverity::Warning);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn mermaid_js_config_adapter_maps_common_fields() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "theme": "dark",
            "themeVariables": {
                "primaryColor": "#ffffff",
                "lineColor": 12
            },
            "flowchart": {
                "direction": "RL",
                "curve": "basis"
            },
            "sequence": {
                "mirrorActors": true,
                "showSequenceNumbers": true
            },
            "securityLevel": "loose"
        }));

        assert!(
            parsed.errors.is_empty(),
            "unexpected errors: {:?}",
            parsed.errors
        );
        assert_eq!(parsed.config.theme.as_deref(), Some("dark"));
        assert_eq!(
            parsed
                .config
                .theme_variables
                .get("primaryColor")
                .map(String::as_str),
            Some("#ffffff")
        );
        assert_eq!(
            parsed
                .config
                .theme_variables
                .get("lineColor")
                .map(String::as_str),
            Some("12")
        );
        assert_eq!(parsed.config.flowchart_direction, Some(GraphDirection::RL));
        assert_eq!(parsed.config.flowchart_curve.as_deref(), Some("basis"));
        assert_eq!(parsed.config.sequence_mirror_actors, Some(true));
        assert_eq!(parsed.config.sequence_show_sequence_numbers, Some(true));
        assert_eq!(parsed.config.sanitize_mode, MermaidSanitizeMode::Lenient);
    }

    #[test]
    fn mermaid_js_config_adapter_reports_unknown_and_type_issues() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "theme": 42,
            "flowchart": "not-an-object",
            "sequence": { "mirrorActors": "yes", "showSequenceNumbers": "yes" },
            "unknownKey": true
        }));

        assert!(!parsed.errors.is_empty());
        assert!(parsed.errors.iter().any(|e| e.field == "theme"));
        assert!(parsed.errors.iter().any(|e| e.field == "flowchart"));
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.field == "sequence.mirrorActors")
        );
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.field == "sequence.showSequenceNumbers")
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.message.contains("unknownKey"))
        );
    }

    #[test]
    fn mermaid_js_config_can_be_projected_to_init_parse() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "theme": "corporate",
            "themeVariables": { "primaryColor": "#0ff" },
            "flowchart": { "rankDir": "LR", "curve": "linear" },
            "sequence": { "mirrorActors": false, "showSequenceNumbers": true }
        }));
        let init_parse = to_init_parse(parsed);

        assert!(init_parse.errors.is_empty());
        assert_eq!(init_parse.config.theme.as_deref(), Some("corporate"));
        assert_eq!(
            init_parse
                .config
                .theme_variables
                .get("primaryColor")
                .map(String::as_str),
            Some("#0ff")
        );
        assert_eq!(
            init_parse.config.flowchart_direction,
            Some(GraphDirection::LR)
        );
        assert_eq!(init_parse.config.flowchart_curve.as_deref(), Some("linear"));
        assert_eq!(init_parse.config.sequence_mirror_actors, Some(false));
        assert_eq!(init_parse.config.sequence_show_sequence_numbers, Some(true));
    }

    #[test]
    fn structured_diagnostic_from_warning_preserves_span_and_code() {
        let warning = super::MermaidWarning {
            code: super::MermaidWarningCode::UnsupportedFeature,
            message: "unsupported directive".to_string(),
            span: Span::at_line(3, 10),
        };

        let structured = StructuredDiagnostic::from_warning(&warning);
        assert_eq!(
            structured.error_code,
            "mermaid/warn/unsupported-feature".to_string()
        );
        assert_eq!(structured.severity, "warning".to_string());
        assert_eq!(structured.source_line, Some(3));
        assert_eq!(structured.source_column, Some(1));
    }

    #[test]
    fn structured_diagnostic_from_error_maps_expected_to_hint() {
        let parse_error = super::MermaidError::Parse {
            message: "unexpected token".to_string(),
            span: Span::at_line(5, 4),
            expected: vec!["node id".to_string(), "arrow".to_string()],
        };
        let structured = StructuredDiagnostic::from_error(&parse_error);
        assert_eq!(structured.error_code, "mermaid/error/parse".to_string());
        assert_eq!(structured.severity, "error".to_string());
        assert!(
            structured
                .remediation_hint
                .as_deref()
                .is_some_and(|hint| hint.contains("Expected one of"))
        );
    }

    #[test]
    fn structured_diagnostic_rank_orders_by_severity() {
        let hint = StructuredDiagnostic {
            severity: "hint".to_string(),
            ..Default::default()
        };
        let warning = StructuredDiagnostic {
            severity: "warning".to_string(),
            ..Default::default()
        };
        let error = StructuredDiagnostic {
            severity: "error".to_string(),
            ..Default::default()
        };

        assert!(hint.severity_rank() < warning.severity_rank());
        assert!(warning.severity_rank() < error.severity_rank());
    }

    #[test]
    fn span_helpers_build_expected_positions() {
        let span = sample_span(7, 3, 11);
        assert_eq!(span.start.line, 7);
        assert_eq!(span.start.col, 3);
        assert_eq!(span.end.col, 11);

        let line_span = Span::at_line(9, 0);
        assert_eq!(line_span.start.line, 9);
        assert_eq!(line_span.start.col, 1);
        assert_eq!(line_span.end.col, 1);
    }

    #[test]
    fn resolve_span_text_range_maps_line_columns_to_byte_offsets() {
        let source = "flowchart LR\nA-->B\n";
        let range = resolve_span_text_range(source, sample_span(2, 1, 5)).expect("range");
        assert_eq!(&source[range.start_byte..range.end_byte], "A-->B");
    }

    #[test]
    fn build_lens_bindings_captures_snippets_from_source_map_entries() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = MermaidSourceMap {
            diagram_type: DiagramType::Flowchart,
            entries: vec![
                MermaidSourceMapEntry {
                    kind: MermaidSourceMapKind::Node,
                    index: 0,
                    element_id: "fm-node-a-0".to_string(),
                    source_id: Some("A".to_string()),
                    span: sample_span(2, 1, 1),
                },
                MermaidSourceMapEntry {
                    kind: MermaidSourceMapKind::Edge,
                    index: 0,
                    element_id: "fm-edge-0".to_string(),
                    source_id: None,
                    span: sample_span(2, 1, 5),
                },
            ],
        };

        let bindings = build_lens_bindings(source, &source_map);
        assert_eq!(bindings[0].snippet.as_deref(), Some("A"));
        assert_eq!(bindings[1].snippet.as_deref(), Some("A-->B"));
    }

    #[test]
    fn apply_lens_edit_rewrites_the_selected_source_region() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = MermaidSourceMap {
            diagram_type: DiagramType::Flowchart,
            entries: vec![MermaidSourceMapEntry {
                kind: MermaidSourceMapKind::Edge,
                index: 0,
                element_id: "fm-edge-0".to_string(),
                source_id: None,
                span: sample_span(2, 1, 5),
            }],
        };
        let edit = MermaidLensEdit {
            element_id: "fm-edge-0".to_string(),
            replacement: "A-.->B".to_string(),
        };

        let result = apply_lens_edit(source, &source_map, &edit).expect("edit should apply");
        assert_eq!(result.previous_snippet, "A-->B");
        assert_eq!(result.updated_source, "flowchart LR\nA-.->B\n");
    }

    #[test]
    fn mermaid_error_code_strings_are_stable() {
        let expectations = [
            (MermaidErrorCode::Parse, "mermaid/error/parse"),
            (MermaidErrorCode::Validation, "mermaid/error/validation"),
            (MermaidErrorCode::Unsupported, "mermaid/error/unsupported"),
        ];

        for (code, expected) in expectations {
            assert_eq!(code.as_str(), expected);
        }
    }

    #[test]
    fn mermaid_warning_code_strings_are_stable() {
        let expectations = [
            (
                MermaidWarningCode::ParseRecovery,
                "mermaid/warn/parse-recovery",
            ),
            (
                MermaidWarningCode::UnsupportedStyle,
                "mermaid/warn/unsupported-style",
            ),
            (
                MermaidWarningCode::UnsupportedLink,
                "mermaid/warn/unsupported-link",
            ),
            (
                MermaidWarningCode::UnsupportedFeature,
                "mermaid/warn/unsupported-feature",
            ),
        ];

        for (code, expected) in expectations {
            assert_eq!(code.as_str(), expected);
        }
    }

    #[test]
    fn mermaid_error_code_and_span_accessors_cover_variants() {
        let span = sample_span(4, 2, 8);
        let parse = MermaidError::Parse {
            message: "parse".to_string(),
            span,
            expected: vec!["node".to_string()],
        };
        let validation = MermaidError::Validation {
            message: "validation".to_string(),
            span,
        };
        let unsupported = MermaidError::Unsupported {
            message: "unsupported".to_string(),
            span,
        };

        assert_eq!(parse.code(), MermaidErrorCode::Parse);
        assert_eq!(validation.code(), MermaidErrorCode::Validation);
        assert_eq!(unsupported.code(), MermaidErrorCode::Unsupported);
        assert_eq!(parse.span(), span);
        assert_eq!(validation.span(), span);
        assert_eq!(unsupported.span(), span);
    }

    #[test]
    fn diagram_type_string_mapping_is_exhaustive() {
        let expectations = [
            (DiagramType::Flowchart, "flowchart"),
            (DiagramType::Sequence, "sequence"),
            (DiagramType::State, "state"),
            (DiagramType::Gantt, "gantt"),
            (DiagramType::Class, "class"),
            (DiagramType::Er, "er"),
            (DiagramType::Mindmap, "mindmap"),
            (DiagramType::Pie, "pie"),
            (DiagramType::GitGraph, "gitGraph"),
            (DiagramType::Journey, "journey"),
            (DiagramType::Requirement, "requirementDiagram"),
            (DiagramType::Timeline, "timeline"),
            (DiagramType::QuadrantChart, "quadrantChart"),
            (DiagramType::Sankey, "sankey"),
            (DiagramType::XyChart, "xyChart"),
            (DiagramType::BlockBeta, "block-beta"),
            (DiagramType::PacketBeta, "packet-beta"),
            (DiagramType::ArchitectureBeta, "architecture-beta"),
            (DiagramType::C4Context, "C4Context"),
            (DiagramType::C4Container, "C4Container"),
            (DiagramType::C4Component, "C4Component"),
            (DiagramType::C4Dynamic, "C4Dynamic"),
            (DiagramType::C4Deployment, "C4Deployment"),
            (DiagramType::Kanban, "kanban"),
            (DiagramType::Unknown, "unknown"),
        ];

        for (diagram_type, expected) in expectations {
            assert_eq!(diagram_type.as_str(), expected);
        }
    }

    #[test]
    fn graph_direction_string_mapping_is_exhaustive() {
        let expectations = [
            (GraphDirection::TB, "TB"),
            (GraphDirection::TD, "TD"),
            (GraphDirection::LR, "LR"),
            (GraphDirection::RL, "RL"),
            (GraphDirection::BT, "BT"),
        ];

        for (direction, expected) in expectations {
            assert_eq!(direction.as_str(), expected);
        }
    }

    #[test]
    fn ir_port_side_hint_tracks_graph_direction() {
        assert_eq!(
            IrPortSideHint::from_direction(GraphDirection::LR),
            IrPortSideHint::Horizontal
        );
        assert_eq!(
            IrPortSideHint::from_direction(GraphDirection::RL),
            IrPortSideHint::Horizontal
        );
        assert_eq!(
            IrPortSideHint::from_direction(GraphDirection::TB),
            IrPortSideHint::Vertical
        );
        assert_eq!(
            IrPortSideHint::from_direction(GraphDirection::TD),
            IrPortSideHint::Vertical
        );
        assert_eq!(
            IrPortSideHint::from_direction(GraphDirection::BT),
            IrPortSideHint::Vertical
        );
    }

    #[test]
    fn arrow_type_string_mapping_is_complete() {
        let expectations = [
            (ArrowType::Line, "---"),
            (ArrowType::Arrow, "-->"),
            (ArrowType::OpenArrow, "-)"),
            (ArrowType::HalfArrowTop, "-|\\"),
            (ArrowType::HalfArrowBottom, "-|/"),
            (ArrowType::HalfArrowTopReverse, "/|-"),
            (ArrowType::HalfArrowBottomReverse, "\\|-"),
            (ArrowType::StickArrowTop, "-\\\\"),
            (ArrowType::StickArrowBottom, "-//"),
            (ArrowType::StickArrowTopReverse, "//-"),
            (ArrowType::StickArrowBottomReverse, "\\\\-"),
            (ArrowType::ThickArrow, "==>"),
            (ArrowType::DottedArrow, "-.->"),
            (ArrowType::DottedOpenArrow, "--)"),
            (ArrowType::DottedCross, "--x"),
            (ArrowType::HalfArrowTopDotted, "--|\\"),
            (ArrowType::HalfArrowBottomDotted, "--|/"),
            (ArrowType::HalfArrowTopReverseDotted, "/|--"),
            (ArrowType::HalfArrowBottomReverseDotted, "\\|--"),
            (ArrowType::StickArrowTopDotted, "--\\\\"),
            (ArrowType::StickArrowBottomDotted, "--//"),
            (ArrowType::StickArrowTopReverseDotted, "//--"),
            (ArrowType::StickArrowBottomReverseDotted, "\\\\--"),
            (ArrowType::Circle, "--o"),
            (ArrowType::Cross, "-x"),
            (ArrowType::ThickLine, "==="),
            (ArrowType::DottedLine, "-.-"),
            (ArrowType::DoubleArrow, "<-->"),
            (ArrowType::DoubleThickArrow, "<==>"),
            (ArrowType::DoubleDottedArrow, "<-.->"),
        ];

        for (arrow, expected) in expectations {
            assert_eq!(arrow.as_str(), expected);
        }
    }

    #[test]
    fn node_shape_roundtrip_covers_all_variants() {
        let shapes = [
            NodeShape::Rect,
            NodeShape::Rounded,
            NodeShape::Stadium,
            NodeShape::Subroutine,
            NodeShape::Diamond,
            NodeShape::Hexagon,
            NodeShape::Circle,
            NodeShape::FilledCircle,
            NodeShape::Asymmetric,
            NodeShape::Cylinder,
            NodeShape::Trapezoid,
            NodeShape::DoubleCircle,
            NodeShape::HorizontalBar,
            NodeShape::Note,
            NodeShape::InvTrapezoid,
            NodeShape::Parallelogram,
            NodeShape::InvParallelogram,
            NodeShape::Triangle,
            NodeShape::Pentagon,
            NodeShape::Star,
            NodeShape::Cloud,
            NodeShape::Tag,
            NodeShape::CrossedCircle,
        ];

        for shape in shapes {
            let encoded = serde_json::to_string(&shape).expect("serialize node shape");
            let decoded: NodeShape =
                serde_json::from_str(&encoded).expect("deserialize node shape");
            assert_eq!(decoded, shape);
        }
    }

    #[test]
    fn diagram_palette_roundtrip_covers_all_variants() {
        let palettes = [
            DiagramPalettePreset::Default,
            DiagramPalettePreset::Corporate,
            DiagramPalettePreset::Neon,
            DiagramPalettePreset::Monochrome,
            DiagramPalettePreset::Pastel,
            DiagramPalettePreset::HighContrast,
        ];

        for palette in palettes {
            let encoded = serde_json::to_string(&palette).expect("serialize palette");
            let decoded: DiagramPalettePreset =
                serde_json::from_str(&encoded).expect("deserialize palette");
            assert_eq!(decoded, palette);
        }
    }

    #[test]
    fn mermaid_config_default_values_are_stable() {
        let config = MermaidConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_nodes, 200);
        assert_eq!(config.max_edges, 400);
        assert_eq!(config.route_budget, 4_000);
        assert_eq!(config.layout_iteration_budget, 200);
        assert_eq!(config.max_label_chars, 48);
        assert_eq!(config.max_label_lines, 3);
        assert_eq!(config.palette, DiagramPalettePreset::Default);
        assert_eq!(config.sanitize_mode, MermaidSanitizeMode::Strict);
        assert_eq!(config.theme, None);
    }

    #[test]
    fn mermaid_config_roundtrip_preserves_theme_overrides() {
        let mut config = MermaidConfig {
            theme: Some("corporate".to_string()),
            flowchart_direction: Some(GraphDirection::RL),
            flowchart_curve: Some("basis".to_string()),
            sequence_mirror_actors: Some(true),
            ..MermaidConfig::default()
        };
        config
            .theme_variables
            .insert("lineColor".into(), "#00ff00".into());

        let encoded = serde_json::to_string(&config).expect("serialize config");
        let decoded: MermaidConfig = serde_json::from_str(&encoded).expect("deserialize config");

        assert_eq!(decoded.theme.as_deref(), Some("corporate"));
        assert_eq!(decoded.flowchart_direction, Some(GraphDirection::RL));
        assert_eq!(decoded.flowchart_curve.as_deref(), Some("basis"));
        assert_eq!(decoded.sequence_mirror_actors, Some(true));
        assert_eq!(
            decoded.theme_variables.get("lineColor").map(String::as_str),
            Some("#00ff00")
        );
    }

    #[test]
    fn native_pressure_quantization_prefers_highest_observed_signal() {
        let report = MermaidNativePressureSignals {
            cpu_pressure_permille: Some(410),
            memory_pressure_permille: Some(880),
            io_pressure_permille: Some(300),
            available_parallelism: Some(8),
            rss_mib: Some(256),
        }
        .into_report();
        assert!(report.telemetry_available);
        assert_eq!(report.quantized_score_permille, 880);
        assert_eq!(report.tier, MermaidPressureTier::Critical);
    }

    #[test]
    fn wasm_pressure_quantization_uses_frame_and_worker_signals() {
        let report = MermaidWasmPressureSignals {
            frame_budget_ms: Some(16),
            frame_time_ms: Some(12),
            event_loop_lag_ms: Some(4),
            worker_saturation_permille: Some(720),
        }
        .into_report();
        assert!(report.telemetry_available);
        assert_eq!(report.source.as_str(), "wasm");
        assert_eq!(report.quantized_score_permille, 750);
        assert_eq!(report.tier, MermaidPressureTier::High);
    }

    #[test]
    fn unavailable_pressure_signal_produces_conservative_unknown_report() {
        let report = MermaidNativePressureSignals::default().into_report();
        assert!(!report.telemetry_available);
        assert!(report.conservative_fallback);
        assert_eq!(report.tier, MermaidPressureTier::Unknown);
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("telemetry unavailable"))
        );
    }

    #[test]
    fn budget_broker_rebalances_after_parse_and_tracks_exhaustion() {
        let pressure = MermaidNativePressureSignals {
            cpu_pressure_permille: Some(910),
            ..MermaidNativePressureSignals::default()
        }
        .into_report();
        let mut broker = crate::MermaidBudgetLedger::new(&pressure);

        assert_eq!(broker.total_budget_ms, 80);
        broker.record_parse(30);
        assert!(broker.parse.exceeded);
        assert!(broker.layout.allocated_ms > broker.render.allocated_ms);

        broker.record_layout(40);
        broker.record_render(20);
        assert!(broker.exhausted);
        assert!(broker.events.iter().any(|event| {
            event.kind == "rebalance" && event.stage.as_deref() == Some("layout")
        }));
        assert!(broker.events.iter().any(|event| {
            event.kind == "accounting" && event.note.as_deref() == Some("global budget exhausted")
        }));
    }

    #[test]
    fn budget_broker_events_capture_remaining_total_at_event_time() {
        let pressure = MermaidNativePressureSignals::default().into_report();
        let mut broker = crate::MermaidBudgetLedger::new(&pressure);

        assert_eq!(broker.events.len(), 4);
        assert!(
            broker
                .events
                .iter()
                .take(3)
                .all(|event| event.kind == "allocate" && event.remaining_total_ms == 120)
        );
        assert_eq!(broker.events[3].kind, "policy_note");
        assert_eq!(broker.events[3].remaining_total_ms, 120);

        broker.record_parse(24);

        let parse_consume = broker
            .events
            .iter()
            .find(|event| event.kind == "consume" && event.stage.as_deref() == Some("parse"))
            .expect("parse consume event should be emitted");
        assert_eq!(parse_consume.used_ms, Some(24));
        assert_eq!(parse_consume.remaining_total_ms, 96);

        let layout_rebalance = broker
            .events
            .iter()
            .find(|event| event.kind == "rebalance" && event.stage.as_deref() == Some("layout"))
            .expect("layout rebalance event should be emitted");
        assert_eq!(layout_rebalance.remaining_total_ms, 96);

        let accounting = broker
            .events
            .iter()
            .find(|event| event.kind == "accounting")
            .expect("accounting event should be emitted");
        assert_eq!(accounting.used_ms, Some(24));
        assert_eq!(accounting.remaining_total_ms, 96);
    }

    #[test]
    fn pressure_tier_boundary_quantization_is_deterministic() {
        // Exact boundary values for tier transitions
        let cases: Vec<(u16, bool, MermaidPressureTier)> = vec![
            (0, true, MermaidPressureTier::Nominal),
            (349, true, MermaidPressureTier::Nominal),
            (350, true, MermaidPressureTier::Elevated),
            (649, true, MermaidPressureTier::Elevated),
            (650, true, MermaidPressureTier::High),
            (849, true, MermaidPressureTier::High),
            (850, true, MermaidPressureTier::Critical),
            (1000, true, MermaidPressureTier::Critical),
            (500, false, MermaidPressureTier::Unknown),
        ];
        for (score, telemetry, expected) in cases {
            assert_eq!(
                MermaidPressureTier::from_quantized_score(score, telemetry),
                expected,
                "score={score}, telemetry={telemetry}"
            );
        }
    }

    #[test]
    fn budget_allocation_varies_deterministically_by_pressure_tier() {
        let tiers_and_budgets: Vec<(MermaidPressureTier, u64)> = vec![
            (MermaidPressureTier::Nominal, 250),
            (MermaidPressureTier::Elevated, 180),
            (MermaidPressureTier::High, 120),
            (MermaidPressureTier::Unknown, 120),
            (MermaidPressureTier::Critical, 80),
        ];
        for (tier, expected_total) in tiers_and_budgets {
            let pressure = MermaidPressureReport {
                tier,
                telemetry_available: true,
                ..MermaidPressureReport::default()
            };
            let broker = MermaidBudgetLedger::new(&pressure);
            assert_eq!(broker.total_budget_ms, expected_total, "tier={tier:?}");
            // parse + layout + render must sum to total
            let sum =
                broker.parse.allocated_ms + broker.layout.allocated_ms + broker.render.allocated_ms;
            assert_eq!(sum, expected_total, "stage sum mismatch for tier={tier:?}");
            // layout gets the lion's share
            assert!(
                broker.layout.allocated_ms >= broker.parse.allocated_ms,
                "layout should get more than parse for tier={tier:?}"
            );
        }
    }

    #[test]
    fn combined_high_cpu_and_memory_pressure_selects_worst_case_tier() {
        let report = MermaidNativePressureSignals {
            cpu_pressure_permille: Some(700),
            memory_pressure_permille: Some(860),
            io_pressure_permille: Some(200),
            available_parallelism: Some(8),
            rss_mib: Some(64),
        }
        .into_report();
        // 860 is the max signal → Critical
        assert_eq!(report.quantized_score_permille, 860);
        assert_eq!(report.tier, MermaidPressureTier::Critical);
    }

    #[test]
    fn all_stages_exceeding_budget_marks_global_exhaustion() {
        let pressure = MermaidNativePressureSignals {
            cpu_pressure_permille: Some(100),
            ..MermaidNativePressureSignals::default()
        }
        .into_report();
        let mut broker = MermaidBudgetLedger::new(&pressure);
        assert_eq!(broker.total_budget_ms, 250);

        // Exceed every stage
        broker.record_parse(200);
        assert!(broker.parse.exceeded);
        broker.record_layout(200);
        assert!(broker.layout.exceeded);
        broker.record_render(200);
        assert!(broker.render.exceeded);
        assert!(broker.exhausted);
        assert_eq!(broker.remaining_total_ms, 0);
        // Event trail records exhaustion
        assert!(
            broker
                .events
                .iter()
                .any(|e| e.kind == "accounting" && e.exceeded),
            "should have at least one exhaustion accounting event"
        );
    }

    #[test]
    fn rebalance_gives_layout_three_quarters_of_remaining_budget() {
        let pressure = MermaidPressureReport {
            tier: MermaidPressureTier::Nominal,
            telemetry_available: true,
            ..MermaidPressureReport::default()
        };
        let mut broker = MermaidBudgetLedger::new(&pressure);
        // Nominal: total=250, parse=50, layout=150, render=50
        assert_eq!(broker.total_budget_ms, 250);

        // Fast parse uses only 10ms
        broker.record_parse(10);
        // Remaining = 250 - 10 = 240
        // Render tail = ceil(240/4) = 60
        // Layout = 240 - 60 = 180
        assert_eq!(broker.layout.allocated_ms, 180);
        assert_eq!(broker.render.allocated_ms, 60);
    }

    #[test]
    fn scale_budget_handles_edge_cases() {
        // Zero allocated still returns at least 1
        assert_eq!(scale_budget(200, 0, 250), 1);
        // Equal to baseline returns default
        assert_eq!(scale_budget(200, 250, 250), 200);
        // Double baseline doubles budget
        assert_eq!(scale_budget(200, 500, 250), 400);
        // Very small allocation scales down proportionally
        assert_eq!(scale_budget(4000, 25, 250), 400);
    }

    #[test]
    fn should_simplify_render_triggers_on_high_pressure_or_low_render_budget() {
        // High pressure → simplify
        let pressure_high = MermaidPressureReport {
            tier: MermaidPressureTier::High,
            telemetry_available: true,
            ..MermaidPressureReport::default()
        };
        let broker_high = MermaidBudgetLedger::new(&pressure_high);
        assert!(broker_high.should_simplify_render());

        // Critical → simplify
        let pressure_crit = MermaidPressureReport {
            tier: MermaidPressureTier::Critical,
            telemetry_available: true,
            ..MermaidPressureReport::default()
        };
        let broker_crit = MermaidBudgetLedger::new(&pressure_crit);
        assert!(broker_crit.should_simplify_render());

        // Nominal → no simplification (render budget > 24ms)
        let pressure_nom = MermaidPressureReport {
            tier: MermaidPressureTier::Nominal,
            telemetry_available: true,
            ..MermaidPressureReport::default()
        };
        let broker_nom = MermaidBudgetLedger::new(&pressure_nom);
        assert!(!broker_nom.should_simplify_render());
    }

    #[test]
    fn budget_ledger_serializes_and_deserializes_roundtrip() {
        let pressure = MermaidNativePressureSignals {
            cpu_pressure_permille: Some(500),
            memory_pressure_permille: Some(300),
            io_pressure_permille: Some(100),
            available_parallelism: Some(4),
            rss_mib: Some(512),
        }
        .into_report();
        let mut broker = MermaidBudgetLedger::new(&pressure);
        broker.record_parse(20);
        broker.record_layout(80);
        broker.record_render(15);

        let json_str =
            serde_json::to_string(&broker).expect("budget ledger should serialize to JSON");
        let roundtrip: MermaidBudgetLedger =
            serde_json::from_str(&json_str).expect("budget ledger should deserialize from JSON");
        assert_eq!(roundtrip.total_budget_ms, broker.total_budget_ms);
        assert_eq!(roundtrip.exhausted, broker.exhausted);
        assert_eq!(roundtrip.pressure_tier, broker.pressure_tier);
        assert_eq!(roundtrip.events.len(), broker.events.len());
        assert_eq!(roundtrip.parse.used_ms, 20);
        assert_eq!(roundtrip.layout.used_ms, 80);
        assert_eq!(roundtrip.render.used_ms, 15);
    }

    #[test]
    fn budget_broker_within_budget_is_not_exhausted() {
        let pressure = MermaidPressureReport {
            tier: MermaidPressureTier::Nominal,
            telemetry_available: true,
            ..MermaidPressureReport::default()
        };
        let mut broker = MermaidBudgetLedger::new(&pressure);
        broker.record_parse(5);
        broker.record_layout(30);
        broker.record_render(10);
        assert!(!broker.exhausted);
        assert!(!broker.parse.exceeded);
        assert!(!broker.layout.exceeded);
        assert!(!broker.render.exceeded);
        assert!(broker.remaining_total_ms > 0);
    }

    #[test]
    fn wasm_frame_overrun_produces_high_pressure() {
        // frame_time 2x budget = 2000 permille, clamped to 1000
        let report = MermaidWasmPressureSignals {
            frame_budget_ms: Some(16),
            frame_time_ms: Some(32),
            event_loop_lag_ms: None,
            worker_saturation_permille: None,
        }
        .into_report();
        assert_eq!(report.quantized_score_permille, 1000);
        assert_eq!(report.tier, MermaidPressureTier::Critical);
    }

    #[test]
    fn wasm_event_loop_lag_scales_pressure() {
        // 10ms lag → 10*50 = 500 permille → Elevated
        let report = MermaidWasmPressureSignals {
            frame_budget_ms: None,
            frame_time_ms: None,
            event_loop_lag_ms: Some(10),
            worker_saturation_permille: None,
        }
        .into_report();
        assert_eq!(report.quantized_score_permille, 500);
        assert_eq!(report.tier, MermaidPressureTier::Elevated);
    }

    #[test]
    fn parallelism_single_core_produces_high_pressure() {
        let report = MermaidNativePressureSignals {
            available_parallelism: Some(1),
            ..MermaidNativePressureSignals::default()
        }
        .into_report();
        assert_eq!(report.quantized_score_permille, 900);
        assert_eq!(report.tier, MermaidPressureTier::Critical);
    }

    #[test]
    fn rss_high_memory_produces_pressure() {
        let report = MermaidNativePressureSignals {
            rss_mib: Some(4096),
            ..MermaidNativePressureSignals::default()
        }
        .into_report();
        // 4096 MiB → 920 permille → Critical
        assert_eq!(report.quantized_score_permille, 920);
        assert_eq!(report.tier, MermaidPressureTier::Critical);
    }

    #[test]
    fn degradation_plan_explain_no_degradation() {
        let plan = MermaidDegradationPlan::default();
        let explanation = plan.explain();
        assert_eq!(explanation.len(), 1);
        assert!(explanation[0].contains("full quality"));
    }

    #[test]
    fn degradation_plan_explain_lists_active_operators() {
        let plan = MermaidDegradationPlan {
            target_fidelity: MermaidFidelity::Compact,
            reduce_decoration: true,
            simplify_routing: true,
            force_glyph_mode: Some(MermaidGlyphMode::Ascii),
            ..MermaidDegradationPlan::default()
        };
        let explanation = plan.explain();
        assert!(explanation.iter().any(|l| l.contains("Decoration reduced")));
        assert!(explanation.iter().any(|l| l.contains("routing simplified")));
        assert!(explanation.iter().any(|l| l.contains("ASCII")));
        assert!(explanation.iter().any(|l| l.contains("Compact")));
        assert!(explanation.iter().any(|l| l.contains("Remediation")));
    }

    #[test]
    fn degradation_plan_is_degraded_detects_any_active_operator() {
        assert!(!MermaidDegradationPlan::default().is_degraded());
        assert!(
            MermaidDegradationPlan {
                reduce_decoration: true,
                ..MermaidDegradationPlan::default()
            }
            .is_degraded()
        );
        assert!(
            MermaidDegradationPlan {
                target_fidelity: MermaidFidelity::Compact,
                ..MermaidDegradationPlan::default()
            }
            .is_degraded()
        );
    }

    #[test]
    fn quality_mode_as_str_is_stable() {
        assert_eq!(MermaidQualityMode::QualityFirst.as_str(), "quality-first");
        assert_eq!(MermaidQualityMode::Auto.as_str(), "auto");
        assert_eq!(MermaidQualityMode::Balanced.as_str(), "balanced");
        assert_eq!(MermaidQualityMode::LatencyFirst.as_str(), "latency-first");
    }

    #[test]
    fn degradation_plan_default_is_no_degradation() {
        let plan = MermaidDegradationPlan::default();
        assert_eq!(plan.target_fidelity, MermaidFidelity::Normal);
        assert!(!plan.hide_labels);
        assert!(!plan.collapse_clusters);
        assert!(!plan.simplify_routing);
        assert!(!plan.reduce_decoration);
        assert!(plan.force_glyph_mode.is_none());
    }

    #[test]
    fn degradation_operator_ordinals_are_unique_and_ascending() {
        let ops = DegradationOperator::all_in_order();
        for pair in ops.windows(2) {
            assert!(
                pair[0].ordinal() < pair[1].ordinal(),
                "{:?} ordinal {} should be < {:?} ordinal {}",
                pair[0],
                pair[0].ordinal(),
                pair[1],
                pair[1].ordinal()
            );
        }
    }

    #[test]
    fn no_degradation_when_everything_is_nominal() {
        let ctx = DegradationContext {
            pressure_tier: MermaidPressureTier::Nominal,
            ..DegradationContext::default()
        };
        let (plan, applied) = crate::compute_degradation_plan_with_trace(&ctx);
        assert!(applied.is_empty());
        assert_eq!(plan.target_fidelity, MermaidFidelity::Normal);
        assert!(!plan.reduce_decoration);
        assert!(!plan.simplify_routing);
        assert!(!plan.hide_labels);
        assert!(!plan.collapse_clusters);
        assert!(plan.force_glyph_mode.is_none());
    }

    #[test]
    fn route_budget_exceeded_triggers_routing_and_decoration() {
        let ctx = DegradationContext {
            pressure_tier: MermaidPressureTier::Nominal,
            route_budget_exceeded: true,
            ..DegradationContext::default()
        };
        let (plan, applied) = crate::compute_degradation_plan_with_trace(&ctx);
        assert!(plan.simplify_routing);
        assert!(plan.reduce_decoration);
        assert_eq!(plan.target_fidelity, MermaidFidelity::Compact);
        assert!(applied.contains(&DegradationOperator::SimplifyRouting));
        assert!(applied.contains(&DegradationOperator::ReduceDecoration));
    }

    #[test]
    fn high_pressure_alone_triggers_decoration_reduction() {
        let ctx = DegradationContext {
            pressure_tier: MermaidPressureTier::High,
            ..DegradationContext::default()
        };
        let (plan, applied) = crate::compute_degradation_plan_with_trace(&ctx);
        assert!(plan.reduce_decoration);
        assert!(applied.contains(&DegradationOperator::ReduceDecoration));
        // No budget exceeded → no other operators
        assert!(!plan.simplify_routing);
        assert_eq!(plan.target_fidelity, MermaidFidelity::Normal);
    }

    #[test]
    fn critical_pressure_with_time_exceeded_drops_to_outline() {
        let ctx = DegradationContext {
            pressure_tier: MermaidPressureTier::Critical,
            time_budget_exceeded: true,
            node_limit_exceeded: true,
            ..DegradationContext::default()
        };
        let (plan, applied) = crate::compute_degradation_plan_with_trace(&ctx);
        assert_eq!(plan.target_fidelity, MermaidFidelity::Outline);
        assert!(plan.reduce_decoration);
        assert!(plan.hide_labels);
        assert!(plan.collapse_clusters);
        assert!(applied.contains(&DegradationOperator::DowngradeToOutline));
    }

    #[test]
    fn degradation_operator_application_is_deterministic() {
        let ctx = DegradationContext {
            pressure_tier: MermaidPressureTier::High,
            route_budget_exceeded: true,
            layout_budget_exceeded: true,
            time_budget_exceeded: true,
            node_limit_exceeded: true,
            edge_limit_exceeded: true,
        };
        let (plan1, applied1) = crate::compute_degradation_plan_with_trace(&ctx);
        let (plan2, applied2) = crate::compute_degradation_plan_with_trace(&ctx);
        assert_eq!(plan1, plan2);
        assert_eq!(applied1, applied2);
    }

    #[test]
    fn degradation_operators_apply_in_canonical_order() {
        let ctx = DegradationContext {
            pressure_tier: MermaidPressureTier::Critical,
            route_budget_exceeded: true,
            time_budget_exceeded: true,
            node_limit_exceeded: true,
            ..DegradationContext::default()
        };
        let (_, applied) = crate::compute_degradation_plan_with_trace(&ctx);
        for pair in applied.windows(2) {
            assert!(
                pair[0].ordinal() < pair[1].ordinal(),
                "operators should be applied in ordinal order"
            );
        }
    }

    #[test]
    fn guard_report_default_has_no_limits_exceeded() {
        let report = MermaidGuardReport::default();
        assert!(!report.node_limit_exceeded);
        assert!(!report.edge_limit_exceeded);
        assert!(!report.limits_exceeded);
        assert!(!report.budget_exceeded);
        assert!(!report.route_budget_exceeded);
        assert!(!report.layout_budget_exceeded);
    }

    #[test]
    fn mermaid_fallback_policy_defaults_match_contract() {
        let policy = MermaidFallbackPolicy::default();
        assert_eq!(policy.unsupported_diagram, MermaidFallbackAction::Error);
        assert_eq!(policy.unsupported_directive, MermaidFallbackAction::Warn);
        assert_eq!(policy.unsupported_style, MermaidFallbackAction::Warn);
        assert_eq!(policy.unsupported_link, MermaidFallbackAction::Warn);
        assert_eq!(policy.unsupported_feature, MermaidFallbackAction::Warn);
    }

    #[test]
    fn parse_mermaid_js_config_requires_object_root() {
        let parsed = parse_mermaid_js_config_value(&json!("not-an-object"));
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(parsed.errors[0].field, "$");
        assert!(parsed.errors[0].message.contains("JSON object"));
    }

    #[test]
    fn parse_mermaid_js_config_accepts_case_insensitive_direction() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "flowchart": { "direction": "lr" }
        }));

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.config.flowchart_direction, Some(GraphDirection::LR));
    }

    #[test]
    fn parse_mermaid_js_config_emits_warning_for_start_on_load() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "startOnLoad": true
        }));

        assert!(parsed.errors.is_empty());
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.message.contains("startOnLoad"))
        );
    }

    #[test]
    fn parse_mermaid_js_config_maps_high_contrast_theme_to_palette() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "theme": "high-contrast"
        }));

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.config.palette, DiagramPalettePreset::HighContrast);
    }

    #[test]
    fn to_init_parse_converts_config_errors_to_parse_errors() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "theme": 123
        }));
        let init_parse = to_init_parse(parsed);

        assert_eq!(init_parse.errors.len(), 1);
        match &init_parse.errors[0] {
            MermaidError::Parse {
                message, expected, ..
            } => {
                assert!(message.contains("theme"));
                assert_eq!(expected, &vec!["a valid Mermaid config value".to_string()]);
            }
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn diagnostic_severity_string_and_emoji_mappings_are_stable() {
        let expectations = [
            (DiagnosticSeverity::Hint, "hint", "💡"),
            (DiagnosticSeverity::Info, "info", "ℹ️"),
            (DiagnosticSeverity::Warning, "warning", "⚠️"),
            (DiagnosticSeverity::Error, "error", "❌"),
        ];

        for (severity, expected_str, expected_emoji) in expectations {
            assert_eq!(severity.as_str(), expected_str);
            assert_eq!(severity.emoji(), expected_emoji);
        }
    }

    #[test]
    fn diagnostic_category_string_mappings_are_stable() {
        let expectations = [
            (DiagnosticCategory::Lexer, "lexer"),
            (DiagnosticCategory::Parser, "parser"),
            (DiagnosticCategory::Semantic, "semantic"),
            (DiagnosticCategory::Recovery, "recovery"),
            (DiagnosticCategory::Inference, "inference"),
            (DiagnosticCategory::Compatibility, "compatibility"),
        ];

        for (category, expected) in expectations {
            assert_eq!(category.as_str(), expected);
        }
    }

    #[test]
    fn diagnostic_with_related_records_location() {
        let related_span = sample_span(11, 2, 5);
        let diagnostic = Diagnostic::warning("duplicate node")
            .with_category(DiagnosticCategory::Semantic)
            .with_related("first definition", related_span);

        assert_eq!(diagnostic.related.len(), 1);
        assert_eq!(diagnostic.related[0].message, "first definition");
        assert_eq!(diagnostic.related[0].span, related_span);
    }

    #[test]
    fn structured_diagnostic_from_diagnostic_sets_coordinates() {
        let diagnostic = Diagnostic::warning("implicit edge recovered")
            .with_category(DiagnosticCategory::Recovery)
            .with_span(sample_span(12, 6, 9))
            .with_suggestion("specify explicit arrow type");

        let structured = StructuredDiagnostic::from_diagnostic(&diagnostic);
        assert_eq!(structured.error_code, "mermaid/diag/recovery");
        assert_eq!(structured.severity, "warning");
        assert_eq!(structured.source_line, Some(12));
        assert_eq!(structured.source_column, Some(6));
        assert_eq!(
            structured.remediation_hint.as_deref(),
            Some("specify explicit arrow type")
        );
    }

    #[test]
    fn structured_diagnostic_builder_methods_are_chainable() {
        let structured = StructuredDiagnostic::default()
            .with_rule_id("FM001")
            .with_confidence(0.92)
            .with_remediation_hint("replace invalid arrow");

        assert_eq!(structured.rule_id.as_deref(), Some("FM001"));
        assert_eq!(structured.confidence, Some(0.92));
        assert_eq!(
            structured.remediation_hint.as_deref(),
            Some("replace invalid arrow")
        );
    }

    #[test]
    fn structured_diagnostic_from_validation_error_has_no_expected_hint() {
        let validation_error = MermaidError::Validation {
            message: "invalid relationship cardinality".to_string(),
            span: sample_span(3, 8, 11),
        };
        let structured = StructuredDiagnostic::from_error(&validation_error);
        assert_eq!(structured.error_code, "mermaid/error/validation");
        assert_eq!(structured.severity, "error");
        assert_eq!(structured.remediation_hint, None);
    }

    #[test]
    fn ir_helpers_find_node_and_batch_add_diagnostics() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..Default::default()
        });
        ir.add_diagnostics(vec![
            Diagnostic::hint("hint"),
            Diagnostic::info("info"),
            Diagnostic::warning("warning"),
            Diagnostic::error("error"),
        ]);

        assert_eq!(ir.find_node_index("A"), Some(0));
        assert_eq!(ir.find_node("B").map(|node| node.id.as_str()), Some("B"));
        assert_eq!(ir.find_node_index("missing"), None);

        let counts = ir.diagnostic_counts();
        assert_eq!(counts.hints, 1);
        assert_eq!(counts.infos, 1);
        assert_eq!(counts.warnings, 1);
        assert_eq!(counts.errors, 1);
        assert_eq!(counts.total(), 4);
    }

    #[test]
    fn ir_roundtrip_single_node_and_edge() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(IrLabel {
            text: "hello".to_string(),
            span: sample_span(1, 1, 5),
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            label: Some(IrLabelId(0)),
            span: sample_span(2, 1, 6),
            er_notation: None,
            source_cardinality: None,
            target_cardinality: None,
            guard: None,
            action: None,
            inline_style: None,
        });

        let encoded = serde_json::to_string(&ir).expect("serialize ir");
        let decoded: MermaidDiagramIr = serde_json::from_str(&encoded).expect("deserialize ir");
        assert_eq!(decoded, ir);
    }

    #[test]
    fn ir_roundtrip_handles_large_node_count() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for index in 0..1_000 {
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                ..Default::default()
            });
        }

        let encoded = serde_json::to_string(&ir).expect("serialize large ir");
        let decoded: MermaidDiagramIr =
            serde_json::from_str(&encoded).expect("deserialize large ir");
        assert_eq!(decoded.nodes.len(), 1_000);
        assert_eq!(decoded, ir);
    }

    #[test]
    fn ir_node_members_support_er_attributes() {
        let node = IrNode {
            id: "User".to_string(),
            members: vec![
                IrEntityAttribute {
                    data_type: "int".to_string(),
                    name: "id".to_string(),
                    key: IrAttributeKey::Pk,
                    comment: Some("primary key".to_string()),
                },
                IrEntityAttribute {
                    data_type: "varchar".to_string(),
                    name: "name".to_string(),
                    key: IrAttributeKey::None,
                    comment: None,
                },
            ],
            ..Default::default()
        };

        assert_eq!(node.members.len(), 2);
        assert_eq!(node.members[0].key, IrAttributeKey::Pk);
        assert_eq!(node.members[1].name, "name");
    }

    #[test]
    fn ir_edge_supports_self_loop_and_port_endpoints() {
        let edge = IrEdge {
            from: IrEndpoint::Port(IrPortId(1)),
            to: IrEndpoint::Port(IrPortId(1)),
            arrow: ArrowType::Circle,
            label: Some(IrLabelId(3)),
            span: sample_span(6, 1, 9),
            er_notation: None,
            source_cardinality: None,
            target_cardinality: None,
            guard: None,
            action: None,
            inline_style: None,
        };

        assert_eq!(edge.from, edge.to);
        assert_eq!(edge.arrow, ArrowType::Circle);
        assert_eq!(edge.label, Some(IrLabelId(3)));
    }

    #[test]
    fn ir_cluster_supports_empty_and_single_member() {
        let empty = IrCluster {
            id: IrClusterId(0),
            title: None,
            members: Vec::new(),
            grid_span: 1,
            span: sample_span(1, 1, 1),
        };
        let single = IrCluster {
            id: IrClusterId(1),
            title: Some(IrLabelId(2)),
            members: vec![IrNodeId(9)],
            grid_span: 1,
            span: sample_span(4, 1, 4),
        };

        assert!(empty.members.is_empty());
        assert_eq!(single.members, vec![IrNodeId(9)]);
        assert_eq!(single.title, Some(IrLabelId(2)));
    }

    #[test]
    fn graph_ir_supports_typed_nodes_edges_clusters_and_subgraphs() {
        let subgraph = IrSubgraph {
            id: IrSubgraphId(0),
            key: "api".to_string(),
            title: Some(IrLabelId(0)),
            parent: None,
            children: vec![IrSubgraphId(1)],
            members: vec![IrNodeId(0), IrNodeId(1)],
            cluster: Some(IrClusterId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 3),
            direction: None,
        };
        let child = IrSubgraph {
            id: IrSubgraphId(1),
            key: "workers".to_string(),
            title: Some(IrLabelId(1)),
            parent: Some(IrSubgraphId(0)),
            children: Vec::new(),
            members: vec![IrNodeId(1)],
            cluster: Some(IrClusterId(1)),
            grid_span: 1,
            span: sample_span(2, 1, 3),
            direction: None,
        };
        let graph_node = IrGraphNode {
            node_id: IrNodeId(1),
            kind: IrNodeKind::Participant,
            clusters: vec![IrClusterId(0), IrClusterId(1)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(1)],
        };
        let graph_edge = IrGraphEdge {
            edge_id: 0,
            kind: IrEdgeKind::Message,
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            span: sample_span(3, 1, 5),
        };
        let graph_cluster = IrGraphCluster {
            cluster_id: IrClusterId(1),
            title: Some(IrLabelId(1)),
            members: vec![IrNodeId(1)],
            subgraph: Some(IrSubgraphId(1)),
            grid_span: 1,
            span: sample_span(2, 1, 3),
        };

        assert_eq!(subgraph.children, vec![IrSubgraphId(1)]);
        assert_eq!(child.parent, Some(IrSubgraphId(0)));
        assert_eq!(graph_node.kind, IrNodeKind::Participant);
        assert_eq!(graph_node.subgraphs.len(), 2);
        assert_eq!(graph_edge.kind, IrEdgeKind::Message);
        assert_eq!(graph_cluster.subgraph, Some(IrSubgraphId(1)));
    }

    #[test]
    fn graph_ir_query_helpers_return_expected_records() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: IrNodeKind::Generic,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0)],
        });
        ir.graph.edges.push(IrGraphEdge {
            edge_id: 0,
            kind: IrEdgeKind::Generic,
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            span: sample_span(1, 1, 1),
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 1),
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "root".to_string(),
            title: None,
            parent: None,
            children: Vec::new(),
            members: vec![IrNodeId(0)],
            cluster: Some(IrClusterId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 1),
            direction: None,
        });

        assert_eq!(
            ir.graph_node(IrNodeId(0)).map(|node| node.kind),
            Some(IrNodeKind::Generic)
        );
        assert_eq!(
            ir.graph.edge(0).map(|edge| edge.kind),
            Some(IrEdgeKind::Generic)
        );
        assert_eq!(
            ir.graph
                .cluster(IrClusterId(0))
                .and_then(|cluster| cluster.subgraph),
            Some(IrSubgraphId(0))
        );
        assert_eq!(
            ir.graph
                .first_subgraph_by_key("root")
                .map(|subgraph| subgraph.id),
            Some(IrSubgraphId(0))
        );
        assert_eq!(ir.graph.subgraphs_by_key("root").len(), 1);
        assert_eq!(ir.graph.root_subgraphs().len(), 1);
        assert_eq!(ir.graph.leaf_subgraphs().len(), 1);
        assert_eq!(
            ir.graph_subgraph(IrSubgraphId(0))
                .map(|subgraph| subgraph.key.as_str()),
            Some("root")
        );
        assert_eq!(ir.graph.node_clusters(IrNodeId(0)).len(), 1);
        assert_eq!(ir.graph.node_subgraphs(IrNodeId(0)).len(), 1);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn graph_ir_traversal_helpers_follow_hierarchy_deterministically() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: IrNodeKind::Generic,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0)],
        });
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(1),
            kind: IrNodeKind::Generic,
            clusters: vec![IrClusterId(0), IrClusterId(1)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(1)],
        });
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(2),
            kind: IrNodeKind::Generic,
            clusters: vec![IrClusterId(0), IrClusterId(1), IrClusterId(2)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(1), IrSubgraphId(2)],
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1), IrNodeId(2)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 1),
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(1),
            title: None,
            members: vec![IrNodeId(1), IrNodeId(2)],
            subgraph: Some(IrSubgraphId(1)),
            grid_span: 1,
            span: sample_span(2, 1, 1),
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(2),
            title: None,
            members: vec![IrNodeId(2)],
            subgraph: Some(IrSubgraphId(2)),
            grid_span: 1,
            span: sample_span(3, 1, 1),
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "root".to_string(),
            title: None,
            parent: None,
            children: vec![IrSubgraphId(1)],
            members: vec![IrNodeId(0), IrNodeId(1)],
            cluster: Some(IrClusterId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 1),
            direction: None,
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(1),
            key: "child".to_string(),
            title: None,
            parent: Some(IrSubgraphId(0)),
            children: vec![IrSubgraphId(2)],
            members: vec![IrNodeId(1), IrNodeId(2)],
            cluster: Some(IrClusterId(1)),
            grid_span: 1,
            span: sample_span(2, 1, 1),
            direction: None,
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(2),
            key: "leaf".to_string(),
            title: None,
            parent: Some(IrSubgraphId(1)),
            children: Vec::new(),
            members: vec![IrNodeId(2)],
            cluster: Some(IrClusterId(2)),
            grid_span: 1,
            span: sample_span(3, 1, 1),
            direction: None,
        });

        assert_eq!(
            ir.graph
                .subgraph_ancestors(IrSubgraphId(2))
                .into_iter()
                .map(|subgraph| subgraph.key.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "child"]
        );
        assert_eq!(
            ir.graph
                .subgraph_descendants(IrSubgraphId(0))
                .into_iter()
                .map(|subgraph| subgraph.key.as_str())
                .collect::<Vec<_>>(),
            vec!["child", "leaf"]
        );
        assert_eq!(
            ir.graph
                .leaf_subgraphs()
                .into_iter()
                .map(|subgraph| subgraph.key.as_str())
                .collect::<Vec<_>>(),
            vec!["leaf"]
        );
        assert_eq!(
            ir.graph.subgraph_members_recursive(IrSubgraphId(0)),
            vec![IrNodeId(0), IrNodeId(1), IrNodeId(2)]
        );
        assert_eq!(
            ir.graph
                .node_clusters(IrNodeId(2))
                .into_iter()
                .map(|cluster| cluster.cluster_id)
                .collect::<Vec<_>>(),
            vec![IrClusterId(0), IrClusterId(1), IrClusterId(2)]
        );
        assert_eq!(
            ir.graph
                .node_subgraphs(IrNodeId(2))
                .into_iter()
                .map(|subgraph| subgraph.id)
                .collect::<Vec<_>>(),
            vec![IrSubgraphId(0), IrSubgraphId(1), IrSubgraphId(2)]
        );
    }

    #[test]
    fn graph_ir_serializes_and_deserializes_without_losing_hierarchy() {
        let mut graph = super::MermaidGraphIr::default();
        graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: IrNodeKind::Generic,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0)],
        });
        graph.edges.push(IrGraphEdge {
            edge_id: 0,
            kind: IrEdgeKind::Generic,
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            span: sample_span(1, 1, 1),
        });
        graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: Some(IrLabelId(0)),
            members: vec![IrNodeId(0)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 1),
        });
        graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "root".to_string(),
            title: Some(IrLabelId(0)),
            parent: None,
            children: Vec::new(),
            members: vec![IrNodeId(0)],
            cluster: Some(IrClusterId(0)),
            grid_span: 1,
            span: sample_span(1, 1, 1),
            direction: None,
        });

        let json = serde_json::to_string(&graph).expect("graph IR should serialize");
        let round_trip: super::MermaidGraphIr =
            serde_json::from_str(&json).expect("graph IR should deserialize");

        assert_eq!(round_trip, graph);
        assert_eq!(
            round_trip
                .first_subgraph_by_key("root")
                .map(|subgraph| subgraph.id),
            Some(IrSubgraphId(0))
        );
        assert_eq!(round_trip.subgraphs_by_key("root").len(), 1);
        assert_eq!(
            round_trip.subgraph_members_recursive(IrSubgraphId(0)),
            vec![IrNodeId(0)]
        );
    }

    #[test]
    fn endpoint_resolution_and_graph_adjacency_helpers_handle_ports() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.ports.push(IrPort {
            node: IrNodeId(1),
            name: "out".to_string(),
            side_hint: IrPortSideHint::Horizontal,
            span: sample_span(1, 1, 1),
        });
        ir.ports.push(IrPort {
            node: IrNodeId(2),
            name: "in".to_string(),
            side_hint: IrPortSideHint::Horizontal,
            span: sample_span(1, 1, 1),
        });
        ir.graph.edges.push(IrGraphEdge {
            edge_id: 0,
            kind: IrEdgeKind::Generic,
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Port(IrPortId(0)),
            span: sample_span(1, 1, 1),
        });
        ir.graph.edges.push(IrGraphEdge {
            edge_id: 1,
            kind: IrEdgeKind::Generic,
            from: IrEndpoint::Port(IrPortId(0)),
            to: IrEndpoint::Port(IrPortId(1)),
            span: sample_span(1, 1, 1),
        });
        ir.graph.edges.push(IrGraphEdge {
            edge_id: 2,
            kind: IrEdgeKind::Generic,
            from: IrEndpoint::Node(IrNodeId(2)),
            to: IrEndpoint::Node(IrNodeId(0)),
            span: sample_span(1, 1, 1),
        });

        assert_eq!(
            IrEndpoint::Port(IrPortId(0)).resolved_node_id(&ir.ports),
            Some(IrNodeId(1))
        );
        assert_eq!(
            ir.resolve_endpoint_node(IrEndpoint::Port(IrPortId(1))),
            Some(IrNodeId(2))
        );
        assert_eq!(ir.graph_incident_edges(IrNodeId(0)).len(), 2);
        assert_eq!(ir.graph_outgoing_edges(IrNodeId(1)).len(), 1);
        assert_eq!(ir.graph_incoming_edges(IrNodeId(2)).len(), 1);
        assert_eq!(
            ir.graph_neighbors(IrNodeId(0)),
            vec![IrNodeId(1), IrNodeId(2)]
        );
        assert_eq!(
            ir.graph_neighbors(IrNodeId(1)),
            vec![IrNodeId(0), IrNodeId(2)]
        );
    }

    #[test]
    fn empty_ir_meta_matches_requested_diagram_type() {
        let ir = MermaidDiagramIr::empty(DiagramType::Class);
        assert_eq!(ir.diagram_type, DiagramType::Class);
        assert_eq!(ir.meta.diagram_type, DiagramType::Class);
        assert_eq!(ir.meta.direction, GraphDirection::TB);
        assert_eq!(ir.meta.support_level, MermaidSupportLevel::Supported);
    }

    #[test]
    fn diagram_type_support_contract_matches_surface_expectations() {
        assert_eq!(
            DiagramType::Flowchart.support_level(),
            MermaidSupportLevel::Supported
        );
        assert_eq!(DiagramType::Flowchart.support_label(), "full");

        assert_eq!(
            DiagramType::GitGraph.support_level(),
            MermaidSupportLevel::Supported
        );
        assert_eq!(DiagramType::GitGraph.support_label(), "full");

        assert_eq!(
            DiagramType::C4Context.support_level(),
            MermaidSupportLevel::Supported
        );
        assert_eq!(DiagramType::C4Context.support_label(), "full");

        assert_eq!(
            DiagramType::ArchitectureBeta.support_level(),
            MermaidSupportLevel::Supported
        );
        assert_eq!(DiagramType::ArchitectureBeta.support_label(), "full");

        assert_eq!(
            DiagramType::Unknown.support_level(),
            MermaidSupportLevel::Unsupported
        );
        assert_eq!(DiagramType::Unknown.support_label(), "unknown");
    }

    #[test]
    fn capability_matrix_is_deterministic_and_has_versioned_claims() {
        let first = capability_matrix();
        let second = capability_matrix();

        assert_eq!(first, second);
        assert_eq!(first.schema_version, MERMAID_SCHEMA_VERSION);
        assert_eq!(first.project, "frankenmermaid");
        assert!(first.claims.len() >= documented_diagram_types().len());
        assert!(first.status_counts.contains_key("implemented"));
    }

    #[test]
    fn layout_guard_observability_is_deterministic_and_uses_kernel_types() {
        let (_cx, first) =
            mermaid_layout_guard_observability("cli.render", "flowchart LR\nA-->B", "sugiyama", 25);
        let (_cx, second) =
            mermaid_layout_guard_observability("cli.render", "flowchart LR\nA-->B", "sugiyama", 25);

        assert_eq!(first, second);
        assert_eq!(first.schema_version, MERMAID_SCHEMA_VERSION);
        assert_eq!(first.policy_id.name(), "fm.layout.guard");
        assert_eq!(first.policy_id.version(), 1);
        assert_ne!(first.trace_id.as_u128(), 0);
        assert_ne!(first.decision_id.as_u128(), 0);
    }

    #[test]
    fn capability_matrix_json_matches_checked_in_artifact() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let artifact_path = manifest_dir.join("../../evidence/capability_matrix.json");

        let actual = capability_matrix_json_pretty().expect("matrix JSON should serialize");
        if std::env::var("BLESS").is_ok() {
            std::fs::write(&artifact_path, &actual).unwrap();
        }

        let expected = std::fs::read_to_string(&artifact_path)
            .expect("capability matrix artifact should exist");

        assert_eq!(actual, expected);
    }

    #[test]
    fn readme_supported_diagram_types_block_matches_generated_markdown() {
        let actual = capability_readme_supported_diagram_types_markdown();

        if std::env::var("BLESS").is_ok() {
            let mut readme = load_readme();
            let start_marker = "<!-- BEGIN GENERATED: supported-diagram-types -->";
            let end_marker = "<!-- END GENERATED: supported-diagram-types -->";
            if let Some(start) = readme.find(start_marker)
                && let Some(rest) = readme.get(start..)
                && let Some(end) = rest.find(end_marker)
            {
                let full_start = start + start_marker.len();
                let full_end = start + end;
                readme.replace_range(full_start..full_end, &format!("\n{actual}\n"));
                let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let readme_path = manifest_dir.join("../../README.md");
                std::fs::write(&readme_path, readme).unwrap();
            }
        }

        let readme = load_readme();
        let expected = extract_generated_readme_block(&readme, "supported-diagram-types");
        assert_eq!(
            actual, expected,
            "README supported diagram types block drifted from capability source of truth"
        );
    }

    #[test]
    fn readme_runtime_capability_metadata_block_matches_generated_markdown() {
        let actual = capability_readme_surface_markdown();

        if std::env::var("BLESS").is_ok() {
            let mut readme = load_readme();
            let start_marker = "<!-- BEGIN GENERATED: runtime-capability-metadata -->";
            let end_marker = "<!-- END GENERATED: runtime-capability-metadata -->";
            if let Some(start) = readme.find(start_marker)
                && let Some(rest) = readme.get(start..)
                && let Some(end) = rest.find(end_marker)
            {
                let full_start = start + start_marker.len();
                let full_end = start + end;
                readme.replace_range(full_start..full_end, &format!("\n{actual}\n"));
            } else {
                readme.push('\n');
                readme.push_str(start_marker);
                readme.push('\n');
                readme.push_str(&actual);
                readme.push('\n');
                readme.push_str(end_marker);
                readme.push('\n');
            }
            let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let readme_path = manifest_dir.join("../../README.md");
            std::fs::write(&readme_path, readme).unwrap();
        }

        let readme = load_readme();
        let expected = extract_generated_readme_block(&readme, "runtime-capability-metadata");

        assert_eq!(
            actual, expected,
            "README runtime capability metadata block drifted from capability source of truth"
        );
    }

    fn load_readme() -> String {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let readme_path = manifest_dir.join("../../README.md");
        std::fs::read_to_string(&readme_path).expect("README should exist")
    }

    fn extract_generated_readme_block(readme: &str, block_name: &str) -> String {
        let start_marker = format!("<!-- BEGIN GENERATED: {block_name} -->");
        let end_marker = format!("<!-- END GENERATED: {block_name} -->");
        let start = readme
            .find(&start_marker)
            .unwrap_or_else(|| panic!("missing start marker for {block_name}"));
        let body_start = start + start_marker.len();
        let end = readme
            .get(body_start..)
            .and_then(|s| s.find(&end_marker))
            .map_or_else(
                || panic!("missing end marker for {block_name}"),
                |offset| body_start + offset,
            );

        readme.get(body_start..end).unwrap_or("").trim().to_string()
    }

    // ── IrSequenceMeta tests ───────────────────────────────────────────

    #[test]
    fn sequence_meta_default_is_empty() {
        let meta = IrSequenceMeta::default();
        assert!(!meta.autonumber);
        assert_eq!(meta.autonumber_start, 1);
        assert_eq!(meta.autonumber_increment, 1);
        assert!(!meta.hide_footbox);
        assert!(meta.activations.is_empty());
        assert!(meta.notes.is_empty());
        assert!(meta.fragments.is_empty());
        assert!(meta.participant_groups.is_empty());
        assert!(meta.lifecycle_events.is_empty());
    }

    #[test]
    fn sequence_meta_serde_round_trip() {
        let meta = IrSequenceMeta {
            autonumber: true,
            autonumber_start: 10,
            autonumber_increment: 5,
            hide_footbox: true,
            activations: vec![IrActivation {
                participant: IrNodeId(0),
                start_edge: 1,
                end_edge: 3,
                depth: 0,
            }],
            notes: vec![IrSequenceNote {
                position: NotePosition::RightOf,
                participants: vec![IrNodeId(0)],
                text: "important".to_string(),
                after_edge: 0,
            }],
            fragments: vec![IrSequenceFragment {
                kind: FragmentKind::Alt,
                label: "condition".to_string(),
                color: None,
                start_edge: 0,
                end_edge: 4,
                alternatives: vec![FragmentAlternative {
                    label: "else".to_string(),
                    start_edge: 2,
                    end_edge: 4,
                }],
                children: vec![],
            }],
            participant_groups: vec![IrParticipantGroup {
                label: "Backend".to_string(),
                color: Some("#aaf".to_string()),
                participants: vec![IrNodeId(1), IrNodeId(2)],
            }],
            lifecycle_events: vec![IrLifecycleEvent {
                kind: LifecycleEventKind::Create,
                participant: IrNodeId(3),
                at_edge: 5,
            }],
        };

        let json = serde_json::to_string(&meta).expect("serialize");
        assert!(json.contains("autonumber_start"));
        assert!(json.contains("autonumber_increment"));
        assert!(json.contains("hide_footbox"));
        let deser: IrSequenceMeta = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(meta, deser);
    }

    #[test]
    fn sequence_meta_empty_vecs_skipped_in_json() {
        let meta = IrSequenceMeta::default();
        let json = serde_json::to_string(&meta).expect("serialize");
        // Empty vecs should be skipped, only autonumber present
        assert!(!json.contains("activations"));
        assert!(!json.contains("autonumber_start"));
        assert!(!json.contains("autonumber_increment"));
        assert!(!json.contains("hide_footbox"));
        assert!(!json.contains("notes"));
        assert!(!json.contains("fragments"));
        assert!(!json.contains("participant_groups"));
        assert!(!json.contains("lifecycle_events"));
    }

    #[test]
    fn ir_with_sequence_meta_round_trip() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.sequence_meta = Some(IrSequenceMeta {
            autonumber: true,
            autonumber_start: 3,
            autonumber_increment: 2,
            ..Default::default()
        });

        let json = serde_json::to_string(&ir).expect("serialize");
        assert!(json.contains("sequence_meta"));
        let deser: MermaidDiagramIr = serde_json::from_str(&json).expect("deserialize");
        assert!(deser.sequence_meta.is_some());
        assert!(deser.sequence_meta.as_ref().unwrap().autonumber);
        assert_eq!(deser.sequence_meta.as_ref().unwrap().autonumber_start, 3);
        assert_eq!(
            deser.sequence_meta.as_ref().unwrap().autonumber_increment,
            2
        );
    }

    #[test]
    fn sequence_meta_autonumber_value_uses_start_and_increment() {
        let meta = IrSequenceMeta {
            autonumber: true,
            autonumber_start: 10,
            autonumber_increment: 5,
            ..Default::default()
        };

        assert_eq!(meta.autonumber_value(0), Some(10));
        assert_eq!(meta.autonumber_value(1), Some(15));
        assert_eq!(meta.autonumber_value(2), Some(20));
    }

    #[test]
    fn ir_without_sequence_meta_omits_field() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let json = serde_json::to_string(&ir).expect("serialize");
        assert!(!json.contains("sequence_meta"));
    }

    #[test]
    fn gantt_meta_serde_round_trip() {
        let meta = IrGanttMeta {
            title: Some("Release Train".to_string()),
            date_format: Some("YYYY-MM-DD".to_string()),
            axis_format: Some("%m/%d".to_string()),
            tick_interval: Some(GanttTickInterval::Week),
            today_marker_style: Some("stroke:#f97316,stroke-width:2px".to_string()),
            inclusive_end_dates: true,
            weekday_start: Some(1),
            excludes: vec![GanttExclude::Weekends],
            sections: vec![IrGanttSection {
                name: "Planning".to_string(),
            }],
            tasks: vec![IrGanttTask {
                node: IrNodeId(0),
                section_idx: 0,
                meta: "done, plan_1, 2026-02-01, 2d".to_string(),
                task_id: Some("plan_1".to_string()),
                start: Some(GanttDate::Absolute("2026-02-01".to_string())),
                end: Some(GanttDate::DurationDays(2)),
                depends_on: Vec::new(),
                progress: Some(0.5),
                task_type: GanttTaskType::Done,
            }],
        };

        let json = serde_json::to_string(&meta).expect("serialize");
        let deser: IrGanttMeta = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(meta, deser);
    }

    #[test]
    fn ir_with_gantt_meta_round_trip() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        ir.gantt_meta = Some(IrGanttMeta {
            sections: vec![IrGanttSection {
                name: "Delivery".to_string(),
            }],
            tasks: vec![IrGanttTask {
                node: IrNodeId(0),
                section_idx: 0,
                task_id: Some("ship".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        });

        let json = serde_json::to_string(&ir).expect("serialize");
        assert!(json.contains("gantt_meta"));
        let deser: MermaidDiagramIr = serde_json::from_str(&json).expect("deserialize");
        assert!(deser.gantt_meta.is_some());
        assert_eq!(deser.gantt_meta.unwrap().sections[0].name, "Delivery");
    }

    #[test]
    fn xy_chart_meta_serde_round_trip() {
        let meta = IrXyChartMeta {
            title: Some("Revenue".to_string()),
            x_axis: IrXyAxis {
                categories: vec!["Jan".to_string(), "Feb".to_string(), "Mar".to_string()],
                ..Default::default()
            },
            y_axis: IrXyAxis {
                label: Some("USD".to_string()),
                min: Some(0.0),
                max: Some(100.0),
                ..Default::default()
            },
            series: vec![
                IrXySeries {
                    kind: IrXySeriesKind::Bar,
                    name: Some("Actual".to_string()),
                    values: vec![30.0, 50.0, 70.0],
                    nodes: vec![IrNodeId(0), IrNodeId(1), IrNodeId(2)],
                },
                IrXySeries {
                    kind: IrXySeriesKind::Line,
                    name: Some("Target".to_string()),
                    values: vec![40.0, 55.0, 80.0],
                    nodes: vec![IrNodeId(3), IrNodeId(4), IrNodeId(5)],
                },
            ],
        };

        let json = serde_json::to_string(&meta).expect("serialize");
        let deser: IrXyChartMeta = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(meta, deser);
    }

    #[test]
    fn ir_with_xy_chart_meta_round_trip() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::XyChart);
        ir.xy_chart_meta = Some(IrXyChartMeta {
            x_axis: IrXyAxis {
                categories: vec!["Q1".to_string(), "Q2".to_string()],
                ..Default::default()
            },
            series: vec![IrXySeries {
                kind: IrXySeriesKind::Bar,
                values: vec![10.0, 12.0],
                nodes: vec![IrNodeId(0), IrNodeId(1)],
                ..Default::default()
            }],
            ..Default::default()
        });

        let json = serde_json::to_string(&ir).expect("serialize");
        assert!(json.contains("xy_chart_meta"));
        let deser: MermaidDiagramIr = serde_json::from_str(&json).expect("deserialize");
        assert!(deser.xy_chart_meta.is_some());
        assert_eq!(
            deser.xy_chart_meta.unwrap().x_axis.categories,
            vec!["Q1".to_string(), "Q2".to_string()]
        );
    }

    #[test]
    fn fragment_kind_variants() {
        for kind in [
            FragmentKind::Loop,
            FragmentKind::Alt,
            FragmentKind::Opt,
            FragmentKind::Par,
            FragmentKind::Critical,
            FragmentKind::Break,
            FragmentKind::Rect,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let deser: FragmentKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(kind, deser);
        }
    }

    #[test]
    fn note_position_variants() {
        for pos in [
            NotePosition::LeftOf,
            NotePosition::RightOf,
            NotePosition::Over,
        ] {
            let json = serde_json::to_string(&pos).expect("serialize");
            let deser: NotePosition = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(pos, deser);
        }
    }

    #[test]
    fn lifecycle_event_kind_variants() {
        for kind in [LifecycleEventKind::Create, LifecycleEventKind::Destroy] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let deser: LifecycleEventKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(kind, deser);
        }
    }

    // ── Graph IR operations tests (bd-1c5.6) ──────────────────────────

    #[test]
    fn ir_empty_has_no_nodes_or_edges() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        assert!(ir.nodes.is_empty());
        assert!(ir.edges.is_empty());
        assert!(ir.clusters.is_empty());
        assert!(ir.labels.is_empty());
        assert!(ir.ports.is_empty());
        assert!(ir.graph.nodes.is_empty());
        assert!(ir.graph.edges.is_empty());
        assert!(ir.graph.clusters.is_empty());
        assert!(ir.graph.subgraphs.is_empty());
    }

    #[test]
    fn ir_add_nodes_and_query() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            shape: NodeShape::Rect,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            shape: NodeShape::Diamond,
            ..IrNode::default()
        });
        assert_eq!(ir.nodes.len(), 2);
        assert_eq!(ir.nodes[0].id, "A");
        assert_eq!(ir.nodes[0].shape, NodeShape::Rect);
        assert_eq!(ir.nodes[1].id, "B");
        assert_eq!(ir.nodes[1].shape, NodeShape::Diamond);
    }

    #[test]
    fn ir_add_edges_with_endpoints() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        assert_eq!(ir.edges.len(), 1);
        assert_eq!(ir.edges[0].from, IrEndpoint::Node(IrNodeId(0)));
        assert_eq!(ir.edges[0].to, IrEndpoint::Node(IrNodeId(1)));
        assert_eq!(ir.edges[0].arrow, ArrowType::Arrow);
    }

    #[test]
    fn ir_endpoint_resolution() {
        let ports = vec![IrPort {
            node: IrNodeId(2),
            name: "port1".to_string(),
            ..IrPort::default()
        }];
        assert_eq!(
            IrEndpoint::Node(IrNodeId(5)).resolved_node_id(&ports),
            Some(IrNodeId(5))
        );
        assert_eq!(
            IrEndpoint::Port(IrPortId(0)).resolved_node_id(&ports),
            Some(IrNodeId(2))
        );
        assert_eq!(IrEndpoint::Unresolved.resolved_node_id(&ports), None);
        assert_eq!(
            IrEndpoint::Port(IrPortId(99)).resolved_node_id(&ports),
            None
        );
    }

    #[test]
    fn ir_labels_interning() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            ..IrLabel::default()
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            ..IrLabel::default()
        });

        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: Some(IrLabelId(1)),
            ..IrNode::default()
        });

        assert_eq!(ir.labels[ir.nodes[0].label.unwrap().0].text, "Start");
        assert_eq!(ir.labels[ir.nodes[1].label.unwrap().0].text, "End");
    }

    #[test]
    fn ir_cluster_members() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "C".to_string(),
            ..IrNode::default()
        });

        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 0,
            span: Span::default(),
        });
        assert_eq!(ir.clusters[0].members.len(), 2);
        assert_eq!(ir.clusters[0].members[0], IrNodeId(0));
        assert_eq!(ir.clusters[0].members[1], IrNodeId(1));
    }

    #[test]
    fn ir_subgraph_parent_child_hierarchy() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "root".to_string(),
            parent: None,
            children: vec![IrSubgraphId(1)],
            ..IrSubgraph::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(1),
            key: "child".to_string(),
            parent: Some(IrSubgraphId(0)),
            children: vec![],
            ..IrSubgraph::default()
        });

        let roots = ir.graph.root_subgraphs();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].key, "root");

        let leaves = ir.graph.leaf_subgraphs();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].key, "child");
    }

    #[test]
    fn graph_ir_node_query_by_id() {
        let mut graph = super::MermaidGraphIr::default();
        graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: IrNodeKind::Generic,
            clusters: vec![],
            subgraphs: vec![],
        });
        graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(1),
            kind: IrNodeKind::Entity,
            clusters: vec![],
            subgraphs: vec![],
        });
        assert_eq!(graph.node(IrNodeId(0)).unwrap().kind, IrNodeKind::Generic);
        assert_eq!(graph.node(IrNodeId(1)).unwrap().kind, IrNodeKind::Entity);
        assert!(graph.node(IrNodeId(99)).is_none());
    }

    #[test]
    fn graph_ir_edge_query() {
        let mut graph = super::MermaidGraphIr::default();
        graph.edges.push(IrGraphEdge {
            edge_id: 0,
            kind: IrEdgeKind::Generic,
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            span: Span::default(),
        });
        assert_eq!(graph.edge(0).unwrap().kind, IrEdgeKind::Generic);
        assert!(graph.edge(99).is_none());
    }

    #[test]
    fn graph_ir_cluster_query() {
        let mut graph = super::MermaidGraphIr::default();
        graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1)],
            subgraph: None,
            grid_span: 0,
            span: Span::default(),
        });
        let cluster = graph.cluster(IrClusterId(0)).unwrap();
        assert_eq!(cluster.members.len(), 2);
        assert!(graph.cluster(IrClusterId(99)).is_none());
    }

    #[test]
    fn graph_ir_subgraph_by_key() {
        let mut graph = super::MermaidGraphIr::default();
        graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "alpha".to_string(),
            ..IrSubgraph::default()
        });
        graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(1),
            key: "beta".to_string(),
            ..IrSubgraph::default()
        });
        graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(2),
            key: "alpha".to_string(),
            ..IrSubgraph::default()
        });

        let alpha = graph.subgraphs_by_key("alpha");
        assert_eq!(alpha.len(), 2);
        assert_eq!(
            graph.first_subgraph_by_key("beta").unwrap().id,
            IrSubgraphId(1)
        );
        assert!(graph.first_subgraph_by_key("missing").is_none());
    }

    #[test]
    fn graph_ir_node_clusters_and_subgraphs() {
        let mut graph = super::MermaidGraphIr::default();
        graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: IrNodeKind::Generic,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0)],
        });
        graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 0,
            span: Span::default(),
        });
        graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "sub1".to_string(),
            members: vec![IrNodeId(0)],
            cluster: Some(IrClusterId(0)),
            ..IrSubgraph::default()
        });

        let node_clusters = graph.node_clusters(IrNodeId(0));
        assert_eq!(node_clusters.len(), 1);
        let node_subgraphs = graph.node_subgraphs(IrNodeId(0));
        assert_eq!(node_subgraphs.len(), 1);
        assert_eq!(node_subgraphs[0].key, "sub1");
    }

    #[test]
    fn ir_serde_roundtrip_preserves_all_fields() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        ir.labels.push(IrLabel {
            text: "Hello".to_string(),
            ..IrLabel::default()
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Diamond,
            implicit: true,
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::DottedArrow,
            ..IrEdge::default()
        });

        let json = serde_json::to_string(&ir).expect("serialize");
        let deser: MermaidDiagramIr = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deser.diagram_type, DiagramType::Flowchart);
        assert_eq!(deser.direction, GraphDirection::LR);
        assert_eq!(deser.nodes.len(), 1);
        assert_eq!(deser.nodes[0].id, "A");
        assert_eq!(deser.nodes[0].shape, NodeShape::Diamond);
        assert!(deser.nodes[0].implicit);
        assert_eq!(deser.edges.len(), 1);
        assert_eq!(deser.edges[0].arrow, ArrowType::DottedArrow);
        assert_eq!(deser.labels.len(), 1);
        assert_eq!(deser.labels[0].text, "Hello");
    }

    #[test]
    fn ir_diagnostics_add_and_filter() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.add_diagnostic(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            category: DiagnosticCategory::Recovery,
            message: "auto-created node".to_string(),
            ..Diagnostic::default()
        });
        ir.add_diagnostic(Diagnostic {
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::Parser,
            message: "syntax error".to_string(),
            ..Diagnostic::default()
        });
        ir.add_diagnostic(Diagnostic {
            severity: DiagnosticSeverity::Info,
            category: DiagnosticCategory::Inference,
            message: "fuzzy match".to_string(),
            ..Diagnostic::default()
        });

        assert_eq!(ir.diagnostics.len(), 3);
        assert!(ir.has_errors());
        let warnings: Vec<_> = ir
            .diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "auto-created node");
    }

    #[test]
    fn ir_implicit_node_flag() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "Explicit".to_string(),
            implicit: false,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Implicit".to_string(),
            implicit: true,
            ..IrNode::default()
        });
        let implicit_count = ir.nodes.iter().filter(|n| n.implicit).count();
        assert_eq!(implicit_count, 1);
    }

    #[test]
    fn ir_edge_kinds_are_distinct() {
        let kinds = [
            IrEdgeKind::Generic,
            IrEdgeKind::Relationship,
            IrEdgeKind::Message,
            IrEdgeKind::Timeline,
            IrEdgeKind::Dependency,
            IrEdgeKind::Commit,
        ];
        for (i, a) in kinds.iter().enumerate() {
            for (j, b) in kinds.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn ir_node_kinds_are_distinct() {
        let kinds = [
            IrNodeKind::Generic,
            IrNodeKind::Entity,
            IrNodeKind::Participant,
            IrNodeKind::State,
            IrNodeKind::Task,
            IrNodeKind::Event,
            IrNodeKind::Commit,
            IrNodeKind::Requirement,
            IrNodeKind::Slice,
            IrNodeKind::Point,
        ];
        for (i, a) in kinds.iter().enumerate() {
            for (j, b) in kinds.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // ── Styling type tests ──────────────────────────────────────────

    #[test]
    fn parse_style_string_basic() {
        let style = parse_style_string("fill:#f9f,stroke:#333,stroke-width:4px");
        assert_eq!(style.properties.get("fill").unwrap(), "#f9f");
        assert_eq!(style.properties.get("stroke").unwrap(), "#333");
        assert_eq!(style.properties.get("stroke-width").unwrap(), "4px");
    }

    #[test]
    fn parse_style_string_semicolons() {
        let style = parse_style_string("fill: #fff; stroke: #000; opacity: 0.5");
        assert_eq!(style.properties.get("fill").unwrap(), "#fff");
        assert_eq!(style.properties.get("stroke").unwrap(), "#000");
        assert_eq!(style.properties.get("opacity").unwrap(), "0.5");
    }

    #[test]
    fn parse_style_string_preserves_parens() {
        let style = parse_style_string("fill:rgb(255,128,0),stroke:rgba(0,0,0,0.5)");
        assert_eq!(style.properties.get("fill").unwrap(), "rgb(255,128,0)");
        assert_eq!(style.properties.get("stroke").unwrap(), "rgba(0,0,0,0.5)");
    }

    #[test]
    fn parse_style_string_preserves_dasharray_commas() {
        let style = parse_style_string("stroke-dasharray:5,5,10,stroke:#333");
        assert_eq!(style.properties.get("stroke-dasharray").unwrap(), "5,5,10");
        assert_eq!(style.properties.get("stroke").unwrap(), "#333");
    }

    #[test]
    fn parse_style_string_with_rejections_reports_unknown_and_unsafe() {
        let (style, rejected) =
            parse_style_string_with_rejections("fill:#fff,shadow:2px,color:javascript:alert(1)");
        assert!(style.properties.contains_key("fill"));
        assert!(rejected.contains(&"shadow".to_string()));
        assert!(rejected.contains(&"color".to_string()));
    }

    #[test]
    fn parse_style_string_empty_input() {
        let style = parse_style_string("");
        assert!(style.is_empty());
    }

    #[test]
    fn parse_style_string_whitespace_only() {
        let style = parse_style_string("   ,  ;  ");
        assert!(style.is_empty());
    }

    #[test]
    fn sanitize_rejects_url() {
        assert!(sanitize_style_value("url(http://evil.com)").is_none());
    }

    #[test]
    fn sanitize_rejects_javascript() {
        assert!(sanitize_style_value("javascript:alert(1)").is_none());
    }

    #[test]
    fn sanitize_rejects_event_handlers() {
        assert!(sanitize_style_value("onclick=alert(1)").is_none());
        assert!(sanitize_style_value("onerror=fetch()").is_none());
    }

    #[test]
    fn sanitize_rejects_xml_injection() {
        assert!(sanitize_style_value("<script>").is_none());
        assert!(sanitize_style_value("val>ue").is_none());
    }

    #[test]
    fn sanitize_rejects_expression() {
        assert!(sanitize_style_value("expression(alert(1))").is_none());
    }

    #[test]
    fn sanitize_accepts_normal_values() {
        assert_eq!(sanitize_style_value("#f9f").unwrap(), "#f9f");
        assert_eq!(sanitize_style_value("4px").unwrap(), "4px");
        assert_eq!(
            sanitize_style_value("rgb(255,128,0)").unwrap(),
            "rgb(255,128,0)"
        );
        assert_eq!(sanitize_style_value("bold").unwrap(), "bold");
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize_style_value("  #fff  ").unwrap(), "#fff");
    }

    #[test]
    fn ir_inline_style_to_css_string() {
        let style = IrInlineStyle::from_pairs(vec![
            ("fill".to_string(), "#f9f".to_string()),
            ("stroke".to_string(), "#333".to_string()),
        ]);
        let css = style.to_css_string();
        assert!(css.contains("fill: #f9f"));
        assert!(css.contains("stroke: #333"));
    }

    #[test]
    fn ir_inline_style_from_pairs_sanitizes() {
        let style = IrInlineStyle::from_pairs(vec![
            ("fill".to_string(), "#fff".to_string()),
            ("bad".to_string(), "javascript:alert(1)".to_string()),
        ]);
        assert_eq!(style.properties.len(), 1);
        assert!(style.properties.contains_key("fill"));
    }

    #[test]
    fn ir_style_def_serde_roundtrip() {
        let def = IrStyleDef {
            name: "important".to_string(),
            properties: BTreeMap::from([
                ("fill".to_string(), "#f9f".to_string()),
                ("stroke".to_string(), "#333".to_string()),
            ]),
            span: Span::default(),
        };
        let json = serde_json::to_string(&def).unwrap();
        let deser: IrStyleDef = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, def);
    }

    #[test]
    fn ir_inline_style_serde_roundtrip() {
        let style = IrInlineStyle {
            properties: BTreeMap::from([("fill".to_string(), "#fff".to_string())]),
        };
        let json = serde_json::to_string(&style).unwrap();
        let deser: IrInlineStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, style);
    }

    #[test]
    fn populate_structured_styles_classdef() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            classes: vec!["important".to_string()],
            ..Default::default()
        });
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Class("important".to_string()),
            style: "fill:#f9f,stroke:#333".to_string(),
            span: Span::default(),
        });

        ir.populate_structured_styles();

        assert_eq!(ir.style_defs.len(), 1);
        assert_eq!(ir.style_defs[0].name, "important");
        assert_eq!(ir.style_defs[0].properties.get("fill").unwrap(), "#f9f");

        let node_style = ir.nodes[0].inline_style.as_ref().unwrap();
        assert_eq!(node_style.properties.get("fill").unwrap(), "#f9f");
        assert_eq!(node_style.properties.get("stroke").unwrap(), "#333");
    }

    #[test]
    fn populate_structured_styles_node_override() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            classes: vec!["cls".to_string()],
            ..Default::default()
        });
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Class("cls".to_string()),
            style: "fill:#fff".to_string(),
            span: Span::default(),
        });
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Node(IrNodeId(0)),
            style: "fill:#000".to_string(),
            span: Span::default(),
        });

        ir.populate_structured_styles();

        // Node-level style overrides classDef.
        let node_style = ir.nodes[0].inline_style.as_ref().unwrap();
        assert_eq!(node_style.properties.get("fill").unwrap(), "#000");
    }

    #[test]
    fn populate_structured_styles_link_default_and_specific() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::LinkDefault,
            style: "stroke:#aaa".to_string(),
            span: Span::default(),
        });
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Link(0),
            style: "stroke:#f00".to_string(),
            span: Span::default(),
        });

        ir.populate_structured_styles();

        let edge_style = ir.edges[0].inline_style.as_ref().unwrap();
        // Specific linkStyle overrides default.
        assert_eq!(edge_style.properties.get("stroke").unwrap(), "#f00");
    }

    #[test]
    fn populate_structured_styles_idempotent() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            classes: vec!["cls".to_string()],
            ..Default::default()
        });
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Class("cls".to_string()),
            style: "fill:#fff".to_string(),
            span: Span::default(),
        });

        ir.populate_structured_styles();
        let first = ir.nodes[0].inline_style.clone();
        let first_defs = ir.style_defs.clone();

        ir.populate_structured_styles();
        assert_eq!(ir.nodes[0].inline_style, first);
        assert_eq!(ir.style_defs, first_defs);
    }

    #[test]
    fn is_allowed_style_property_checks() {
        assert!(is_allowed_style_property("fill"));
        assert!(is_allowed_style_property("stroke-width"));
        assert!(is_allowed_style_property("font-size"));
        assert!(is_allowed_style_property("filter"));
        assert!(!is_allowed_style_property("display"));
        assert!(!is_allowed_style_property("position"));
    }

    #[test]
    fn parse_style_string_rejects_unsafe_values() {
        let style =
            parse_style_string("fill:url(http://evil.com),stroke:#333,color:javascript:alert(1)");
        // fill and color should be rejected, stroke should remain.
        assert!(!style.properties.contains_key("fill"));
        assert!(!style.properties.contains_key("color"));
        assert_eq!(style.properties.get("stroke").unwrap(), "#333");
    }

    #[test]
    fn node_inline_style_serializes_in_ir() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            inline_style: Some(IrInlineStyle {
                properties: BTreeMap::from([("fill".to_string(), "#abc".to_string())]),
            }),
            ..Default::default()
        });

        let json = serde_json::to_string(&ir).unwrap();
        let deser: MermaidDiagramIr = serde_json::from_str(&json).unwrap();
        let node_style = deser.nodes[0].inline_style.as_ref().unwrap();
        assert_eq!(node_style.properties.get("fill").unwrap(), "#abc");
    }

    // ── FxHash collection type tests ─────────────────────────────────

    #[test]
    fn node_map_and_set_work_with_sequential_ids() {
        let mut map: NodeMap<&str> = NodeMap::default();
        map.insert(IrNodeId(0), "alpha");
        map.insert(IrNodeId(1), "beta");
        map.insert(IrNodeId(2), "gamma");
        assert_eq!(map.len(), 3);
        assert_eq!(map.get(&IrNodeId(1)), Some(&"beta"));

        let mut set: NodeSet = NodeSet::default();
        set.insert(IrNodeId(0));
        set.insert(IrNodeId(5));
        assert!(set.contains(&IrNodeId(0)));
        assert!(!set.contains(&IrNodeId(3)));
    }

    #[test]
    fn edge_map_works_with_index_keys() {
        let mut map: EdgeMap<f32> = EdgeMap::default();
        map.insert(0, 1.5);
        map.insert(42, 2.7);
        assert_eq!(map.get(&0), Some(&1.5));
        assert_eq!(map.get(&42), Some(&2.7));
    }
}
