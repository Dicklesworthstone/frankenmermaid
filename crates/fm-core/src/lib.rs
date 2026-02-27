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
        ArrowType, Diagnostic, DiagnosticCategory, DiagnosticSeverity, DiagramPalettePreset,
        DiagramType, GraphDirection, IrAttributeKey, IrCluster, IrClusterId, IrEdge, IrEndpoint,
        IrEntityAttribute, IrLabel, IrLabelId, IrNode, IrNodeId, IrPortId, IrPortSideHint,
        MermaidConfig, MermaidDiagramIr, MermaidError, MermaidErrorCode, MermaidFallbackAction,
        MermaidFallbackPolicy, MermaidSanitizeMode, MermaidSupportLevel, MermaidWarningCode,
        NodeShape, Position, Span, StructuredDiagnostic, parse_mermaid_js_config_value,
        to_init_parse,
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
            (ArrowType::ThickArrow, "==>"),
            (ArrowType::DottedArrow, "-.->"),
            (ArrowType::Circle, "--o"),
            (ArrowType::Cross, "--x"),
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
            NodeShape::Asymmetric,
            NodeShape::Cylinder,
            NodeShape::Trapezoid,
            NodeShape::DoubleCircle,
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
            span: sample_span(1, 1, 1),
        };
        let single = IrCluster {
            id: IrClusterId(1),
            title: Some(IrLabelId(2)),
            members: vec![IrNodeId(9)],
            span: sample_span(4, 1, 4),
        };

        assert!(empty.members.is_empty());
        assert_eq!(single.members, vec![IrNodeId(9)]);
        assert_eq!(single.title, Some(IrLabelId(2)));
    }

    #[test]
    fn empty_ir_meta_matches_requested_diagram_type() {
        let ir = MermaidDiagramIr::empty(DiagramType::Class);
        assert_eq!(ir.diagram_type, DiagramType::Class);
        assert_eq!(ir.meta.diagram_type, DiagramType::Class);
        assert_eq!(ir.meta.direction, GraphDirection::TB);
        assert_eq!(ir.meta.support_level, MermaidSupportLevel::Supported);
    }
}
