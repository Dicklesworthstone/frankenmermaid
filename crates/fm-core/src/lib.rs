#![forbid(unsafe_code)]

mod font_metrics;

pub use font_metrics::{
    CharWidthClass, DiagnosticLevel, FontMetrics, FontMetricsConfig, FontMetricsDiagnostic,
    FontPreset,
};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    pub href: Option<String>,
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
                if !raw_value.is_boolean() {
                    push_type_error(&mut parsed, "startOnLoad", raw_value, "must be a boolean");
                }
                push_warning(
                    &mut parsed,
                    "Config key 'startOnLoad' is accepted but currently ignored".to_string(),
                );
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
        let (source_line, source_column) = diagnostic
            .span
            .map(|span| (Some(span.start.line), Some(span.start.col)))
            .unwrap_or((None, None));

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
    pub fn with_confidence(mut self, confidence: f32) -> Self {
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
    use serde_json::json;

    use super::{
        ArrowType, Diagnostic, DiagnosticCategory, DiagnosticSeverity, DiagramType, GraphDirection,
        MermaidDiagramIr, MermaidSanitizeMode, Span, StructuredDiagnostic,
        parse_mermaid_js_config_value, to_init_parse,
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
                "mirrorActors": true
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
        assert_eq!(parsed.config.sanitize_mode, MermaidSanitizeMode::Lenient);
    }

    #[test]
    fn mermaid_js_config_adapter_reports_unknown_and_type_issues() {
        let parsed = parse_mermaid_js_config_value(&json!({
            "theme": 42,
            "flowchart": "not-an-object",
            "sequence": { "mirrorActors": "yes" },
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
            "sequence": { "mirrorActors": false }
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
}
