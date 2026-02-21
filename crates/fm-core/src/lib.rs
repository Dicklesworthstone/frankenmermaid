#![forbid(unsafe_code)]

mod font_metrics;

pub use font_metrics::{
    CharWidthClass, DiagnosticLevel, FontMetrics, FontMetricsConfig, FontMetricsDiagnostic,
    FontPreset,
};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    pub fn code(&self) -> MermaidErrorCode {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrNodeId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrPortId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrLabelId(pub usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct IrClusterId(pub usize);

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
    Asymmetric,
    Cylinder,
    Trapezoid,
    DoubleCircle,
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
    ThickArrow,
    DottedArrow,
    Circle,
    Cross,
}

impl ArrowType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Line => "---",
            Self::Arrow => "-->",
            Self::ThickArrow => "==>",
            Self::DottedArrow => "-.->",
            Self::Circle => "--o",
            Self::Cross => "--x",
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrNode {
    pub id: String,
    pub label: Option<IrLabelId>,
    pub shape: NodeShape,
    pub classes: Vec<String>,
    pub span_primary: Span,
    pub span_all: Vec<Span>,
    pub implicit: bool,
    /// Entity attributes/members (for ER diagrams)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<IrEntityAttribute>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrEdge {
    pub from: IrEndpoint,
    pub to: IrEndpoint,
    pub arrow: ArrowType,
    pub label: Option<IrLabelId>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IrCluster {
    pub id: IrClusterId,
    pub title: Option<IrLabelId>,
    pub members: Vec<IrNodeId>,
    pub span: Span,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidInitParse {
    pub config: MermaidInitConfig,
    pub warnings: Vec<MermaidWarning>,
    pub errors: Vec<MermaidError>,
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
pub enum MermaidFidelity {
    Rich,
    #[default]
    Normal,
    Compact,
    Outline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MermaidDegradationPlan {
    pub target_fidelity: MermaidFidelity,
    pub hide_labels: bool,
    pub collapse_clusters: bool,
    pub simplify_routing: bool,
    pub reduce_decoration: bool,
    pub force_glyph_mode: Option<MermaidGlyphMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
    pub degradation: MermaidDegradationPlan,
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
    pub init: MermaidInitParse,
    pub theme_overrides: MermaidThemeOverrides,
    pub guard: MermaidGuardReport,
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
    pub fn with_category(mut self, category: DiagnosticCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the source span.
    #[must_use]
    pub fn with_span(mut self, span: Span) -> Self {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MermaidDiagramIr {
    pub diagram_type: DiagramType,
    pub direction: GraphDirection,
    pub nodes: Vec<IrNode>,
    pub edges: Vec<IrEdge>,
    pub ports: Vec<IrPort>,
    pub clusters: Vec<IrCluster>,
    pub labels: Vec<IrLabel>,
    pub constraints: Vec<IrConstraint>,
    pub meta: MermaidDiagramMeta,
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
            labels: Vec::new(),
            constraints: Vec::new(),
            meta: MermaidDiagramMeta {
                diagram_type,
                direction: GraphDirection::TB,
                support_level: MermaidSupportLevel::Supported,
                init: MermaidInitParse::default(),
                theme_overrides: MermaidThemeOverrides::default(),
                guard: MermaidGuardReport::default(),
            },
            diagnostics: Vec::new(),
        }
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

#[cfg(test)]
mod tests {
    use super::{
        ArrowType, Diagnostic, DiagnosticCategory, DiagnosticSeverity, DiagramType, GraphDirection,
        MermaidDiagramIr, Span,
    };

    #[test]
    fn creates_empty_ir() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        assert_eq!(ir.direction, GraphDirection::TB);
        assert_eq!(ir.nodes.len(), 0);
        assert_eq!(ir.edges.len(), 0);
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
}
