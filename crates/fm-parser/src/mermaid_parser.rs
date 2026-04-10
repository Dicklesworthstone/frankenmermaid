use std::collections::BTreeMap;

use chumsky::prelude::*;
use fm_core::{
    ArrowType, Diagnostic, DiagnosticCategory, DiagramType, GanttDate, GanttExclude, GanttTaskType,
    GanttTickInterval, GraphDirection, IrAttributeKey, IrC4NodeMeta, IrGanttMeta, IrGanttSection,
    IrGanttTask, IrLabelSegment, IrNodeId, IrXyAxis, IrXyChartMeta, IrXySeries, IrXySeriesKind,
    MermaidParseMode, MermaidSupportLevel, NodeShape, Span, parse_mermaid_js_config_value,
    to_init_parse,
};
use serde_json::Value;

use crate::{
    DetectedType, ParseResult, ParserConfig,
    ir_builder::{IrBuilder, ParsedLabel},
    is_sankey_header, matches_keyword_header, normalize_identifier,
};

const FLOW_OPERATORS: [(&str, ArrowType); 14] = [
    ("-.->", ArrowType::DottedArrow),
    ("<-.->", ArrowType::DoubleDottedArrow),
    ("-.-", ArrowType::DottedLine),
    ("==>", ArrowType::ThickArrow),
    ("<==>", ArrowType::DoubleThickArrow),
    ("-->", ArrowType::Arrow),
    ("<-->", ArrowType::DoubleArrow),
    ("---", ArrowType::Line),
    ("--o", ArrowType::Circle),
    ("--x", ArrowType::Cross),
    ("--", ArrowType::Line),
    ("==", ArrowType::ThickLine),
    ("-.", ArrowType::DottedLine),
    ("..", ArrowType::DottedLine),
];

const SEQUENCE_OPERATORS: [(&str, ArrowType); 26] = [
    ("<<-->>", ArrowType::DoubleDottedArrow),
    ("<<->>", ArrowType::DoubleArrow),
    ("--|\\", ArrowType::HalfArrowTopDotted),
    ("--|/", ArrowType::HalfArrowBottomDotted),
    ("--\\\\", ArrowType::StickArrowTopDotted),
    ("--//", ArrowType::StickArrowBottomDotted),
    ("/|--", ArrowType::HalfArrowTopReverseDotted),
    ("\\|--", ArrowType::HalfArrowBottomReverseDotted),
    ("//--", ArrowType::StickArrowTopReverseDotted),
    ("\\\\--", ArrowType::StickArrowBottomReverseDotted),
    ("-->>", ArrowType::DottedArrow),
    ("->>", ArrowType::Arrow),
    ("-|\\", ArrowType::HalfArrowTop),
    ("-|/", ArrowType::HalfArrowBottom),
    ("-\\\\", ArrowType::StickArrowTop),
    ("-//", ArrowType::StickArrowBottom),
    ("/|-", ArrowType::HalfArrowTopReverse),
    ("\\|-", ArrowType::HalfArrowBottomReverse),
    ("//-", ArrowType::StickArrowTopReverse),
    ("\\\\-", ArrowType::StickArrowBottomReverse),
    ("--)", ArrowType::DottedOpenArrow),
    ("-)", ArrowType::OpenArrow),
    ("-->", ArrowType::DottedLine),
    ("->", ArrowType::Line),
    ("--x", ArrowType::DottedCross),
    ("-x", ArrowType::Cross),
];

const CLASS_OPERATORS: [(&str, ArrowType); 6] = [
    ("<|--", ArrowType::Arrow),
    ("--|>", ArrowType::Arrow),
    ("..>", ArrowType::DottedArrow),
    ("<..", ArrowType::DottedArrow),
    ("-->", ArrowType::Arrow),
    ("--", ArrowType::Line),
];

const PACKET_OPERATORS: [(&str, ArrowType); 4] = [
    ("-->", ArrowType::Arrow),
    ("->", ArrowType::Arrow),
    ("--", ArrowType::Line),
    ("==", ArrowType::ThickArrow),
];

const ER_OPERATORS: [(&str, ArrowType); 14] = [
    ("||--o{", ArrowType::Arrow),
    ("||--|{", ArrowType::Arrow),
    ("}|--||", ArrowType::Arrow),
    ("}o--||", ArrowType::Arrow),
    ("|o--o|", ArrowType::Arrow),
    ("}|..|{", ArrowType::DottedArrow),
    ("||..||", ArrowType::DottedArrow),
    ("||--||", ArrowType::Line),
    ("o|--|{", ArrowType::Arrow),
    ("}|--|{", ArrowType::Arrow),
    ("|o--||", ArrowType::Arrow),
    ("}o--o{", ArrowType::Arrow),
    ("--", ArrowType::Line),
    ("..", ArrowType::DottedArrow),
];

const DANGLING_PLACEHOLDER_PREFIX: &str = "__fm_dangling_line_";

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeToken {
    id: String,
    label: Option<ParsedLabel>,
    icon: Option<String>,
    shape: NodeShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockDef {
    id: String,
    label: Option<String>,
    shape: NodeShape,
    span_cols: usize,
    is_space: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SequenceParticipantDeclaration {
    id: String,
    label: Option<ParsedLabel>,
    shape: NodeShape,
    classes: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SequenceStatement {
    Participant(String),
    Actor(String),
    Message(String),
    Autonumber {
        start: Option<u32>,
        increment: Option<u32>,
    },
    HideFootbox,
    Link {
        actor: String,
        label: String,
        url: String,
    },
    Links {
        actor: String,
        entries: Vec<(String, String)>,
    },
    Note {
        position: fm_core::NotePosition,
        participants: Vec<String>,
        text: String,
    },
    Activate(String),
    Deactivate(String),
    BoxStart {
        label: String,
        color: Option<String>,
    },
    CreateParticipant(String),
    Destroy(String),
    FragmentStart {
        kind: fm_core::FragmentKind,
        label: String,
    },
    FragmentElse {
        label: String,
    },
    FragmentEnd,
}

#[derive(Debug, Clone)]
enum ClassStatement {
    /// Class block start with name and optional generic type parameters.
    BlockStart(String, Vec<String>),
    Ast(FlowAst),
    Node(NodeToken),
    Member(fm_core::IrClassMember),
    Stereotype(String, fm_core::ClassStereotype),
    /// Cardinality labels to attach to the most recently created edge.
    Cardinality(Option<String>, Option<String>),
    End,
}

/// Parse mermaid input with auto-detection. Convenience entry point for external callers
/// and reproduction scripts.
#[must_use]
#[allow(dead_code)]
pub fn parse_mermaid(input: &str) -> ParseResult {
    let detection = crate::detect_type_with_confidence(input);
    parse_mermaid_with_detection_and_config(
        input,
        detection,
        MermaidParseMode::Compat,
        &ParserConfig::default(),
    )
}

/// Parse mermaid input with pre-computed detection results.
#[must_use]
#[allow(dead_code)] // Used by crate-level tests through the private module path.
pub fn parse_mermaid_with_detection(
    input: &str,
    detection: DetectedType,
    parse_mode: MermaidParseMode,
) -> ParseResult {
    parse_mermaid_with_detection_and_config(input, detection, parse_mode, &ParserConfig::default())
}

/// Parse mermaid input with pre-computed detection results and explicit parser behavior config.
#[must_use]
pub fn parse_mermaid_with_detection_and_config(
    input: &str,
    detection: DetectedType,
    parse_mode: MermaidParseMode,
    config: &ParserConfig,
) -> ParseResult {
    let (content, front_matter_payload) = split_front_matter_block(input);
    let diagram_type = detection.diagram_type;
    let input_lines = content.lines().count();
    let mut builder = IrBuilder::with_capacity_hint(diagram_type, input_lines);
    builder.set_parse_mode(parse_mode);
    builder.set_parser_config(*config);

    // Add detection warnings to builder
    for warning in &detection.warnings {
        builder.add_warning(warning.clone());
    }

    if let Some(front_matter_payload) = front_matter_payload {
        parse_front_matter_config(front_matter_payload, &mut builder);
    }

    parse_init_directives(content, &mut builder);

    match diagram_type {
        DiagramType::Flowchart => parse_flowchart(content, &mut builder),
        DiagramType::Sequence => parse_sequence(content, &mut builder),
        DiagramType::Class => parse_class(content, &mut builder),
        DiagramType::State => parse_state(content, &mut builder),
        DiagramType::Requirement => parse_requirement(content, &mut builder),
        DiagramType::Mindmap => parse_mindmap(content, &mut builder),
        DiagramType::Er => parse_er(content, &mut builder),
        DiagramType::Journey => parse_journey(content, &mut builder),
        DiagramType::Timeline => parse_timeline(content, &mut builder),
        DiagramType::Sankey => parse_sankey(content, &mut builder),
        DiagramType::ArchitectureBeta => parse_architecture(content, &mut builder),
        DiagramType::C4Context
        | DiagramType::C4Container
        | DiagramType::C4Component
        | DiagramType::C4Dynamic
        | DiagramType::C4Deployment => parse_c4(content, &mut builder),
        DiagramType::PacketBeta => parse_packet(content, &mut builder),
        DiagramType::XyChart => parse_xychart(content, &mut builder),
        DiagramType::Gantt => parse_gantt(content, &mut builder),
        DiagramType::Pie => parse_pie(content, &mut builder),
        DiagramType::QuadrantChart => parse_quadrant(content, &mut builder),
        DiagramType::GitGraph => parse_gitgraph(content, &mut builder),
        DiagramType::BlockBeta => parse_block_beta(content, &mut builder),
        DiagramType::Kanban => parse_kanban(content, &mut builder),
        DiagramType::Unknown => {
            apply_unknown_contract(content, &mut builder, parse_mode);
        }
    }

    // Extract accessibility directives (accTitle / accDescr) from any diagram type.
    extract_accessibility_directives(content, &mut builder);

    // Extract style/classDef/linkStyle directives for all diagram types.
    // (Previously only called from parse_flowchart; now runs generically
    // so that class, state, block-beta, etc. all support styling.)
    if diagram_type != DiagramType::Flowchart {
        extract_style_directives(content, &mut builder);
    }

    if builder.node_count() == 0 && builder.edge_count() == 0 {
        builder.add_warning("No parseable nodes or edges were found");
    }

    builder.finish(detection.confidence, detection.method)
}

fn apply_unknown_contract(content: &str, builder: &mut IrBuilder, parse_mode: MermaidParseMode) {
    match parse_mode {
        MermaidParseMode::Strict => {
            builder.add_diagnostic(
                Diagnostic::error(
                    "Unable to detect a supported diagram family in strict mode; refusing fallback",
                )
                .with_category(DiagnosticCategory::Compatibility)
                .with_suggestion(
                    "Add an explicit Mermaid diagram header or switch to --parse-mode compat/recover"
                        .to_string(),
                ),
            );
        }
        MermaidParseMode::Compat => {
            builder.add_diagnostic(
                Diagnostic::warning(
                    "Unable to detect diagram family; attempting compatibility-mode flowchart salvage",
                )
                .with_category(DiagnosticCategory::Compatibility)
                .with_suggestion(
                    "Add an explicit Mermaid header to avoid compatibility fallback".to_string(),
                ),
            );
            builder.add_warning(
                "Unable to detect diagram type; using compatibility-mode flowchart salvage",
            );
            parse_flowchart(content, builder);
        }
        MermaidParseMode::Recover => {
            builder.add_diagnostic(
                Diagnostic::warning(
                    "Unable to detect diagram family; falling back to flowchart-style recovery",
                )
                .with_category(DiagnosticCategory::Recovery)
                .with_suggestion(
                    "Add an explicit Mermaid header to reduce recovery guesswork".to_string(),
                ),
            );
            builder.add_warning(
                "Unable to detect diagram type; using recovery-mode flowchart salvage",
            );
            parse_flowchart(content, builder);
        }
    }
}

#[allow(dead_code)] // Reserved for future detectible diagram families that still need fallback handling.
fn apply_support_contract(
    content: &str,
    builder: &mut IrBuilder,
    diagram_type: DiagramType,
    parse_mode: MermaidParseMode,
) {
    let support_level = diagram_type.support_level();
    if support_level != MermaidSupportLevel::Unsupported {
        builder.add_diagnostic(
            Diagnostic::info(format!(
                "Diagram family '{}' is parsed with declared support level '{}'",
                diagram_type.as_str(),
                diagram_type.support_label()
            ))
            .with_category(DiagnosticCategory::Compatibility),
        );
        parse_flowchart(content, builder);
        return;
    }

    match parse_mode {
        MermaidParseMode::Strict => {
            builder.add_diagnostic(
                Diagnostic::error(format!(
                    "Diagram family '{}' is unsupported in strict mode; no fallback applied",
                    diagram_type.as_str()
                ))
                .with_category(DiagnosticCategory::Compatibility)
                .with_suggestion(
                    "Choose a supported family or switch to --parse-mode compat/recover"
                        .to_string(),
                ),
            );
        }
        MermaidParseMode::Compat => {
            builder.add_diagnostic(
                Diagnostic::warning(format!(
                    "Diagram family '{}' is unsupported; applying best-effort flowchart salvage",
                    diagram_type.as_str()
                ))
                .with_category(DiagnosticCategory::Compatibility)
                .with_suggestion(
                    "Treat this output as degraded compatibility mode, not full semantic support"
                        .to_string(),
                ),
            );
            builder.add_warning(format!(
                "Diagram type '{}' is unsupported; using compatibility-mode flowchart salvage",
                diagram_type.as_str()
            ));
            parse_flowchart(content, builder);
        }
        MermaidParseMode::Recover => {
            builder.add_diagnostic(
                Diagnostic::warning(format!(
                    "Diagram family '{}' is unsupported; applying recovery-mode flowchart salvage",
                    diagram_type.as_str()
                ))
                .with_category(DiagnosticCategory::Recovery)
                .with_suggestion(
                    "Expect partial semantics; recovery mode prioritizes extracting structure"
                        .to_string(),
                ),
            );
            builder.add_warning(format!(
                "Diagram type '{}' is unsupported; using recovery-mode flowchart salvage",
                diagram_type.as_str()
            ));
            parse_flowchart(content, builder);
        }
    }
}

// ---------------------------------------------------------------------------
// Chumsky-based flowchart parser — intermediate AST
// ---------------------------------------------------------------------------

/// Intermediate AST node produced by the chumsky flowchart parser.
/// Lowered to IR via [`lower_flowchart`].
#[derive(Debug, Clone)]
#[allow(dead_code)] // Subgraph variant used in future expansion
enum FlowAst {
    Direction(GraphDirection),
    Node(FlowAstNode),
    Edge {
        from: FlowAstNode,
        arrow: ArrowType,
        label: Option<String>,
        to: FlowAstNode,
    },
    ClassAssign {
        nodes: Vec<String>,
        class: String,
    },
    ClickDirective {
        node: String,
        target: String,
        tooltip: Option<String>,
        is_callback: bool,
    },
    StyleOrLinkStyle,
    ClassDef,
}

#[derive(Debug, Clone)]
struct FlowAstNode {
    id: String,
    label: Option<ParsedLabel>,
    icon: Option<String>,
    shape: NodeShape,
}

impl From<NodeToken> for FlowAstNode {
    fn from(value: NodeToken) -> Self {
        Self {
            id: value.id,
            label: value.label,
            icon: value.icon,
            shape: value.shape,
        }
    }
}

#[derive(Debug, Clone)]
enum FlowDocumentItem {
    Statements {
        asts: Vec<FlowAst>,
        line_number: usize,
        source_line: String,
    },
    Subgraph {
        id: String,
        title: Option<String>,
        line_number: usize,
        source_line: String,
        body: Vec<Self>,
    },
}

#[derive(Debug, Default)]
struct FlowDocumentParseResult {
    items: Vec<FlowDocumentItem>,
    warnings: Vec<String>,
    header_direction: Option<GraphDirection>,
}

#[derive(Debug, Clone)]
enum BlockBetaStatement {
    Columns(usize),
    Edges(Vec<FlowAst>),
    Blocks(Vec<BlockDef>),
}

#[derive(Debug, Clone)]
enum BlockBetaDocumentItem {
    Statement {
        statement: BlockBetaStatement,
        line_number: usize,
        source_line: String,
    },
    Group {
        id: String,
        span_cols: Option<usize>,
        line_number: usize,
        source_line: String,
        body: Vec<Self>,
    },
}

#[derive(Debug, Default)]
struct BlockBetaDocumentParseResult {
    items: Vec<BlockBetaDocumentItem>,
    warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Chumsky flowchart statement parser (character-level on &str)
// ---------------------------------------------------------------------------
// Parses a single semicolon-free statement (trimmed line or `;`-split segment).
// Document structure (lines, comments, header) is handled by the outer loop.

/// Build a chumsky parser for a single flowchart statement.
fn flow_statement_parser<'a>() -> impl Parser<'a, &'a str, FlowAst, extra::Err<Rich<'a, char>>> {
    // -- Whitespace helpers --------------------------------------------------
    let ws_char = any().filter(|c: &char| *c == ' ' || *c == '\t');
    let inline_ws = ws_char.repeated().to(());
    let required_ws = ws_char.repeated().at_least(1).to(());

    // -- Identifier (bare word) ---------------------------------------------
    let ident = any()
        .filter(|c: &char| c.is_ascii_alphanumeric() || matches!(*c, '_' | '-' | '.' | '/'))
        .repeated()
        .at_least(1)
        .to_slice();

    // -- Node shapes ---------------------------------------------------------
    // Multi-char delimiters must be tried before single-char ones.
    let double_circle_content = just("((")
        .ignore_then(any().and_is(just("))").not()).repeated().to_slice())
        .then_ignore(just("))"));

    let hexagon_content = just("{{")
        .ignore_then(any().and_is(just("}}").not()).repeated().to_slice())
        .then_ignore(just("}}"));

    let rect_content = just('[')
        .ignore_then(any().filter(|c: &char| *c != ']').repeated().to_slice())
        .then_ignore(just(']'));

    let rounded_content = just('(')
        .ignore_then(any().filter(|c: &char| *c != ')').repeated().to_slice())
        .then_ignore(just(')'));

    let diamond_content = just('{')
        .ignore_then(any().filter(|c: &char| *c != '}').repeated().to_slice())
        .then_ignore(just('}'));

    let node_shape = choice((
        double_circle_content.map(|label: &str| (label, NodeShape::DoubleCircle)),
        hexagon_content.map(|label: &str| (label, NodeShape::Hexagon)),
        rect_content.map(|label: &str| (label, NodeShape::Rect)),
        rounded_content.map(|label: &str| (label, NodeShape::Rounded)),
        diamond_content.map(|label: &str| (label, NodeShape::Diamond)),
    ));

    let node = ident.then(node_shape.or_not()).map(
        |(id_str, shape_opt): (&str, Option<(&str, NodeShape)>)| {
            let id = id_str.to_string();
            match shape_opt {
                Some((label_raw, shape)) => {
                    let trimmed = label_raw.trim();
                    let mut label = parse_label((!trimmed.is_empty()).then_some(trimmed));
                    let icon = extract_icon_prefix(label.as_mut());
                    clear_empty_label(&mut label);
                    FlowAstNode {
                        id,
                        label,
                        icon,
                        shape,
                    }
                }
                None => FlowAstNode {
                    id,
                    label: None,
                    icon: None,
                    shape: NodeShape::Rect,
                },
            }
        },
    );

    // -- Arrow operators (longest-first) -------------------------------------
    let arrow = choice((
        just("-.->").to(ArrowType::DottedArrow),
        just("==>").to(ArrowType::ThickArrow),
        just("-->").to(ArrowType::Arrow),
        just("---").to(ArrowType::Line),
        just("--o").to(ArrowType::Circle),
        just("--x").to(ArrowType::Cross),
    ));

    // -- Pipe label  |text| -------------------------------------------------
    let pipe_label = just('|')
        .ignore_then(any().filter(|c: &char| *c != '|').repeated().to_slice())
        .then_ignore(just('|'))
        .map(|s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        });

    // -- direction directive: direction LR ----------------------------------
    let direction = just("direction")
        .then(required_ws)
        .ignore_then(choice((
            just("LR").to(GraphDirection::LR),
            just("RL").to(GraphDirection::RL),
            just("TB").to(GraphDirection::TB),
            just("TD").to(GraphDirection::TD),
            just("BT").to(GraphDirection::BT),
        )))
        .then_ignore(inline_ws)
        .then_ignore(end())
        .map(FlowAst::Direction);

    // -- Edge: node arrow [|label|] node ------------------------------------
    let edge = node
        .then_ignore(inline_ws)
        .then(arrow)
        .then_ignore(inline_ws)
        .then(pipe_label.or_not())
        .then_ignore(inline_ws)
        .then(node)
        .then_ignore(inline_ws)
        .then_ignore(end())
        .map(|(((from, arrow_type), label_opt), to)| FlowAst::Edge {
            from,
            arrow: arrow_type,
            label: label_opt.flatten(),
            to,
        });

    // -- class directive: class nodeA,nodeB className -----------------------
    let class_assign = just("class")
        .then(required_ws)
        .ignore_then(
            ident
                .separated_by(just(','))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(required_ws)
        .then(any().repeated().at_least(1).to_slice())
        .then_ignore(end())
        .map(|(node_ids, class_raw): (Vec<&str>, &str)| {
            let nodes: Vec<String> = node_ids
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            FlowAst::ClassAssign {
                nodes,
                class: class_raw.trim().to_string(),
            }
        });

    // -- click directive: click nodeId target ["tooltip"] -------------------
    let bare_token = any()
        .filter(|c: &char| !c.is_whitespace())
        .repeated()
        .at_least(1)
        .to_slice()
        .map(|s: &str| s.to_string());

    let quoted_string = just('"')
        .ignore_then(any().filter(|c: &char| *c != '"').repeated().to_slice())
        .then_ignore(just('"'))
        .map(|s: &str| s.to_string());

    let click_callback_directive = just("click")
        .then(required_ws)
        .ignore_then(ident)
        .then_ignore(required_ws)
        .then(choice((just("call"), just("callback"))))
        .then_ignore(required_ws)
        .then(bare_token)
        .then(required_ws.ignore_then(quoted_string).or_not())
        .then_ignore(end())
        .map(
            |(((node_id, _keyword), callback), tooltip): (
                ((&str, &str), String),
                Option<String>,
            )| {
                FlowAst::ClickDirective {
                    node: node_id.to_string(),
                    target: callback,
                    tooltip,
                    is_callback: true,
                }
            },
        );

    let quoted_string_link = just('"')
        .ignore_then(any().filter(|c: &char| *c != '"').repeated().to_slice())
        .then_ignore(just('"'))
        .map(|s: &str| s.to_string());

    let quoted_string_tooltip = just('"')
        .ignore_then(any().filter(|c: &char| *c != '"').repeated().to_slice())
        .then_ignore(just('"'))
        .map(|s: &str| s.to_string());

    let click_link_directive = just("click")
        .then(required_ws)
        .ignore_then(ident)
        .then_ignore(required_ws)
        .then(quoted_string_link.or(bare_token))
        .then(required_ws.ignore_then(quoted_string_tooltip).or_not())
        .then_ignore(end())
        .map(
            |((node_id, target), tooltip): ((&str, String), Option<String>)| {
                FlowAst::ClickDirective {
                    node: node_id.to_string(),
                    target,
                    tooltip,
                    is_callback: false,
                }
            },
        );

    let click_directive = choice((click_callback_directive, click_link_directive));

    // -- style/linkStyle/classDef (skip) ------------------------------------
    let skip_directive = choice((
        just("style ").to(()),
        just("linkStyle ").to(()),
        just("classDef ").to(()),
    ))
    .then(any().repeated())
    .then_ignore(end())
    .to(FlowAst::StyleOrLinkStyle);

    // -- Statement: try edge first (more specific), then directives, then node
    choice((
        skip_directive,
        class_assign,
        click_directive,
        direction,
        edge,
        node.then_ignore(inline_ws)
            .then_ignore(end())
            .map(FlowAst::Node),
    ))
}

// ---------------------------------------------------------------------------
// Lowering: FlowAst → IrBuilder calls
// ---------------------------------------------------------------------------

fn lower_flow_ast(
    ast: &FlowAst,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
) {
    let span = span_for(line_number, source_line);

    match ast {
        FlowAst::Direction(dir) => {
            // If inside a subgraph, set the subgraph's direction override;
            // otherwise set the global diagram direction.
            if let Some(&subgraph_index) = active_subgraphs.last() {
                builder.set_subgraph_direction(subgraph_index, *dir);
            } else {
                builder.set_direction(*dir);
            }
        }
        FlowAst::Node(n) => {
            if let Some(node_id) = intern_flow_ast_node(builder, n, span) {
                add_node_to_active_groups(builder, active_clusters, active_subgraphs, node_id);
            }
        }
        FlowAst::Edge {
            from,
            arrow,
            label,
            to,
        } => {
            let from_id = intern_flow_ast_node(builder, from, span);
            let to_id = intern_flow_ast_node(builder, to, span);
            if let (Some(f), Some(t)) = (from_id, to_id) {
                add_node_to_active_groups(builder, active_clusters, active_subgraphs, f);
                add_node_to_active_groups(builder, active_clusters, active_subgraphs, t);
                builder.push_edge(f, t, *arrow, label.as_deref(), span);
            }
        }
        FlowAst::ClassAssign { nodes, class } => {
            for class_name in class.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                for node_key in nodes {
                    builder.add_class_to_node(node_key, class_name, span);
                }
            }
        }
        FlowAst::ClickDirective {
            node,
            target,
            tooltip,
            is_callback,
        } => {
            let cleaned = target
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim_matches('`')
                .trim();
            if cleaned.is_empty() {
                builder.add_warning(format!(
                    "Line {line_number}: click directive target is empty after normalization"
                ));
            } else if *is_callback {
                // Callback: store function name, add interactive class.
                builder.add_class_to_node(node, "has-callback", span);
                builder.set_node_callback(node, cleaned, span);
                if let Some(tip) = tooltip {
                    let tip_cleaned = tip
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_matches('`')
                        .trim();
                    if !tip_cleaned.is_empty() {
                        builder.set_node_tooltip(node, tip_cleaned, span);
                    }
                }
            } else if !is_safe_click_target(cleaned) {
                builder.add_warning(format!(
                    "Line {line_number}: unsafe click link target blocked: {cleaned}"
                ));
            } else {
                builder.add_class_to_node(node, "has-link", span);
                builder.set_node_link(node, cleaned, span);
                if let Some(tip) = tooltip {
                    let tip_cleaned = tip
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_matches('`')
                        .trim();
                    if !tip_cleaned.is_empty() {
                        builder.set_node_tooltip(node, tip_cleaned, span);
                    }
                }
            }
        }
        FlowAst::StyleOrLinkStyle | FlowAst::ClassDef => {
            // Intentionally skipped — same as the hand-written parser
        }
    }
}

fn lower_flow_document_item(
    item: &FlowDocumentItem,
    builder: &mut IrBuilder,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
) {
    match item {
        FlowDocumentItem::Statements {
            asts,
            line_number,
            source_line,
        } => {
            for ast in asts {
                lower_flow_ast(
                    ast,
                    *line_number,
                    source_line,
                    builder,
                    active_clusters,
                    active_subgraphs,
                );
            }
        }
        FlowDocumentItem::Subgraph {
            id,
            title,
            line_number,
            source_line,
            body,
        } => {
            let span = span_for(*line_number, source_line);
            let lookup_key = flow_subgraph_lookup_key(id, title.as_deref());
            let Some(cluster_index) = builder.ensure_cluster(&lookup_key, title.as_deref(), span)
            else {
                return;
            };
            let parent_subgraph = active_subgraphs.last().copied();
            let Some(subgraph_index) = builder.ensure_subgraph(
                &lookup_key,
                id,
                title.as_deref(),
                span,
                parent_subgraph,
                Some(cluster_index),
            ) else {
                return;
            };

            let mut child_clusters = active_clusters.to_vec();
            child_clusters.push(cluster_index);
            let mut child_subgraphs = active_subgraphs.to_vec();
            child_subgraphs.push(subgraph_index);

            for child in body {
                lower_flow_document_item(child, builder, &child_clusters, &child_subgraphs);
            }
        }
    }
}

fn flow_subgraph_lookup_key(id: &str, title: Option<&str>) -> String {
    match clean_label(title) {
        Some(title_text) => format!("{id}@title:{title_text}"),
        None => id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Top-level parse_flowchart — line-by-line with chumsky statement parser
// ---------------------------------------------------------------------------

fn parse_flowchart(input: &str, builder: &mut IrBuilder) {
    let document = parse_flowchart_document(input, builder.parser_config());
    if let Some(direction) = document.header_direction {
        builder.set_direction(direction);
    }
    for warning in &document.warnings {
        builder.add_warning(warning.clone());
    }
    for item in &document.items {
        lower_flow_document_item(item, builder, &[], &[]);
    }

    // Extract style directives from the raw input and add to IR.
    // This runs AFTER lowering so node IDs are resolved for `style nodeId` lookups.
    extract_style_directives(input, builder);
}

fn parse_flowchart_document(input: &str, config: &ParserConfig) -> FlowDocumentParseResult {
    let lines: Vec<(usize, &str)> = input
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect();
    let mut next_index = 0;
    let mut warnings = Vec::new();
    let mut header_direction = None;
    let (items, unclosed_subgraphs) = parse_flowchart_document_items(
        &lines,
        &mut next_index,
        true,
        &mut warnings,
        &mut header_direction,
        config,
    );
    if unclosed_subgraphs > 0 {
        warnings.push(format!(
            "Flowchart ended with {unclosed_subgraphs} unclosed subgraph block(s)"
        ));
    }
    FlowDocumentParseResult {
        items,
        warnings,
        header_direction,
    }
}

fn parse_flowchart_document_items(
    lines: &[(usize, &str)],
    next_index: &mut usize,
    is_root: bool,
    warnings: &mut Vec<String>,
    header_direction: &mut Option<GraphDirection>,
    config: &ParserConfig,
) -> (Vec<FlowDocumentItem>, usize) {
    let mut items = Vec::new();
    let mut unclosed_subgraphs = 0;

    while let Some((line_number, line)) = lines.get(*next_index).copied() {
        *next_index += 1;

        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        let uncommented_line = strip_flowchart_inline_comment(trimmed);
        if uncommented_line.is_empty() {
            continue;
        }

        let mut line_items = Vec::new();
        let mut parsed_line = false;

        for statement in split_statements(uncommented_line) {
            let normalized_statement = statement.trim();
            if normalized_statement.is_empty() {
                parsed_line = true;
                continue;
            }

            if is_flowchart_header(normalized_statement) {
                if !is_root {
                    warnings.push(format!(
                        "Line {line_number}: nested flowchart header ignored inside subgraph"
                    ));
                } else if let Some(dir) = parse_graph_direction(normalized_statement) {
                    *header_direction = Some(dir);
                }
                parsed_line = true;
                continue;
            }

            if let Some((cluster_key, cluster_title)) =
                parse_subgraph_statement(normalized_statement, config)
            {
                let (body, child_unclosed) = parse_flowchart_document_items(
                    lines,
                    next_index,
                    false,
                    warnings,
                    header_direction,
                    config,
                );
                unclosed_subgraphs += child_unclosed;
                line_items.push(FlowDocumentItem::Subgraph {
                    id: cluster_key,
                    title: cluster_title,
                    line_number,
                    source_line: line.to_string(),
                    body,
                });
                parsed_line = true;
                continue;
            }

            if normalized_statement == "end" {
                if !is_root {
                    items.extend(line_items);
                    return (items, unclosed_subgraphs);
                }
                warnings.push(format!(
                    "Line {line_number}: encountered 'end' without matching 'subgraph'"
                ));
                parsed_line = true;
                continue;
            }

            if let Some(asts) = parse_flowchart_statement_asts(
                normalized_statement,
                line_number,
                line,
                warnings,
                config,
            ) {
                line_items.push(FlowDocumentItem::Statements {
                    asts,
                    line_number,
                    source_line: line.to_string(),
                });
                parsed_line = true;
            }
        }

        if !parsed_line {
            warnings.push(format!(
                "Line {line_number}: unsupported flowchart syntax: {trimmed}"
            ));
        }

        items.extend(line_items);
    }

    if !is_root {
        unclosed_subgraphs += 1;
    }

    (items, unclosed_subgraphs)
}

fn parse_flowchart_statement_asts(
    statement: &str,
    line_number: usize,
    source_line: &str,
    warnings: &mut Vec<String>,
    config: &ParserConfig,
) -> Option<Vec<FlowAst>> {
    let (ast, errors) = flow_statement_parser()
        .parse(statement)
        .into_output_errors();
    if errors.is_empty()
        && let Some(ast_node) = ast
    {
        return Some(vec![ast_node]);
    }

    if let Some(ast) = parse_class_assignment_ast(statement) {
        return Some(vec![ast]);
    }
    if let Some(ast) = parse_click_directive_ast(statement, line_number, warnings) {
        return Some(vec![ast]);
    }
    if is_non_graph_statement(statement) {
        // Parse the style directive into IR style refs instead of discarding.
        // We still return StyleOrLinkStyle to the chumsky lowerer (which skips it),
        // but the side effect populates ir.style_refs via the builder.
        return Some(vec![FlowAst::StyleOrLinkStyle]);
    }
    if let Some(asts) =
        parse_edge_statement_asts(statement, &FLOW_OPERATORS, true, config, line_number)
    {
        return Some(asts);
    }
    if find_operator(statement, &FLOW_OPERATORS).is_some() {
        return None;
    }
    if let Some(node) = parse_node_token_with_config(statement, config) {
        return Some(vec![FlowAst::Node(FlowAstNode {
            id: node.id,
            label: node.label,
            icon: node.icon,
            shape: node.shape,
        })]);
    }

    let _ = source_line;
    None
}

fn add_node_to_active_clusters(
    builder: &mut IrBuilder,
    active_clusters: &[usize],
    node_id: IrNodeId,
) {
    for &cluster_index in active_clusters {
        builder.add_node_to_cluster(cluster_index, node_id);
    }
}

fn add_node_to_active_groups(
    builder: &mut IrBuilder,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
    node_id: IrNodeId,
) {
    add_node_to_active_clusters(builder, active_clusters, node_id);
    for &subgraph_index in active_subgraphs {
        builder.add_node_to_subgraph(subgraph_index, node_id);
    }
}

fn intern_node_token(builder: &mut IrBuilder, node: &NodeToken, span: Span) -> Option<IrNodeId> {
    let node_id = builder.intern_node_label(&node.id, node.label.as_ref(), node.shape, span)?;
    if let Some(icon) = node.icon.as_deref() {
        builder.set_node_icon(node_id, icon);
    }
    Some(node_id)
}

fn intern_flow_ast_node(
    builder: &mut IrBuilder,
    node: &FlowAstNode,
    span: Span,
) -> Option<IrNodeId> {
    let node_id = if is_dangling_placeholder_node_id(&node.id) {
        builder.intern_placeholder_node(&node.id, span)?
    } else {
        builder.intern_node_label(&node.id, node.label.as_ref(), node.shape, span)?
    };
    if let Some(icon) = node.icon.as_deref() {
        builder.set_node_icon(node_id, icon);
    }
    Some(node_id)
}

fn parse_subgraph_statement(
    statement: &str,
    config: &ParserConfig,
) -> Option<(String, Option<String>)> {
    let statement = statement.trim_start();
    let rest = statement.strip_prefix("subgraph")?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }

    let body = rest.trim_start();
    if body.is_empty() {
        return None;
    }

    // Prefer explicit node-style labels (`id[Title]`, `id(Title)`, etc.) when
    // delimiter characters are present. This preserves the same title
    // normalization behavior as normal node parsing.
    let has_explicit_label_delimiters =
        body.contains('[') || body.contains('(') || body.contains('{') || body.contains('>');
    if has_explicit_label_delimiters && let Some(node) = parse_node_token_with_config(body, config)
    {
        let key = normalize_identifier(&node.id);
        if !key.is_empty() {
            return Some((key, node.label.map(|label| label.text)));
        }
    }

    // Mermaid commonly supports `subgraph <id> <title>` and
    // `subgraph <id> "<title>"`. Use robust quoted extraction to handle
    // spaces and escapes in either field.
    if let Some((key_raw, rest)) = extract_quoted_or_word(body) {
        let key = normalize_identifier(&key_raw);
        if !key.is_empty() {
            let title_raw = rest.trim();
            let explicit_title = matches!(title_raw.chars().next(), Some('"' | '\'' | '`'));
            if title_raw.is_empty() || explicit_title || looks_like_explicit_subgraph_id(&key_raw) {
                let title = if title_raw.is_empty() {
                    None
                } else {
                    normalize_subgraph_title(title_raw)
                };
                return Some((key, title));
            }
        }
    }

    if let Some(node) = parse_node_token_with_config(body, config) {
        let key = normalize_identifier(&node.id);
        if !key.is_empty() {
            return Some((key, node.label.map(|label| label.text)));
        }
    }

    let key = normalize_identifier(body);
    if key.is_empty() {
        return None;
    }
    let title = normalize_subgraph_title(body).filter(|value| value != &key);
    Some((key, title))
}

fn looks_like_explicit_subgraph_id(raw: &str) -> bool {
    let trimmed = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`');
    !trimmed.is_empty()
        && trimmed.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.' | '/')
        })
}

fn parse_sequence(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("sequenceDiagram") {
            continue;
        }

        let Some(statement) = parse_sequence_statement(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported sequence syntax: {trimmed}"
            ));
            continue;
        };

        lower_sequence_statement(statement, line_number, line, builder);
    }
}

fn parse_sequence_statement(line: &str) -> Option<SequenceStatement> {
    if let Some(autonumber) = parse_sequence_autonumber(line) {
        return Some(autonumber);
    }

    if line.eq_ignore_ascii_case("hide footbox") {
        return Some(SequenceStatement::HideFootbox);
    }

    if let Some(link) = parse_sequence_actor_menu(line) {
        return Some(link);
    }

    if let Some(note) = parse_sequence_note(line) {
        return Some(note);
    }

    if let Some(rest) = line.strip_prefix("activate ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::Activate(name.to_string()));
        }
    }

    if let Some(rest) = line.strip_prefix("deactivate ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::Deactivate(name.to_string()));
        }
    }

    if line == "end" {
        // `end` closes both box groups and fragments; the builder
        // will dispatch appropriately based on which is open.
        return Some(SequenceStatement::FragmentEnd);
    }

    // Fragment start keywords
    if let Some(frag) = parse_sequence_fragment_start(line) {
        return Some(frag);
    }

    // Fragment else/and/option dividers — require word boundary (space or end-of-line)
    // to avoid matching e.g. "elsewhere" or "android".
    if line == "else" {
        return Some(SequenceStatement::FragmentElse {
            label: String::new(),
        });
    }
    if let Some(rest) = line.strip_prefix("else ") {
        return Some(SequenceStatement::FragmentElse {
            label: rest.trim().to_string(),
        });
    }
    if line == "and" || line == "option" {
        return Some(SequenceStatement::FragmentElse {
            label: String::new(),
        });
    }
    if let Some(rest) = line.strip_prefix("and ") {
        return Some(SequenceStatement::FragmentElse {
            label: rest.trim().to_string(),
        });
    }
    if let Some(rest) = line.strip_prefix("option ") {
        return Some(SequenceStatement::FragmentElse {
            label: rest.trim().to_string(),
        });
    }

    if let Some(rest) = line.strip_prefix("box ") {
        let rest = rest.trim();
        return Some(parse_sequence_box_start(rest));
    }
    if line == "box" {
        return Some(SequenceStatement::BoxStart {
            label: String::new(),
            color: None,
        });
    }

    if let Some(rest) = line.strip_prefix("create participant ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::CreateParticipant(name.to_string()));
        }
    }

    if let Some(rest) = line.strip_prefix("create actor ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::CreateParticipant(name.to_string()));
        }
    }

    if let Some(rest) = line.strip_prefix("destroy ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::Destroy(name.to_string()));
        }
    }

    if let Some(rest) = line.strip_prefix("participant ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::Participant(name.to_string()));
        }
    }

    if let Some(rest) = line.strip_prefix("actor ") {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(SequenceStatement::Actor(name.to_string()));
        }
    }

    parse_sequence_message_ast(line).map(SequenceStatement::Message)
}

fn parse_sequence_autonumber(line: &str) -> Option<SequenceStatement> {
    let rest = line.strip_prefix("autonumber")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(SequenceStatement::Autonumber {
            start: None,
            increment: None,
        });
    }

    let parts: Vec<&str> = rest.split_whitespace().collect();
    match parts.as_slice() {
        [start] => Some(SequenceStatement::Autonumber {
            start: Some(start.parse().ok()?),
            increment: None,
        }),
        [start, increment] => Some(SequenceStatement::Autonumber {
            start: Some(start.parse().ok()?),
            increment: Some(increment.parse().ok()?),
        }),
        _ => None,
    }
}

/// Parse `Note left of Alice: text`, `Note right of Bob: text`,
/// `Note over Alice: text`, `Note over Alice,Bob: text`.
fn parse_sequence_note(line: &str) -> Option<SequenceStatement> {
    let rest = line.strip_prefix("Note ")?;

    let (position, after_kw) = if let Some(r) = rest.strip_prefix("left of ") {
        (fm_core::NotePosition::LeftOf, r)
    } else if let Some(r) = rest.strip_prefix("right of ") {
        (fm_core::NotePosition::RightOf, r)
    } else if let Some(r) = rest.strip_prefix("over ") {
        (fm_core::NotePosition::Over, r)
    } else {
        return None;
    };

    // Split at colon to get participants and text
    let (participants_str, text) = after_kw.split_once(':')?;
    let participants: Vec<String> = participants_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if participants.is_empty() {
        return None;
    }

    let text = normalize_sequence_display_text(text);

    Some(SequenceStatement::Note {
        position,
        participants,
        text,
    })
}

fn decode_mermaid_entities(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut cursor = 0usize;

    while cursor < text.len() {
        let amp_start = text[cursor..].find('&').map(|offset| cursor + offset);
        let hash_start = text[cursor..].find('#').map(|offset| cursor + offset);
        let Some(start) = (match (amp_start, hash_start) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        }) else {
            break;
        };

        decoded.push_str(&text[cursor..start]);

        let Some(relative_end) = text[start..].find(';') else {
            decoded.push_str(&text[start..]);
            return decoded;
        };
        let end = start + relative_end;
        let token = if text[start..].starts_with('&') {
            &text[start + 1..end]
        } else {
            &text[start..end]
        };

        if let Some(ch) = decode_mermaid_entity_token(token) {
            decoded.push(ch);
        } else {
            decoded.push_str(&text[start..=end]);
        }
        cursor = end + 1;
    }

    decoded.push_str(&text[cursor..]);
    decoded
}

fn normalize_sequence_display_text(text: &str) -> String {
    let with_line_breaks = text
        .trim()
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace("<br />", "\n");
    decode_mermaid_entities(&with_line_breaks)
}

fn decode_mermaid_entity_token(token: &str) -> Option<char> {
    if token.is_empty() {
        return None;
    }

    let bare = token.strip_prefix('#').unwrap_or(token);

    if let Some(numeric) = token.strip_prefix('#') {
        if let Some(hex) = numeric.strip_prefix(['x', 'X']) {
            let value = u32::from_str_radix(hex, 16).ok()?;
            return char::from_u32(value);
        }

        if numeric.chars().all(|ch| ch.is_ascii_digit()) {
            let value = numeric.parse::<u32>().ok()?;
            return char::from_u32(value);
        }
    }

    match bare {
        "amp" => Some('&'),
        "apos" => Some('\''),
        "copy" => Some('©'),
        "gt" => Some('>'),
        "hearts" => Some('♥'),
        "infin" => Some('∞'),
        "lt" => Some('<'),
        "nbsp" => Some('\u{00A0}'),
        "quot" => Some('"'),
        "semi" => Some(';'),
        "tab" => Some('\t'),
        _ => None,
    }
}

fn parse_sequence_actor_menu(line: &str) -> Option<SequenceStatement> {
    if let Some(rest) = line.strip_prefix("link ") {
        let (actor_raw, after_actor) = rest.split_once(':')?;
        let actor = normalize_identifier(actor_raw);
        if actor.is_empty() {
            return None;
        }
        let (label_raw, url_raw) = after_actor.split_once('@')?;
        let label = clean_label(Some(label_raw.trim()))?;
        let url = clean_label(Some(url_raw.trim()))?;
        return Some(SequenceStatement::Link { actor, label, url });
    }

    if let Some(rest) = line.strip_prefix("links ") {
        let (actor_raw, json_raw) = rest.split_once(':')?;
        let actor = normalize_identifier(actor_raw);
        if actor.is_empty() {
            return None;
        }
        let value = serde_json::from_str::<Value>(json_raw.trim()).ok()?;
        let object = value.as_object()?;
        let mut entries = Vec::new();
        for (label, url_value) in object {
            let Some(url) = url_value.as_str().and_then(|raw| clean_label(Some(raw))) else {
                continue;
            };
            let Some(label) = clean_label(Some(label)) else {
                continue;
            };
            entries.push((label, url));
        }
        return Some(SequenceStatement::Links { actor, entries });
    }

    None
}

/// Parse `box [color] Label` into a `BoxStart` statement.
/// Color can be a CSS color name, hex (#abc), `rgb()`, etc.
fn parse_sequence_box_start(rest: &str) -> SequenceStatement {
    let rest = rest.trim();
    let (color, label) = if let Some((color, label)) = extract_sequence_box_color(rest) {
        (Some(color), label.to_string())
    } else {
        (None, rest.to_string())
    };

    SequenceStatement::BoxStart { label, color }
}

fn extract_sequence_box_color(rest: &str) -> Option<(String, &str)> {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }

    if rest.starts_with('#') {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let color = &rest[..end];
        return Some((color.to_string(), rest[end..].trim_start()));
    }

    let lower = rest.to_ascii_lowercase();
    for prefix in ["rgba(", "rgb(", "hsla(", "hsl("] {
        if lower.starts_with(prefix)
            && let Some(color_len) = consume_css_function(rest)
        {
            let color = &rest[..color_len];
            return Some((color.to_string(), rest[color_len..].trim_start()));
        }
    }

    let token_end = rest
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(rest.len());
    let token = &rest[..token_end];
    if is_css_named_color(token) {
        return Some((token.to_string(), rest[token_end..].trim_start()));
    }

    None
}

fn consume_css_function(value: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

fn is_css_named_color(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "aliceblue"
            | "antiquewhite"
            | "aqua"
            | "aquamarine"
            | "azure"
            | "beige"
            | "bisque"
            | "black"
            | "blanchedalmond"
            | "blue"
            | "blueviolet"
            | "brown"
            | "burlywood"
            | "cadetblue"
            | "chartreuse"
            | "chocolate"
            | "coral"
            | "cornflowerblue"
            | "cornsilk"
            | "crimson"
            | "cyan"
            | "darkblue"
            | "darkcyan"
            | "darkgoldenrod"
            | "darkgray"
            | "darkgreen"
            | "darkgrey"
            | "darkkhaki"
            | "darkmagenta"
            | "darkolivegreen"
            | "darkorange"
            | "darkorchid"
            | "darkred"
            | "darksalmon"
            | "darkseagreen"
            | "darkslateblue"
            | "darkslategray"
            | "darkslategrey"
            | "darkturquoise"
            | "darkviolet"
            | "deeppink"
            | "deepskyblue"
            | "dimgray"
            | "dimgrey"
            | "dodgerblue"
            | "firebrick"
            | "floralwhite"
            | "forestgreen"
            | "fuchsia"
            | "gainsboro"
            | "ghostwhite"
            | "gold"
            | "goldenrod"
            | "gray"
            | "green"
            | "greenyellow"
            | "grey"
            | "honeydew"
            | "hotpink"
            | "indianred"
            | "indigo"
            | "ivory"
            | "khaki"
            | "lavender"
            | "lavenderblush"
            | "lawngreen"
            | "lemonchiffon"
            | "lightblue"
            | "lightcoral"
            | "lightcyan"
            | "lightgoldenrodyellow"
            | "lightgray"
            | "lightgreen"
            | "lightgrey"
            | "lightpink"
            | "lightsalmon"
            | "lightseagreen"
            | "lightskyblue"
            | "lightslategray"
            | "lightslategrey"
            | "lightsteelblue"
            | "lightyellow"
            | "lime"
            | "limegreen"
            | "linen"
            | "magenta"
            | "maroon"
            | "mediumaquamarine"
            | "mediumblue"
            | "mediumorchid"
            | "mediumpurple"
            | "mediumseagreen"
            | "mediumslateblue"
            | "mediumspringgreen"
            | "mediumturquoise"
            | "mediumvioletred"
            | "midnightblue"
            | "mintcream"
            | "mistyrose"
            | "moccasin"
            | "navajowhite"
            | "navy"
            | "oldlace"
            | "olive"
            | "olivedrab"
            | "orange"
            | "orangered"
            | "orchid"
            | "palegoldenrod"
            | "palegreen"
            | "paleturquoise"
            | "palevioletred"
            | "papayawhip"
            | "peachpuff"
            | "peru"
            | "pink"
            | "plum"
            | "powderblue"
            | "purple"
            | "rebeccapurple"
            | "red"
            | "rosybrown"
            | "royalblue"
            | "saddlebrown"
            | "salmon"
            | "sandybrown"
            | "seagreen"
            | "seashell"
            | "sienna"
            | "silver"
            | "skyblue"
            | "slateblue"
            | "slategray"
            | "slategrey"
            | "snow"
            | "springgreen"
            | "steelblue"
            | "tan"
            | "teal"
            | "thistle"
            | "tomato"
            | "transparent"
            | "turquoise"
            | "violet"
            | "wheat"
            | "white"
            | "whitesmoke"
            | "yellow"
            | "yellowgreen"
    )
}

/// Parse fragment start keywords: loop, alt, opt, par, critical, break, rect.
fn parse_sequence_fragment_start(line: &str) -> Option<SequenceStatement> {
    let (kind, rest) = if let Some(r) = line.strip_prefix("loop") {
        (fm_core::FragmentKind::Loop, r)
    } else if let Some(r) = line.strip_prefix("alt") {
        (fm_core::FragmentKind::Alt, r)
    } else if let Some(r) = line.strip_prefix("opt") {
        (fm_core::FragmentKind::Opt, r)
    } else if let Some(r) = line.strip_prefix("par") {
        (fm_core::FragmentKind::Par, r)
    } else if let Some(r) = line.strip_prefix("critical") {
        (fm_core::FragmentKind::Critical, r)
    } else if let Some(r) = line.strip_prefix("break") {
        (fm_core::FragmentKind::Break, r)
    } else if let Some(r) = line.strip_prefix("rect") {
        (fm_core::FragmentKind::Rect, r)
    } else {
        return None;
    };

    // Ensure keyword boundary (not a prefix of a longer word)
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }

    let label = rest.trim().to_string();
    Some(SequenceStatement::FragmentStart { kind, label })
}

fn lower_sequence_statement(
    statement: SequenceStatement,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) {
    match statement {
        SequenceStatement::Participant(declaration) => {
            if register_participant(&declaration, false, line_number, source_line, builder) {
                // Track in current box group if one is open
                if let Some(parsed) = parse_sequence_participant_declaration(&declaration, false) {
                    builder.track_participant_in_group(&parsed.id);
                }
            } else {
                builder.add_warning(format!(
                    "Line {line_number}: unable to parse participant declaration: {}",
                    source_line.trim()
                ));
            }
        }
        SequenceStatement::Actor(declaration) => {
            if register_participant(&declaration, true, line_number, source_line, builder) {
                // Track in current box group if one is open
                if let Some(parsed) = parse_sequence_participant_declaration(&declaration, true) {
                    builder.track_participant_in_group(&parsed.id);
                }
            } else {
                builder.add_warning(format!(
                    "Line {line_number}: unable to parse actor declaration: {}",
                    source_line.trim()
                ));
            }
        }
        SequenceStatement::Message(statement) => {
            let _ = lower_sequence_message(&statement, line_number, source_line, builder);
        }
        SequenceStatement::Autonumber { start, increment } => {
            builder.enable_autonumber_with(start.unwrap_or(1), increment.unwrap_or(1));
        }
        SequenceStatement::HideFootbox => {
            builder.hide_sequence_footbox();
        }
        SequenceStatement::Link { actor, label, url } => {
            if is_safe_click_target(&url) {
                builder.add_node_menu_link(
                    &actor,
                    &label,
                    &url,
                    span_for(line_number, source_line),
                );
            } else {
                builder.add_warning(format!(
                    "Line {line_number}: unsafe sequence actor menu link blocked: {url}"
                ));
            }
        }
        SequenceStatement::Links { actor, entries } => {
            let span = span_for(line_number, source_line);
            for (label, url) in entries {
                if !is_safe_click_target(&url) {
                    builder.add_warning(format!(
                        "Line {line_number}: unsafe sequence actor menu link blocked: {url}"
                    ));
                    continue;
                }
                builder.add_node_menu_link(&actor, &label, &url, span);
            }
        }
        SequenceStatement::Note {
            position,
            participants,
            text,
        } => {
            builder.add_sequence_note(position, &participants, text);
        }
        SequenceStatement::Activate(name) => {
            builder.activate_participant(&name);
        }
        SequenceStatement::Deactivate(name) => {
            builder.deactivate_participant(&name);
        }
        SequenceStatement::BoxStart { label, color } => {
            builder.begin_participant_group(label, color);
        }
        SequenceStatement::CreateParticipant(declaration) => {
            let span = span_for(line_number, source_line);
            let actor_keyword = source_line.trim_start().starts_with("create actor ");
            if register_participant(
                &declaration,
                actor_keyword,
                line_number,
                source_line,
                builder,
            ) {
                if let Some(parsed) =
                    parse_sequence_participant_declaration(&declaration, actor_keyword)
                {
                    builder.add_lifecycle_create(&parsed.id);
                }
            } else {
                builder.add_warning(format!(
                    "Line {line_number}: unable to parse create participant: {}",
                    source_line.trim()
                ));
                let _ = span; // suppress unused warning
            }
        }
        SequenceStatement::Destroy(name) => {
            builder.add_lifecycle_destroy(&name);
        }
        SequenceStatement::FragmentStart { kind, label } => {
            if kind == fm_core::FragmentKind::Rect {
                builder.begin_fragment(kind, String::new(), Some(label));
            } else {
                builder.begin_fragment(kind, label, None);
            }
        }
        SequenceStatement::FragmentElse { label } => {
            builder.add_fragment_alternative(label);
        }
        SequenceStatement::FragmentEnd => {
            // `end` can close either a fragment or a box group;
            // try fragment first, fall back to box group.
            if !builder.end_fragment() {
                builder.end_participant_group();
            }
        }
    }
}

fn parse_class(input: &str, builder: &mut IrBuilder) {
    let mut in_block: Option<String> = None; // Currently open class block name
    // Stack of (namespace_name, subgraph_index) for nested namespace blocks.
    let mut namespace_stack: Vec<(String, usize)> = Vec::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("classDiagram") {
            continue;
        }

        // Handle namespace blocks: `namespace Name {`
        if trimmed.starts_with("namespace ") && trimmed.ends_with('{') {
            let ns_name = trimmed
                .strip_prefix("namespace")
                .unwrap_or("")
                .trim()
                .trim_end_matches('{')
                .trim();
            if !ns_name.is_empty() {
                let span = span_for(line_number, line);
                let parent = namespace_stack.last().map(|(_, idx)| *idx);
                let cluster_index = builder.ensure_cluster(ns_name, Some(ns_name), span);
                if let Some(sg_idx) = builder.ensure_subgraph(
                    ns_name,
                    ns_name,
                    Some(ns_name),
                    span,
                    parent,
                    cluster_index,
                ) {
                    namespace_stack.push((ns_name.to_string(), sg_idx));
                }
            }
            continue;
        }

        // Close namespace or class block
        if trimmed == "}" {
            if in_block.is_some() {
                in_block = None;
            } else if !namespace_stack.is_empty() {
                namespace_stack.pop();
            }
            continue;
        }

        // Inside a class block: parse member declarations
        if let Some(ref class_name) = in_block {
            if let Some(member) = parse_class_member(trimmed) {
                let cn = class_name.clone();
                lower_class_statement(ClassStatement::Member(member), line_number, line, builder);
                let _ = cn; // class_name is used via the builder's current_class context
            }
            continue;
        }

        let Some(statements) = parse_class_statements(trimmed, builder.parser_config()) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported class syntax: {trimmed}"
            ));
            continue;
        };

        for statement in statements {
            if let ClassStatement::BlockStart(ref name, _) = statement {
                in_block = Some(name.clone());
            }

            // If inside a namespace, track node key to add to subgraph after lowering.
            let ns_node_key = match (&statement, namespace_stack.last()) {
                (ClassStatement::Node(node), Some(_)) => Some(node.id.clone()),
                (ClassStatement::BlockStart(name, _), Some(_)) => Some(name.clone()),
                _ => None,
            };
            let ns_sg_idx = namespace_stack.last().map(|(_, idx)| *idx);

            lower_class_statement(statement, line_number, line, builder);

            if let (Some(key), Some(sg_idx)) = (ns_node_key, ns_sg_idx)
                && let Some(node_id) = builder.node_id_by_key(&key).copied()
            {
                builder.add_node_to_subgraph(sg_idx, node_id);
            }
        }
    }
}

/// Extract generic type parameters from a class name using `~T~` syntax.
///
/// Examples:
/// - `"List~T~"` → `("List", vec!["T"])`
/// - `"Map~K,V~"` → `("Map", vec!["K", "V"])`
/// - `"Animal"` → `("Animal", vec![])`
fn extract_class_generics(raw_name: &str) -> (&str, Vec<String>) {
    let Some(tilde_start) = raw_name.find('~') else {
        return (raw_name, Vec::new());
    };
    let Some(tilde_end) = raw_name[tilde_start + 1..].find('~') else {
        return (raw_name, Vec::new());
    };
    let class_name = &raw_name[..tilde_start];
    let generics_str = &raw_name[tilde_start + 1..tilde_start + 1 + tilde_end];
    let generics: Vec<String> = generics_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (class_name, generics)
}

/// Parse a class member declaration like `+String name`, `-int age`, `#doSomething() void`.
fn parse_class_member(line: &str) -> Option<fm_core::IrClassMember> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Parse visibility prefix
    let (visibility, rest) = match trimmed.as_bytes().first() {
        Some(b'+') => (fm_core::ClassVisibility::Public, &trimmed[1..]),
        Some(b'-') => (fm_core::ClassVisibility::Private, &trimmed[1..]),
        Some(b'#') => (fm_core::ClassVisibility::Protected, &trimmed[1..]),
        Some(b'~') => (fm_core::ClassVisibility::Package, &trimmed[1..]),
        _ => (fm_core::ClassVisibility::Public, trimmed),
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    // Check if it's a method (contains parentheses)
    let is_method = rest.contains('(');

    // Check for static ($) and abstract (*) markers at end
    let is_static = rest.ends_with('$');
    let is_abstract = rest.ends_with('*');
    let rest = rest.trim_end_matches('$').trim_end_matches('*').trim();

    // Parse type annotation
    let (name_part, return_type) = if let Some((name, typ)) = rest.rsplit_once(':') {
        // Colon-separated: `name : Type` or `method() : ReturnType`
        (name.trim(), Some(typ.trim().to_string()))
    } else if is_method {
        // For methods without colon, check for return type after closing paren
        // e.g., `eat() void` → name="eat()", return_type=Some("void")
        if let Some(paren_end) = rest.rfind(')') {
            let after_paren = rest[paren_end + 1..].trim();
            if after_paren.is_empty() {
                (rest, None)
            } else {
                (&rest[..=paren_end], Some(after_paren.to_string()))
            }
        } else {
            (rest, None)
        }
    } else {
        (rest, None)
    };

    let name = name_part.to_string();

    let kind = if is_method {
        fm_core::ClassMemberKind::Method
    } else {
        fm_core::ClassMemberKind::Attribute
    };

    Some(fm_core::IrClassMember {
        visibility,
        kind,
        name,
        return_type,
        is_static,
        is_abstract,
    })
}

/// Strip quoted cardinality labels from a class-diagram relationship line.
///
/// Input:  `ClassA "1" --> "*" ClassB : label`
/// Output: `("ClassA --> ClassB : label", Some("1"), Some("*"))`
///
/// The source cardinality is a quoted string after the left-hand class name
/// and before the operator.  The target cardinality is a quoted string after
/// the operator and before the right-hand class name.
fn strip_class_cardinality(statement: &str) -> (String, Option<String>, Option<String>) {
    let mut source_card = None;
    let mut target_card = None;

    // Find operator position.
    let op_pos = find_operator(statement, &CLASS_OPERATORS);
    let Some((op_idx, op_str, _)) = op_pos else {
        return (statement.to_string(), None, None);
    };

    let left = &statement[..op_idx];
    let right = &statement[op_idx + op_str.len()..];

    // Extract source cardinality from left side: `ClassA "1"` → `ClassA`, `"1"`
    let cleaned_left;
    if let Some(q_start) = left.rfind('"') {
        let before_last_quote = &left[..q_start];
        if let Some(q_open) = before_last_quote.rfind('"') {
            source_card = Some(left[q_open + 1..q_start].to_string());
            cleaned_left = format!("{}{}", left[..q_open].trim_end(), " ");
        } else {
            cleaned_left = left.to_string();
        }
    } else {
        cleaned_left = left.to_string();
    }

    // Extract target cardinality from right side: `"*" ClassB : label` → `ClassB : label`, `"*"`
    let cleaned_right;
    let trimmed_right = right.trim_start();
    if let Some(after_quote) = trimmed_right.strip_prefix('"') {
        if let Some(close_quote) = after_quote.find('"') {
            target_card = Some(after_quote[..close_quote].to_string());
            cleaned_right = after_quote[close_quote + 1..].to_string();
        } else {
            cleaned_right = right.to_string();
        }
    } else {
        cleaned_right = right.to_string();
    }

    if source_card.is_none() && target_card.is_none() {
        return (statement.to_string(), None, None);
    }

    let result = format!("{}{}{}", cleaned_left.trim_end(), op_str, cleaned_right);
    (result, source_card, target_card)
}

fn parse_class_statements(line: &str, config: &ParserConfig) -> Option<Vec<ClassStatement>> {
    if line.starts_with("class ") && line.ends_with('{') {
        let raw_name = line
            .trim_start_matches("class")
            .trim()
            .trim_end_matches('{')
            .trim();
        let (class_name, generics) = extract_class_generics(raw_name);
        return Some(vec![ClassStatement::BlockStart(
            class_name.to_string(),
            generics,
        )]);
    }

    if line.starts_with('}') {
        return Some(vec![ClassStatement::End]);
    }

    // Parse stereotype annotations: `<<interface>> ClassName` or
    // `class ClassName` followed by `<<stereotype>>`
    if line.starts_with("<<")
        && let Some(end) = line.find(">>")
    {
        let annotation = &line[2..end];
        let class_name = line[end + 2..].trim().to_string();
        let stereotype = match annotation.to_lowercase().as_str() {
            "interface" => fm_core::ClassStereotype::Interface,
            "abstract" => fm_core::ClassStereotype::Abstract,
            "enum" | "enumeration" => fm_core::ClassStereotype::Enum,
            "service" => fm_core::ClassStereotype::Service,
            _ => fm_core::ClassStereotype::Custom(annotation.to_string()),
        };
        if !class_name.is_empty() {
            return Some(vec![ClassStatement::Stereotype(class_name, stereotype)]);
        }
    }

    let mut statements = Vec::new();
    for statement in split_statements(line) {
        if let Some(ast) = parse_class_assignment_ast(statement) {
            statements.push(ClassStatement::Ast(ast));
            continue;
        }

        // `class Name` or `class Name~T~` without braces: declare a class node
        if statement.starts_with("class ") {
            let rest = statement.strip_prefix("class ").unwrap_or("").trim();
            if !rest.is_empty() && !rest.contains("--") && !rest.contains("..") {
                let (clean_name, generics) = extract_class_generics(rest);
                if let Some(node) = parse_node_token_with_config(clean_name, config) {
                    if generics.is_empty() {
                        statements.push(ClassStatement::Node(node));
                    } else {
                        // For inline generics, use BlockStart+End to carry them.
                        statements.push(ClassStatement::BlockStart(node.id.clone(), generics));
                        statements.push(ClassStatement::End);
                    }
                    continue;
                }
            }
        }

        // Try edge parsing — first strip cardinality labels if present.
        let (cleaned_statement, source_card, target_card) = strip_class_cardinality(statement);
        let edge_input = if source_card.is_some() || target_card.is_some() {
            cleaned_statement.as_str()
        } else {
            statement
        };
        if let Some(asts) =
            parse_edge_statement_asts(edge_input, &CLASS_OPERATORS, false, config, 0)
        {
            for ast in asts {
                statements.push(ClassStatement::Ast(ast));
                // Attach cardinality to this edge in lower_class_statement.
                if source_card.is_some() || target_card.is_some() {
                    statements.push(ClassStatement::Cardinality(
                        source_card.clone(),
                        target_card.clone(),
                    ));
                }
            }
            continue;
        }
        if let Some(node) = parse_node_token_with_config(statement, config) {
            statements.push(ClassStatement::Node(node));
        }
    }

    (!statements.is_empty()).then_some(statements)
}

fn lower_class_statement(
    statement: ClassStatement,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) {
    match statement {
        ClassStatement::BlockStart(class_name, generics) => {
            if let Some(node) = parse_node_token_with_config(&class_name, builder.parser_config()) {
                let span = span_for(line_number, source_line);
                let _ = intern_node_token(builder, &node, span);
                builder.set_current_class(&node.id);
                if !generics.is_empty() {
                    builder.set_class_generics(&node.id, generics);
                }
            }
        }
        ClassStatement::Ast(ast) => {
            lower_flow_ast(&ast, line_number, source_line, builder, &[], &[]);
        }
        ClassStatement::Node(node) => {
            let span = span_for(line_number, source_line);
            let _ = intern_node_token(builder, &node, span);
        }
        ClassStatement::Member(member) => {
            builder.add_class_member(member);
        }
        ClassStatement::Stereotype(class_name, stereotype) => {
            builder.set_class_stereotype(&class_name, stereotype);
        }
        ClassStatement::Cardinality(source, target) => {
            builder.set_last_edge_cardinality(source.as_deref(), target.as_deref());
        }
        ClassStatement::End => {
            builder.clear_current_class();
        }
    }
}

fn parse_state(input: &str, builder: &mut IrBuilder) {
    // Multi-line note accumulator: (target, position, lines, start_line_number)
    let mut note_block: Option<(String, String, Vec<String>, usize)> = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();

        // Inside a multi-line note block: collect until `end note`.
        if let Some((target, position, mut lines, start)) = note_block.take() {
            if trimmed == "end note" {
                let text = lines.join("\n");
                let span = span_for(start, "");
                builder.ir_mut().state_notes.push(fm_core::IrStateNote {
                    target: target.clone(),
                    position,
                    text,
                    span,
                });
                let _ = builder.intern_node(&target, None, NodeShape::Rounded, span);
            } else {
                lines.push(trimmed.to_string());
                note_block = Some((target, position, lines, start));
            }
            continue;
        }

        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("stateDiagram") {
            continue;
        }

        // Skip bare opening braces (composite state starts are handled on the declaration line).
        if trimmed == "{" {
            continue;
        }

        // Check for multi-line note start: `note right of StateName` (no colon)
        if trimmed.starts_with("note ") && !trimmed.contains(':') {
            let rest = trimmed.strip_prefix("note ").unwrap_or("");
            let (position, after_pos) = if let Some(r) = rest.strip_prefix("right of ") {
                ("right".to_string(), r)
            } else if let Some(r) = rest.strip_prefix("left of ") {
                ("left".to_string(), r)
            } else {
                // Not a recognized note format, fall through to normal parsing.
                ("right".to_string(), rest)
            };
            let target = after_pos.trim().to_string();
            if !target.is_empty() {
                note_block = Some((target, position, Vec::new(), line_number));
                continue;
            }
        }

        let Some(statements) = parse_state_statements(trimmed, builder.parser_config()) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported state syntax: {trimmed}"
            ));
            continue;
        };

        for statement in statements {
            lower_state_statement(statement, line_number, line, builder);
        }
    }
}

fn parse_state_statements(line: &str, config: &ParserConfig) -> Option<Vec<StateStatement>> {
    if line.starts_with("direction ") {
        return parse_graph_direction(line)
            .map(|direction| vec![StateStatement::Direction(direction)]);
    }

    // Note syntax: `note right of StateName : text` or `note left of StateName : text`
    if line.starts_with("note ")
        && let Some(note) = parse_state_note(line)
    {
        return Some(vec![note]);
    }

    // Composite state: `state "Label" as Name {` or `state Name {`
    if let Some(declaration) = line.strip_prefix("state ") {
        let declaration = declaration.trim();
        if declaration.ends_with('{') {
            let name = declaration.trim_end_matches('{').trim();
            let composite = parse_state_composite_declaration(name)?;
            return Some(vec![
                StateStatement::Declaration(name.to_string()),
                StateStatement::CompositeStart(composite),
            ]);
        }
        if !declaration.is_empty() {
            return Some(vec![StateStatement::Declaration(declaration.to_string())]);
        }
    }

    if line == "}" {
        return Some(vec![StateStatement::CompositeEnd]);
    }

    if line == "--" {
        return Some(vec![StateStatement::RegionSeparator]);
    }

    // `[*]` standalone is handled as a node in the edge parsing
    let mut statements = Vec::new();
    for statement in split_statements(line) {
        if let Some(asts) = parse_edge_statement_asts(statement, &FLOW_OPERATORS, false, config, 0)
        {
            statements.push(StateStatement::Edge(asts));
            continue;
        }
        if let Some(node) = parse_node_token_with_config(statement, config) {
            statements.push(StateStatement::Node(node));
        }
    }

    (!statements.is_empty()).then_some(statements)
}

fn parse_state_note(line: &str) -> Option<StateStatement> {
    let rest = line.strip_prefix("note ")?;
    let (position, after_pos) = if let Some(r) = rest.strip_prefix("right of ") {
        ("right".to_string(), r)
    } else if let Some(r) = rest.strip_prefix("left of ") {
        ("left".to_string(), r)
    } else {
        return None;
    };

    let (target, text) = after_pos.split_once(':')?;
    Some(StateStatement::Note {
        target: target.trim().to_string(),
        position,
        text: text.trim().to_string(),
    })
}

fn lower_state_statement(
    statement: StateStatement,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);
    match statement {
        StateStatement::Direction(direction) => builder.set_direction(direction),
        StateStatement::Declaration(declaration) => {
            let _ = register_state_declaration(&declaration, line_number, source_line, builder);
        }
        StateStatement::Edge(asts) => {
            for ast in asts {
                lower_state_flow_ast(&ast, line_number, source_line, builder, span);
            }
        }
        StateStatement::Node(node) => {
            lower_state_node(
                &node.id,
                node.label.as_ref().map(ParsedLabel::as_str),
                node.shape,
                builder,
                span,
            );
        }
        StateStatement::CompositeStart(composite) => {
            builder.begin_state_cluster(&composite.id, composite.title.as_deref(), span);
        }
        StateStatement::CompositeEnd => {
            if !builder.end_state_cluster() {
                builder.add_warning(format!(
                    "Line {line_number}: encountered '}}' without matching composite state"
                ));
            }
        }
        StateStatement::RegionSeparator => {
            if !builder.advance_state_region(span) {
                builder.add_warning(format!(
                    "Line {line_number}: encountered '--' outside a composite state"
                ));
            }
        }
        StateStatement::Note {
            target,
            position,
            text,
        } => {
            let span = span_for(line_number, source_line);
            let _ = builder.intern_node(&target, None, NodeShape::Rounded, span);
            builder.ir_mut().state_notes.push(fm_core::IrStateNote {
                target,
                position,
                text,
                span,
            });
        }
    }
}

fn lower_state_flow_ast(
    ast: &FlowAst,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
    span: Span,
) {
    match ast {
        FlowAst::Node(node) => {
            lower_state_node(
                &node.id,
                node.label.as_ref().map(ParsedLabel::as_str),
                node.shape,
                builder,
                span,
            );
        }
        FlowAst::Edge {
            from,
            arrow,
            label,
            to,
        } => {
            let (from_id_key, from_label, from_shape) = state_edge_endpoint(
                &from.id,
                from.label.as_ref().map(ParsedLabel::as_str),
                from.shape,
                true,
            );
            let (to_id_key, to_label, to_shape) = state_edge_endpoint(
                &to.id,
                to.label.as_ref().map(ParsedLabel::as_str),
                to.shape,
                false,
            );
            let from_id = builder.intern_node(from_id_key, from_label, from_shape, span);
            let to_id = builder.intern_node(to_id_key, to_label, to_shape, span);
            if let (Some(f), Some(t)) = (from_id, to_id) {
                builder.attach_state_node(f);
                builder.attach_state_node(t);

                // Extract guard [condition] and action / action() from label.
                let (clean_label, guard, action) = extract_state_guard_action(label.as_deref());
                builder.push_edge(f, t, *arrow, clean_label.as_deref(), span);
                if (guard.is_some() || action.is_some())
                    && let Some(edge) = builder.ir_mut().edges.last_mut()
                {
                    edge.guard = guard;
                    edge.action = action;
                }
            }
        }
        _ => {
            lower_flow_ast(ast, line_number, source_line, builder, &[], &[]);
        }
    }
}

/// Extract guard `[condition]` and action `/ action()` from a state transition label.
///
/// Input:  `"complete [isValid] / cleanup()"`
/// Output: `(Some("complete"), Some("isValid"), Some("cleanup()"))`
fn extract_state_guard_action(
    label: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(label) = label else {
        return (None, None, None);
    };
    let label = label.trim();
    if label.is_empty() {
        return (None, None, None);
    }

    let mut clean = label.to_string();
    let mut guard = None;
    let mut action = None;

    // Extract guard: [condition]
    if let Some(bracket_start) = clean.find('[')
        && let Some(bracket_end) = clean[bracket_start..].find(']')
    {
        guard = Some(
            clean[bracket_start + 1..bracket_start + bracket_end]
                .trim()
                .to_string(),
        );
        clean = format!(
            "{}{}",
            clean[..bracket_start].trim_end(),
            clean[bracket_start + bracket_end + 1..].trim_start()
        );
    }

    // Extract action: / action()
    if let Some(slash_pos) = clean.find(" / ") {
        action = Some(clean[slash_pos + 3..].trim().to_string());
        clean = clean[..slash_pos].trim().to_string();
    } else if let Some(slash_pos) = clean.find('/') {
        // Also handle without spaces: "/action"
        let after = clean[slash_pos + 1..].trim();
        if !after.is_empty() {
            action = Some(after.to_string());
            clean = clean[..slash_pos].trim().to_string();
        }
    }

    let clean_label = if clean.is_empty() { None } else { Some(clean) };
    (clean_label, guard, action)
}

fn lower_state_node(
    id: &str,
    label: Option<&str>,
    shape: NodeShape,
    builder: &mut IrBuilder,
    span: Span,
) {
    let (id, label, shape) = if id == STATE_PSEUDO_TOKEN {
        (STATE_START_NODE_ID, None, NodeShape::FilledCircle)
    } else {
        (id, label, shape)
    };
    if let Some(node_id) = builder.intern_node(id, label, shape, span) {
        builder.attach_state_node(node_id);
    }
}

fn state_edge_endpoint<'a>(
    id: &'a str,
    label: Option<&'a str>,
    shape: NodeShape,
    is_source: bool,
) -> (&'a str, Option<&'a str>, NodeShape) {
    if id != STATE_PSEUDO_TOKEN {
        return (id, label, shape);
    }

    if is_source {
        (STATE_START_NODE_ID, None, NodeShape::FilledCircle)
    } else {
        (STATE_END_NODE_ID, None, NodeShape::DoubleCircle)
    }
}

fn parse_requirement(input: &str, builder: &mut IrBuilder) {
    let mut inside_requirement_block = false;
    let mut current_req_node: Option<fm_core::IrNodeId> = None;
    let mut current_req_type: Option<String> = None;

    let requirement_keywords = [
        "requirement ",
        "functionalRequirement ",
        "performanceRequirement ",
        "interfaceRequirement ",
        "physicalRequirement ",
        "designConstraint ",
        "element ",
    ];

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "requirementDiagram" {
            continue;
        }

        // Check for any requirement block start.
        let mut matched_keyword = false;
        for keyword in &requirement_keywords {
            if let Some(rest) = trimmed.strip_prefix(keyword) {
                let requirement_name = rest.trim_end_matches('{').trim();
                if let Some(node) =
                    parse_node_token_with_config(requirement_name, builder.parser_config())
                {
                    let span = span_for(line_number, line);
                    current_req_node = builder.intern_node_label(
                        &node.id,
                        node.label.as_ref(),
                        NodeShape::Rect,
                        span,
                    );
                    current_req_type = Some(keyword.trim().to_string());
                    inside_requirement_block = trimmed.ends_with('{');
                    matched_keyword = true;
                    break;
                }
            }
        }
        if matched_keyword {
            continue;
        }

        if trimmed.starts_with('{') {
            inside_requirement_block = true;
            continue;
        }
        if trimmed.starts_with('}') {
            // Finalize the requirement block: store accumulated metadata.
            if let Some(node_id) = current_req_node
                && let Some(node) = builder.node_mut(node_id)
            {
                let meta = node
                    .requirement_meta
                    .get_or_insert_with(fm_core::IrRequirementNodeMeta::default);
                if meta.requirement_type.is_none() {
                    meta.requirement_type = current_req_type.take();
                }
            }
            inside_requirement_block = false;
            current_req_node = None;
            current_req_type = None;
            continue;
        }

        // Extract metadata fields inside a requirement block.
        if inside_requirement_block {
            if let Some(rest) = trimmed.strip_prefix("id:")
                && let Some(node_id) = current_req_node
                && let Some(node) = builder.node_mut(node_id)
            {
                let meta = node
                    .requirement_meta
                    .get_or_insert_with(fm_core::IrRequirementNodeMeta::default);
                meta.req_id = Some(rest.trim().to_string());
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("text:")
                && let Some(node_id) = current_req_node
                && let Some(node) = builder.node_mut(node_id)
            {
                let meta = node
                    .requirement_meta
                    .get_or_insert_with(fm_core::IrRequirementNodeMeta::default);
                meta.text = Some(rest.trim().to_string());
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("risk:")
                && let Some(node_id) = current_req_node
                && let Some(node) = builder.node_mut(node_id)
            {
                let meta = node
                    .requirement_meta
                    .get_or_insert_with(fm_core::IrRequirementNodeMeta::default);
                meta.risk = Some(rest.trim().to_string());
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("verifymethod:")
                && let Some(node_id) = current_req_node
                && let Some(node) = builder.node_mut(node_id)
            {
                let meta = node
                    .requirement_meta
                    .get_or_insert_with(fm_core::IrRequirementNodeMeta::default);
                meta.verify_method = Some(rest.trim().to_string());
                continue;
            }
        }

        if parse_requirement_relation(trimmed, line_number, line, builder) {
            continue;
        }

        builder.add_warning(format!(
            "Line {line_number}: unsupported requirement syntax: {trimmed}"
        ));
    }
}

fn parse_mindmap(input: &str, builder: &mut IrBuilder) {
    let mut ancestry: Vec<(usize, fm_core::IrNodeId)> = Vec::new();
    let mut last_node_id: Option<fm_core::IrNodeId> = None;
    // Track which top-level branch index each node belongs to.
    let mut root_seen = false;
    let mut branch_index: usize = 0;
    // Map from first-level children: depth of root, branch counter.
    let mut root_depth: Option<usize> = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if trimmed == "mindmap" {
            continue;
        }

        // Handle icon directive (::icon(...)) - applies to last node
        if trimmed.starts_with("::icon(") {
            if let Some(node_id) = last_node_id
                && let Some(rest) = trimmed.strip_prefix("::icon(")
                && let Some(icon_body) = rest.strip_suffix(')')
                && let Some(icon) = clean_label(Some(icon_body))
            {
                builder.set_node_icon(node_id, &icon);
            }
            continue;
        }

        // Handle class directive (:::class1 class2) - applies to last node
        if let Some(class_suffix) = trimmed.strip_prefix(":::") {
            let classes = class_suffix.trim();
            if let Some(node_id) = last_node_id {
                let span = span_for(line_number, line);
                for class in classes.split_whitespace() {
                    if let Some(node) = builder.get_node_by_id(node_id) {
                        let key = node.id.clone();
                        builder.add_class_to_node(&key, class, span);
                    }
                }
            }
            continue;
        }

        let depth = leading_indent_width(line);
        let Some(node) = parse_mindmap_node_token(trimmed, builder.parser_config()) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported mindmap syntax: {trimmed}"
            ));
            continue;
        };

        let span = span_for(line_number, line);
        let Some(node_id) = intern_node_token(builder, &node, span) else {
            continue;
        };

        last_node_id = Some(node_id);

        while let Some((ancestor_depth, _)) = ancestry.last() {
            if *ancestor_depth >= depth {
                let _ = ancestry.pop();
            } else {
                break;
            }
        }

        if let Some((_, parent_id)) = ancestry.last().copied() {
            builder.push_edge(parent_id, node_id, ArrowType::Line, None, span);
        }

        ancestry.push((depth, node_id));

        // Assign branch colors: root node is branch 0, each first-level child
        // increments the branch index.
        if !root_seen {
            root_seen = true;
            root_depth = Some(depth);
            builder.add_class_to_node(&node.id, "mindmap-root", span);
        } else if let Some(rd) = root_depth {
            // First-level children: one indent deeper than root.
            if ancestry.len() <= 2 && depth > rd {
                branch_index += 1;
            }
        }
        // Apply branch color class to all nodes.
        let branch_class = format!("mindmap-branch-{}", branch_index % 8);
        builder.add_class_to_node(&node.id, &branch_class, span);
    }
}

/// Parse a mindmap node token with mindmap-specific shapes.
/// Mindmap supports: square [], rounded (), circle (()), bang ))((, cloud )(, hexagon {{}}
fn parse_mindmap_node_token(raw: &str, config: &ParserConfig) -> Option<NodeToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strip class suffix (:::class1 class2) from the node definition
    let core = trimmed.split(":::").next().unwrap_or(trimmed).trim();
    if core.is_empty() {
        return None;
    }

    // Bang shape: id))text((
    if let Some(parsed) = parse_mindmap_bang(core) {
        return Some(parsed);
    }

    // Cloud shape: id)text(
    if let Some(parsed) = parse_mindmap_cloud(core) {
        return Some(parsed);
    }

    // Hexagon shape: id{{text}}
    if let Some(parsed) = parse_mindmap_hexagon(core) {
        return Some(parsed);
    }

    // Circle shape: id((text)) - reuse existing double-circle parser
    if let Some(parsed) = parse_double_circle_with_config(core, config) {
        // For mindmap, (( )) is just a circle, not double-circle
        return Some(NodeToken {
            id: parsed.id,
            label: parsed.label,
            icon: parsed.icon,
            shape: NodeShape::Circle,
        });
    }

    // Rounded shape: id(text)
    if let Some(parsed) = parse_wrapped_with_config(core, '(', ')', NodeShape::Rounded, config) {
        return Some(parsed);
    }

    // Square shape: id[text]
    if let Some(parsed) = parse_wrapped_with_config(core, '[', ']', NodeShape::Rect, config) {
        return Some(parsed);
    }

    // Default shape: plain text (no delimiters)
    let id = normalize_identifier(core);
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(core)).filter(|value| value.text != id);
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape: NodeShape::Rect,
    })
}

/// Parse bang shape: id))text((
fn parse_mindmap_bang(raw: &str) -> Option<NodeToken> {
    let start = raw.find("))")?;
    if !raw.ends_with("((") {
        return None;
    }

    let id_raw = raw[..start].trim();
    let label_raw = raw[start + 2..raw.len().saturating_sub(2)].trim();
    let mut id = normalize_identifier(id_raw);
    if id.is_empty() {
        id = normalize_identifier(label_raw);
    }
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(label_raw));
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape: NodeShape::Asymmetric,
    })
}

/// Parse cloud shape: id)text(
fn parse_mindmap_cloud(raw: &str) -> Option<NodeToken> {
    // Must have ) followed by ( at the end, but NOT )) or ((
    let start = raw.find(')')?;
    if !raw.ends_with('(') {
        return None;
    }
    // Exclude bang shape (starts with )))
    if raw[start..].starts_with("))") {
        return None;
    }
    // Exclude circle shape (ends with (()
    if raw.ends_with("((") {
        return None;
    }

    let id_raw = raw[..start].trim();
    let label_raw = raw[start + 1..raw.len().saturating_sub(1)].trim();
    let mut id = normalize_identifier(id_raw);
    if id.is_empty() {
        id = normalize_identifier(label_raw);
    }
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(label_raw));
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape: NodeShape::Cloud,
    })
}

/// Parse hexagon shape: id{{text}}
fn parse_mindmap_hexagon(raw: &str) -> Option<NodeToken> {
    let start = raw.find("{{")?;
    if !raw.ends_with("}}") {
        return None;
    }

    let id_raw = raw[..start].trim();
    let label_raw = raw[start + 2..raw.len().saturating_sub(2)].trim();
    let mut id = normalize_identifier(id_raw);
    if id.is_empty() {
        id = normalize_identifier(label_raw);
    }
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(label_raw));
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape: NodeShape::Hexagon,
    })
}

fn parse_er(input: &str, builder: &mut IrBuilder) {
    let mut current_entity: Option<IrNodeId> = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "erDiagram" {
            continue;
        }

        // Start of entity block: ENTITY_NAME {
        if trimmed.ends_with('{') {
            let entity_name = trimmed.trim_end_matches('{').trim();
            if let Some(node) = parse_node_token_with_config(entity_name, builder.parser_config()) {
                let span = span_for(line_number, line);
                current_entity = intern_node_token(builder, &node, span);
                continue;
            }
        }

        // End of entity block
        if trimmed.starts_with('}') {
            current_entity = None;
            continue;
        }

        // Relationship line (outside entity block or mixed)
        if parse_er_relationship(trimmed, line_number, line, builder) {
            continue;
        }

        // Inside entity block - parse attribute
        if let Some(entity_id) = current_entity
            && let Some(attr) = parse_er_attribute(trimmed)
        {
            builder.add_entity_attribute(
                entity_id,
                &attr.data_type,
                &attr.name,
                attr.key,
                attr.comment.as_deref(),
            );
            continue;
        }

        // Standalone entity declaration
        if let Some(node) = parse_node_token_with_config(trimmed, builder.parser_config()) {
            let span = span_for(line_number, line);
            let _ = intern_node_token(builder, &node, span);
            continue;
        }

        builder.add_warning(format!(
            "Line {line_number}: unsupported er syntax: {trimmed}"
        ));
    }
}

/// Parsed ER attribute.
struct ErAttribute {
    data_type: String,
    name: String,
    key: IrAttributeKey,
    comment: Option<String>,
}

/// Parse an ER entity attribute line.
///
/// Syntax: `type name [key] ["comment"]`
/// Examples:
/// - `int id PK`
/// - `string name FK "references customer"`
/// - `varchar(255) email UK`
/// - `date created_at`
fn parse_er_attribute(line: &str) -> Option<ErAttribute> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split into parts using robust quoted value extraction to handle escapes correctly
    let mut parts = Vec::new();
    let mut remaining = trimmed;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        if let Some((value, rest)) = extract_quoted_value(remaining) {
            parts.push(value);
            remaining = rest;
        } else {
            let end = remaining
                .find(|c: char| c.is_whitespace())
                .unwrap_or(remaining.len());
            parts.push(remaining[..end].to_string());
            remaining = &remaining[end..];
        }
    }

    // Need at least type and name
    if parts.len() < 2 {
        return None;
    }

    let data_type = parts[0].clone();
    let name = parts[1].clone();

    // Check for key modifier and comment in remaining parts
    let mut key = IrAttributeKey::None;
    let mut comment = None;

    for (i, part) in parts.iter().enumerate().skip(2) {
        let upper = part.to_uppercase();
        match upper.as_str() {
            "PK" => key = IrAttributeKey::Pk,
            "FK" => key = IrAttributeKey::Fk,
            "UK" => key = IrAttributeKey::Uk,
            _ => {
                // If this is not a key and we haven't set a comment, it might be a comment
                if comment.is_none() && i >= 2 {
                    comment = Some(part.clone());
                }
            }
        }
    }

    Some(ErAttribute {
        data_type,
        name,
        key,
        comment,
    })
}

fn parse_requirement_relation(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    // Mermaid requirement relationship syntax:
    //   `A - satisfies -> B`  or  `A -> B`
    // The relationship type label appears between `- ` and ` ->`.

    let Some((left_raw, right_raw)) = statement.split_once("->") else {
        return false;
    };

    // Extract relationship type from `A - type ` prefix.
    let (left_part, relation_label) = if let Some(dash_pos) = left_raw.rfind(" - ") {
        let rel_type = left_raw[dash_pos + 3..].trim();
        let left = left_raw[..dash_pos].trim();
        if rel_type.is_empty() {
            (left_raw.trim(), None)
        } else {
            (left, Some(rel_type))
        }
    } else {
        (left_raw.trim(), None)
    };

    let left_id = left_part
        .split_whitespace()
        .next()
        .map(normalize_identifier)
        .unwrap_or_default();
    let right_id = right_raw
        .split_whitespace()
        .next()
        .map(normalize_identifier)
        .unwrap_or_default();
    if left_id.is_empty() || right_id.is_empty() {
        return false;
    }

    // Map relationship type to arrow style for visual differentiation.
    let arrow = match relation_label {
        Some("contains") => ArrowType::ThickArrow,
        Some("copies" | "derives" | "refines") => ArrowType::DottedArrow,
        _ => ArrowType::Arrow,
    };

    let span = span_for(line_number, source_line);
    let from = builder.intern_node(&left_id, None, NodeShape::Rect, span);
    let to = builder.intern_node(&right_id, None, NodeShape::Rect, span);
    match (from, to) {
        (Some(from_id), Some(to_id)) => {
            builder.push_edge(from_id, to_id, arrow, relation_label, span);
            true
        }
        _ => false,
    }
}

fn parse_class_assignment_ast(statement: &str) -> Option<FlowAst> {
    let rest = statement.strip_prefix("class ")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    let mut parts = rest.split_whitespace();
    let node_list_raw = parts.next()?;
    let class_list_raw = parts.collect::<Vec<_>>().join(" ");
    if class_list_raw.is_empty() {
        return None;
    }

    let nodes: Vec<String> = node_list_raw
        .split(',')
        .map(normalize_identifier)
        .filter(|node_id| !node_id.is_empty())
        .collect();
    if nodes.is_empty() {
        return None;
    }

    let class = class_list_raw.trim().to_string();
    if class.is_empty() {
        return None;
    }

    Some(FlowAst::ClassAssign { nodes, class })
}

fn parse_click_directive_ast(
    statement: &str,
    line_number: usize,
    warnings: &mut Vec<String>,
) -> Option<FlowAst> {
    let rest = statement.strip_prefix("click ")?;

    let Some((node_token, after_node)) = take_token(rest) else {
        warnings.push(format!(
            "Line {line_number}: malformed click directive (missing node id): {statement}"
        ));
        return Some(FlowAst::StyleOrLinkStyle);
    };
    let node = normalize_identifier(node_token);
    if node.is_empty() {
        warnings.push(format!(
            "Line {line_number}: malformed click directive (invalid node id): {statement}"
        ));
        return Some(FlowAst::StyleOrLinkStyle);
    }

    let Some((target_token, after_target)) = take_token(after_node) else {
        warnings.push(format!(
            "Line {line_number}: malformed click directive (missing target): {statement}"
        ));
        return Some(FlowAst::StyleOrLinkStyle);
    };

    let (target, remaining) = if target_token.eq_ignore_ascii_case("href") {
        let Some((href_target, remaining)) = take_token(after_target) else {
            warnings.push(format!(
                "Line {line_number}: malformed click directive (missing href target): {statement}"
            ));
            return Some(FlowAst::StyleOrLinkStyle);
        };
        (href_target.to_string(), remaining)
    } else if target_token.eq_ignore_ascii_case("call")
        || target_token.eq_ignore_ascii_case("callback")
    {
        // Parse callback function name: `click nodeId call functionName ["tooltip"]`
        let Some((fn_name, cb_remaining)) = take_token(after_target) else {
            warnings.push(format!(
                "Line {line_number}: click callback directive missing function name: {statement}"
            ));
            return Some(FlowAst::StyleOrLinkStyle);
        };
        let tooltip = take_token(cb_remaining).map(|(tok, _)| {
            tok.trim_matches('"')
                .trim_matches('\'')
                .trim_matches('`')
                .to_string()
        });
        return Some(FlowAst::ClickDirective {
            node,
            target: fn_name.to_string(),
            tooltip,
            is_callback: true,
        });
    } else {
        (target_token.to_string(), after_target)
    };

    // Optional tooltip: `click nodeId "url" "tooltip"`
    let tooltip = take_token(remaining).map(|(tok, _)| {
        tok.trim_matches('"')
            .trim_matches('\'')
            .trim_matches('`')
            .to_string()
    });

    Some(FlowAst::ClickDirective {
        node,
        target,
        tooltip,
        is_callback: false,
    })
}

fn take_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let first_char = trimmed.chars().next()?;
    if matches!(first_char, '"' | '\'' | '`') {
        for (idx, ch) in trimmed.char_indices().skip(1) {
            if ch == first_char {
                let token = &trimmed[..=idx];
                let rest = &trimmed[idx + 1..];
                return Some((token, rest));
            }
        }
        return Some((trimmed, ""));
    }

    let split_idx = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let token = &trimmed[..split_idx];
    let rest = &trimmed[split_idx..];
    Some((token, rest))
}

fn is_safe_click_target(target: &str) -> bool {
    let decoded = decode_percent_triplets(target);
    // Trim leading/trailing whitespace and control characters that browsers might ignore
    let trimmed = decoded.trim_matches(|c: char| c.is_whitespace() || c.is_control());
    let lower = trimmed.to_ascii_lowercase();

    if let Some(colon_idx) = lower.find(':') {
        let scheme = &lower[..colon_idx];
        // Only allow explicitly safe schemes.
        // This naturally rejects any scheme that contains entities (e.g. `java&#115;cript`)
        // or whitespace (e.g. `java script`) because it won't match the strict string literals.
        matches!(scheme, "http" | "https" | "mailto" | "tel")
    } else {
        // No literal colon found, so it must be a relative path.
        // We must ensure it doesn't hide a colon via XML entities, which the browser
        // would decode and potentially treat as a dangerous scheme.
        if lower.contains("&#") || lower.contains("&colon") {
            return false;
        }
        !trimmed.is_empty()
    }
}

fn decode_percent_triplets(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = decode_hex_nibble(bytes[index + 1]);
            let low = decode_hex_nibble(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).to_string()
}

const fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn parse_journey(input: &str, builder: &mut IrBuilder) {
    let mut previous_step = None;
    let mut current_section: Option<usize> = None;
    let mut current_section_subgraph: Option<usize> = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "journey" {
            continue;
        }

        if let Some(title) = trimmed.strip_prefix("title ")
            && let Some(title) = clean_label(Some(title))
        {
            builder.set_title(title);
            continue;
        }

        let span = span_for(line_number, line);

        if let Some(section_name) = trimmed.strip_prefix("section ") {
            let Some(section_title) = clean_label(Some(section_name)) else {
                builder.add_warning(format!(
                    "Line {line_number}: journey section name is empty: {trimmed}"
                ));
                current_section = None;
                current_section_subgraph = None;
                continue;
            };
            let section_key = format!(
                "journey-section-{}",
                normalize_compound_identifier(&section_title)
            );
            let Some(cluster_index) =
                builder.ensure_cluster(&section_key, Some(&section_title), span)
            else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid journey section identifier: {trimmed}"
                ));
                current_section = None;
                current_section_subgraph = None;
                continue;
            };
            let Some(subgraph_index) = builder.ensure_subgraph(
                &section_key,
                &section_key,
                Some(&section_title),
                span,
                None,
                Some(cluster_index),
            ) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid journey section declaration: {trimmed}"
                ));
                current_section = None;
                current_section_subgraph = None;
                continue;
            };
            current_section = Some(cluster_index);
            current_section_subgraph = Some(subgraph_index);
            continue;
        }

        let Some(step) = parse_journey_step(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported journey syntax: {trimmed}"
            ));
            continue;
        };
        let step_id = normalize_compound_identifier(&step.name);
        if step_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: journey step identifier could not be derived: {trimmed}"
            ));
            continue;
        }

        let current_step =
            builder.intern_node(&step_id, Some(&step.name), NodeShape::Rounded, span);
        if let Some(step_node) = current_step {
            builder.add_class_to_node(&step_id, "journey-step", span);
            if let Some(score) = step.score {
                builder.add_class_to_node(&step_id, &format!("journey-score-{score}"), span);
            }
            for actor in &step.actors {
                builder.add_class_to_node(&step_id, "journey-actor", span);
                builder.add_class_to_node(
                    &step_id,
                    &format!("journey-actor-{}", normalize_compound_identifier(actor)),
                    span,
                );
            }
            if let Some(section_idx) = current_section {
                builder.add_node_to_cluster(section_idx, step_node);
            }
            if let Some(subgraph_idx) = current_section_subgraph {
                builder.add_node_to_subgraph(subgraph_idx, step_node);
            }
        }
        if let (Some(prev), Some(current)) = (previous_step, current_step) {
            builder.push_edge(prev, current, ArrowType::Line, None, span);
        }
        if current_step.is_some() {
            previous_step = current_step;
        }
    }
}

struct JourneyStep {
    name: String,
    score: Option<u8>,
    actors: Vec<String>,
}

fn parse_journey_step(line: &str) -> Option<JourneyStep> {
    let mut segments = line.split(':').map(str::trim);
    let name = clean_label(segments.next())?;

    let score = segments.next().and_then(|raw| raw.parse::<u8>().ok());
    let actors = segments
        .next()
        .map(|raw| {
            raw.split(',')
                .filter_map(|actor| clean_label(Some(actor)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(JourneyStep {
        name,
        score,
        actors,
    })
}

// ---------------------------------------------------------------------------
// Kanban board parser
// ---------------------------------------------------------------------------

fn parse_kanban(input: &str, builder: &mut IrBuilder) {
    let mut current_column: Option<usize> = None;
    let mut current_column_subgraph: Option<usize> = None;
    let mut current_column_indent: Option<usize> = None;
    let mut card_count = 0_usize;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "kanban" || trimmed.starts_with("kanban ") {
            continue;
        }

        let span = span_for(line_number, line);

        // Determine indentation level using standard mindmap/indent utility
        let indent_width = leading_indent_width(line);

        // Strip @{} metadata and parse item.
        let (clean_text, metadata) = strip_kanban_metadata(trimmed);
        let (item_id, item_label) = parse_kanban_item(clean_text);

        // It is a column header if there is no current column, or if the indent
        // is less than or equal to the current column's indent.
        let is_column_header = match current_column_indent {
            None => true,
            Some(col_indent) => indent_width <= col_indent,
        };

        if is_column_header {
            // This is a column header.
            current_column_indent = Some(indent_width);
            let column_key = format!("kanban-col-{}", normalize_compound_identifier(&item_id));
            if column_key == "kanban-col-" {
                builder.add_warning(format!(
                    "Line {line_number}: kanban column name is empty: {trimmed}"
                ));
                continue;
            }

            let Some(cluster_index) = builder.ensure_cluster(&column_key, Some(&item_label), span)
            else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid kanban column identifier: {trimmed}"
                ));
                current_column = None;
                current_column_subgraph = None;
                current_column_indent = None;
                continue;
            };
            let Some(subgraph_index) = builder.ensure_subgraph(
                &column_key,
                &column_key,
                Some(&item_label),
                span,
                None,
                Some(cluster_index),
            ) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid kanban column declaration: {trimmed}"
                ));
                current_column = None;
                current_column_subgraph = None;
                current_column_indent = None;
                continue;
            };
            current_column = Some(cluster_index);
            current_column_subgraph = Some(subgraph_index);

            // Apply WIP limit from @{wip: N} metadata.
            if let Some(wip) = metadata.wip {
                builder.set_cluster_grid_span(cluster_index, wip);
            }
        } else {
            // This is a card within the current column.
            let card_key = if item_id == item_label {
                // No explicit ID; generate one from the label.
                format!("kanban-card-{}", normalize_compound_identifier(&item_label))
            } else {
                item_id.clone()
            };

            if card_key.is_empty() {
                builder.add_warning(format!(
                    "Line {line_number}: kanban card identifier could not be derived: {trimmed}"
                ));
                continue;
            }

            let node_id =
                builder.intern_node(&card_key, Some(&item_label), NodeShape::Rounded, span);
            if let Some(nid) = node_id {
                builder.add_class_to_node(&card_key, "kanban-card", span);

                // Apply priority CSS class.
                if let Some(ref priority) = metadata.priority {
                    builder.add_class_to_node(
                        &card_key,
                        &format!("kanban-priority-{}", priority.to_ascii_lowercase()),
                        span,
                    );
                }
                // Apply assigned CSS class.
                if let Some(ref assigned) = metadata.assigned {
                    let sanitized = assigned
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                        .to_ascii_lowercase();
                    builder.add_class_to_node(
                        &card_key,
                        &format!("kanban-assigned-{sanitized}"),
                        span,
                    );
                }
                // Apply tag classes.
                for tag in &metadata.tags {
                    let sanitized = tag
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                        .to_ascii_lowercase();
                    builder.add_class_to_node(&card_key, &format!("kanban-tag-{sanitized}"), span);
                }

                if let Some(cluster_idx) = current_column {
                    builder.add_node_to_cluster(cluster_idx, nid);
                }
                if let Some(subgraph_idx) = current_column_subgraph {
                    builder.add_node_to_subgraph(subgraph_idx, nid);
                }
                card_count += 1;
            }
        }
    }

    if card_count == 0 && current_column.is_none() {
        builder.add_warning("No kanban columns or cards found");
    }
}

/// Parse a kanban item that may have bracket syntax: `id[Label]` or plain text.
/// Parsed kanban `@{key: value}` metadata.
struct KanbanMetadata {
    priority: Option<String>,
    assigned: Option<String>,
    wip: Option<usize>,
    tags: Vec<String>,
}

/// Strip `@{...}` metadata from end of line and parse key-value pairs.
fn strip_kanban_metadata(text: &str) -> (&str, KanbanMetadata) {
    let mut meta = KanbanMetadata {
        priority: None,
        assigned: None,
        wip: None,
        tags: Vec::new(),
    };

    let Some(at_pos) = text.find("@{") else {
        return (text, meta);
    };
    let Some(close_pos) = text[at_pos..].find('}') else {
        return (text, meta);
    };
    let body = &text[at_pos + 2..at_pos + close_pos];
    let clean_text = text[..at_pos].trim();

    for pair in body.split(',') {
        let pair = pair.trim();
        if let Some((key, value)) = pair.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().trim_matches('"').trim_matches('\'').trim();
            match key.as_str() {
                "priority" => meta.priority = Some(value.to_string()),
                "assigned" | "assign" => meta.assigned = Some(value.to_string()),
                "wip" => meta.wip = value.parse().ok(),
                "tag" | "tags" => {
                    meta.tags.extend(value.split_whitespace().map(String::from));
                }
                _ => {}
            }
        }
    }

    (clean_text, meta)
}

fn parse_kanban_item(text: &str) -> (String, String) {
    // Try to parse id[Label] syntax.
    if let Some(bracket_start) = text.find('[')
        && let Some(bracket_end) = text.rfind(']')
        && bracket_end > bracket_start
    {
        let id = text[..bracket_start].trim().to_string();
        let label = text[bracket_start + 1..bracket_end].trim().to_string();
        if !id.is_empty() && !label.is_empty() {
            return (id, label);
        }
        if !label.is_empty() {
            return (normalize_compound_identifier(&label), label);
        }
    }
    // Plain text: use normalized form as ID.
    let label = text.to_string();
    let id = normalize_compound_identifier(text);
    (id, label)
}

fn parse_timeline(input: &str, builder: &mut IrBuilder) {
    let mut previous_period: Option<IrNodeId> = None;
    let mut current_period: Option<IrNodeId> = None;
    let mut current_section: Option<usize> = None;
    let mut current_section_subgraph: Option<usize> = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        // Skip header
        if trimmed == "timeline" {
            continue;
        }

        if let Some(title_text) = trimmed.strip_prefix("title ") {
            if let Some(title) = clean_label(Some(title_text)) {
                builder.set_title(title);
            }
            continue;
        }

        // Handle section
        if let Some(section_name) = trimmed.strip_prefix("section ") {
            let section_name = section_name.trim();
            let span = span_for(line_number, line);
            current_section = builder.ensure_cluster(section_name, Some(section_name), span);
            current_section_subgraph = current_section.and_then(|section_idx| {
                builder.ensure_subgraph(
                    section_name,
                    section_name,
                    Some(section_name),
                    span,
                    None,
                    Some(section_idx),
                )
            });
            continue;
        }

        let span = span_for(line_number, line);

        // Check if this is a continuation event (starts with :)
        if let Some(continuation) = trimmed.strip_prefix(':') {
            // This is a continuation event for the current period
            if let Some(period_id) = current_period {
                let events_text = continuation.trim();
                parse_timeline_events(
                    events_text,
                    period_id,
                    line_number,
                    line,
                    current_section,
                    current_section_subgraph,
                    builder,
                );
            } else {
                builder.add_warning(format!(
                    "Line {line_number}: continuation event without preceding time period: {trimmed}"
                ));
            }
            continue;
        }

        // This should be a time period line: {period} : {event1} : {event2} ...
        if let Some(colon_pos) = trimmed.find(':') {
            let period_text = trimmed[..colon_pos].trim();
            let events_text = trimmed[colon_pos + 1..].trim();

            if period_text.is_empty() {
                builder.add_warning(format!("Line {line_number}: empty time period: {trimmed}"));
                continue;
            }

            // Create time period node
            let period_id = normalize_identifier(period_text);
            if period_id.is_empty() {
                builder.add_warning(format!(
                    "Line {line_number}: could not derive identifier for time period: {period_text}"
                ));
                continue;
            }

            let period_node =
                builder.intern_node(&period_id, Some(period_text), NodeShape::Rect, span);

            if let Some(period_node_id) = period_node {
                builder.add_class_to_node(&period_id, "timeline-period", span);
                // Section color class for background band differentiation.
                if current_section.is_some() {
                    builder.add_class_to_node(
                        &period_id,
                        &format!("timeline-section-{}", current_section.unwrap_or(0) % 8),
                        span,
                    );
                }

                // Add to current section if any
                if let Some(section_idx) = current_section {
                    builder.add_node_to_cluster(section_idx, period_node_id);
                }
                if let Some(subgraph_idx) = current_section_subgraph {
                    builder.add_node_to_subgraph(subgraph_idx, period_node_id);
                }

                // Link to previous period (timeline sequence)
                if let Some(prev_id) = previous_period {
                    builder.push_edge(prev_id, period_node_id, ArrowType::Arrow, None, span);
                }

                previous_period = Some(period_node_id);
                current_period = Some(period_node_id);

                // Parse events for this period
                if !events_text.is_empty() {
                    parse_timeline_events(
                        events_text,
                        period_node_id,
                        line_number,
                        line,
                        current_section,
                        current_section_subgraph,
                        builder,
                    );
                }
            }
        } else {
            // Line without colon - treat as a time period with no events
            let period_text = trimmed;
            let period_id = normalize_identifier(period_text);
            if period_id.is_empty() {
                builder.add_warning(format!(
                    "Line {line_number}: unsupported timeline syntax: {trimmed}"
                ));
                continue;
            }

            let period_node =
                builder.intern_node(&period_id, Some(period_text), NodeShape::Rect, span);

            if let Some(period_node_id) = period_node {
                if let Some(section_idx) = current_section {
                    builder.add_node_to_cluster(section_idx, period_node_id);
                }
                if let Some(subgraph_idx) = current_section_subgraph {
                    builder.add_node_to_subgraph(subgraph_idx, period_node_id);
                }

                if let Some(prev_id) = previous_period {
                    builder.push_edge(prev_id, period_node_id, ArrowType::Arrow, None, span);
                }

                previous_period = Some(period_node_id);
                current_period = Some(period_node_id);
            }
        }
    }
}

/// Parse events from a timeline event string (possibly colon-separated).
fn parse_timeline_events(
    events_text: &str,
    period_id: IrNodeId,
    line_number: usize,
    source_line: &str,
    current_section: Option<usize>,
    current_section_subgraph: Option<usize>,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);

    // Events can be separated by colons
    for event_text in events_text.split(':') {
        let event_text_trimmed = event_text.trim();
        if event_text_trimmed.is_empty() {
            continue;
        }

        let event_id = normalize_identifier(event_text);
        if event_id.is_empty() {
            continue;
        }

        // Create event node and link to period
        if let Some(event_node_id) =
            builder.intern_node(&event_id, Some(event_text), NodeShape::Rounded, span)
        {
            builder.add_class_to_node(&event_id, "timeline-event", span);
            if let Some(section_idx) = current_section {
                builder.add_node_to_cluster(section_idx, event_node_id);
                builder.add_class_to_node(
                    &event_id,
                    &format!("timeline-section-{}", section_idx % 8),
                    span,
                );
            }
            if let Some(subgraph_idx) = current_section_subgraph {
                builder.add_node_to_subgraph(subgraph_idx, event_node_id);
            }
            // Events are children of their time period
            builder.push_edge(period_id, event_node_id, ArrowType::Line, None, span);
        }
    }
}

fn parse_packet(input: &str, builder: &mut IrBuilder) {
    let mut field_index = 0_usize;
    let mut previous_field: Option<IrNodeId> = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("packet-beta") {
            continue;
        }

        let span = span_for(line_number, line);

        // Parse packet field: `0-7: "Source Port"` or `0-31: Source Port`
        if let Some((range_part, label_part)) = trimmed.split_once(':') {
            let range = range_part.trim();
            let label = label_part
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            if !label.is_empty() {
                let field_id = format!("pkt-field-{field_index}");
                let display_label = if range.contains('-') {
                    format!("{label}\n[{range}]")
                } else {
                    label.to_string()
                };

                if let Some(node_id) =
                    builder.intern_node(&field_id, Some(&display_label), NodeShape::Rect, span)
                {
                    builder.add_class_to_node(&field_id, "packet-field", span);

                    // Parse bit range for width hint.
                    if let Some((start_str, end_str)) = range.split_once('-')
                        && let (Ok(start), Ok(end)) = (
                            start_str.trim().parse::<usize>(),
                            end_str.trim().parse::<usize>(),
                        )
                    {
                        let bit_width = end.saturating_sub(start) + 1;
                        builder.add_class_to_node(
                            &field_id,
                            &format!("packet-bits-{bit_width}"),
                            span,
                        );
                        if bit_width > 8 {
                            builder.add_class_to_node(&field_id, "packet-field-wide", span);
                        }
                    }

                    // Chain fields sequentially.
                    if let Some(prev) = previous_field {
                        builder.push_edge(prev, node_id, ArrowType::Line, None, span);
                    }
                    previous_field = Some(node_id);
                    field_index += 1;
                }
                continue;
            }
        }

        // Fallback: try generic node/edge parsing.
        let mut parsed_line = false;
        for statement in split_statements(trimmed) {
            if parse_edge_statement(statement, line_number, line, &PACKET_OPERATORS, builder) {
                parsed_line = true;
                continue;
            }
            if let Some(node) = parse_node_token_with_config(statement, builder.parser_config()) {
                let _ = intern_node_token(builder, &node, span);
                parsed_line = true;
            }
        }

        if !parsed_line {
            builder.add_warning(format!(
                "Line {line_number}: unsupported packet syntax: {trimmed}"
            ));
        }
    }
}

fn parse_er_relationship(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let (relation, label) = if let Some((left, right)) = statement.split_once(':') {
        (left.trim(), clean_label(Some(right)))
    } else {
        (statement.trim(), None)
    };

    let Some((operator_idx, operator, arrow)) = find_operator(relation, &ER_OPERATORS) else {
        return false;
    };

    let left_raw = relation[..operator_idx].trim();
    let right_raw = relation[operator_idx + operator.len()..].trim();
    if left_raw.is_empty() || right_raw.is_empty() {
        return false;
    }

    let Some(left_node) = parse_node_token_with_config(left_raw, builder.parser_config()) else {
        return false;
    };
    let Some(right_node) = parse_node_token_with_config(right_raw, builder.parser_config()) else {
        return false;
    };

    let span = span_for(line_number, source_line);
    let from = builder.intern_node_label(
        &left_node.id,
        left_node.label.as_ref(),
        NodeShape::Rect,
        span,
    );
    let to = builder.intern_node_label(
        &right_node.id,
        right_node.label.as_ref(),
        NodeShape::Rect,
        span,
    );

    match (from, to) {
        (Some(from_node), Some(to_node)) => {
            builder.push_edge(from_node, to_node, arrow, label.as_deref(), span);
            builder.set_last_edge_er_notation(operator);
            true
        }
        _ => false,
    }
}

fn parse_gantt(input: &str, builder: &mut IrBuilder) {
    let mut gantt_meta = IrGanttMeta::default();
    let mut current_section_idx = 0_usize;
    let mut task_ids_to_nodes: BTreeMap<String, IrNodeId> = BTreeMap::new();
    let mut pending_dependencies: Vec<(IrNodeId, String, Span)> = Vec::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "gantt" {
            continue;
        }

        if let Some(title) = trimmed.strip_prefix("title ") {
            let title = clean_label(Some(title));
            gantt_meta.title = title.clone();
            if let Some(title) = title {
                builder.set_title(title);
            }
            continue;
        }

        if let Some(date_format) = trimmed.strip_prefix("dateFormat ") {
            gantt_meta.date_format = clean_label(Some(date_format));
            continue;
        }

        if let Some(axis_format) = trimmed.strip_prefix("axisFormat ") {
            gantt_meta.axis_format = clean_label(Some(axis_format));
            continue;
        }

        if let Some(tick_interval) = trimmed.strip_prefix("tickInterval ") {
            gantt_meta.tick_interval = parse_gantt_tick_interval(tick_interval);
            if gantt_meta.tick_interval.is_none() {
                builder.add_warning(format!(
                    "Line {line_number}: unsupported gantt tickInterval: {tick_interval}"
                ));
            }
            continue;
        }

        if let Some(excludes) = trimmed.strip_prefix("excludes ") {
            gantt_meta.excludes = parse_gantt_excludes(excludes, gantt_meta.date_format.as_deref());
            if gantt_meta.excludes.is_empty() {
                builder.add_warning(format!(
                    "Line {line_number}: gantt excludes directive is empty"
                ));
            }
            continue;
        }

        if let Some(style) = trimmed.strip_prefix("todayMarker ") {
            gantt_meta.today_marker_style = clean_label(Some(style));
            continue;
        }

        if trimmed == "inclusiveEndDates" {
            gantt_meta.inclusive_end_dates = true;
            continue;
        }

        if let Some(weekday) = trimmed.strip_prefix("weekday ") {
            gantt_meta.weekday_start = parse_gantt_weekday(weekday);
            if gantt_meta.weekday_start.is_none() {
                builder.add_warning(format!(
                    "Line {line_number}: unsupported gantt weekday: {weekday}"
                ));
            }
            continue;
        }

        if let Some(section_name) = trimmed.strip_prefix("section ") {
            let Some(section_name) = clean_label(Some(section_name)) else {
                builder.add_warning(format!("Line {line_number}: gantt section name is empty"));
                continue;
            };
            current_section_idx = gantt_meta.sections.len();
            gantt_meta
                .sections
                .push(IrGanttSection { name: section_name });
            continue;
        }

        let Some((task_name, raw_meta)) = trimmed.split_once(':') else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported gantt syntax: {trimmed}"
            ));
            continue;
        };
        let Some(task_name) = clean_label(Some(task_name)) else {
            builder.add_warning(format!(
                "Line {line_number}: task identifier could not be derived: {trimmed}"
            ));
            continue;
        };

        let task_id_raw = normalize_identifier(&task_name);
        if task_id_raw.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: task identifier could not be derived: {trimmed}"
            ));
            continue;
        }
        let task_id = format!("{task_id_raw}_{line_number}");

        let span = span_for(line_number, line);
        let Some(node) = builder.intern_node(&task_id, Some(&task_name), NodeShape::Rect, span)
        else {
            continue;
        };

        let parsed_meta = parse_gantt_task_metadata(raw_meta, gantt_meta.date_format.as_deref());
        if let Some(task_id_ref) = parsed_meta.task_id.as_ref() {
            task_ids_to_nodes.entry(task_id_ref.clone()).or_insert(node);
        }
        if let Some(after_task_id) = parsed_meta.depends_on.first() {
            pending_dependencies.push((node, after_task_id.clone(), span));
        }

        let mut classes = Vec::new();
        match parsed_meta.task_type {
            GanttTaskType::Done => classes.push("gantt-done"),
            GanttTaskType::Active => classes.push("gantt-active"),
            GanttTaskType::Critical => classes.push("gantt-critical"),
            GanttTaskType::Milestone => classes.push("gantt-milestone"),
            GanttTaskType::Normal => {}
        }
        for class_name in classes {
            builder.add_class_to_node(&task_id, class_name, span);
        }

        gantt_meta.tasks.push(IrGanttTask {
            node,
            section_idx: current_section_idx,
            meta: raw_meta.trim().to_string(),
            task_id: parsed_meta.task_id,
            start: parsed_meta.start,
            end: parsed_meta.end,
            depends_on: parsed_meta.depends_on,
            progress: parsed_meta.progress,
            task_type: parsed_meta.task_type,
        });
    }

    for (node, dependency, span) in pending_dependencies {
        if let Some(from) = task_ids_to_nodes.get(&dependency).copied() {
            builder.push_edge(from, node, ArrowType::Arrow, None, span);
        } else {
            builder.add_warning(format!("Unresolved gantt dependency 'after {dependency}'"));
        }
    }

    if !gantt_meta.sections.is_empty()
        || !gantt_meta.tasks.is_empty()
        || gantt_meta.title.is_some()
        || gantt_meta.date_format.is_some()
        || gantt_meta.axis_format.is_some()
        || gantt_meta.tick_interval.is_some()
        || !gantt_meta.excludes.is_empty()
    {
        builder.set_gantt_meta(gantt_meta);
    }
}

#[derive(Debug, Default)]
struct ParsedGanttTaskMeta {
    task_id: Option<String>,
    start: Option<GanttDate>,
    end: Option<GanttDate>,
    depends_on: Vec<String>,
    progress: Option<f32>,
    task_type: GanttTaskType,
}

fn parse_gantt_task_metadata(raw_meta: &str, date_format: Option<&str>) -> ParsedGanttTaskMeta {
    let mut parsed = ParsedGanttTaskMeta::default();

    for token in raw_meta
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "milestone" => {
                parsed.task_type = GanttTaskType::Milestone;
                continue;
            }
            "active" => {
                parsed.task_type = GanttTaskType::Active;
                continue;
            }
            "done" => {
                parsed.task_type = GanttTaskType::Done;
                continue;
            }
            "crit" | "critical" => {
                parsed.task_type = GanttTaskType::Critical;
                continue;
            }
            _ => {}
        }

        if let Some(after) = lower.strip_prefix("after ") {
            let dependency = normalize_compound_identifier(after);
            if !dependency.is_empty() {
                if parsed.start.is_none() {
                    parsed.start = Some(GanttDate::AfterTask(dependency.clone()));
                }
                parsed.depends_on.push(dependency);
            }
            continue;
        }

        if parsed.start.is_none()
            && let Some(date) = parse_gantt_absolute_date(token, date_format)
        {
            parsed.start = Some(GanttDate::Absolute(date));
            continue;
        }

        if parsed.end.is_none()
            && let Some(duration_days) = parse_gantt_duration_days(token)
        {
            parsed.end = Some(GanttDate::DurationDays(duration_days));
            continue;
        }

        if parsed.end.is_none()
            && let Some(date) = parse_gantt_absolute_date(token, date_format)
        {
            parsed.end = Some(GanttDate::Absolute(date));
            continue;
        }

        if parsed.progress.is_none()
            && let Some(progress) = parse_gantt_progress(token)
        {
            parsed.progress = Some(progress);
            continue;
        }

        if parsed.task_id.is_none() {
            let task_id = normalize_compound_identifier(token);
            if !task_id.is_empty() {
                parsed.task_id = Some(task_id);
            }
        }
    }

    parsed
}

fn parse_gantt_absolute_date(token: &str, date_format: Option<&str>) -> Option<String> {
    normalize_gantt_date_with_format(token, date_format.unwrap_or("YYYY-MM-DD"))
        .or_else(|| normalize_gantt_date_with_format(token, "YYYY-MM-DD"))
}

fn normalize_gantt_date_with_format(token: &str, date_format: &str) -> Option<String> {
    let format_parts = split_gantt_date_parts(date_format, |ch| ch.is_ascii_alphabetic())?;
    let token_parts = split_gantt_date_parts(token, |ch| ch.is_ascii_digit())?;
    if format_parts.len() != 3 || token_parts.len() != 3 || format_parts.len() != token_parts.len()
    {
        return None;
    }

    let mut year = None;
    let mut month = None;
    let mut day = None;
    for (format_part, token_part) in format_parts.iter().zip(token_parts.iter()) {
        match *format_part {
            "YYYY" => year = token_part.parse::<u32>().ok(),
            "MM" => month = token_part.parse::<u32>().ok(),
            "DD" => day = token_part.parse::<u32>().ok(),
            _ => return None,
        }
    }

    let (year, month, day) = (year?, month?, day?);
    if !is_valid_gantt_calendar_date(year, month, day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn is_valid_gantt_calendar_date(year: u32, month: u32, day: u32) -> bool {
    let Some(max_day) = gantt_days_in_month(year, month) else {
        return false;
    };
    (1..=max_day).contains(&day)
}

const fn gantt_days_in_month(year: u32, month: u32) -> Option<u32> {
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_gantt_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    Some(max_day)
}

const fn is_gantt_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn split_gantt_date_parts<F>(value: &str, predicate: F) -> Option<Vec<&str>>
where
    F: Fn(char) -> bool,
{
    let parts = value
        .trim()
        .split(|ch: char| !predicate(ch))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() { None } else { Some(parts) }
}

fn parse_gantt_duration_days(token: &str) -> Option<u32> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let lower = token.to_ascii_lowercase();
    if let Some(days) = lower.strip_suffix('d') {
        return days.trim().parse::<u32>().ok();
    }
    if let Some(weeks) = lower.strip_suffix('w') {
        return weeks.trim().parse::<u32>().ok().map(|weeks| weeks * 7);
    }
    None
}

fn parse_gantt_progress(token: &str) -> Option<f32> {
    let value = token.trim().strip_suffix('%')?.trim();
    let percentage = value.parse::<f32>().ok()?;
    Some((percentage / 100.0).clamp(0.0, 1.0))
}

fn parse_gantt_tick_interval(token: &str) -> Option<GanttTickInterval> {
    match token.trim().to_ascii_lowercase().as_str() {
        "1day" | "day" | "daily" => Some(GanttTickInterval::Day),
        "1week" | "week" | "weekly" => Some(GanttTickInterval::Week),
        "1month" | "month" | "monthly" => Some(GanttTickInterval::Month),
        "1quarter" | "quarter" | "quarterly" => Some(GanttTickInterval::Quarter),
        "1year" | "year" | "yearly" => Some(GanttTickInterval::Year),
        _ => None,
    }
}

fn parse_gantt_excludes(raw: &str, date_format: Option<&str>) -> Vec<GanttExclude> {
    let mut excludes = Vec::new();
    let mut dates = Vec::new();

    for token in raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        if token.eq_ignore_ascii_case("weekends") {
            excludes.push(GanttExclude::Weekends);
        } else {
            dates.push(
                parse_gantt_absolute_date(token, date_format).unwrap_or_else(|| token.to_string()),
            );
        }
    }

    if !dates.is_empty() {
        excludes.push(GanttExclude::Dates(dates));
    }

    excludes
}

fn parse_gantt_weekday(token: &str) -> Option<u8> {
    match token.trim().to_ascii_lowercase().as_str() {
        "sunday" => Some(0),
        "monday" => Some(1),
        "tuesday" => Some(2),
        "wednesday" => Some(3),
        "thursday" => Some(4),
        "friday" => Some(5),
        "saturday" => Some(6),
        _ => None,
    }
}

fn parse_pie(input: &str, builder: &mut IrBuilder) {
    let mut pie_meta = fm_core::IrPieMeta::default();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        // Header line — may include "showData" and/or "title ..." on the same line.
        if trimmed.starts_with("pie") {
            if trimmed.contains("showData") {
                pie_meta.show_data = true;
            }
            // Extract inline title: "pie title My Chart"
            if let Some(title_start) = trimmed.find("title ") {
                let title = trimmed[title_start + "title ".len()..].trim();
                if !title.is_empty() {
                    pie_meta.title = Some(title.to_string());
                    builder.set_title(title.to_string());
                }
            }
            continue;
        }

        // Title directive.
        if let Some(rest) = trimmed.strip_prefix("title ") {
            let title = rest.trim();
            if !title.is_empty() {
                pie_meta.title = Some(title.to_string());
                builder.set_title(title.to_string());
            }
            continue;
        }
        if trimmed == "showData" {
            pie_meta.show_data = true;
            continue;
        }

        // Slice: "Label" : value
        let Some(slice_name) = parse_name_before_colon(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported pie syntax: {trimmed}"
            ));
            continue;
        };

        let slice_id = normalize_identifier(slice_name);
        if slice_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: pie slice identifier could not be derived: {trimmed}"
            ));
            continue;
        }

        // Extract numeric value after the colon.
        let value = match trimmed.split_once(':').map(|(_, v)| v.trim()) {
            Some(v) if !v.is_empty() => {
                if let Ok(n) = v.parse::<f32>() {
                    n
                } else {
                    builder.add_warning(format!(
                        "Line {line_number}: invalid pie slice value '{v}', defaulting to 0"
                    ));
                    0.0
                }
            }
            _ => 0.0,
        };

        pie_meta.slices.push(fm_core::IrPieSlice {
            label: slice_name.to_string(),
            value,
        });

        let span = span_for(line_number, line);
        let _ = builder.intern_node(&slice_id, Some(slice_name), NodeShape::Circle, span);
    }

    if !pie_meta.slices.is_empty() || pie_meta.title.is_some() {
        builder.set_pie_meta(pie_meta);
    }
}

fn parse_quadrant(input: &str, builder: &mut IrBuilder) {
    let mut meta = fm_core::IrQuadrantMeta::default();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if trimmed == "quadrantChart" {
            continue;
        }

        // Title directive.
        if let Some(rest) = trimmed.strip_prefix("title ") {
            let title = rest.trim();
            if !title.is_empty() {
                meta.title = Some(title.to_string());
                builder.set_title(title.to_string());
            }
            continue;
        }

        // Axis labels: "x-axis Label Left --> Label Right"
        if let Some(rest) = trimmed.strip_prefix("x-axis ") {
            let rest = rest.trim();
            if let Some((left, right)) = rest.split_once("-->") {
                meta.x_axis_left = Some(left.trim().to_string());
                meta.x_axis_right = Some(right.trim().to_string());
            } else {
                meta.x_axis_left = Some(rest.to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("y-axis ") {
            let rest = rest.trim();
            if let Some((bottom, top)) = rest.split_once("-->") {
                meta.y_axis_bottom = Some(bottom.trim().to_string());
                meta.y_axis_top = Some(top.trim().to_string());
            } else {
                meta.y_axis_bottom = Some(rest.to_string());
            }
            continue;
        }

        // Quadrant labels: "quadrant-1 Label", "quadrant-2 Label", etc.
        if let Some(rest) = trimmed.strip_prefix("quadrant-") {
            if let Some((_digit, label)) = rest.split_once(' ') {
                let label = label.trim();
                if !label.is_empty() {
                    meta.quadrant_labels.push(label.to_string());
                }
            }
            continue;
        }

        // Data point: "Label: [x, y]"
        let Some(point_name) = parse_name_before_colon(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported quadrant syntax: {trimmed}"
            ));
            continue;
        };

        let point_id = normalize_identifier(point_name);
        if point_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: quadrant point identifier could not be derived: {trimmed}"
            ));
            continue;
        }

        // Parse "[x, y]" after the colon.
        let coords = trimmed.split_once(':').map_or("", |(_, v)| v.trim());
        let coords = coords.trim_start_matches('[').trim_end_matches(']').trim();
        let (x, y) = if let Some((xs, ys)) = coords.split_once(',') {
            (
                xs.trim().parse::<f32>().unwrap_or(0.5),
                ys.trim().parse::<f32>().unwrap_or(0.5),
            )
        } else {
            // Single value: try to parse as x, default y=0.5
            let x = coords.parse::<f32>().unwrap_or(0.5);
            (x, 0.5)
        };

        meta.points.push(fm_core::IrQuadrantPoint {
            label: point_name.to_string(),
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
        });

        let span = span_for(line_number, line);
        let _ = builder.intern_node(&point_id, Some(point_name), NodeShape::Circle, span);
    }

    if !meta.points.is_empty()
        || meta.title.is_some()
        || !meta.quadrant_labels.is_empty()
        || meta.x_axis_left.is_some()
    {
        builder.set_quadrant_meta(meta);
    }
}

fn parse_xychart(input: &str, builder: &mut IrBuilder) {
    let mut xy_chart_meta = IrXyChartMeta::default();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("xychart") {
            continue;
        }

        if let Some(title) = trimmed.strip_prefix("title ") {
            let title = clean_label(Some(title));
            xy_chart_meta.title = title.clone();
            if let Some(title) = title {
                builder.set_title(title);
            }
            continue;
        }

        if lower.starts_with("x-axis ") {
            xy_chart_meta.x_axis = parse_xychart_axis(&trimmed["x-axis ".len()..]);
            continue;
        }

        if lower.starts_with("y-axis ") {
            xy_chart_meta.y_axis = parse_xychart_axis(&trimmed["y-axis ".len()..]);
            continue;
        }

        let Some((series_kind, series_name, raw_values)) = parse_xychart_series(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported xychart syntax: {trimmed}"
            ));
            continue;
        };

        let values = parse_xychart_numeric_values(raw_values, line_number, trimmed, builder);
        if values.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: xychart series is missing values: {trimmed}"
            ));
            continue;
        }

        let span = span_for(line_number, line);
        let base_name = series_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(series_kind);
        let base_id = normalize_compound_identifier(base_name);
        let mut previous_node = None;
        let mut series_nodes = Vec::with_capacity(values.len());

        for (point_index, value) in values.iter().enumerate() {
            let x_label = xy_chart_meta
                .x_axis
                .categories
                .get(point_index)
                .cloned()
                .unwrap_or_else(|| (point_index + 1).to_string());
            let node_label = format!("{base_name} {x_label}: {value}");
            let node_id = format!("{base_id}_{}", point_index + 1);
            let shape = match series_kind {
                "bar" => NodeShape::Rect,
                _ => NodeShape::Circle,
            };
            let Some(node) = builder.intern_node(&node_id, Some(&node_label), shape, span) else {
                continue;
            };
            series_nodes.push(node);

            if matches!(series_kind, "line" | "area")
                && let Some(previous) = previous_node
            {
                builder.push_edge(previous, node, ArrowType::Line, None, span);
            }
            previous_node = Some(node);
        }

        xy_chart_meta.series.push(IrXySeries {
            kind: match series_kind {
                "line" => IrXySeriesKind::Line,
                "area" => IrXySeriesKind::Area,
                _ => IrXySeriesKind::Bar,
            },
            name: series_name,
            values,
            nodes: series_nodes,
        });
    }

    if xy_chart_meta.title.is_some()
        || !xy_chart_meta.x_axis.categories.is_empty()
        || xy_chart_meta.x_axis.min.is_some()
        || xy_chart_meta.x_axis.max.is_some()
        || xy_chart_meta.y_axis.label.is_some()
        || xy_chart_meta.y_axis.min.is_some()
        || xy_chart_meta.y_axis.max.is_some()
        || !xy_chart_meta.series.is_empty()
    {
        builder.set_xy_chart_meta(xy_chart_meta);
    }
}

fn parse_sankey(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if is_sankey_header(&lower) || lower.starts_with("title ") {
            continue;
        }

        let Some((source, target, value)) = parse_sankey_record(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported sankey syntax: {trimmed}"
            ));
            continue;
        };

        let span = span_for(line_number, line);
        let Some(source_label) = clean_label(Some(&source)) else {
            builder.add_warning(format!(
                "Line {line_number}: sankey row is missing a source label: {trimmed}"
            ));
            continue;
        };
        let Some(target_label) = clean_label(Some(&target)) else {
            builder.add_warning(format!(
                "Line {line_number}: sankey row is missing a target label: {trimmed}"
            ));
            continue;
        };

        if value.parse::<f64>().is_err() {
            builder.add_warning(format!(
                "Line {line_number}: sankey flow value is not numeric; preserving raw label '{value}'"
            ));
        }

        let Some(source_node) =
            builder.intern_node(&source_label, Some(&source_label), NodeShape::Rect, span)
        else {
            builder.add_warning(format!(
                "Line {line_number}: sankey source node could not be created: {trimmed}"
            ));
            continue;
        };
        let Some(target_node) =
            builder.intern_node(&target_label, Some(&target_label), NodeShape::Rect, span)
        else {
            builder.add_warning(format!(
                "Line {line_number}: sankey target node could not be created: {trimmed}"
            ));
            continue;
        };

        builder.add_class_to_node(&source_label, "sankey-node", span);
        builder.add_class_to_node(&target_label, "sankey-node", span);

        builder.push_edge(
            source_node,
            target_node,
            ArrowType::Arrow,
            Some(&value),
            span,
        );

        // Add flow data attribute for gradient rendering.
        if let Some(edge) = builder.ir_mut().edges.last_mut() {
            edge.source_cardinality = None; // Not used for sankey
            edge.target_cardinality = None;
        }
    }
}

fn parse_c4(input: &str, builder: &mut IrBuilder) {
    let mut boundary_stack: Vec<(usize, usize)> = Vec::new();

    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = strip_flowchart_inline_comment(raw_line).trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if is_c4_header(trimmed) || trimmed.starts_with("title ") {
            continue;
        }

        match trimmed {
            "LAYOUT_TOP_DOWN()" => {
                builder.set_direction(GraphDirection::TB);
                continue;
            }
            "LAYOUT_LEFT_RIGHT()" => {
                builder.set_direction(GraphDirection::LR);
                continue;
            }
            "SHOW_LEGEND()" => {
                builder.set_c4_show_legend(true);
                continue;
            }
            "HIDE_LEGEND()" => {
                builder.set_c4_show_legend(false);
                continue;
            }
            _ => {}
        }

        if trimmed.starts_with("UpdateLayoutConfig(") {
            continue;
        }

        if trimmed == "}" {
            let _ = boundary_stack.pop();
            continue;
        }

        let Some((function_name, arguments, opens_block)) = parse_function_call(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported C4 syntax: {trimmed}"
            ));
            continue;
        };

        let span = span_for(line_number, raw_line);
        match function_name.as_str() {
            "Person" | "Person_Ext" | "System" | "System_Ext" | "SystemDb" | "SystemDb_Ext"
            | "SystemQueue" | "SystemQueue_Ext" | "Container" | "Container_Ext" | "ContainerDb"
            | "ContainerDb_Ext" | "ContainerQueue" | "ContainerQueue_Ext" | "Component"
            | "Component_Ext" | "ComponentDb" | "ComponentDb_Ext" | "ComponentQueue"
            | "ComponentQueue_Ext" => {
                if let Some(node_id) = parse_c4_node(&function_name, &arguments, span, builder) {
                    add_node_to_active_c4_boundaries(&boundary_stack, node_id, builder);
                } else {
                    builder.add_warning(format!(
                        "Line {line_number}: malformed C4 element declaration: {trimmed}"
                    ));
                }
            }
            "Rel" | "BiRel" | "Rel_Back" | "Rel_L" | "Rel_R" | "Rel_U" | "Rel_D" => {
                if !parse_c4_relationship(&function_name, &arguments, span, builder) {
                    builder.add_warning(format!(
                        "Line {line_number}: malformed C4 relationship declaration: {trimmed}"
                    ));
                }
            }
            "System_Boundary"
            | "Container_Boundary"
            | "Enterprise_Boundary"
            | "Boundary"
            | "Deployment_Node" => {
                if !opens_block {
                    builder.add_warning(format!(
                        "Line {line_number}: C4 boundary should open a block with '{{': {trimmed}"
                    ));
                    continue;
                }

                let Some(boundary) = parse_c4_boundary(
                    &function_name,
                    &arguments,
                    span,
                    boundary_stack
                        .last()
                        .map(|(_, subgraph_index)| *subgraph_index),
                    builder,
                ) else {
                    builder.add_warning(format!(
                        "Line {line_number}: malformed C4 boundary declaration: {trimmed}"
                    ));
                    continue;
                };
                boundary_stack.push(boundary);
            }
            _ => {
                builder.add_warning(format!(
                    "Line {line_number}: unsupported C4 directive '{function_name}'"
                ));
            }
        }
    }
}

fn parse_architecture(input: &str, builder: &mut IrBuilder) {
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = strip_flowchart_inline_comment(raw_line).trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("architecture-beta") || trimmed.starts_with("title ") {
            continue;
        }

        let span = span_for(line_number, raw_line);

        if let Some(declaration) = parse_architecture_declaration(trimmed, "group") {
            let title = declaration
                .label
                .as_deref()
                .or(Some(declaration.id.as_str()));
            let Some(cluster_index) = builder.ensure_cluster(&declaration.id, title, span) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid architecture group identifier: {trimmed}"
                ));
                continue;
            };
            let parent_subgraph = declaration.parent.as_ref().and_then(|parent| {
                groups
                    .get(parent)
                    .map(|(_, subgraph_index)| *subgraph_index)
            });
            if declaration.parent.is_some() && parent_subgraph.is_none() {
                builder.add_warning(format!(
                    "Line {line_number}: architecture group parent is unknown: {trimmed}"
                ));
            }
            let Some(subgraph_index) = builder.ensure_subgraph(
                &declaration.id,
                &declaration.id,
                title,
                span,
                parent_subgraph,
                Some(cluster_index),
            ) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid architecture group declaration: {trimmed}"
                ));
                continue;
            };
            groups.insert(declaration.id.clone(), (cluster_index, subgraph_index));
            continue;
        }

        if let Some(declaration) = parse_architecture_declaration(trimmed, "service") {
            let label = declaration
                .label
                .as_deref()
                .or(Some(declaration.id.as_str()));
            // Map well-known icons to appropriate shapes.
            let shape = match declaration.icon.as_deref() {
                Some("database" | "db" | "disk") => NodeShape::Cylinder,
                Some("cloud") => NodeShape::Rounded,
                _ => NodeShape::Rect,
            };
            let Some(node_id) = builder.intern_node(&declaration.id, label, shape, span) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid architecture service declaration: {trimmed}"
                ));
                continue;
            };
            builder.add_class_to_node(&declaration.id, "architecture", span);
            builder.add_class_to_node(&declaration.id, "architecture-service", span);
            if let Some(icon) = declaration.icon.as_deref() {
                builder.set_node_icon(node_id, icon);
                builder.add_class_to_node(
                    &declaration.id,
                    &format!("architecture-icon-{icon}"),
                    span,
                );
            }
            add_node_to_architecture_parent(
                line_number,
                trimmed,
                declaration.parent.as_deref(),
                &groups,
                node_id,
                builder,
            );
            continue;
        }

        if let Some(declaration) = parse_architecture_declaration(trimmed, "junction") {
            let label = declaration.label.as_deref();
            let Some(node_id) =
                builder.intern_node(&declaration.id, label, NodeShape::Circle, span)
            else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid architecture junction declaration: {trimmed}"
                ));
                continue;
            };
            builder.add_class_to_node(&declaration.id, "architecture", span);
            builder.add_class_to_node(&declaration.id, "architecture-junction", span);
            if let Some(icon) = declaration.icon.as_deref() {
                builder.add_warning(format!(
                    "Line {line_number}: architecture junction icons are ignored: {icon}"
                ));
            }
            add_node_to_architecture_parent(
                line_number,
                trimmed,
                declaration.parent.as_deref(),
                &groups,
                node_id,
                builder,
            );
            continue;
        }

        if let Some((from_id, to_id, arrow)) = parse_architecture_edge(trimmed) {
            let Some(from_node) =
                builder.intern_node(&from_id, Some(&from_id), NodeShape::Rect, span)
            else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid architecture edge source: {trimmed}"
                ));
                continue;
            };
            let Some(to_node) = builder.intern_node(&to_id, Some(&to_id), NodeShape::Rect, span)
            else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid architecture edge target: {trimmed}"
                ));
                continue;
            };
            builder.push_edge(from_node, to_node, arrow, None, span);
            continue;
        }

        builder.add_warning(format!(
            "Line {line_number}: unsupported architecture syntax: {trimmed}"
        ));
    }
}

/// Git graph state tracker for parsing.
struct GitGraphState {
    /// Map of branch names to their current head commit node ID
    branches: BTreeMap<String, IrNodeId>,
    /// Current branch name
    current_branch: String,
    /// Auto-generated commit counter for unnamed commits
    commit_counter: usize,
    /// Ordered list of branch names for color indexing.
    branch_order: Vec<String>,
}

impl GitGraphState {
    fn new() -> Self {
        Self {
            branches: BTreeMap::new(),
            current_branch: "main".to_string(),
            commit_counter: 0,
            branch_order: vec!["main".to_string()],
        }
    }

    /// Get the color index for a branch (stable ordering).
    fn branch_index(&self, branch: &str) -> usize {
        self.branch_order
            .iter()
            .position(|b| b == branch)
            .unwrap_or(0)
    }

    fn next_commit_id(&mut self) -> String {
        self.commit_counter += 1;
        format!("commit_{}", self.commit_counter)
    }

    fn current_head(&self) -> Option<IrNodeId> {
        self.branches.get(&self.current_branch).copied()
    }

    fn set_head(&mut self, branch: &str, node_id: IrNodeId) {
        self.branches.insert(branch.to_string(), node_id);
    }
}

enum GitGraphCommand {
    Commit(GitCommitOptions),
    Branch(String),
    Checkout(String),
    Merge(GitMergeOptions),
    CherryPick(String),
}

enum StateStatement {
    Direction(GraphDirection),
    Declaration(String),
    Edge(Vec<FlowAst>),
    Node(NodeToken),
    CompositeStart(StateCompositeDeclaration),
    CompositeEnd,
    RegionSeparator,
    Note {
        target: String,
        #[allow(dead_code)]
        position: String,
        text: String,
    },
}

const STATE_PSEUDO_TOKEN: &str = "__state_start_end";
const STATE_START_NODE_ID: &str = "__state_start";
const STATE_END_NODE_ID: &str = "__state_end";

fn parse_block_beta(input: &str, builder: &mut IrBuilder) {
    let document = parse_block_beta_document(input, builder.parser_config());
    for warning in &document.warnings {
        builder.add_warning(warning.clone());
    }
    for item in &document.items {
        lower_block_beta_document_item(item, builder, &[], &[]);
    }
}

fn parse_block_beta_document(input: &str, config: &ParserConfig) -> BlockBetaDocumentParseResult {
    let lines: Vec<(usize, &str)> = input
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect();
    let mut next_index = 0;
    let mut warnings = Vec::new();
    let (items, unclosed_groups) =
        parse_block_beta_document_items(&lines, &mut next_index, false, &mut warnings, config);
    if unclosed_groups > 0 {
        warnings.push(format!(
            "Block-beta diagram ended with {unclosed_groups} unclosed block group(s)"
        ));
    }
    BlockBetaDocumentParseResult { items, warnings }
}

fn parse_block_beta_document_items(
    lines: &[(usize, &str)],
    next_index: &mut usize,
    stop_on_end: bool,
    warnings: &mut Vec<String>,
    config: &ParserConfig,
) -> (Vec<BlockBetaDocumentItem>, usize) {
    let mut items = Vec::new();
    let mut unclosed_groups = 0;

    while let Some((line_number, line)) = lines.get(*next_index).copied() {
        *next_index += 1;

        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("block-beta") {
            continue;
        }

        if lower == "end" {
            if stop_on_end {
                return (items, unclosed_groups);
            }
            warnings.push(format!(
                "Line {line_number}: block-beta end without matching block group"
            ));
            continue;
        }

        if let Some((group_key, span_cols)) = parse_block_beta_group_start(trimmed) {
            let (body, child_unclosed) =
                parse_block_beta_document_items(lines, next_index, true, warnings, config);
            unclosed_groups += child_unclosed;
            items.push(BlockBetaDocumentItem::Group {
                id: group_key,
                span_cols,
                line_number,
                source_line: line.to_string(),
                body,
            });
            continue;
        }

        if let Some(columns) = parse_block_beta_columns(trimmed) {
            items.push(BlockBetaDocumentItem::Statement {
                statement: BlockBetaStatement::Columns(columns),
                line_number,
                source_line: line.to_string(),
            });
            continue;
        }

        if let Some(asts) =
            parse_edge_statement_asts(trimmed, &FLOW_OPERATORS, false, config, line_number)
        {
            items.push(BlockBetaDocumentItem::Statement {
                statement: BlockBetaStatement::Edges(asts),
                line_number,
                source_line: line.to_string(),
            });
            continue;
        }

        let blocks = parse_block_beta_blocks(trimmed, line_number, config);
        if !blocks.is_empty() {
            items.push(BlockBetaDocumentItem::Statement {
                statement: BlockBetaStatement::Blocks(blocks),
                line_number,
                source_line: line.to_string(),
            });
            continue;
        }

        warnings.push(format!(
            "Line {line_number}: unsupported block-beta syntax: {trimmed}"
        ));
    }

    if stop_on_end {
        unclosed_groups += 1;
    }

    (items, unclosed_groups)
}

fn lower_block_beta_document_item(
    item: &BlockBetaDocumentItem,
    builder: &mut IrBuilder,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
) {
    match item {
        BlockBetaDocumentItem::Statement {
            statement,
            line_number,
            source_line,
        } => {
            let span = span_for(*line_number, source_line);
            match statement {
                BlockBetaStatement::Columns(columns) => {
                    if *columns == 0 {
                        builder.add_warning(format!(
                            "Line {line_number}: block-beta columns must be >= 1"
                        ));
                    } else {
                        builder.set_block_beta_columns(*columns);
                    }
                }
                BlockBetaStatement::Edges(asts) => {
                    for ast in asts {
                        lower_flow_ast(
                            ast,
                            *line_number,
                            source_line,
                            builder,
                            active_clusters,
                            active_subgraphs,
                        );
                    }
                }
                BlockBetaStatement::Blocks(blocks) => {
                    for block in blocks {
                        let span_cols = block.span_cols.max(1);
                        if block.span_cols == 0 {
                            builder.add_warning(format!(
                                "Line {line_number}: block-beta block '{}' span must be >= 1",
                                block.id
                            ));
                        }
                        let Some(node_id) = builder.intern_node(
                            &block.id,
                            block.label.as_deref(),
                            block.shape,
                            span,
                        ) else {
                            builder.add_warning(format!(
                                "Line {line_number}: invalid block-beta block identifier: {}",
                                block.id
                            ));
                            continue;
                        };

                        builder.add_class_to_node(&block.id, "block-beta", span);
                        if block.is_space {
                            builder.add_class_to_node(&block.id, "block-beta-space", span);
                        }
                        if span_cols > 1 {
                            builder.add_class_to_node(
                                &block.id,
                                &format!("block-beta-span-{span_cols}"),
                                span,
                            );
                        }

                        add_node_to_active_groups(
                            builder,
                            active_clusters,
                            active_subgraphs,
                            node_id,
                        );
                    }
                }
            }
        }
        BlockBetaDocumentItem::Group {
            id,
            span_cols,
            line_number,
            source_line,
            body,
        } => {
            let span = span_for(*line_number, source_line);
            let Some(cluster_index) = builder.ensure_cluster(id, Some(id), span) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid block-beta group identifier: {}",
                    source_line.trim()
                ));
                return;
            };
            let parent_subgraph = active_subgraphs.last().copied();
            let Some(subgraph_index) = builder.ensure_subgraph(
                id,
                id,
                Some(id),
                span,
                parent_subgraph,
                Some(cluster_index),
            ) else {
                builder.add_warning(format!(
                    "Line {line_number}: invalid block-beta group identifier: {}",
                    source_line.trim()
                ));
                return;
            };

            if let Some(span_cols) = span_cols {
                let normalized_span = (*span_cols).max(1);
                if *span_cols == 0 {
                    builder.add_warning(format!(
                        "Line {line_number}: block-beta group '{id}' span must be >= 1"
                    ));
                }
                builder.set_cluster_grid_span(cluster_index, normalized_span);
                builder.set_subgraph_grid_span(subgraph_index, normalized_span);
            }

            let mut child_clusters = active_clusters.to_vec();
            child_clusters.push(cluster_index);
            let mut child_subgraphs = active_subgraphs.to_vec();
            child_subgraphs.push(subgraph_index);
            for child in body {
                lower_block_beta_document_item(child, builder, &child_clusters, &child_subgraphs);
            }
        }
    }
}

fn parse_block_beta_columns(line: &str) -> Option<usize> {
    let lower = line.to_ascii_lowercase();
    let rest = lower.strip_prefix("columns")?.trim();
    rest.parse::<usize>().ok()
}

fn parse_block_beta_group_start(line: &str) -> Option<(String, Option<usize>)> {
    let rest = line.strip_prefix("block:")?.trim();
    if rest.is_empty() {
        return None;
    }

    let (raw_key, span_cols) = match rest.rsplit_once(':') {
        Some((candidate_key, candidate_span))
            if candidate_span.trim().chars().all(|ch| ch.is_ascii_digit()) =>
        {
            (
                candidate_key.trim(),
                candidate_span.trim().parse::<usize>().ok(),
            )
        }
        _ => (rest, None),
    };

    let key = normalize_identifier(raw_key);
    (!key.is_empty()).then_some((key, span_cols))
}

fn parse_block_beta_blocks(line: &str, line_number: usize, config: &ParserConfig) -> Vec<BlockDef> {
    let lower = line.to_ascii_lowercase();
    if lower == "space" || lower.starts_with("space:") {
        let span_cols = lower
            .strip_prefix("space:")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(1);
        return vec![BlockDef {
            id: format!("__space_{line_number}"),
            label: None,
            shape: NodeShape::Rect,
            span_cols,
            is_space: true,
        }];
    }

    split_block_beta_defs(line)
        .into_iter()
        .filter_map(|token| try_parse_block_beta_def(token.trim(), config))
        .collect()
}

fn split_block_beta_defs(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut square_depth = 0_usize;

    for ch in line.chars() {
        if let Some(quote) = in_quote {
            current.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != '`' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                in_quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                square_depth = square_depth.saturating_add(1);
                current.push(ch);
            }
            ']' => {
                square_depth = square_depth.saturating_sub(1);
                current.push(ch);
            }
            _ if ch.is_whitespace() && square_depth == 0 => {
                let token = current.trim();
                if !token.is_empty() {
                    tokens.push(token.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let token = current.trim();
    if !token.is_empty() {
        tokens.push(token.to_string());
    }

    tokens
}

fn try_parse_block_beta_def(token: &str, config: &ParserConfig) -> Option<BlockDef> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    if find_operator(trimmed, &FLOW_OPERATORS).is_some() {
        return None;
    }

    let (core, span_cols) = match trimmed.rsplit_once(':') {
        Some((candidate_core, candidate_span))
            if candidate_span.trim().chars().all(|ch| ch.is_ascii_digit()) =>
        {
            (
                candidate_core.trim(),
                candidate_span.trim().parse::<usize>().ok().unwrap_or(1),
            )
        }
        _ => (trimmed, 1),
    };

    let node = parse_node_token_with_config(core, config)?;
    Some(BlockDef {
        id: node.id,
        label: node.label.map(|label| label.text),
        shape: node.shape,
        span_cols,
        is_space: false,
    })
}

fn parse_gitgraph(input: &str, builder: &mut IrBuilder) {
    let mut state = GitGraphState::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        // Skip header line (case-insensitive)
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("gitgraph") {
            // Check for options like LR, TB after gitGraph
            if let Some(direction) = parse_gitgraph_direction(trimmed) {
                builder.set_direction(direction);
            }
            continue;
        }

        if let Some(command) = parse_gitgraph_command(trimmed) {
            match command {
                Ok(command) => {
                    lower_gitgraph_command(command, line_number, line, &mut state, builder);
                }
                Err(message) => builder.add_warning(format!("Line {line_number}: {message}")),
            }
            continue;
        }

        builder.add_warning(format!(
            "Line {line_number}: unsupported gitGraph syntax: {trimmed}"
        ));
    }
}

fn parse_gitgraph_command(line: &str) -> Option<Result<GitGraphCommand, String>> {
    if let Some(rest) = strip_git_command(line, "commit") {
        return Some(Ok(GitGraphCommand::Commit(parse_git_commit_options(rest))));
    }

    if let Some(rest) = strip_git_command(line, "branch") {
        return Some(Ok(GitGraphCommand::Branch(rest.trim().to_string())));
    }

    if let Some(rest) = strip_git_command(line, "checkout") {
        return Some(Ok(GitGraphCommand::Checkout(rest.trim().to_string())));
    }

    if let Some(rest) = strip_git_command(line, "switch") {
        return Some(Ok(GitGraphCommand::Checkout(rest.trim().to_string())));
    }

    if let Some(rest) = strip_git_command(line, "merge") {
        return Some(
            parse_git_merge_options(rest.trim())
                .map(GitGraphCommand::Merge)
                .ok_or_else(|| "merge requires a branch name".to_string()),
        );
    }

    if let Some(rest) = strip_git_command(line, "cherry-pick") {
        return Some(
            parse_git_cherry_pick_id(rest.trim())
                .map(GitGraphCommand::CherryPick)
                .map_err(str::to_string),
        );
    }

    None
}

fn lower_gitgraph_command(
    command: GitGraphCommand,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    match command {
        GitGraphCommand::Commit(options) => {
            parse_git_commit(options, line_number, source_line, state, builder);
        }
        GitGraphCommand::Branch(branch_name) => {
            parse_git_branch(&branch_name, line_number, source_line, state, builder);
        }
        GitGraphCommand::Checkout(branch_name) => {
            parse_git_checkout(&branch_name, line_number, source_line, state, builder);
        }
        GitGraphCommand::Merge(options) => {
            parse_git_merge(options, line_number, source_line, state, builder);
        }
        GitGraphCommand::CherryPick(commit_id) => {
            parse_git_cherry_pick(&commit_id, line_number, source_line, state, builder);
        }
    }
}

/// Strip a git command prefix, requiring a word boundary (space or end of string).
fn strip_git_command<'a>(line: &'a str, command: &str) -> Option<&'a str> {
    let lower = line.to_ascii_lowercase();
    if !lower.starts_with(command) {
        return None;
    }
    let rest = &line[command.len()..];
    // Must be followed by whitespace, end of string, or certain punctuation
    if rest.is_empty() {
        return Some(rest);
    }
    let next_char = rest.chars().next()?;
    if next_char.is_whitespace() || next_char == ':' {
        Some(rest)
    } else {
        None
    }
}

fn parse_gitgraph_direction(header: &str) -> Option<GraphDirection> {
    // Parse direction from tokens after "gitGraph"
    for token in header.split_whitespace().skip(1) {
        let upper = token.to_ascii_uppercase();
        match upper.as_str() {
            "LR" => return Some(GraphDirection::LR),
            "RL" => return Some(GraphDirection::RL),
            "BT" => return Some(GraphDirection::BT),
            "TB" | "TD" => return Some(GraphDirection::TB),
            _ => {}
        }
    }
    None
}

/// Parse a commit command and its options.
///
/// Syntax: `commit [id: "id"] [msg: "message"] [tag: "tag"] [type: NORMAL|REVERSE|HIGHLIGHT]`
fn parse_git_commit(
    options: GitCommitOptions,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);

    // Determine commit ID
    let commit_id = options.id.unwrap_or_else(|| state.next_commit_id());

    // Build label from message and/or tag
    let label = match (&options.msg, &options.tag) {
        (Some(msg), Some(tag)) => Some(format!("{msg} [{tag}]")),
        (Some(msg), None) => Some(msg.clone()),
        (None, Some(tag)) => Some(format!("[{tag}]")),
        (None, None) => None,
    };

    // Map commit type to shape: REVERSE uses filled circle, HIGHLIGHT uses double circle.
    let shape = match options.commit_type.as_deref() {
        Some("REVERSE") => NodeShape::FilledCircle,
        Some("HIGHLIGHT") => NodeShape::DoubleCircle,
        _ => NodeShape::Circle,
    };

    // Create the commit node
    let Some(node_id) = builder.intern_node(&commit_id, label.as_deref(), shape, span) else {
        return;
    };

    // Apply commit type CSS class.
    if let Some(ref commit_type) = options.commit_type {
        builder.add_class_to_node(
            &commit_id,
            &format!("git-commit-{}", commit_type.to_ascii_lowercase()),
            span,
        );
    }

    // Apply branch color class.
    let branch_index = state.branch_index(&state.current_branch.clone());
    builder.add_class_to_node(
        &commit_id,
        &format!("git-branch-{}", branch_index % 8),
        span,
    );

    // Link from current branch head if it exists
    if let Some(parent_id) = state.current_head() {
        builder.push_edge(parent_id, node_id, ArrowType::Line, None, span);
    }

    // Update current branch head
    state.set_head(&state.current_branch.clone(), node_id);
}

/// Parsed git commit options.
struct GitCommitOptions {
    id: Option<String>,
    msg: Option<String>,
    tag: Option<String>,
    commit_type: Option<String>,
}

struct GitMergeOptions {
    branch: String,
    id: Option<String>,
    tag: Option<String>,
}

fn parse_git_commit_options(rest: &str) -> GitCommitOptions {
    let mut options = GitCommitOptions {
        id: None,
        msg: None,
        tag: None,
        commit_type: None,
    };

    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return options;
    }

    // Parse key: "value" pairs
    let mut remaining = trimmed;
    while !remaining.is_empty() {
        remaining = remaining.trim_start();

        // Try to match id: "value"
        if let Some(rest_after_id) = remaining.strip_prefix("id:")
            && let Some((value, rest)) = extract_quoted_or_word(rest_after_id.trim_start())
        {
            options.id = Some(value);
            remaining = rest;
            continue;
        }

        // Try to match msg: "value"
        if let Some(rest_after_msg) = remaining.strip_prefix("msg:")
            && let Some((value, rest)) = extract_quoted_or_word(rest_after_msg.trim_start())
        {
            options.msg = Some(value);
            remaining = rest;
            continue;
        }

        // Try to match tag: "value"
        if let Some(rest_after_tag) = remaining.strip_prefix("tag:")
            && let Some((value, rest)) = extract_quoted_or_word(rest_after_tag.trim_start())
        {
            options.tag = Some(value);
            remaining = rest;
            continue;
        }

        // Try to match type: VALUE (NORMAL, REVERSE, HIGHLIGHT)
        if let Some(rest_after_type) = remaining.strip_prefix("type:") {
            let type_rest = rest_after_type.trim_start();
            let end = type_rest
                .find(|c: char| c.is_whitespace())
                .unwrap_or(type_rest.len());
            let type_value = type_rest[..end].trim();
            if !type_value.is_empty() {
                options.commit_type = Some(type_value.to_ascii_uppercase());
            }
            remaining = &type_rest[end..];
            continue;
        }

        // If we can't parse anything else, break to avoid infinite loop
        break;
    }

    options
}

/// Extract a quoted string value, returning the value and remaining input.
fn extract_quoted_value(input: &str) -> Option<(String, &str)> {
    let trimmed = input.trim_start();
    let quote_char = trimmed.chars().next()?;

    if !matches!(quote_char, '"' | '\'') {
        return None;
    }

    let mut value = String::new();
    let mut escaped = false;
    let mut consumed_bytes = quote_char.len_utf8();

    for ch in trimmed[consumed_bytes..].chars() {
        consumed_bytes += ch.len_utf8();
        if escaped {
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote_char {
            let rest = &trimmed[consumed_bytes..];
            return Some((value, rest));
        } else {
            value.push(ch);
        }
    }

    None // Unclosed quote
}

fn extract_quoted_or_word(input: &str) -> Option<(String, &str)> {
    if let Some(parsed) = extract_quoted_value(input) {
        return Some(parsed);
    }

    let trimmed = input.trim_start();
    let end = trimmed
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(trimmed.len());
    let value = trimmed[..end].trim();
    if value.is_empty() {
        return None;
    }

    Some((value.to_string(), &trimmed[end..]))
}

fn parse_git_branch(
    branch_name: &str,
    line_number: usize,
    _source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let normalized = normalize_identifier(branch_name);
    if normalized.is_empty() {
        builder.add_warning(format!("Line {line_number}: empty branch name in gitGraph"));
        return;
    }

    // Track branch ordering for color assignment.
    if !state.branch_order.contains(&normalized) {
        state.branch_order.push(normalized.clone());
    }

    // When creating a branch, it inherits the current head
    if let Some(current_head) = state.current_head() {
        state.set_head(&normalized, current_head);
    }
    // If no current head, the branch starts empty (first commit will set it)
}

fn parse_git_checkout(
    branch_name: &str,
    line_number: usize,
    _source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let normalized = normalize_identifier(branch_name);
    if normalized.is_empty() {
        builder.add_warning(format!("Line {line_number}: empty branch name in checkout"));
        return;
    }

    // Allow checking out branches that don't exist yet (they'll be created on first commit)
    state.current_branch = normalized;
}

fn parse_git_merge(
    options: GitMergeOptions,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);
    let GitMergeOptions {
        branch: branch_name,
        id,
        tag,
    } = options;

    // Get the head of the branch being merged
    let merge_source = if let Some(id) = state.branches.get(&branch_name).copied() {
        id
    } else {
        builder.add_warning(format!(
            "Line {line_number}: cannot merge non-existent branch '{branch_name}'"
        ));
        return;
    };

    let merge_id = id.unwrap_or_else(|| state.next_commit_id());
    let label = tag.unwrap_or_else(|| format!("merge {branch_name}"));

    let Some(merge_node) = builder.intern_node(&merge_id, Some(&label), NodeShape::Circle, span)
    else {
        return;
    };

    // Create edge from merge source to merge commit
    builder.push_edge(
        merge_source,
        merge_node,
        ArrowType::DottedArrow,
        Some(&label),
        span,
    );

    // Create edge from current head to merge commit
    if let Some(current_head) = state.current_head() {
        builder.push_edge(current_head, merge_node, ArrowType::Line, None, span);
    }

    // Update current branch head
    state.set_head(&state.current_branch.clone(), merge_node);
}

fn parse_git_merge_options(spec: &str) -> Option<GitMergeOptions> {
    let mut parts = spec.trim().splitn(2, char::is_whitespace);
    let branch = normalize_identifier(parts.next()?.trim());
    if branch.is_empty() {
        return None;
    }

    let mut options = GitMergeOptions {
        branch,
        id: None,
        tag: None,
    };

    let mut remaining = parts.next().unwrap_or_default().trim();
    while !remaining.is_empty() {
        remaining = remaining.trim_start();

        if let Some(rest_after_id) = remaining.strip_prefix("id:")
            && let Some((value, rest)) = extract_quoted_or_word(rest_after_id.trim_start())
        {
            options.id = Some(value);
            remaining = rest;
            continue;
        }

        if let Some(rest_after_tag) = remaining.strip_prefix("tag:")
            && let Some((value, rest)) = extract_quoted_or_word(rest_after_tag.trim_start())
        {
            options.tag = Some(value);
            remaining = rest;
            continue;
        }

        let end = remaining
            .find(|ch: char| ch.is_whitespace())
            .unwrap_or(remaining.len());
        remaining = &remaining[end..];
    }

    Some(options)
}

fn parse_git_cherry_pick(
    source_commit_id: &str,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);
    // Create a new commit that references the cherry-picked one
    let new_commit_id = state.next_commit_id();
    let label = format!("cherry-pick {source_commit_id}");

    let Some(new_node) = builder.intern_node(&new_commit_id, Some(&label), NodeShape::Circle, span)
    else {
        return;
    };

    // Link from current head
    if let Some(current_head) = state.current_head() {
        builder.push_edge(current_head, new_node, ArrowType::Line, None, span);
    }

    // Update current branch head
    state.set_head(&state.current_branch.clone(), new_node);
}

fn parse_git_cherry_pick_id(spec: &str) -> Result<String, &'static str> {
    let id_prefix = "id:";
    let Some(id_start) = spec.find(id_prefix) else {
        return Err("cherry-pick requires id: parameter");
    };
    let rest = spec[id_start + id_prefix.len()..].trim_start();
    let Some((commit_id, _)) = extract_quoted_or_word(rest) else {
        return Err("cherry-pick id must be a string value");
    };
    if commit_id.is_empty() {
        return Err("cherry-pick id must be a string value");
    }
    Ok(commit_id)
}

fn register_state_declaration(
    declaration: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let Some(state_declaration) = parse_state_composite_declaration(declaration) else {
        return false;
    };

    let explicit_label = state_declaration.explicit_label;
    let label = state_declaration.title.clone();
    let span = span_for(line_number, source_line);
    let (shape, label) = match state_declaration.stereotype {
        Some(StatePseudoState::Fork | StatePseudoState::Join) => (NodeShape::HorizontalBar, None),
        Some(StatePseudoState::Choice) => (NodeShape::Diamond, label),
        Some(StatePseudoState::History) => (
            NodeShape::Circle,
            if explicit_label {
                label
            } else {
                Some(String::from("H"))
            },
        ),
        Some(StatePseudoState::DeepHistory) => (
            NodeShape::DoubleCircle,
            if explicit_label {
                label
            } else {
                Some(String::from("H*"))
            },
        ),
        None => (NodeShape::Rounded, label),
    };
    let _ = builder.intern_node(&state_declaration.id, label.as_deref(), shape, span);
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateCompositeDeclaration {
    id: String,
    title: Option<String>,
    explicit_label: bool,
    stereotype: Option<StatePseudoState>,
}

fn parse_state_composite_declaration(declaration: &str) -> Option<StateCompositeDeclaration> {
    let body = declaration.trim().trim_end_matches('{').trim();
    if body.is_empty() {
        return None;
    }

    let (body_without_stereotype, stereotype) = parse_state_stereotype(body);
    let (raw_id, raw_label) = if let Some((label_part, id_part)) =
        parse_state_declaration_alias(body_without_stereotype)
    {
        (id_part, Some(label_part))
    } else {
        (body_without_stereotype, None)
    };

    let id = normalize_identifier(raw_id);
    if id.is_empty() {
        return None;
    }

    Some(StateCompositeDeclaration {
        id,
        title: raw_label
            .and_then(|value| clean_label(Some(value)))
            .or_else(|| clean_label(Some(raw_id))),
        explicit_label: raw_label.is_some(),
        stereotype,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatePseudoState {
    Fork,
    Join,
    Choice,
    History,
    DeepHistory,
}

fn parse_state_declaration_alias(body: &str) -> Option<(&str, &str)> {
    let (label_part, id_part) = body.rsplit_once(" as ")?;
    Some((label_part.trim(), id_part.trim()))
}

fn parse_state_stereotype(body: &str) -> (&str, Option<StatePseudoState>) {
    let Some(start_idx) = body.rfind("<<") else {
        return (body, None);
    };
    let Some(end_idx) = body[start_idx + 2..].find(">>") else {
        return (body, None);
    };
    let end_idx = start_idx + 2 + end_idx;
    let stereotype = body[start_idx + 2..end_idx].trim().to_ascii_lowercase();
    let pseudo_state = match stereotype.as_str() {
        "fork" => Some(StatePseudoState::Fork),
        "join" => Some(StatePseudoState::Join),
        "choice" => Some(StatePseudoState::Choice),
        "history" => Some(StatePseudoState::History),
        "deephistory" => Some(StatePseudoState::DeepHistory),
        _ => None,
    };
    let trimmed_body = body[..start_idx].trim_end();
    if trimmed_body.is_empty() {
        (body, None)
    } else {
        (trimmed_body, pseudo_state)
    }
}

fn register_participant(
    declaration: &str,
    actor_keyword: bool,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let Some(parsed) = parse_sequence_participant_declaration(declaration, actor_keyword) else {
        return false;
    };
    let span = span_for(line_number, source_line);
    let Some(node_id) =
        builder.intern_node_label(&parsed.id, parsed.label.as_ref(), parsed.shape, span)
    else {
        return false;
    };
    if let Some(node) = builder.node_mut(node_id) {
        node.shape = parsed.shape;
    }
    for class_name in parsed.classes {
        builder.add_class_to_node(&parsed.id, class_name, span);
    }
    true
}

fn parse_sequence_participant_declaration(
    declaration: &str,
    actor_keyword: bool,
) -> Option<SequenceParticipantDeclaration> {
    let trimmed = declaration.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (before_alias, external_alias) = if let Some((left, right)) = trimmed.rsplit_once(" as ") {
        if right.trim_start().starts_with('{') {
            (trimmed, None)
        } else {
            (
                left.trim_end(),
                parse_label(Some(right)).map(|mut value| {
                    value.text = normalize_sequence_display_text(&value.text);
                    value.segments.clear();
                    value
                }),
            )
        }
    } else {
        (trimmed, None)
    };

    let (without_config, config) =
        if let Some((prefix, raw_json)) = split_trailing_json_object(before_alias) {
            match serde_json::from_str::<Value>(raw_json) {
                Ok(parsed) => (prefix, Some(parsed)),
                Err(_) => (before_alias, None),
            }
        } else {
            (before_alias, None)
        };

    let raw_id = without_config.trim();

    let participant_id = normalize_identifier(raw_id);
    if participant_id.is_empty() {
        return None;
    }

    let inline_alias = config
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|obj| obj.get("alias"))
        .and_then(Value::as_str)
        .and_then(|value| parse_label(Some(value)))
        .map(|mut value| {
            value.text = normalize_sequence_display_text(&value.text);
            value.segments.clear();
            value
        });
    let participant_type = config
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|obj| obj.get("type"))
        .and_then(Value::as_str);

    let label = external_alias.or(inline_alias).or_else(|| {
        parse_label(Some(raw_id)).map(|mut value| {
            value.text = normalize_sequence_display_text(&value.text);
            value.segments.clear();
            value
        })
    });
    let (shape, mut classes) = sequence_participant_visuals(participant_type, actor_keyword);
    classes.insert(0, "sequence-participant");
    if actor_keyword {
        classes.push("sequence-actor");
    }

    Some(SequenceParticipantDeclaration {
        id: participant_id,
        label,
        shape,
        classes,
    })
}

fn split_trailing_json_object(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim();
    if !trimmed.ends_with('}') {
        return None;
    }
    let start = trimmed.rfind('{')?;
    Some((trimmed[..start].trim_end(), &trimmed[start..]))
}

fn sequence_participant_visuals(
    participant_type: Option<&str>,
    actor_keyword: bool,
) -> (NodeShape, Vec<&'static str>) {
    match participant_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("boundary") => (NodeShape::Subroutine, vec!["sequence-participant-boundary"]),
        Some("control") => (NodeShape::Circle, vec!["sequence-participant-control"]),
        Some("entity") => (NodeShape::Rect, vec!["sequence-participant-entity"]),
        Some("database") => (NodeShape::Cylinder, vec!["sequence-participant-database"]),
        Some("collections" | "collection") => (
            NodeShape::Subroutine,
            vec!["sequence-participant-collections"],
        ),
        Some("queue") => (NodeShape::HorizontalBar, vec!["sequence-participant-queue"]),
        Some("actor") => (NodeShape::Stadium, vec!["sequence-participant-actor"]),
        Some(_) | None if actor_keyword => (NodeShape::Stadium, vec!["sequence-participant-actor"]),
        Some(_) | None => (NodeShape::Rect, Vec::new()),
    }
}

fn parse_sequence_message_ast(statement: &str) -> Option<String> {
    let (operator_idx, operator, _) = find_operator(statement, &SEQUENCE_OPERATORS)?;
    let left = statement[..operator_idx].trim();
    let right = statement[operator_idx + operator.len()..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let target_raw = right
        .split_once(':')
        .map_or(right, |(target, _)| target)
        .trim();
    let left = strip_sequence_central_suffix(left);
    let target_raw = strip_sequence_central_prefix(target_raw);
    let from_id = normalize_identifier(left);
    let to_id = normalize_identifier(target_raw);
    if from_id.is_empty() || to_id.is_empty() {
        return None;
    }

    Some(statement.to_string())
}

fn lower_sequence_message(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let Some((operator_idx, operator, arrow)) = find_operator(statement, &SEQUENCE_OPERATORS)
    else {
        return false;
    };

    let left = statement[..operator_idx].trim();
    let right = statement[operator_idx + operator.len()..].trim();
    if left.is_empty() || right.is_empty() {
        return false;
    }

    let (target_raw, message_label) = if let Some((target, label)) = right.split_once(':') {
        (
            target.trim(),
            clean_label(Some(label)).map(|label| normalize_sequence_display_text(&label)),
        )
    } else {
        (right, None)
    };

    let left = strip_sequence_central_suffix(left);
    let target_raw = strip_sequence_central_prefix(target_raw);

    // Check for +/- activation modifiers on target
    let (target_clean, activate_target, deactivate_target) =
        if let Some(stripped) = target_raw.strip_prefix('+') {
            (stripped, true, false)
        } else if let Some(stripped) = target_raw.strip_prefix('-') {
            (stripped, false, true)
        } else {
            (target_raw, false, false)
        };

    let from_id = normalize_identifier(left);
    let to_id = normalize_identifier(target_clean);
    if from_id.is_empty() || to_id.is_empty() {
        return false;
    }

    let span = span_for(line_number, source_line);

    let left_label = parse_label(Some(left)).filter(|label| label.text != from_id);
    let from = builder.intern_node_label(&from_id, left_label.as_ref(), NodeShape::Rect, span);

    let right_label = parse_label(Some(target_clean)).filter(|label| label.text != to_id);
    let to = builder.intern_node_label(&to_id, right_label.as_ref(), NodeShape::Rect, span);

    match (from, to) {
        (Some(from_node), Some(to_node)) => {
            builder.push_edge(from_node, to_node, arrow, message_label.as_deref(), span);
            if activate_target {
                builder.activate_participant(&to_id);
            }
            if deactivate_target {
                builder.deactivate_participant(&to_id);
            }
            true
        }
        _ => false,
    }
}

fn strip_sequence_central_prefix(raw: &str) -> &str {
    raw.trim()
        .strip_prefix("()")
        .map_or(raw.trim(), str::trim_start)
}

fn strip_sequence_central_suffix(raw: &str) -> &str {
    raw.trim()
        .strip_suffix("()")
        .map_or(raw.trim(), str::trim_end)
}

fn parse_edge_statement(
    statement: &str,
    line_number: usize,
    source_line: &str,
    operators: &[(&str, ArrowType)],
    builder: &mut IrBuilder,
) -> bool {
    parse_edge_statement_with_nodes(statement, line_number, source_line, operators, builder)
        .is_some()
}

fn parse_edge_statement_asts(
    statement: &str,
    operators: &[(&str, ArrowType)],
    allow_parallel_node_lists: bool,
    config: &ParserConfig,
    line_number: usize,
) -> Option<Vec<FlowAst>> {
    let (first_operator_idx, first_operator, first_arrow) = find_operator(statement, operators)?;
    let left_raw = statement[..first_operator_idx].trim();
    if left_raw.is_empty() {
        return None;
    }

    let mut from_nodes = parse_node_list_with_config(left_raw, allow_parallel_node_lists, config)?;
    let mut asts = Vec::new();
    let mut operator_idx = first_operator_idx;
    let mut operator = first_operator;
    let mut arrow = first_arrow;
    let mut placeholder_index = 0_usize;

    loop {
        let rhs_start = operator_idx + operator.len();
        let mut next_operator = find_operator_from_index(statement, rhs_start, operators);

        let edge_label;
        let right_without_label;
        let mut current_arrow = arrow;

        // Check for A -- label --> B syntax
        if is_arrow_prefix(operator)
            && let Some((n_idx, n_op, n_arrow)) = next_operator
        {
            let label_part = statement[rhs_start..n_idx].trim();
            edge_label = clean_label(Some(label_part));
            current_arrow = n_arrow;

            // Now we need to find the node AFTER the second operator
            let after_next_start = n_idx + n_op.len();
            next_operator = find_operator_from_index(statement, after_next_start, operators);
            right_without_label = match next_operator {
                Some((next_idx, _, _)) => &statement[after_next_start..next_idx],
                None => &statement[after_next_start..],
            }
            .trim();
        } else {
            let right_segment = match next_operator {
                Some((next_idx, _, _)) => &statement[rhs_start..next_idx],
                None => &statement[rhs_start..],
            }
            .trim();

            if right_segment.is_empty() {
                if !config.create_placeholder_nodes {
                    return (!asts.is_empty()).then_some(asts);
                }
                let placeholder = dangling_placeholder_node(line_number, placeholder_index);
                for from_node in &from_nodes {
                    asts.push(FlowAst::Edge {
                        from: from_node.clone().into(),
                        arrow,
                        label: None,
                        to: placeholder.clone().into(),
                    });
                }
                break;
            }

            let (label, target) = extract_pipe_label(right_segment);
            edge_label = label;
            right_without_label = target;
        }

        let to_nodes = match (!right_without_label.is_empty())
            .then(|| {
                parse_node_list_with_config(right_without_label, allow_parallel_node_lists, config)
            })
            .flatten()
        {
            Some(nodes) => nodes,
            None if config.create_placeholder_nodes && right_without_label.is_empty() => {
                let placeholder = dangling_placeholder_node(line_number, placeholder_index);
                placeholder_index += 1;
                vec![placeholder]
            }
            None => return (!asts.is_empty()).then_some(asts),
        };

        for from_node in &from_nodes {
            for to_node in &to_nodes {
                asts.push(FlowAst::Edge {
                    from: from_node.clone().into(),
                    arrow: current_arrow,
                    label: edge_label.clone(),
                    to: to_node.clone().into(),
                });
            }
        }

        if let Some((next_idx, next_operator_token, next_arrow)) = next_operator {
            from_nodes = to_nodes;
            operator_idx = next_idx;
            operator = next_operator_token;
            arrow = next_arrow;
            continue;
        }

        break;
    }

    (!asts.is_empty()).then_some(asts)
}

fn parse_node_list_with_config(
    raw: &str,
    allow_parallel_node_lists: bool,
    config: &ParserConfig,
) -> Option<Vec<NodeToken>> {
    if !allow_parallel_node_lists && contains_top_level_ampersand(raw) {
        return None;
    }

    let parts = split_top_level_ampersands(raw);
    if parts.is_empty() {
        return None;
    }

    let mut nodes = Vec::with_capacity(parts.len());
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            return None;
        }
        nodes.push(parse_node_token_with_config(trimmed, config)?);
    }
    Some(nodes)
}

fn contains_top_level_ampersand(raw: &str) -> bool {
    split_top_level_ampersands(raw).len() > 1
}

fn split_top_level_ampersands(raw: &str) -> Vec<&str> {
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut square_depth = 0_usize;
    let mut paren_depth = 0_usize;
    let mut brace_depth = 0_usize;
    let mut last_start = 0_usize;
    let mut parts = Vec::new();

    for (idx, ch) in raw.char_indices() {
        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != '`' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                in_quote = Some(ch);
                continue;
            }
            '[' => {
                square_depth = square_depth.saturating_add(1);
                continue;
            }
            ']' => {
                square_depth = square_depth.saturating_sub(1);
                continue;
            }
            '(' => {
                paren_depth = paren_depth.saturating_add(1);
                continue;
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                continue;
            }
            '{' => {
                brace_depth = brace_depth.saturating_add(1);
                continue;
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                continue;
            }
            '&' if square_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                parts.push(&raw[last_start..idx]);
                last_start = idx + ch.len_utf8();
                continue;
            }
            _ => {}
        }
    }

    parts.push(&raw[last_start..]);
    parts
}

fn is_arrow_prefix(op: &str) -> bool {
    matches!(op, "--" | "==" | "-." | "..")
}

fn parse_edge_statement_with_nodes(
    statement: &str,
    line_number: usize,
    source_line: &str,
    operators: &[(&str, ArrowType)],
    builder: &mut IrBuilder,
) -> Option<Vec<IrNodeId>> {
    let (first_operator_idx, first_operator, first_arrow) = find_operator(statement, operators)?;
    let left_raw = statement[..first_operator_idx].trim();
    if left_raw.is_empty() {
        return None;
    }

    let span = span_for(line_number, source_line);
    let left_node = parse_node_token_with_config(left_raw, builder.parser_config())?;
    let mut from_node = builder.intern_node_label(
        &left_node.id,
        left_node.label.as_ref(),
        left_node.shape,
        span,
    )?;
    let mut touched_nodes = vec![from_node];

    let mut operator_idx = first_operator_idx;
    let mut operator = first_operator;
    let mut arrow = first_arrow;
    let mut placeholder_index = 0_usize;

    loop {
        let rhs_start = operator_idx + operator.len();
        let next_operator = find_operator_from_index(statement, rhs_start, operators);
        let right_segment = match next_operator {
            Some((next_idx, _, _)) => &statement[rhs_start..next_idx],
            None => &statement[rhs_start..],
        }
        .trim();

        if right_segment.is_empty() {
            let to_node = if builder.parser_config().create_placeholder_nodes {
                let placeholder_id = dangling_placeholder_node_id(line_number, placeholder_index);
                builder.intern_placeholder_node(&placeholder_id, span)?
            } else {
                return (touched_nodes.len() > 1).then_some(touched_nodes);
            };
            builder.push_edge(from_node, to_node, arrow, None, span);
            touched_nodes.push(to_node);
            break;
        }

        let (edge_label, right_without_label) = extract_pipe_label(right_segment);
        let right_node =
            match parse_node_token_with_config(right_without_label, builder.parser_config()) {
                Some(node) => node,
                None if builder.parser_config().create_placeholder_nodes
                    && right_without_label.is_empty() =>
                {
                    let placeholder_id =
                        dangling_placeholder_node_id(line_number, placeholder_index);
                    placeholder_index += 1;
                    let to_node = builder.intern_placeholder_node(&placeholder_id, span)?;
                    builder.push_edge(from_node, to_node, arrow, edge_label.as_deref(), span);
                    touched_nodes.push(to_node);
                    if let Some((next_idx, next_operator_token, next_arrow)) = next_operator {
                        from_node = to_node;
                        operator_idx = next_idx;
                        operator = next_operator_token;
                        arrow = next_arrow;
                        continue;
                    }
                    break;
                }
                None => return (touched_nodes.len() > 1).then_some(touched_nodes),
            };
        let to_node = match builder.intern_node_label(
            &right_node.id,
            right_node.label.as_ref(),
            right_node.shape,
            span,
        ) {
            Some(node_id) => node_id,
            None => return (touched_nodes.len() > 1).then_some(touched_nodes),
        };

        builder.push_edge(from_node, to_node, arrow, edge_label.as_deref(), span);
        touched_nodes.push(to_node);

        if let Some((next_idx, next_operator_token, next_arrow)) = next_operator {
            from_node = to_node;
            operator_idx = next_idx;
            operator = next_operator_token;
            arrow = next_arrow;
            continue;
        }

        break;
    }

    Some(touched_nodes)
}

fn find_operator<'a>(
    statement: &str,
    operators: &'a [(&'a str, ArrowType)],
) -> Option<(usize, &'a str, ArrowType)> {
    find_operator_from_index(statement, 0, operators)
}

fn find_operator_from_index<'a>(
    statement: &str,
    start_index: usize,
    operators: &'a [(&'a str, ArrowType)],
) -> Option<(usize, &'a str, ArrowType)> {
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut square_depth = 0_usize;
    let mut paren_depth = 0_usize;
    let mut brace_depth = 0_usize;

    for (idx, ch) in statement.char_indices() {
        if idx < start_index {
            continue;
        }

        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != '`' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                in_quote = Some(ch);
                continue;
            }
            '[' => {
                square_depth = square_depth.saturating_add(1);
                continue;
            }
            ']' => {
                square_depth = square_depth.saturating_sub(1);
                continue;
            }
            '(' => {
                paren_depth = paren_depth.saturating_add(1);
                continue;
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                continue;
            }
            '{' => {
                brace_depth = brace_depth.saturating_add(1);
                continue;
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                continue;
            }
            _ => {}
        }

        if square_depth != 0 || paren_depth != 0 || brace_depth != 0 {
            continue;
        }

        let tail = &statement[idx..];
        let mut best_match: Option<(&str, ArrowType)> = None;
        for (operator, arrow) in operators {
            if tail.starts_with(operator) {
                match best_match {
                    Some((best_operator, _)) if operator.len() <= best_operator.len() => {}
                    _ => best_match = Some((operator, *arrow)),
                }
            }
        }

        if let Some((operator, arrow)) = best_match {
            return Some((idx, operator, arrow));
        }
    }

    None
}

fn extract_pipe_label(right_hand_side: &str) -> (Option<String>, &str) {
    let trimmed = right_hand_side.trim();
    let Some(after_open) = trimmed.strip_prefix('|') else {
        return (None, trimmed);
    };
    let Some(close_idx) = after_open.find('|') else {
        return (None, trimmed);
    };

    let label = clean_label(Some(&after_open[..close_idx]));
    let remainder = after_open[close_idx + 1..].trim();
    (label, remainder)
}

fn parse_node_token_with_config(raw: &str, config: &ParserConfig) -> Option<NodeToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed == "[*]" {
        return Some(NodeToken {
            id: STATE_PSEUDO_TOKEN.to_string(),
            label: None,
            icon: None,
            shape: NodeShape::Circle,
        });
    }

    let core = trimmed.split(":::").next().unwrap_or(trimmed).trim();
    if core.is_empty() {
        return None;
    }

    if let Some(parsed) = parse_double_circle_with_config(core, config) {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, "[(", ")]", NodeShape::Cylinder, config)
    {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, "[[", "]]", NodeShape::Subroutine, config)
    {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_with_config(core, '[', ']', NodeShape::Rect, config) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_with_config(core, '(', ')', NodeShape::Rounded, config) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_with_config(core, '{', '}', NodeShape::Diamond, config) {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, ">", "]", NodeShape::Asymmetric, config)
    {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, "[/", "/]", NodeShape::Parallelogram, config)
    {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, "[\\", "\\]", NodeShape::InvParallelogram, config)
    {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, "[/", "\\]", NodeShape::Trapezoid, config)
    {
        return Some(parsed);
    }
    if let Some(parsed) =
        parse_wrapped_str_with_config(core, "[\\", "/]", NodeShape::InvTrapezoid, config)
    {
        return Some(parsed);
    }

    if !config.auto_close_delimiters && looks_like_unclosed_node_delimiter(core) {
        return None;
    }

    let id = normalize_identifier(core);
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(core)).filter(|value| value.text != id);
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape: NodeShape::Rect,
    })
}

fn parse_double_circle_with_config(raw: &str, config: &ParserConfig) -> Option<NodeToken> {
    let start = raw.find("((")?;
    if !raw.ends_with("))") && !config.auto_close_delimiters {
        return None;
    }

    let id_raw = raw[..start].trim();
    let label_end = if raw.ends_with("))") {
        raw.len().saturating_sub(2)
    } else {
        raw.len()
    };
    let label_raw = raw[start + 2..label_end].trim();
    let mut id = normalize_identifier(id_raw);
    if id.is_empty() {
        id = normalize_identifier(label_raw);
    }
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(label_raw));
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape: NodeShape::DoubleCircle,
    })
}

fn parse_wrapped_with_config(
    raw: &str,
    open: char,
    close: char,
    shape: NodeShape,
    config: &ParserConfig,
) -> Option<NodeToken> {
    let start = raw.find(open)?;
    if !raw.ends_with(close) && !config.auto_close_delimiters {
        return None;
    }

    let inner_start = start + open.len_utf8();
    let end = if raw.ends_with(close) {
        raw.len().saturating_sub(close.len_utf8())
    } else {
        raw.len()
    };
    if inner_start > end {
        return None;
    }

    let id_raw = raw[..start].trim();
    let label_raw = raw[inner_start..end].trim();
    let mut id = normalize_identifier(id_raw);
    if id.is_empty() {
        id = normalize_identifier(label_raw);
    }
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(label_raw));
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape,
    })
}

fn parse_wrapped_str_with_config(
    raw: &str,
    open: &str,
    close: &str,
    shape: NodeShape,
    config: &ParserConfig,
) -> Option<NodeToken> {
    let start = raw.find(open)?;
    if !raw.ends_with(close) && !config.auto_close_delimiters {
        return None;
    }

    let inner_start = start + open.len();
    let end = if raw.ends_with(close) {
        raw.len().saturating_sub(close.len())
    } else {
        raw.len()
    };
    if inner_start > end {
        return None;
    }

    let id_raw = raw[..start].trim();
    let label_raw = raw[inner_start..end].trim();
    let mut id = normalize_identifier(id_raw);
    if id.is_empty() {
        id = normalize_identifier(label_raw);
    }
    if id.is_empty() {
        return None;
    }

    let mut label = parse_label(Some(label_raw));
    let icon = extract_icon_prefix(label.as_mut());
    clear_empty_label(&mut label);
    Some(NodeToken {
        id,
        label,
        icon,
        shape,
    })
}

fn looks_like_unclosed_node_delimiter(raw: &str) -> bool {
    (raw.contains("((") && !raw.ends_with("))"))
        || (raw.contains("[(") && !raw.ends_with(")]"))
        || (raw.contains("[[") && !raw.ends_with("]]"))
        || (raw.contains("[/") && !raw.ends_with("/]") && !raw.ends_with("\\]"))
        || (raw.contains("[\\") && !raw.ends_with("\\]") && !raw.ends_with("/]"))
        || (raw.contains('[') && !raw.ends_with(']'))
        || (raw.contains('(') && !raw.ends_with(')'))
        || (raw.contains('{') && !raw.ends_with('}'))
        || (raw.contains('>') && !raw.ends_with(']'))
}

fn dangling_placeholder_node(line_number: usize, placeholder_index: usize) -> NodeToken {
    NodeToken {
        id: dangling_placeholder_node_id(line_number, placeholder_index),
        label: None,
        icon: None,
        shape: NodeShape::Rect,
    }
}

fn dangling_placeholder_node_id(line_number: usize, placeholder_index: usize) -> String {
    format!(
        "{DANGLING_PLACEHOLDER_PREFIX}{line_number}_placeholder_{}",
        placeholder_index + 1
    )
}

fn is_dangling_placeholder_node_id(id: &str) -> bool {
    id.starts_with(DANGLING_PLACEHOLDER_PREFIX)
}

fn normalize_compound_identifier(raw: &str) -> String {
    let cleaned = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim();
    if cleaned.is_empty() {
        return String::new();
    }

    let mut normalized = String::with_capacity(cleaned.len());
    let mut previous_was_sep = false;
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/') {
            normalized.push(ch);
            previous_was_sep = false;
        } else if !previous_was_sep {
            normalized.push('_');
            previous_was_sep = true;
        }
    }

    normalized.trim_matches('_').to_string()
}

fn clean_label(raw: Option<&str>) -> Option<String> {
    parse_label(raw).map(|label| label.text)
}

fn clear_empty_label(label: &mut Option<ParsedLabel>) {
    if label.as_ref().is_some_and(|value| value.text.is_empty()) {
        *label = None;
    }
}

fn extract_icon_prefix(label: Option<&mut ParsedLabel>) -> Option<String> {
    let label = label?;
    let trimmed = label.text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((icon, remainder)) = split_fontawesome_icon_prefix(trimmed) {
        let icon_str = icon.to_string();
        label.text = remainder.unwrap_or_default().to_string();
        return Some(icon_str);
    }

    if let Some((icon, remainder)) = split_emoji_icon_prefix(trimmed) {
        let icon_str = icon.to_string();
        label.text = remainder.unwrap_or_default().to_string();
        return Some(icon_str);
    }

    None
}

fn split_fontawesome_icon_prefix(text: &str) -> Option<(&str, Option<&str>)> {
    let trimmed = text.trim();
    let remainder = trimmed.strip_prefix("fa:")?;
    let icon_end = remainder
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(remainder.len());
    if icon_end == 0 {
        return None;
    }

    let icon = &trimmed[..3 + icon_end];
    let remainder = remainder[icon_end..].trim();
    Some((icon, (!remainder.is_empty()).then_some(remainder)))
}

fn split_emoji_icon_prefix(text: &str) -> Option<(&str, Option<&str>)> {
    let trimmed = text.trim();
    let mut chars = trimmed.char_indices();
    let (_, first) = chars.next()?;
    if first.is_ascii() {
        return None;
    }

    let next_boundary = chars.next().map_or(trimmed.len(), |(idx, _)| idx);
    let icon = &trimmed[..next_boundary];
    let remainder = trimmed[next_boundary..].trim();
    Some((icon, (!remainder.is_empty()).then_some(remainder)))
}

fn parse_label(raw: Option<&str>) -> Option<ParsedLabel> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_quotes = trimmed.trim_matches('"').trim_matches('\'').trim();
    if without_quotes.is_empty() {
        return None;
    }

    if let Some(markdown_body) = without_quotes
        .strip_prefix('`')
        .and_then(|value| value.strip_suffix('`'))
    {
        let normalized = markdown_body
            .trim()
            .replace("<br/>", "\n")
            .replace("<br>", "\n")
            .replace("<br />", "\n");
        let decoded = decode_mermaid_entities(&normalized);
        let segments = parse_markdown_label_segments(&decoded);
        let text = flatten_label_segments(&segments);
        if text.trim().is_empty() {
            return None;
        }
        return Some(ParsedLabel { text, segments });
    }

    let decoded = decode_mermaid_entities(without_quotes.trim());
    if decoded.is_empty() {
        None
    } else {
        Some(ParsedLabel::plain(decoded))
    }
}

fn parse_markdown_label_segments(text: &str) -> Vec<IrLabelSegment> {
    let mut segments = Vec::new();
    let mut buffer = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut code = false;
    let mut strike = false;
    let chars: Vec<char> = text.chars().collect();
    let mut idx = 0usize;

    let flush = |segments: &mut Vec<IrLabelSegment>,
                 buffer: &mut String,
                 bold: bool,
                 italic: bool,
                 code: bool,
                 strike: bool| {
        if !buffer.is_empty() {
            segments.push(IrLabelSegment::Text {
                text: std::mem::take(buffer),
                bold,
                italic,
                code,
                strike,
            });
        }
    };

    while idx < chars.len() {
        match chars[idx] {
            '\n' => {
                flush(&mut segments, &mut buffer, bold, italic, code, strike);
                segments.push(IrLabelSegment::LineBreak);
                idx += 1;
            }
            '~' if !code && chars.get(idx + 1) == Some(&'~') => {
                flush(&mut segments, &mut buffer, bold, italic, code, strike);
                strike = !strike;
                idx += 2;
            }
            '*' if !code && chars.get(idx + 1) == Some(&'*') => {
                flush(&mut segments, &mut buffer, bold, italic, code, strike);
                bold = !bold;
                idx += 2;
            }
            '*' if !code => {
                flush(&mut segments, &mut buffer, bold, italic, code, strike);
                italic = !italic;
                idx += 1;
            }
            '`' => {
                flush(&mut segments, &mut buffer, bold, italic, code, strike);
                code = !code;
                idx += 1;
            }
            ch => {
                buffer.push(ch);
                idx += 1;
            }
        }
    }

    flush(&mut segments, &mut buffer, bold, italic, code, strike);
    segments
}

fn flatten_label_segments(segments: &[IrLabelSegment]) -> String {
    let mut text = String::new();
    for segment in segments {
        match segment {
            IrLabelSegment::Text { text: value, .. } => text.push_str(value),
            IrLabelSegment::LineBreak => text.push('\n'),
        }
    }
    text
}

fn normalize_subgraph_title(raw: &str) -> Option<String> {
    let title = clean_label(Some(raw))?;
    let unwrapped = title
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .or_else(|| {
            title
                .strip_prefix('(')
                .and_then(|value| value.strip_suffix(')'))
        })
        .or_else(|| {
            title
                .strip_prefix('{')
                .and_then(|value| value.strip_suffix('}'))
        })
        .map_or(title.as_str(), str::trim);
    (!unwrapped.is_empty()).then_some(unwrapped.to_string())
}

fn parse_name_before_colon(line: &str) -> Option<&str> {
    let (left, _) = line.split_once(':')?;
    let candidate = left.trim().trim_matches('"').trim_matches('\'').trim();
    (!candidate.is_empty()).then_some(candidate)
}

fn bracket_contents(line: &str) -> Option<&str> {
    let start = line.find('[')?;
    let end = line.rfind(']')?;
    (end > start).then_some(&line[start + 1..end])
}

fn parse_chart_value_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_matches('"').trim_matches('\'').to_string())
        .collect()
}

fn parse_xychart_numeric_values(
    raw: &str,
    line_number: usize,
    line: &str,
    builder: &mut IrBuilder,
) -> Vec<f32> {
    let mut values = Vec::new();
    for raw_value in parse_chart_value_list(raw) {
        match raw_value.parse::<f32>() {
            Ok(value) if value.is_finite() => values.push(value),
            _ => builder.add_warning(format!(
                "Line {line_number}: invalid xychart numeric value '{raw_value}' in {line}"
            )),
        }
    }
    values
}

fn parse_xychart_axis(raw: &str) -> IrXyAxis {
    let trimmed = raw.trim();
    if let Some(categories) = bracket_contents(trimmed) {
        return IrXyAxis {
            categories: parse_chart_value_list(categories),
            ..Default::default()
        };
    }

    let mut axis = IrXyAxis::default();
    let mut remaining = trimmed;

    if let Some((label, rest)) = extract_quoted_value(remaining) {
        axis.label = Some(label);
        remaining = rest.trim();
    }

    if let Some((range_start, range_end)) = parse_axis_range(remaining) {
        axis.min = Some(range_start);
        axis.max = Some(range_end);
        return axis;
    }

    if axis.label.is_none() && !remaining.is_empty() {
        axis.label = clean_label(Some(remaining));
    }

    axis
}

fn parse_axis_range(raw: &str) -> Option<(f32, f32)> {
    let (start, end) = raw.split_once("-->")?;
    let start = start.trim().parse::<f32>().ok()?;
    let end = end.trim().parse::<f32>().ok()?;
    Some((start, end))
}

fn parse_xychart_series(line: &str) -> Option<(&str, Option<String>, &str)> {
    let (series_kind, remainder) = line.split_once(char::is_whitespace)?;
    let series_kind = series_kind.trim();
    if !matches!(series_kind, "line" | "bar" | "area") {
        return None;
    }

    let values = bracket_contents(remainder)?;
    let name_segment = remainder.split_once('[').map(|(left, _)| left.trim())?;
    let series_name = (!name_segment.is_empty()).then(|| {
        name_segment
            .trim_matches('"')
            .trim_matches('\'')
            .to_string()
    });
    Some((series_kind, series_name, values))
}

fn parse_init_directives(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        let Some(payload) = extract_init_payload(trimmed) else {
            continue;
        };

        let span = span_for(line_number, line);
        let parsed_value = match parse_init_payload_value(payload) {
            Ok(value) => value,
            Err(error) => {
                let message =
                    format!("Line {line_number}: invalid init directive payload: {error}");
                builder.add_warning(message.clone());
                builder.add_init_error(message, span);
                continue;
            }
        };

        apply_mermaid_config_value(parsed_value, &format!("Line {line_number}"), span, builder);
    }
}

fn parse_front_matter_config(front_matter_payload: &str, builder: &mut IrBuilder) {
    let span = Span::at_line(1, front_matter_payload.chars().count());
    #[cfg(target_arch = "wasm32")]
    {
        let message = String::from(
            "Front matter: YAML config is unavailable in wasm32 builds; ignoring front matter",
        );
        builder.add_warning(message.clone());
        builder.add_init_error(message, span);
        return;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let yaml_value = match serde_yaml::from_str::<Value>(front_matter_payload) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("Front matter: invalid YAML config: {error}");
                builder.add_warning(message.clone());
                builder.add_init_error(message, span);
                return;
            }
        };

        if let Some(title) = yaml_value.get("title").and_then(Value::as_str)
            && let Some(title) = clean_label(Some(title))
        {
            builder.set_title(title);
        }

        let config_value = yaml_value
            .get("config")
            .cloned()
            .unwrap_or_else(|| yaml_value.clone());
        apply_mermaid_config_value(config_value, "Front matter", span, builder);
    }
}

fn apply_mermaid_config_value(value: Value, context: &str, span: Span, builder: &mut IrBuilder) {
    let parsed = to_init_parse(parse_mermaid_js_config_value(&value));

    if let Some(theme) = parsed.config.theme {
        builder.set_init_theme(theme);
    }
    for (key, value) in parsed.config.theme_variables {
        builder.insert_theme_variable(key, value);
    }
    if let Some(direction) = parsed.config.flowchart_direction {
        builder.set_init_flowchart_direction(direction);
    }
    if let Some(curve) = parsed.config.flowchart_curve {
        builder.set_init_flowchart_curve(curve);
    }
    if let Some(mirror_actors) = parsed.config.sequence_mirror_actors {
        builder.set_init_sequence_mirror_actors(mirror_actors);
    }
    if let Some(show_numbers) = parsed.config.sequence_show_sequence_numbers {
        builder.set_init_sequence_show_sequence_numbers(show_numbers);
    }

    for warning in parsed.warnings {
        let message = format!("{context}: {}", warning.message);
        builder.add_warning(message.clone());
        builder.add_init_warning(message, span);
    }

    for error in parsed.errors {
        let message = format!("{context}: {error}");
        builder.add_warning(message.clone());
        builder.add_init_error(message, span);
    }
}

fn split_front_matter_block(input: &str) -> (&str, Option<&str>) {
    let mut segments = input.split_inclusive('\n');
    let Some(first_segment) = segments.next() else {
        return (input, None);
    };
    let first_line = first_segment.trim_end_matches(['\r', '\n']);
    if first_line.trim() != "---" {
        return (input, None);
    }

    let payload_start = first_segment.len();
    let mut offset = payload_start;
    for segment in segments {
        let line = segment.trim_end_matches(['\r', '\n']);
        let segment_start = offset;
        offset += segment.len();
        if line.trim() == "---" {
            let payload = input[payload_start..segment_start].trim_matches(['\r', '\n']);
            let body = &input[offset..];
            return (body, Some(payload));
        }
    }

    (input, None)
}

fn extract_init_payload(trimmed: &str) -> Option<&str> {
    if !(trimmed.starts_with("%%{") && trimmed.ends_with("}%%")) {
        return None;
    }
    let inner = &trimmed[3..trimmed.len().saturating_sub(3)];
    let (directive, payload) = inner.trim().split_once(':')?;
    if !directive.trim().eq_ignore_ascii_case("init") {
        return None;
    }
    let payload = payload.trim();
    (!payload.is_empty()).then_some(payload)
}

fn parse_init_payload_value(payload: &str) -> Result<Value, String> {
    serde_json::from_str::<Value>(payload).or_else(|json_error| {
        json5::from_str::<Value>(payload).map_err(|json5_error| {
            format!("JSON parse failed ({json_error}); JSON5 parse failed ({json5_error})")
        })
    })
}

fn leading_indent_width(line: &str) -> usize {
    let mut width = 0_usize;
    for ch in line.chars() {
        match ch {
            ' ' => width += 1,
            '\t' => width += 2,
            _ => break,
        }
    }
    width
}

fn split_statements(line: &str) -> impl Iterator<Item = &str> {
    let mut statements = Vec::new();
    let mut current_start = 0;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut square_depth = 0_usize;
    let mut paren_depth = 0_usize;
    let mut brace_depth = 0_usize;

    for (i, c) in line.char_indices() {
        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && q != '`' {
                escaped = true;
                continue;
            }
            if c == q {
                in_quote = None;
            }
        } else if c == '"' || c == '\'' || c == '`' {
            in_quote = Some(c);
        } else if c == '[' {
            square_depth = square_depth.saturating_add(1);
        } else if c == ']' {
            square_depth = square_depth.saturating_sub(1);
        } else if c == '(' {
            paren_depth = paren_depth.saturating_add(1);
        } else if c == ')' {
            paren_depth = paren_depth.saturating_sub(1);
        } else if c == '{' {
            brace_depth = brace_depth.saturating_add(1);
        } else if c == '}' {
            brace_depth = brace_depth.saturating_sub(1);
        } else if c == ';' && square_depth == 0 && paren_depth == 0 && brace_depth == 0 {
            let segment = line[current_start..i].trim();
            if !segment.is_empty() {
                statements.push(segment);
            }
            current_start = i + 1;
        }
    }

    let remainder = line[current_start..].trim();
    if !remainder.is_empty() {
        statements.push(remainder);
    }

    statements.into_iter()
}

fn parse_sankey_record(line: &str) -> Option<(String, String, String)> {
    let fields = split_csv_fields(line)?;
    if fields.len() != 3 {
        return None;
    }

    Some((
        fields[0].trim().to_string(),
        fields[1].trim().to_string(),
        fields[2].trim().to_string(),
    ))
}

fn split_csv_fields(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(quote) = in_quote {
            if ch == quote {
                if chars.peek().copied() == Some(quote) {
                    current.push(quote);
                    let _ = chars.next();
                } else {
                    in_quote = None;
                }
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '"' | '\'' => in_quote = Some(ch),
            ',' => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if in_quote.is_some() {
        return None;
    }

    fields.push(current.trim().to_string());
    Some(fields)
}

fn parse_function_call(line: &str) -> Option<(String, Vec<String>, bool)> {
    let open_paren = line.find('(')?;
    let function_name = line[..open_paren].trim();
    if function_name.is_empty() {
        return None;
    }

    let close_paren = find_matching_paren(line, open_paren)?;
    let arguments = split_top_level_arguments(&line[open_paren + 1..close_paren]);
    let remainder = line[close_paren + 1..].trim();
    let opens_block = remainder == "{";

    Some((function_name.to_string(), arguments, opens_block))
}

fn find_matching_paren(line: &str, open_paren: usize) -> Option<usize> {
    let mut depth = 0_usize;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for (index, ch) in line.char_indices().skip_while(|(idx, _)| *idx < open_paren) {
        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != '`' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => in_quote = Some(ch),
            '(' => depth = depth.saturating_add(1),
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

fn split_top_level_arguments(raw: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut nested_parens = 0_usize;

    for ch in raw.chars() {
        if let Some(quote) = in_quote {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != '`' {
                current.push(ch);
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            current.push(ch);
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                in_quote = Some(ch);
                current.push(ch);
            }
            '(' => {
                nested_parens = nested_parens.saturating_add(1);
                current.push(ch);
            }
            ')' => {
                nested_parens = nested_parens.saturating_sub(1);
                current.push(ch);
            }
            ',' if nested_parens == 0 => {
                arguments.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        arguments.push(current.trim().to_string());
    }

    arguments
}

fn is_c4_header(line: &str) -> bool {
    matches!(
        line,
        "C4Context" | "C4Container" | "C4Component" | "C4Dynamic" | "C4Deployment"
    )
}

fn parse_c4_node(
    function_name: &str,
    arguments: &[String],
    span: Span,
    builder: &mut IrBuilder,
) -> Option<IrNodeId> {
    let node_id = clean_label(arguments.first().map(String::as_str))?;
    let label =
        clean_label(arguments.get(1).map(String::as_str)).unwrap_or_else(|| node_id.clone());
    let shape = c4_node_shape(function_name);
    let node_id_value = builder.intern_node(&node_id, Some(&label), shape, span)?;
    builder.set_c4_node_meta(node_id_value, c4_node_meta(function_name, arguments));

    for class_name in c4_node_classes(function_name) {
        builder.add_class_to_node(&node_id, class_name, span);
    }

    Some(node_id_value)
}

fn c4_node_shape(function_name: &str) -> NodeShape {
    if function_name.contains("Db") {
        NodeShape::Cylinder
    } else if function_name.contains("Queue") {
        NodeShape::Parallelogram
    } else if function_name.starts_with("Person") {
        NodeShape::Rounded
    } else {
        NodeShape::Rect
    }
}

fn c4_node_classes(function_name: &str) -> &'static [&'static str] {
    match function_name {
        "Person" => &["c4", "c4-person"],
        "Person_Ext" => &["c4", "c4-person", "c4-external"],
        "System" => &["c4", "c4-system"],
        "System_Ext" => &["c4", "c4-system", "c4-external"],
        "SystemDb" => &["c4", "c4-system", "c4-database"],
        "SystemDb_Ext" => &["c4", "c4-system", "c4-database", "c4-external"],
        "SystemQueue" => &["c4", "c4-system", "c4-queue"],
        "SystemQueue_Ext" => &["c4", "c4-system", "c4-queue", "c4-external"],
        "Container" => &["c4", "c4-container"],
        "Container_Ext" => &["c4", "c4-container", "c4-external"],
        "ContainerDb" => &["c4", "c4-container", "c4-database"],
        "ContainerDb_Ext" => &["c4", "c4-container", "c4-database", "c4-external"],
        "ContainerQueue" => &["c4", "c4-container", "c4-queue"],
        "ContainerQueue_Ext" => &["c4", "c4-container", "c4-queue", "c4-external"],
        "Component" => &["c4", "c4-component"],
        "Component_Ext" => &["c4", "c4-component", "c4-external"],
        "ComponentDb" => &["c4", "c4-component", "c4-database"],
        "ComponentDb_Ext" => &["c4", "c4-component", "c4-database", "c4-external"],
        "ComponentQueue" => &["c4", "c4-component", "c4-queue"],
        "ComponentQueue_Ext" => &["c4", "c4-component", "c4-queue", "c4-external"],
        _ => &["c4"],
    }
}

fn c4_node_meta(function_name: &str, arguments: &[String]) -> IrC4NodeMeta {
    let (element_type, technology_index, description_index) = match function_name {
        "Person" | "Person_Ext" => ("Person", None, Some(2)),
        "System" | "System_Ext" | "SystemDb" | "SystemDb_Ext" | "SystemQueue"
        | "SystemQueue_Ext" => ("System", None, Some(2)),
        "Container" | "Container_Ext" | "ContainerDb" | "ContainerDb_Ext" | "ContainerQueue"
        | "ContainerQueue_Ext" => ("Container", Some(2), Some(3)),
        "Component" | "Component_Ext" | "ComponentDb" | "ComponentDb_Ext" | "ComponentQueue"
        | "ComponentQueue_Ext" => ("Component", Some(2), Some(3)),
        _ => ("C4", None, None),
    };

    IrC4NodeMeta {
        element_type: element_type.to_string(),
        technology: technology_index
            .and_then(|index| clean_label(arguments.get(index).map(String::as_str))),
        description: description_index
            .and_then(|index| clean_label(arguments.get(index).map(String::as_str))),
    }
}

fn parse_c4_relationship(
    function_name: &str,
    arguments: &[String],
    span: Span,
    builder: &mut IrBuilder,
) -> bool {
    let Some(from_id) = clean_label(arguments.first().map(String::as_str)) else {
        return false;
    };
    let Some(to_id) = clean_label(arguments.get(1).map(String::as_str)) else {
        return false;
    };

    let label = clean_label(arguments.get(2).map(String::as_str));
    let technology = clean_label(arguments.get(3).map(String::as_str));
    let description = clean_label(arguments.get(4).map(String::as_str));
    let combined_label = match (label, technology, description) {
        (Some(label), Some(technology), Some(description)) => {
            Some(format!("{label} [{technology}] - {description}"))
        }
        (Some(label), Some(technology), None) => Some(format!("{label} [{technology}]")),
        (Some(label), None, Some(description)) => Some(format!("{label} - {description}")),
        (Some(label), None, None) => Some(label),
        (None, Some(technology), Some(description)) => {
            Some(format!("[{technology}] - {description}"))
        }
        (None, Some(technology), None) => Some(format!("[{technology}]")),
        (None, None, Some(description)) => Some(description),
        (None, None, None) => None,
    };

    let Some(from_node) = builder.intern_node(&from_id, Some(&from_id), NodeShape::Rect, span)
    else {
        return false;
    };
    let Some(to_node) = builder.intern_node(&to_id, Some(&to_id), NodeShape::Rect, span) else {
        return false;
    };

    let (actual_from, actual_to, arrow) = match function_name {
        "BiRel" => (from_node, to_node, ArrowType::Line),
        "Rel_Back" => (to_node, from_node, ArrowType::Arrow),
        _ => (from_node, to_node, ArrowType::Arrow),
    };
    builder.push_edge(
        actual_from,
        actual_to,
        arrow,
        combined_label.as_deref(),
        span,
    );
    true
}

fn parse_c4_boundary(
    function_name: &str,
    arguments: &[String],
    span: Span,
    parent_subgraph: Option<usize>,
    builder: &mut IrBuilder,
) -> Option<(usize, usize)> {
    let boundary_key = clean_label(arguments.first().map(String::as_str))?;
    let display_label =
        clean_label(arguments.get(1).map(String::as_str)).unwrap_or_else(|| boundary_key.clone());
    let title = format!("{function_name}({boundary_key}, {display_label})");
    let cluster_index = builder.ensure_cluster(&boundary_key, Some(&title), span)?;
    let subgraph_index = builder.ensure_subgraph(
        &boundary_key,
        &boundary_key,
        Some(&title),
        span,
        parent_subgraph,
        Some(cluster_index),
    )?;
    Some((cluster_index, subgraph_index))
}

fn add_node_to_active_c4_boundaries(
    boundary_stack: &[(usize, usize)],
    node_id: IrNodeId,
    builder: &mut IrBuilder,
) {
    for (cluster_index, subgraph_index) in boundary_stack {
        builder.add_node_to_cluster(*cluster_index, node_id);
        builder.add_node_to_subgraph(*subgraph_index, node_id);
    }
}

#[derive(Debug, Clone)]
struct ArchitectureDeclaration {
    id: String,
    icon: Option<String>,
    label: Option<String>,
    parent: Option<String>,
}

fn parse_architecture_declaration(line: &str, keyword: &str) -> Option<ArchitectureDeclaration> {
    let remainder = line.strip_prefix(keyword)?.trim_start();
    if remainder.is_empty() {
        return None;
    }

    let mut split_at = remainder.len();
    for (idx, ch) in remainder.char_indices() {
        if ch.is_whitespace() || matches!(ch, '(' | '[') {
            split_at = idx;
            break;
        }
    }

    let id = clean_label(Some(&remainder[..split_at]))?;
    let mut cursor = remainder[split_at..].trim_start();
    let mut icon = None;
    let mut label = None;
    let mut parent = None;

    if let Some(rest) = cursor.strip_prefix('(') {
        let end = rest.find(')')?;
        icon = clean_label(Some(&rest[..end]));
        cursor = rest[end + 1..].trim_start();
    }

    if let Some(rest) = cursor.strip_prefix('[') {
        let end = rest.find(']')?;
        label = clean_label(Some(&rest[..end]));
        cursor = rest[end + 1..].trim_start();
    }

    if let Some(rest) = cursor.strip_prefix("in ") {
        parent = clean_label(Some(rest));
        cursor = "";
    }

    if !cursor.trim().is_empty() {
        return None;
    }

    Some(ArchitectureDeclaration {
        id,
        icon,
        label,
        parent,
    })
}

fn add_node_to_architecture_parent(
    line_number: usize,
    source_line: &str,
    parent: Option<&str>,
    groups: &BTreeMap<String, (usize, usize)>,
    node_id: IrNodeId,
    builder: &mut IrBuilder,
) {
    let Some(parent_key) = parent else {
        return;
    };
    let Some((cluster_index, subgraph_index)) = groups.get(parent_key).copied() else {
        builder.add_warning(format!(
            "Line {line_number}: architecture parent group '{parent_key}' is unknown: {source_line}"
        ));
        return;
    };
    builder.add_node_to_cluster(cluster_index, node_id);
    builder.add_node_to_subgraph(subgraph_index, node_id);
}

fn parse_architecture_edge(line: &str) -> Option<(String, String, ArrowType)> {
    const OPERATORS: [(&str, ArrowType, bool); 4] = [
        ("<-->", ArrowType::Line, true),
        ("-->", ArrowType::Arrow, false),
        ("<--", ArrowType::Arrow, true),
        ("--", ArrowType::Line, false),
    ];

    for (operator, arrow, reverse) in OPERATORS {
        if let Some(index) = line.find(operator) {
            let left = line[..index].trim();
            let right = line[index + operator.len()..].trim();
            let from = parse_architecture_endpoint(left)?;
            let to = parse_architecture_endpoint(right)?;
            return if reverse {
                Some((to, from, arrow))
            } else {
                Some((from, to, arrow))
            };
        }
    }

    None
}

fn parse_architecture_endpoint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((lhs, rhs)) = trimmed.split_once(':') {
        let left = lhs.trim();
        let right = rhs.trim();
        if is_architecture_side_token(left) {
            return clean_label(Some(right));
        }
        if is_architecture_side_token(right) {
            return clean_label(Some(left));
        }
    }

    clean_label(Some(trimmed))
}

fn is_architecture_side_token(token: &str) -> bool {
    matches!(token, "L" | "R" | "T" | "B")
}

fn strip_flowchart_inline_comment(line: &str) -> &str {
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut square_depth = 0_usize;
    let mut paren_depth = 0_usize;
    let mut brace_depth = 0_usize;

    for (idx, ch) in line.char_indices() {
        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != '`' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' | '`' => {
                in_quote = Some(ch);
            }
            '[' => square_depth = square_depth.saturating_add(1),
            ']' => square_depth = square_depth.saturating_sub(1),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '%' => {
                let rest = &line[idx..];
                let prev_is_ws_or_start = match line[..idx].chars().next_back() {
                    None => true,
                    Some(prev) => prev.is_whitespace(),
                };
                if rest.starts_with("%%")
                    && prev_is_ws_or_start
                    && square_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                {
                    return line[..idx].trim_end();
                }
            }
            _ => {}
        }
    }

    line
}

fn parse_graph_direction(header: &str) -> Option<GraphDirection> {
    for token in header.split_whitespace() {
        match token {
            "LR" => return Some(GraphDirection::LR),
            "RL" => return Some(GraphDirection::RL),
            "TB" => return Some(GraphDirection::TB),
            "TD" => return Some(GraphDirection::TD),
            "BT" => return Some(GraphDirection::BT),
            _ => {}
        }
    }
    None
}

fn span_for(line_number: usize, line: &str) -> Span {
    Span::at_line(line_number, line.chars().count())
}

pub fn first_significant_line(input: &str) -> Option<&str> {
    let (content, _) = split_front_matter_block(input);
    content.lines().map(str::trim).find(|line| {
        !line.is_empty() && !is_comment(line) && !line.starts_with("%%{") && !line.ends_with("}%%")
    })
}

fn is_flowchart_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    matches_keyword_header(&lower, "flowchart") || matches_keyword_header(&lower, "graph")
}

const STYLE_DIRECTIVE_DIAGNOSTIC_MESSAGE: &str = "style directives (classDef/style/linkStyle) are parsed, but only SVG output currently \
applies them; terminal/canvas renderers use theme defaults";
const STYLE_DIRECTIVE_DIAGNOSTIC_SUGGESTION: &str = "Use --format svg for styled output or avoid style directives on terminal/canvas surfaces \
until full support lands.";

fn is_non_graph_statement(line: &str) -> bool {
    let check = |line: &str, keyword: &str| {
        line.starts_with(keyword)
            && line
                .as_bytes()
                .get(keyword.len())
                .is_none_or(|&c| (c as char).is_whitespace())
    };
    check(line, "style") || check(line, "classDef") || check(line, "linkStyle")
}

/// Extract `classDef`, `style`, and `linkStyle` directives from raw input
/// and add them as `IrStyleRef` entries to the IR via the builder.
///
/// These directives are parsed and stored in the IR, but not yet wired through
/// to rendering output. A compatibility warning is emitted for each directive
/// to inform users that their styles will not be applied.
fn extract_style_directives(input: &str, builder: &mut IrBuilder) {
    let mut saw_style_directive = false;
    let mut first_style_span = None;

    for (line_number, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        let span = span_for(line_number + 1, raw_line);

        if let Some(rest) = line.strip_prefix("classDef ") {
            saw_style_directive = true;
            if first_style_span.is_none() {
                first_style_span = Some(span);
            }
            // classDef className fill:#fff,stroke:#000,...
            let rest = rest.trim();
            if let Some((name, style)) = rest.split_once(' ') {
                let name = name.trim();
                let style = style.trim();
                if !name.is_empty() && !style.is_empty() {
                    let (_parsed, rejected) = fm_core::parse_style_string_with_rejections(style);
                    if !rejected.is_empty() {
                        let rejected_list = rejected.join(", ");
                        let message = format!(
                            "classDef '{name}' includes unsupported or unsafe style properties (ignored): {rejected_list}"
                        );
                        builder.add_diagnostic(
                            Diagnostic::warning(message)
                                .with_category(DiagnosticCategory::Compatibility)
                                .with_span(span)
                                .with_rule_id("classdef-unsupported-style"),
                        );
                    }
                    builder.push_style_ref(
                        fm_core::IrStyleTarget::Class(name.to_string()),
                        style.to_string(),
                        span,
                    );
                }
            }
        } else if let Some(rest) = line.strip_prefix("style ") {
            saw_style_directive = true;
            if first_style_span.is_none() {
                first_style_span = Some(span);
            }
            // style nodeA,nodeB,nodeC fill:#fff,...
            // Supports comma-separated node IDs before the first space-separated
            // property declaration.
            let rest = rest.trim();
            if let Some((targets, style)) = rest.split_once(' ') {
                let style = style.trim();
                if !style.is_empty() {
                    for target in targets.split(',') {
                        let target = target.trim();
                        if !target.is_empty()
                            && let Some(&node_id) = builder.node_id_by_key(target)
                        {
                            builder.push_style_ref(
                                fm_core::IrStyleTarget::Node(node_id),
                                style.to_string(),
                                span,
                            );
                        }
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("linkStyle ") {
            saw_style_directive = true;
            if first_style_span.is_none() {
                first_style_span = Some(span);
            }
            // linkStyle 0 stroke:#f00,...
            let rest = rest.trim();
            if let Some((index_str, style)) = rest.split_once(' ') {
                let index_str = index_str.trim();
                let style = style.trim();
                if !style.is_empty() {
                    if index_str.eq_ignore_ascii_case("default") {
                        builder.push_style_ref(
                            fm_core::IrStyleTarget::LinkDefault,
                            style.to_string(),
                            span,
                        );
                    } else if let Ok(link_index) = index_str.parse::<usize>() {
                        builder.push_style_ref(
                            fm_core::IrStyleTarget::Link(link_index),
                            style.to_string(),
                            span,
                        );
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("class ") {
            // class nodeA,nodeB className — assign CSS class to nodes.
            let rest = rest.trim();
            if let Some((nodes_raw, class_name)) = rest.rsplit_once(' ') {
                let class_name = class_name.trim();
                if !class_name.is_empty() {
                    for node_key in nodes_raw.split(',') {
                        let node_key = node_key.trim();
                        if !node_key.is_empty() {
                            builder.add_class_to_node(node_key, class_name, span);
                        }
                    }
                }
            }
        }
    }

    if saw_style_directive {
        let mut diagnostic = Diagnostic::warning(STYLE_DIRECTIVE_DIAGNOSTIC_MESSAGE)
            .with_category(DiagnosticCategory::Compatibility)
            .with_suggestion(STYLE_DIRECTIVE_DIAGNOSTIC_SUGGESTION.to_string())
            .with_rule_id("style-directive-not-applied");
        if let Some(span) = first_style_span {
            diagnostic = diagnostic.with_span(span);
        }
        builder.add_diagnostic(diagnostic);
        builder.add_warning(STYLE_DIRECTIVE_DIAGNOSTIC_MESSAGE.to_string());
    }
}

/// Extract `accTitle` and `accDescr` accessibility directives.
///
/// Supported syntax:
/// - `accTitle: My Title`
/// - `accDescr: Single-line description`
/// - `accDescr { multi-line description }`
fn extract_accessibility_directives(input: &str, builder: &mut IrBuilder) {
    let mut in_acc_descr_block = false;
    let mut descr_lines: Vec<&str> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        if in_acc_descr_block {
            if trimmed == "}" {
                in_acc_descr_block = false;
                let descr = descr_lines.join("\n");
                if !descr.is_empty() {
                    builder.set_acc_descr(descr);
                }
                descr_lines.clear();
            } else {
                descr_lines.push(trimmed);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("accTitle:") {
            let title = rest.trim();
            if !title.is_empty() {
                builder.set_acc_title(title.to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("accDescr:") {
            let descr = rest.trim();
            if !descr.is_empty() {
                builder.set_acc_descr(descr.to_string());
            }
        } else if trimmed.starts_with("accDescr") && trimmed.ends_with('{') {
            in_acc_descr_block = true;
            descr_lines.clear();
        }
    }

    // Flush unclosed accDescr block — treat end-of-input as implicit close.
    if in_acc_descr_block
        && builder.parser_config().auto_close_delimiters
        && !descr_lines.is_empty()
    {
        let descr = descr_lines.join("\n");
        if !descr.is_empty() {
            builder.set_acc_descr(descr);
        }
    }
}

fn is_comment(line: &str) -> bool {
    line.starts_with("%%")
}

#[cfg(test)]
mod tests {
    use fm_core::{
        ArrowType, DiagnosticCategory, DiagnosticSeverity, DiagramType, GanttDate, GanttExclude,
        GanttTaskType, GanttTickInterval, GraphDirection, IrEndpoint, IrLabelSegment,
        IrXySeriesKind, MermaidParseMode, NodeShape,
    };

    use super::{
        STYLE_DIRECTIVE_DIAGNOSTIC_MESSAGE, is_dangling_placeholder_node_id, parse_mermaid,
    };
    use crate::{ParserConfig, detect_type, parse_with_mode_and_config};

    #[test]
    fn detects_supported_headers() {
        assert_eq!(detect_type("stateDiagram-v2\nA --> B"), DiagramType::State);
        assert_eq!(
            detect_type("sequenceDiagram\nA->>B: Hi"),
            DiagramType::Sequence
        );
        assert_eq!(detect_type("classDiagram\nA -- B"), DiagramType::Class);
    }

    #[test]
    fn block_alias_requires_word_boundary_for_light_detector() {
        assert_ne!(detect_type("blockquote\nA"), DiagramType::BlockBeta);
    }

    #[test]
    fn detects_header_after_front_matter() {
        let input = "---\nconfig:\n  theme: dark\n---\nsequenceDiagram\nAlice->>Bob: hi";
        assert_eq!(detect_type(input), DiagramType::Sequence);
    }

    #[test]
    fn flowchart_parses_edges_and_labels() {
        let parsed = parse_mermaid("flowchart LR\nA[Start] -->|go| B(End)");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(parsed.ir.direction, GraphDirection::LR);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
        assert_eq!(parsed.ir.labels.len(), 3);
    }

    #[test]
    fn flowchart_parses_chained_edges_left_to_right() {
        let parsed = parse_mermaid("flowchart LR\nA-->B-->C");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "A"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "B"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "C"));
    }

    #[test]
    fn flowchart_parallel_edge_sources_expand_cartesian_product() {
        let parsed = parse_mermaid("flowchart LR\nA & B --> C");
        assert!(
            parsed.warnings.is_empty(),
            "warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.edges.len(), 2);

        let edges: std::collections::BTreeSet<(String, String)> = parsed
            .ir
            .edges
            .iter()
            .map(|edge| {
                let from = match edge.from {
                    IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.clone(),
                    _ => panic!("expected node source"),
                };
                let to = match edge.to {
                    IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.clone(),
                    _ => panic!("expected node target"),
                };
                (from, to)
            })
            .collect();

        assert_eq!(
            edges,
            std::collections::BTreeSet::from([
                ("A".to_string(), "C".to_string()),
                ("B".to_string(), "C".to_string()),
            ])
        );
    }

    #[test]
    fn flowchart_parallel_edge_targets_expand_cartesian_product() {
        let parsed = parse_mermaid("flowchart LR\nA --> B & C");
        assert!(
            parsed.warnings.is_empty(),
            "warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.edges.len(), 2);

        let edges: std::collections::BTreeSet<(String, String)> = parsed
            .ir
            .edges
            .iter()
            .map(|edge| {
                let from = match edge.from {
                    IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.clone(),
                    _ => panic!("expected node source"),
                };
                let to = match edge.to {
                    IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.clone(),
                    _ => panic!("expected node target"),
                };
                (from, to)
            })
            .collect();

        assert_eq!(
            edges,
            std::collections::BTreeSet::from([
                ("A".to_string(), "B".to_string()),
                ("A".to_string(), "C".to_string()),
            ])
        );
    }

    #[test]
    fn flowchart_parallel_edges_expand_both_sides_and_chain_forward() {
        let parsed = parse_mermaid("flowchart LR\nA & B --> C & D --> E");
        assert!(
            parsed.warnings.is_empty(),
            "warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.edges.len(), 6);

        let edges: std::collections::BTreeSet<(String, String)> = parsed
            .ir
            .edges
            .iter()
            .map(|edge| {
                let from = match edge.from {
                    IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.clone(),
                    _ => panic!("expected node source"),
                };
                let to = match edge.to {
                    IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.clone(),
                    _ => panic!("expected node target"),
                };
                (from, to)
            })
            .collect();

        assert_eq!(
            edges,
            std::collections::BTreeSet::from([
                ("A".to_string(), "C".to_string()),
                ("A".to_string(), "D".to_string()),
                ("B".to_string(), "C".to_string()),
                ("B".to_string(), "D".to_string()),
                ("C".to_string(), "E".to_string()),
                ("D".to_string(), "E".to_string()),
            ])
        );
    }

    #[test]
    fn flowchart_ampersand_inside_node_label_does_not_split_parallel_edges() {
        let parsed = parse_mermaid("flowchart LR\nA[Research & Dev] --> B");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.nodes.len(), 2);
        let label = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .and_then(|node| node.label)
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|value| value.text.as_str());
        assert_eq!(label, Some("Research & Dev"));
    }

    #[test]
    fn flowchart_markdown_backtick_label_preserves_plain_text_and_markup() {
        let parsed = parse_mermaid(
            "flowchart LR\nA[\"`**Bold** and *italic* `code` ~~gone~~ &#9829;<br/>next`\"]",
        );
        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .expect("node A");
        let label_id = node.label.expect("markdown label");
        let label = &parsed.ir.labels[label_id.0];
        assert_eq!(label.text, "Bold and italic code gone ♥\nnext");

        let segments = parsed
            .ir
            .label_markup
            .get(&label_id)
            .expect("rich label markup");
        assert!(segments.iter().any(|segment| matches!(
            segment,
            IrLabelSegment::Text { text, bold: true, .. } if text == "Bold"
        )));
        assert!(segments.iter().any(|segment| matches!(
            segment,
            IrLabelSegment::Text {
                text,
                italic: true,
                ..
            } if text == "italic"
        )));
        assert!(segments.iter().any(|segment| matches!(
            segment,
            IrLabelSegment::Text { text, code: true, .. } if text == "code"
        )));
        assert!(segments.iter().any(|segment| matches!(
            segment,
            IrLabelSegment::Text {
                text,
                strike: true,
                ..
            } if text == "gone"
        )));
        assert!(segments.contains(&IrLabelSegment::LineBreak));
    }

    #[test]
    fn state_diagram_parallel_edge_syntax_does_not_create_edges() {
        // Parallel edge syntax (`A & B --> C`) is a flowchart feature.
        // In state diagrams, the `&` is not a recognized operator and the
        // line is treated as a single malformed transition.
        let parsed = parse_mermaid("stateDiagram-v2\nA & B --> C");
        assert_eq!(
            parsed.ir.edges.len(),
            0,
            "state diagram should not expand & into parallel edges"
        );
    }

    #[test]
    fn flowchart_does_not_split_statement_semicolons_inside_labels() {
        let parsed = parse_mermaid("flowchart LR\nA[foo;bar] --> B");
        assert_eq!(parsed.ir.edges.len(), 1);
        let label = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .and_then(|node| node.label)
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|value| value.text.as_str());
        assert_eq!(label, Some("foo;bar"));
    }

    #[test]
    fn flowchart_malformed_chain_keeps_parsed_prefix_in_cluster() {
        let parsed = parse_mermaid("flowchart TB\nsubgraph g[Group]\nA-->B-->\nend");
        assert_eq!(parsed.ir.edges.len(), 2);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "A"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "B"));
        assert!(parsed.ir.nodes.iter().any(|node| node.implicit));

        assert_eq!(parsed.ir.clusters.len(), 1);
        let cluster = &parsed.ir.clusters[0];
        let member_ids: std::collections::BTreeSet<String> = cluster
            .members
            .iter()
            .filter_map(|member| parsed.ir.nodes.get(member.0).map(|node| node.id.clone()))
            .collect();
        assert!(member_ids.contains("A"));
        assert!(member_ids.contains("B"));
        assert!(
            member_ids
                .iter()
                .any(|member_id| is_dangling_placeholder_node_id(member_id))
        );
    }

    #[test]
    fn flowchart_class_directive_assigns_node_classes() {
        let parsed = parse_mermaid("flowchart LR\nA-->B\nclass A,B critical,highlight");
        let node_a = parsed.ir.nodes.iter().find(|node| node.id == "A");
        let node_b = parsed.ir.nodes.iter().find(|node| node.id == "B");

        assert!(node_a.is_some());
        assert!(node_b.is_some());
        let node_a = node_a.expect("node A should exist");
        let node_b = node_b.expect("node B should exist");
        assert!(
            node_a
                .classes
                .iter()
                .any(|class_name| class_name == "critical")
        );
        assert!(
            node_a
                .classes
                .iter()
                .any(|class_name| class_name == "highlight")
        );
        assert!(
            node_b
                .classes
                .iter()
                .any(|class_name| class_name == "critical")
        );
        assert!(
            node_b
                .classes
                .iter()
                .any(|class_name| class_name == "highlight")
        );
    }

    #[test]
    fn flowchart_subgraphs_populate_clusters_with_members() {
        let parsed = parse_mermaid(
            "flowchart TB\nsubgraph api [API Layer]\nA-->B\nend\nsubgraph db [DB Layer]\nC-->D\nend\nB-->C",
        );

        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(parsed.ir.clusters.len(), 2);

        let node_index_by_id: std::collections::BTreeMap<String, usize> = parsed
            .ir
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| (node.id.clone(), idx))
            .collect();

        let api_cluster = parsed
            .ir
            .clusters
            .iter()
            .find(|cluster| {
                cluster
                    .title
                    .and_then(|title_id| parsed.ir.labels.get(title_id.0))
                    .is_some_and(|label| label.text.as_str() == "API Layer")
            })
            .expect("expected API Layer cluster");
        let db_cluster = parsed
            .ir
            .clusters
            .iter()
            .find(|cluster| {
                cluster
                    .title
                    .and_then(|title_id| parsed.ir.labels.get(title_id.0))
                    .is_some_and(|label| label.text.as_str() == "DB Layer")
            })
            .expect("expected DB Layer cluster");

        let a_idx = node_index_by_id.get("A").copied().expect("A should exist");
        let b_idx = node_index_by_id.get("B").copied().expect("B should exist");
        let c_idx = node_index_by_id.get("C").copied().expect("C should exist");
        let d_idx = node_index_by_id.get("D").copied().expect("D should exist");

        assert!(api_cluster.members.iter().any(|member| member.0 == a_idx));
        assert!(api_cluster.members.iter().any(|member| member.0 == b_idx));
        assert!(db_cluster.members.iter().any(|member| member.0 == c_idx));
        assert!(db_cluster.members.iter().any(|member| member.0 == d_idx));
    }

    #[test]
    fn flowchart_subgraph_end_allows_inline_comment() {
        let parsed =
            parse_mermaid("flowchart TB\nsubgraph api [API]\nA-->B\nend %% close api\nC-->D");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.clusters.len(), 1);

        let api_cluster = &parsed.ir.clusters[0];
        let member_ids: std::collections::BTreeSet<String> = api_cluster
            .members
            .iter()
            .filter_map(|member| parsed.ir.nodes.get(member.0).map(|node| node.id.clone()))
            .collect();
        assert_eq!(
            member_ids,
            std::collections::BTreeSet::from(["A".to_string(), "B".to_string()])
        );
    }

    #[test]
    fn flowchart_subgraph_parses_whitespace_and_quoted_title_forms() {
        let parsed = parse_mermaid("flowchart TB\nsubgraph\tapi \"API Layer\"\nA-->B\nend");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.clusters.len(), 1);

        let cluster = &parsed.ir.clusters[0];
        let title = cluster
            .title
            .and_then(|title_id| parsed.ir.labels.get(title_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(title, Some("API Layer"));
    }

    #[test]
    fn flowchart_subgraph_title_only_preserves_full_title() {
        let parsed = parse_mermaid("flowchart TB\nsubgraph API Layer\nA-->B\nend");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.clusters.len(), 1);

        let cluster = &parsed.ir.clusters[0];
        let title = cluster
            .title
            .and_then(|title_id| parsed.ir.labels.get(title_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(title, Some("API Layer"));
    }

    #[test]
    fn flowchart_subgraph_explicit_key_preserves_unquoted_title() {
        let parsed = parse_mermaid("flowchart TB\nsubgraph api API Layer\nA-->B\nend");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.clusters.len(), 1);

        let cluster = &parsed.ir.clusters[0];
        let title = cluster
            .title
            .and_then(|title_id| parsed.ir.labels.get(title_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(title, Some("API Layer"));
    }

    #[test]
    fn flowchart_nested_subgraphs_populate_graph_hierarchy() {
        let parsed = parse_mermaid(
            "flowchart TB\nsubgraph api [API]\nA-->B\nsubgraph workers [Workers]\nB-->C\nend\nend",
        );

        assert_eq!(parsed.ir.graph.subgraphs.len(), 2);
        assert_eq!(parsed.ir.graph.clusters.len(), 2);

        let api_idx = parsed
            .ir
            .graph
            .subgraphs
            .iter()
            .position(|subgraph| subgraph.key == "api")
            .expect("api subgraph should exist");
        let workers_idx = parsed
            .ir
            .graph
            .subgraphs
            .iter()
            .position(|subgraph| subgraph.key == "workers")
            .expect("workers subgraph should exist");

        let api = &parsed.ir.graph.subgraphs[api_idx];
        let workers = &parsed.ir.graph.subgraphs[workers_idx];
        assert_eq!(api.parent, None);
        assert_eq!(api.children, vec![fm_core::IrSubgraphId(workers_idx)]);
        assert_eq!(workers.parent, Some(fm_core::IrSubgraphId(api_idx)));

        let node_b = parsed
            .ir
            .find_node_index("B")
            .and_then(|index| parsed.ir.graph.nodes.get(index))
            .expect("node B should exist");
        assert_eq!(node_b.subgraphs.len(), 2);
        assert_eq!(node_b.clusters.len(), 2);

        let worker_cluster = &parsed.ir.graph.clusters[workers_idx];
        assert_eq!(
            worker_cluster.subgraph,
            Some(fm_core::IrSubgraphId(workers_idx))
        );
        assert!(!worker_cluster.members.is_empty());
    }

    #[test]
    fn flowchart_duplicate_subgraph_keys_remain_distinct_groups() {
        let parsed = parse_mermaid(
            "flowchart TB\nsubgraph api [First]\nA-->B\nend\nsubgraph api [Second]\nC-->D\nend",
        );

        assert_eq!(parsed.ir.clusters.len(), 2);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 2);

        let first_cluster = &parsed.ir.clusters[0];
        let second_cluster = &parsed.ir.clusters[1];
        assert_ne!(first_cluster.id, second_cluster.id);

        let first_members: std::collections::BTreeSet<String> = first_cluster
            .members
            .iter()
            .filter_map(|member| parsed.ir.nodes.get(member.0).map(|node| node.id.clone()))
            .collect();
        let second_members: std::collections::BTreeSet<String> = second_cluster
            .members
            .iter()
            .filter_map(|member| parsed.ir.nodes.get(member.0).map(|node| node.id.clone()))
            .collect();
        assert_eq!(
            first_members,
            std::collections::BTreeSet::from(["A".to_string(), "B".to_string()])
        );
        assert_eq!(
            second_members,
            std::collections::BTreeSet::from(["C".to_string(), "D".to_string()])
        );

        let first_subgraph = &parsed.ir.graph.subgraphs[0];
        let second_subgraph = &parsed.ir.graph.subgraphs[1];
        assert_eq!(first_subgraph.key, "api");
        assert_eq!(second_subgraph.key, "api");
        assert_eq!(first_subgraph.members.len(), 2);
        assert_eq!(second_subgraph.members.len(), 2);
        assert_ne!(first_subgraph.members, second_subgraph.members);
    }

    #[test]
    fn flowchart_inline_comment_does_not_strip_node_labels_with_percent_signs() {
        let parsed = parse_mermaid("flowchart TB\nA[50%% done]-->B");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.edges.len(), 1);

        let label = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .and_then(|node| node.label)
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(label, Some("50%% done"));
    }

    #[test]
    fn flowchart_inline_comment_terminates_the_line_before_statement_split() {
        let parsed = parse_mermaid("flowchart TB\nA-->B %% note; C-->D");
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.edges.len(), 1);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "A"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "B"));
        assert!(!parsed.ir.nodes.iter().any(|node| node.id == "C"));
        assert!(!parsed.ir.nodes.iter().any(|node| node.id == "D"));
    }

    #[test]
    fn flowchart_direction_statement_updates_graph_direction() {
        let parsed = parse_mermaid("flowchart\n direction LR\n A-->B");
        assert_eq!(parsed.ir.direction, GraphDirection::LR);
        assert_eq!(parsed.ir.meta.direction, GraphDirection::LR);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
    }

    #[test]
    fn flowchart_nested_header_does_not_override_top_level_direction() {
        let parsed =
            parse_mermaid("flowchart TB\nsubgraph api [API]\nflowchart RL\nA-->B\nend\nB-->C");
        assert_eq!(parsed.ir.direction, GraphDirection::TB);
        assert_eq!(parsed.ir.meta.direction, GraphDirection::TB);
        assert!(parsed.warnings.iter().any(|warning| {
            warning.contains("nested flowchart header ignored inside subgraph")
        }));
    }

    #[test]
    fn flowchart_subgraph_direction_statement_is_stored_on_subgraph() {
        let parsed =
            parse_mermaid("flowchart LR\nsubgraph api [API]\n  direction TB\n  A-->B\nend\nB-->C");
        let subgraph = parsed.ir.graph.root_subgraphs().into_iter().next();

        assert!(
            parsed.warnings.is_empty(),
            "warnings: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.ir.direction, GraphDirection::LR);
        assert_eq!(
            subgraph.and_then(|value| value.direction),
            Some(GraphDirection::TB)
        );
    }

    #[test]
    fn flowchart_click_directive_marks_safe_link_nodes() {
        let parsed = parse_mermaid("flowchart LR\nA-->B\nclick A \"https://example.com/docs\"");
        let node_a = parsed.ir.nodes.iter().find(|node| node.id == "A");

        assert!(node_a.is_some());
        let node_a = node_a.expect("node A should exist");
        assert!(
            node_a
                .classes
                .iter()
                .any(|class_name| class_name == "has-link")
        );
    }

    #[test]
    fn flowchart_click_directive_warns_on_unsafe_links() {
        let parsed = parse_mermaid("flowchart LR\nA-->B\nclick A \"javascript:alert(1)\"");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("unsafe click link target blocked"))
        );
    }

    #[test]
    fn flowchart_click_directive_blocks_percent_encoded_scheme_bypass() {
        let parsed = parse_mermaid("flowchart LR\nA-->B\nclick A \"javascript%3Aalert(1)\"");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("unsafe click link target blocked"))
        );
    }

    #[test]
    fn flowchart_click_callback_directive_sets_callback_and_tooltip() {
        let parsed = parse_mermaid("flowchart LR\nA-->B\nclick B call focusNode \"Focus node\"");
        let node_b = parsed.ir.nodes.iter().find(|node| node.id == "B");

        assert!(node_b.is_some());
        let node_b = node_b.expect("node B should exist");
        assert_eq!(node_b.callback.as_deref(), Some("focusNode"));
        assert_eq!(node_b.tooltip.as_deref(), Some("Focus node"));
        assert!(
            node_b
                .classes
                .iter()
                .any(|class_name| class_name == "has-callback")
        );
        assert!(node_b.href.is_none());
    }

    #[test]
    fn sequence_parses_messages() {
        let parsed = parse_mermaid(
            "sequenceDiagram\nparticipant Alice\nparticipant Bob\nAlice->>Bob: Hello",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sequence);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn sequence_supports_solid_line_without_arrowhead() {
        let parsed = parse_mermaid("sequenceDiagram\nAlice->Bob: Ping");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Line);
    }

    #[test]
    fn sequence_supports_dotted_line_without_arrowhead() {
        let parsed = parse_mermaid("sequenceDiagram\nAlice-->Bob: Ping");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::DottedLine);
    }

    #[test]
    fn sequence_supports_bidirectional_arrowheads() {
        let parsed = parse_mermaid("sequenceDiagram\nAlice<<->>Bob: Sync");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::DoubleArrow);
    }

    #[test]
    fn sequence_supports_dotted_bidirectional_arrowheads() {
        let parsed = parse_mermaid("sequenceDiagram\nAlice<<-->>Bob: Sync");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::DoubleDottedArrow);
    }

    #[test]
    fn sequence_supports_async_open_arrowheads() {
        let parsed = parse_mermaid("sequenceDiagram\nAlice-)Bob: Async");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::OpenArrow);

        let dotted = parse_mermaid("sequenceDiagram\nAlice--)Bob: Async");
        assert_eq!(dotted.ir.edges.len(), 1);
        assert_eq!(dotted.ir.edges[0].arrow, ArrowType::DottedOpenArrow);
    }

    #[test]
    fn sequence_distinguishes_solid_and_dotted_cross() {
        let solid = parse_mermaid("sequenceDiagram\nAlice-xBob: Stop");
        assert_eq!(solid.ir.edges[0].arrow, ArrowType::Cross);

        let dotted = parse_mermaid("sequenceDiagram\nAlice--xBob: Stop");
        assert_eq!(dotted.ir.edges[0].arrow, ArrowType::DottedCross);
    }

    #[test]
    fn sequence_supports_half_arrow_variants() {
        let top = parse_mermaid("sequenceDiagram\nAlice-|\\Bob: HalfTop");
        assert_eq!(top.ir.edges[0].arrow, ArrowType::HalfArrowTop);

        let bottom = parse_mermaid("sequenceDiagram\nAlice--|/Bob: HalfBottom");
        assert_eq!(bottom.ir.edges[0].arrow, ArrowType::HalfArrowBottomDotted);

        let reverse_top = parse_mermaid("sequenceDiagram\nAlice/|-Bob: ReverseTop");
        assert_eq!(
            reverse_top.ir.edges[0].arrow,
            ArrowType::HalfArrowTopReverse
        );

        let reverse_bottom = parse_mermaid("sequenceDiagram\nAlice\\|--Bob: ReverseBottom");
        assert_eq!(
            reverse_bottom.ir.edges[0].arrow,
            ArrowType::HalfArrowBottomReverseDotted
        );
    }

    #[test]
    fn sequence_supports_stick_half_arrow_variants() {
        let top = parse_mermaid("sequenceDiagram\nAlice-\\\\Bob: StickTop");
        assert_eq!(top.ir.edges[0].arrow, ArrowType::StickArrowTop);

        let bottom = parse_mermaid("sequenceDiagram\nAlice--//Bob: StickBottom");
        assert_eq!(bottom.ir.edges[0].arrow, ArrowType::StickArrowBottomDotted);

        let reverse_top = parse_mermaid("sequenceDiagram\nAlice//-Bob: ReverseStickTop");
        assert_eq!(
            reverse_top.ir.edges[0].arrow,
            ArrowType::StickArrowTopReverse
        );

        let reverse_bottom = parse_mermaid("sequenceDiagram\nAlice\\\\--Bob: ReverseStickBottom");
        assert_eq!(
            reverse_bottom.ir.edges[0].arrow,
            ArrowType::StickArrowBottomReverseDotted
        );
    }

    #[test]
    fn sequence_supports_central_connection_syntax() {
        let right = parse_mermaid("sequenceDiagram\nAlice->>()John: Hello");
        assert_eq!(right.ir.edges.len(), 1);
        assert_eq!(right.ir.edges[0].arrow, ArrowType::Arrow);
        assert!(right.warnings.is_empty());

        let left = parse_mermaid("sequenceDiagram\nAlice()->>John: How are you?");
        assert_eq!(left.ir.edges.len(), 1);
        assert_eq!(left.ir.edges[0].arrow, ArrowType::Arrow);
        assert!(left.warnings.is_empty());

        let dual = parse_mermaid("sequenceDiagram\nJohn()->>()Alice: Great!");
        assert_eq!(dual.ir.edges.len(), 1);
        assert_eq!(dual.ir.edges[0].arrow, ArrowType::Arrow);
        assert!(dual.warnings.is_empty());
    }

    #[test]
    fn sequence_preserves_specific_participant_parse_warning() {
        let parsed = parse_mermaid("sequenceDiagram\nparticipant \"\"");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("unable to parse participant declaration"))
        );
    }

    #[test]
    fn sequence_preserves_specific_actor_parse_warning() {
        let parsed = parse_mermaid("sequenceDiagram\nactor \"\"");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("unable to parse actor declaration"))
        );
    }

    #[test]
    fn class_parses_nodes_edges_and_assignments() {
        let parsed = parse_mermaid("classDiagram\nA -- B\nclass A critical");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Class);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);

        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .expect("class node A should exist");
        assert!(
            node.classes
                .iter()
                .any(|class_name| class_name == "critical")
        );
    }

    #[test]
    fn class_preserves_unsupported_warning() {
        let parsed = parse_mermaid("classDiagram\n???");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("unsupported class syntax"))
        );
    }

    #[test]
    fn state_parses_declarations_and_transitions() {
        let parsed = parse_mermaid("stateDiagram-v2\nstate Idle\n[*] --> Idle\nIdle --> Done");
        assert_eq!(parsed.ir.diagram_type, DiagramType::State);
        assert!(parsed.ir.nodes.len() >= 2);
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn er_parses_entities_and_relationships() {
        let parsed = parse_mermaid("erDiagram\nCUSTOMER ||--o{ ORDER : places");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Er);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.labels.len(), 1);
    }

    #[test]
    fn er_parses_entity_attributes() {
        use fm_core::IrAttributeKey;

        let input = r"erDiagram
    CUSTOMER {
        int id PK
        string name
        string email UK
    }
";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Er);
        assert_eq!(parsed.ir.nodes.len(), 1);

        let customer = &parsed.ir.nodes[0];
        assert_eq!(customer.id, "CUSTOMER");
        assert_eq!(customer.members.len(), 3);

        // Check first attribute: int id PK
        assert_eq!(customer.members[0].data_type, "int");
        assert_eq!(customer.members[0].name, "id");
        assert_eq!(customer.members[0].key, IrAttributeKey::Pk);

        // Check second attribute: string name
        assert_eq!(customer.members[1].data_type, "string");
        assert_eq!(customer.members[1].name, "name");
        assert_eq!(customer.members[1].key, IrAttributeKey::None);

        // Check third attribute: string email UK
        assert_eq!(customer.members[2].data_type, "string");
        assert_eq!(customer.members[2].name, "email");
        assert_eq!(customer.members[2].key, IrAttributeKey::Uk);
    }

    #[test]
    fn er_parses_attributes_with_comments() {
        use fm_core::IrAttributeKey;

        let input = r#"erDiagram
    ORDER {
        int order_id PK "unique identifier"
        int customer_id FK "references CUSTOMER"
        date created_at
    }
"#;
        let parsed = parse_mermaid(input);
        let order = &parsed.ir.nodes[0];
        assert_eq!(order.members.len(), 3);

        // Check FK with comment
        assert_eq!(order.members[1].key, IrAttributeKey::Fk);
        assert_eq!(
            order.members[1].comment.as_deref(),
            Some("references CUSTOMER")
        );

        // Check attribute without key
        assert_eq!(order.members[2].data_type, "date");
        assert_eq!(order.members[2].name, "created_at");
        assert_eq!(order.members[2].key, IrAttributeKey::None);
    }

    #[test]
    fn er_parses_complex_schema() {
        let input = r"erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    CUSTOMER {
        int id PK
        string name
    }
    ORDER {
        int id PK
        int customer_id FK
    }
    LINE_ITEM {
        int id PK
        int order_id FK
        int quantity
    }
";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Er);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);

        // Check CUSTOMER has 2 attributes
        let customer = parsed.ir.nodes.iter().find(|n| n.id == "CUSTOMER").unwrap();
        assert_eq!(customer.members.len(), 2);

        // Check ORDER has 2 attributes
        let order = parsed.ir.nodes.iter().find(|n| n.id == "ORDER").unwrap();
        assert_eq!(order.members.len(), 2);

        // Check LINE_ITEM has 3 attributes
        let line_item = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.id == "LINE_ITEM")
            .unwrap();
        assert_eq!(line_item.members.len(), 3);
    }

    #[test]
    fn er_handles_complex_type_names() {
        let input = r"erDiagram
    TABLE {
        varchar(255) email
        decimal(10,2) price
        timestamp created_at
    }
";
        let parsed = parse_mermaid(input);
        let table = &parsed.ir.nodes[0];

        assert_eq!(table.members[0].data_type, "varchar(255)");
        assert_eq!(table.members[1].data_type, "decimal(10,2)");
        assert_eq!(table.members[2].data_type, "timestamp");
    }

    #[test]
    fn journey_parses_steps_as_linear_edges() {
        let parsed = parse_mermaid("journey\nsection Sprint\nWrite code: 5: me\nShip: 3: me");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Journey);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);

        let write_code = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "Write_code")
            .expect("journey step node");
        assert!(
            write_code
                .classes
                .iter()
                .any(|class_name| class_name == "journey-score-5")
        );
        assert!(
            write_code
                .classes
                .iter()
                .any(|class_name| class_name == "journey-actor-me")
        );
        assert_eq!(parsed.ir.clusters.len(), 1);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 1);
    }

    #[test]
    fn journey_sections_group_steps_and_keep_multiple_actors() {
        let parsed = parse_mermaid(
            "journey\nsection Board\nBacklog: 5: Alice, Bob\nsection Ship\nRelease: 4: Alice",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::Journey);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.clusters.len(), 2);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 2);

        let backlog = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "Backlog")
            .expect("backlog node");
        assert!(
            backlog
                .classes
                .iter()
                .any(|class_name| class_name == "journey-actor-Alice")
        );
        assert!(
            backlog
                .classes
                .iter()
                .any(|class_name| class_name == "journey-actor-Bob")
        );

        let first_section = &parsed.ir.graph.subgraphs[0];
        let second_section = &parsed.ir.graph.subgraphs[1];
        assert_eq!(first_section.members.len(), 1);
        assert_eq!(second_section.members.len(), 1);
    }

    #[test]
    fn timeline_parses_periods_and_events() {
        let parsed = parse_mermaid("timeline\n2025 : kickoff\n2026 : launch");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Timeline);
        // 2 time periods + 2 events = 4 nodes
        assert_eq!(parsed.ir.nodes.len(), 4);
        // 1 edge between periods (2025->2026) + 2 edges from periods to events = 3 edges
        assert_eq!(parsed.ir.edges.len(), 3);
    }

    #[test]
    fn packet_beta_parses_connections() {
        let parsed = parse_mermaid("packet-beta\nClient -> Gateway\nGateway -> Backend");
        assert_eq!(parsed.ir.diagram_type, DiagramType::PacketBeta);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn block_beta_parses_basic_blocks_without_flowchart_fallback_warning() {
        let parsed = parse_mermaid("block-beta\ncolumns 2\nalpha[Alpha]\nbeta[Beta]");
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);
        assert_eq!(parsed.ir.meta.block_beta_columns, Some(2));
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 0);
        assert!(
            parsed
                .warnings
                .iter()
                .all(|warning| !warning.contains("using best-effort flowchart parsing"))
        );

        let alpha = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "alpha")
            .unwrap();
        let beta = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "beta")
            .unwrap();
        assert!(
            alpha
                .classes
                .iter()
                .any(|class_name| class_name == "block-beta")
        );
        assert!(
            beta.classes
                .iter()
                .any(|class_name| class_name == "block-beta")
        );
        assert!(
            parsed
                .warnings
                .iter()
                .all(|warning| !warning.contains("block-beta columns"))
        );
    }

    #[test]
    fn block_beta_parses_multiple_blocks_on_one_line() {
        let parsed = parse_mermaid("block-beta\nsvc[Service] db[(Database)] cache:2");
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "svc"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "db"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "cache"));

        let db = parsed.ir.nodes.iter().find(|node| node.id == "db").unwrap();
        assert_eq!(db.shape, NodeShape::Cylinder);
        let cache = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "cache")
            .unwrap();
        assert!(
            cache
                .classes
                .iter()
                .any(|class_name| class_name == "block-beta-span-2")
        );
    }

    #[test]
    fn block_beta_nested_groups_populate_clusters_and_subgraphs() {
        let parsed = parse_mermaid(
            "block-beta\nblock:api\nsvc[Service]\nblock:data:2\ndb[(Database)]\nend\nend",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);
        assert_eq!(parsed.ir.clusters.len(), 2);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 2);

        let api = parsed.ir.graph.subgraphs_by_key("api");
        let data = parsed.ir.graph.subgraphs_by_key("data");
        assert_eq!(api.len(), 1);
        assert_eq!(data.len(), 1);
        let api = api[0];
        let data = data[0];
        assert_eq!(api.parent, None);
        assert_eq!(data.parent, Some(api.id));
        assert_eq!(api.grid_span, 1);
        assert_eq!(data.grid_span, 2);

        let svc = parsed.ir.find_node_index("svc").unwrap();
        let db = parsed.ir.find_node_index("db").unwrap();
        let svc_graph = &parsed.ir.graph.nodes[svc];
        let db_graph = &parsed.ir.graph.nodes[db];
        assert_eq!(svc_graph.subgraphs, vec![api.id]);
        assert_eq!(db_graph.subgraphs, vec![api.id, data.id]);
        assert!(
            parsed
                .warnings
                .iter()
                .all(|warning| !warning.contains("group span layout is not implemented yet"))
        );
    }

    #[test]
    fn block_beta_parses_edges_between_blocks() {
        let parsed = parse_mermaid("block-beta\nalpha[Alpha] beta[Beta]\nalpha --> beta");
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
    }

    #[test]
    fn block_beta_space_blocks_are_materialized_as_placeholder_nodes() {
        let parsed = parse_mermaid("block-beta\nspace:2");
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);
        assert_eq!(parsed.ir.nodes.len(), 1);
        let space = &parsed.ir.nodes[0];
        assert!(space.id.starts_with("__space_"));
        assert!(
            space
                .classes
                .iter()
                .any(|class_name| class_name == "block-beta-space")
        );
        assert_eq!(parsed.ir.meta.block_beta_columns, None);
        assert!(
            parsed
                .warnings
                .iter()
                .all(|warning| !warning.contains("span-aware layout is not implemented yet"))
        );
    }

    #[test]
    fn block_beta_zero_spans_warn_and_coerce_to_one() {
        let parsed = parse_mermaid("block-beta\nblock:api:0\ncache:0\nend");
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);

        let api = parsed.ir.graph.subgraphs_by_key("api");
        assert_eq!(api.len(), 1);
        assert_eq!(api[0].grid_span, 1);

        let cache = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "cache")
            .unwrap();
        assert!(
            cache
                .classes
                .iter()
                .all(|class_name| class_name != "block-beta-span-0")
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("group 'api' span must be >= 1"))
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("block-beta block 'cache' span must be >= 1"))
        );
    }

    #[test]
    fn block_beta_document_parsing_preserves_end_warnings() {
        let stray_end = parse_mermaid("block-beta\nend");
        assert!(
            stray_end
                .warnings
                .iter()
                .any(|warning| warning.contains("end without matching block group"))
        );

        let unclosed = parse_mermaid("block-beta\nblock:api\nsvc[Service]");
        assert!(
            unclosed
                .warnings
                .iter()
                .any(|warning| warning.contains("ended with 1 unclosed block group"))
        );
        assert!(unclosed.ir.nodes.iter().any(|node| node.id == "svc"));
    }

    #[test]
    fn gantt_parses_tasks_as_nodes() {
        let parsed = parse_mermaid(
            "gantt\ntitle Release\nsection Phase 1\nDesign :a1, 2026-02-01, 3d\nBuild :a2, after a1, 5d",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::Gantt);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        let gantt_meta = parsed.ir.gantt_meta.as_ref().expect("gantt meta");
        assert_eq!(gantt_meta.title.as_deref(), Some("Release"));
        assert_eq!(gantt_meta.sections.len(), 1);
        assert_eq!(gantt_meta.sections[0].name, "Phase 1");
        assert_eq!(gantt_meta.tasks.len(), 2);
        assert_eq!(gantt_meta.tasks[0].task_id.as_deref(), Some("a1"));
        assert_eq!(
            gantt_meta.tasks[0].start,
            Some(GanttDate::Absolute("2026-02-01".to_string()))
        );
        assert_eq!(gantt_meta.tasks[0].end, Some(GanttDate::DurationDays(3)));
        assert_eq!(gantt_meta.tasks[1].depends_on, vec!["a1".to_string()]);
    }

    #[test]
    fn pie_parses_slice_entries() {
        let parsed = parse_mermaid("pie\n\"Cats\" : 40\n\"Dogs\" : 60");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Pie);
        assert_eq!(parsed.ir.nodes.len(), 2);
    }

    #[test]
    fn quadrant_parses_points() {
        let parsed = parse_mermaid(
            "quadrantChart\nx-axis Low --> High\ny-axis Slow --> Fast\nFeatureA: [0.2, 0.9]",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::QuadrantChart);
        assert_eq!(parsed.ir.nodes.len(), 1);
    }

    #[test]
    fn xychart_parses_series_into_points_and_edges() {
        let parsed = parse_mermaid(
            "xychart-beta\ntitle \"Quarterly\"\nx-axis [Q1, Q2, Q3]\ny-axis \"Revenue\" 0 --> 30\nbar Revenue [12, 18, 24]\nline Target [10, 15, 20]",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::XyChart);
        let xy_meta = parsed.ir.xy_chart_meta.as_ref().expect("xy chart meta");
        assert_eq!(xy_meta.title.as_deref(), Some("Quarterly"));
        assert_eq!(xy_meta.x_axis.categories, vec!["Q1", "Q2", "Q3"]);
        assert_eq!(xy_meta.y_axis.label.as_deref(), Some("Revenue"));
        assert_eq!(xy_meta.y_axis.min, Some(0.0));
        assert_eq!(xy_meta.y_axis.max, Some(30.0));
        assert_eq!(xy_meta.series.len(), 2);
        assert_eq!(xy_meta.series[0].kind, IrXySeriesKind::Bar);
        assert_eq!(xy_meta.series[1].kind, IrXySeriesKind::Line);
        assert_eq!(xy_meta.series[0].values, vec![12.0, 18.0, 24.0]);
        assert_eq!(xy_meta.series[1].values, vec![10.0, 15.0, 20.0]);
        assert_eq!(parsed.ir.nodes.len(), 6);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "Revenue_1"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "Target_3"));
    }

    #[test]
    fn requirement_parses_requirements_and_relations() {
        let parsed = parse_mermaid(
            "requirementDiagram\nrequirement REQ_1 {\n  id: 1\n}\nrequirement REQ_2 {\n  id: 2\n}\nREQ_1 -> REQ_2",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::Requirement);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
    }

    #[test]
    fn mindmap_parses_indented_tree_structure() {
        let parsed = parse_mermaid("mindmap\nRoot\n  BranchA\n    LeafA\n  BranchB");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Mindmap);
        assert_eq!(parsed.ir.nodes.len(), 4);
        assert_eq!(parsed.ir.edges.len(), 3);
    }

    #[test]
    fn mindmap_parses_square_shape() {
        let parsed = parse_mermaid("mindmap\n  id[Square Node]");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].shape, NodeShape::Rect);
        assert_eq!(parsed.ir.nodes[0].id, "id");
    }

    #[test]
    fn mindmap_parses_rounded_shape() {
        let parsed = parse_mermaid("mindmap\n  id(Rounded Node)");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].shape, NodeShape::Rounded);
    }

    #[test]
    fn mindmap_parses_circle_shape() {
        let parsed = parse_mermaid("mindmap\n  id((Circle Node))");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].shape, NodeShape::Circle);
    }

    #[test]
    fn mindmap_parses_bang_shape() {
        let parsed = parse_mermaid("mindmap\n  id))Bang Node((");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].shape, NodeShape::Asymmetric);
        assert_eq!(parsed.ir.nodes[0].id, "id");
    }

    #[test]
    fn mindmap_parses_cloud_shape() {
        let parsed = parse_mermaid("mindmap\n  id)Cloud Node(");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].shape, NodeShape::Cloud);
        assert_eq!(parsed.ir.nodes[0].id, "id");
    }

    #[test]
    fn mindmap_parses_hexagon_shape() {
        let parsed = parse_mermaid("mindmap\n  id{{Hexagon Node}}");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].shape, NodeShape::Hexagon);
        assert_eq!(parsed.ir.nodes[0].id, "id");
    }

    #[test]
    fn mindmap_handles_icon_directive() {
        let parsed = parse_mermaid("mindmap\n  Root\n    Child\n    ::icon(fa fa-book)");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert!(parsed.warnings.is_empty());
        let child = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "Child")
            .unwrap();
        assert_eq!(child.icon.as_deref(), Some("fa fa-book"));
    }

    #[test]
    fn flowchart_extracts_fontawesome_icon_prefix_from_label() {
        let parsed = parse_mermaid("flowchart LR\nA[fa:server API]");
        let node = parsed.ir.nodes.iter().find(|node| node.id == "A").unwrap();
        let label = node
            .label
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(node.icon.as_deref(), Some("fa:server"));
        assert_eq!(label, Some("API"));
    }

    #[test]
    fn flowchart_extracts_emoji_icon_prefix_from_label() {
        let parsed = parse_mermaid("flowchart LR\nA[🚀 Deploy]");
        let node = parsed.ir.nodes.iter().find(|node| node.id == "A").unwrap();
        let label = node
            .label
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(node.icon.as_deref(), Some("🚀"));
        assert_eq!(label, Some("Deploy"));
    }

    #[test]
    fn mindmap_handles_class_directive() {
        let parsed = parse_mermaid("mindmap\n  Root\n    A[Node A]\n    :::urgent large");
        assert_eq!(parsed.ir.nodes.len(), 2);
        // Classes are applied to the previous node
        let node_a = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        assert!(node_a.classes.contains(&"urgent".to_string()));
        assert!(node_a.classes.contains(&"large".to_string()));
    }

    #[test]
    fn mindmap_complex_hierarchy() {
        let input = r"mindmap
  root((mindmap))
    Origins
      Long history
      Popularisation
        Tony Buzan
    Research
      On effectiveness
    Tools
      Pen and paper
      Mermaid";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Mindmap);
        // root, Origins, Long history, Popularisation, Tony Buzan, Research, On effectiveness, Tools, Pen and paper, Mermaid = 10 nodes
        assert_eq!(parsed.ir.nodes.len(), 10);
        // Check root has circle shape
        let root = parsed.ir.nodes.iter().find(|n| n.id == "root").unwrap();
        assert_eq!(root.shape, NodeShape::Circle);
    }

    #[test]
    fn timeline_parses_basic_structure() {
        let input = r"timeline
    title History of Social Media
    2002 : LinkedIn
    2004 : Facebook
    2005 : YouTube";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Timeline);
        // 3 time periods + 3 events = 6 nodes
        assert_eq!(parsed.ir.nodes.len(), 6);
        // Timeline sequence edges (2002->2004->2005) + event edges (period->event)
        assert!(parsed.ir.edges.len() >= 5);
    }

    #[test]
    fn timeline_parses_multiple_events_per_period() {
        let input = "timeline\n    2004 : Facebook : Google";
        let parsed = parse_mermaid(input);
        // 1 time period + 2 events = 3 nodes
        assert_eq!(parsed.ir.nodes.len(), 3);
        // 2 edges from period to each event
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn timeline_parses_continuation_events() {
        let input = r"timeline
    2004 : Facebook
         : Google
         : Orkut";
        let parsed = parse_mermaid(input);
        // 1 time period + 3 events = 4 nodes
        assert_eq!(parsed.ir.nodes.len(), 4);
        // 3 edges from period to each event
        assert_eq!(parsed.ir.edges.len(), 3);
    }

    #[test]
    fn timeline_parses_sections() {
        let input = r"timeline
    title Timeline
    section Early Days
        2002 : LinkedIn
    section Growth Era
        2004 : Facebook";
        let parsed = parse_mermaid(input);
        // 2 time periods + 2 events = 4 nodes
        assert_eq!(parsed.ir.nodes.len(), 4);
        // 2 clusters (sections)
        assert_eq!(parsed.ir.clusters.len(), 2);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 2);
        assert_eq!(parsed.ir.graph.clusters.len(), 2);

        let early_days = parsed.ir.graph.subgraphs_by_key("Early Days");
        let growth_era = parsed.ir.graph.subgraphs_by_key("Growth Era");

        assert_eq!(early_days.len(), 1);
        assert_eq!(growth_era.len(), 1);

        let early_days = early_days[0];
        let growth_era = growth_era[0];

        assert_eq!(early_days.parent, None);
        assert_eq!(growth_era.parent, None);
        assert_eq!(early_days.members.len(), 2);
        assert_eq!(growth_era.members.len(), 2);
    }

    #[test]
    fn timeline_events_have_rounded_shape() {
        let parsed = parse_mermaid("timeline\n    2004 : Facebook");
        let event = parsed.ir.nodes.iter().find(|n| n.id == "Facebook").unwrap();
        assert_eq!(event.shape, NodeShape::Rounded);
    }

    #[test]
    fn timeline_periods_have_rect_shape() {
        let parsed = parse_mermaid("timeline\n    2004 : Facebook");
        let period = parsed.ir.nodes.iter().find(|n| n.id == "2004").unwrap();
        assert_eq!(period.shape, NodeShape::Rect);
    }

    #[test]
    fn init_directive_applies_theme_and_direction_hint() {
        let parsed = parse_mermaid(
            "%%{init: {\"theme\":\"dark\",\"themeVariables\":{\"primaryColor\":\"#fff\"},\"flowchart\":{\"direction\":\"RL\"}}}%%\nflowchart LR\nA-->B",
        );
        assert_eq!(parsed.ir.meta.init.config.theme.as_deref(), Some("dark"));
        assert_eq!(
            parsed.ir.meta.theme_overrides.theme.as_deref(),
            Some("dark")
        );
        assert_eq!(
            parsed
                .ir
                .meta
                .theme_overrides
                .theme_variables
                .get("primaryColor")
                .map(String::as_str),
            Some("#fff")
        );
        assert_eq!(
            parsed.ir.meta.init.config.flowchart_direction,
            Some(GraphDirection::RL)
        );
        assert!(parsed.ir.meta.init.errors.is_empty());
    }

    #[test]
    fn init_directive_accepts_json5_style_payload() {
        let parsed = parse_mermaid(
            "%%{init: { theme: 'dark', themeVariables: { primaryColor: '#0ff' }, flowchart: { direction: 'RL' } }}%%\nflowchart LR\nA-->B",
        );
        assert_eq!(parsed.ir.meta.init.config.theme.as_deref(), Some("dark"));
        assert_eq!(
            parsed
                .ir
                .meta
                .theme_overrides
                .theme_variables
                .get("primaryColor")
                .map(String::as_str),
            Some("#0ff")
        );
        assert_eq!(
            parsed.ir.meta.init.config.flowchart_direction,
            Some(GraphDirection::RL)
        );
        assert!(parsed.ir.meta.init.errors.is_empty());
    }

    #[test]
    fn init_directive_maps_curve_and_sequence_options() {
        let parsed = parse_mermaid(
            "%%{init: {\"flowchart\":{\"curve\":\"basis\",\"rankDir\":\"LR\"},\"sequence\":{\"mirrorActors\":true,\"showSequenceNumbers\":true}}}%%\nflowchart TB\nA-->B",
        );
        assert_eq!(
            parsed.ir.meta.init.config.flowchart_curve.as_deref(),
            Some("basis")
        );
        assert_eq!(
            parsed.ir.meta.init.config.flowchart_direction,
            Some(GraphDirection::LR)
        );
        assert_eq!(
            parsed.ir.meta.init.config.sequence_mirror_actors,
            Some(true)
        );
        assert_eq!(
            parsed.ir.meta.init.config.sequence_show_sequence_numbers,
            Some(true)
        );
        assert!(parsed.ir.meta.init.errors.is_empty());
    }

    #[test]
    fn front_matter_yaml_config_is_applied_and_skipped_from_body() {
        let parsed = parse_mermaid(
            "---\nconfig:\n  theme: dark\n  themeVariables:\n    primaryColor: '#123456'\n  flowchart:\n    rankDir: RL\n    curve: linear\n  sequence:\n    mirrorActors: false\n---\nflowchart LR\nA-->B",
        );

        assert_eq!(parsed.ir.meta.init.config.theme.as_deref(), Some("dark"));
        assert_eq!(
            parsed
                .ir
                .meta
                .theme_overrides
                .theme_variables
                .get("primaryColor")
                .map(String::as_str),
            Some("#123456")
        );
        assert_eq!(
            parsed.ir.meta.init.config.flowchart_direction,
            Some(GraphDirection::RL)
        );
        assert_eq!(
            parsed.ir.meta.init.config.flowchart_curve.as_deref(),
            Some("linear")
        );
        assert_eq!(
            parsed.ir.meta.init.config.sequence_mirror_actors,
            Some(false)
        );
        assert_eq!(
            parsed.ir.meta.init.config.sequence_show_sequence_numbers,
            None
        );
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert!(parsed.ir.meta.init.errors.is_empty());
    }

    #[test]
    fn sequence_init_show_sequence_numbers_enables_autonumber() {
        let parsed = parse_mermaid(
            "%%{init: {\"sequence\":{\"showSequenceNumbers\":true}}}%%\nsequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi",
        );

        assert_eq!(
            parsed.ir.meta.init.config.sequence_show_sequence_numbers,
            Some(true)
        );
        assert!(
            parsed
                .ir
                .sequence_meta
                .as_ref()
                .is_some_and(|meta| meta.autonumber)
        );
    }

    #[test]
    fn sequence_front_matter_show_sequence_numbers_enables_autonumber() {
        let parsed = parse_mermaid(
            "---\nconfig:\n  sequence:\n    showSequenceNumbers: true\n---\nsequenceDiagram\n  Alice->>Bob: Hello",
        );

        assert_eq!(
            parsed.ir.meta.init.config.sequence_show_sequence_numbers,
            Some(true)
        );
        assert!(
            parsed
                .ir
                .sequence_meta
                .as_ref()
                .is_some_and(|meta| meta.autonumber)
        );
    }

    #[test]
    fn sequence_hide_footbox_sets_meta_flag() {
        let parsed = parse_mermaid("sequenceDiagram\n  hide footbox\n  Alice->>Bob: Hello");

        assert!(
            parsed
                .ir
                .sequence_meta
                .as_ref()
                .is_some_and(|meta| meta.hide_footbox)
        );
    }

    #[test]
    fn sequence_hide_footbox_is_case_insensitive() {
        let parsed = parse_mermaid("sequenceDiagram\n  HIDE FOOTBOX\n  Alice->>Bob: Hello");

        assert!(
            parsed
                .ir
                .sequence_meta
                .as_ref()
                .is_some_and(|meta| meta.hide_footbox)
        );
    }

    #[test]
    fn invalid_init_directive_records_parse_error() {
        let parsed = parse_mermaid("%%{init: {not_json}}%%\nflowchart LR\nA-->B");
        assert_eq!(parsed.ir.meta.init.errors.len(), 1);
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn content_heuristics_detects_flowchart_from_arrows() {
        // With improved detection, "A --> B" is recognized as a flowchart via content heuristics
        let parsed = parse_mermaid("A --> B");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        // Should have a warning about detection method
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn truly_unknown_input_falls_back_gracefully() {
        // Input with no recognizable patterns
        let parsed = parse_mermaid("some random text\nmore text");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart); // Falls back to flowchart
        // Should have warnings about detection and empty parse
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn gitgraph_detects_type() {
        assert_eq!(detect_type("gitGraph\ncommit"), DiagramType::GitGraph);
        assert_eq!(detect_type("gitGraph LR\ncommit"), DiagramType::GitGraph);
    }

    #[test]
    fn gitgraph_parses_simple_commits() {
        let parsed = parse_mermaid("gitGraph\ncommit\ncommit\ncommit");
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed.ir.nodes.len(), 3);
        // 2 edges linking the 3 commits
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn gitgraph_parses_commit_with_id_and_message() {
        let parsed = parse_mermaid(
            r#"gitGraph
commit id: "abc123" msg: "Initial commit"
commit id: "def456" msg: "Add feature""#,
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);

        // Check node IDs are as specified
        let node1 = &parsed.ir.nodes[0];
        assert_eq!(node1.id, "abc123");

        let node2 = &parsed.ir.nodes[1];
        assert_eq!(node2.id, "def456");
    }

    #[test]
    fn gitgraph_parses_commit_with_tag() {
        let parsed = parse_mermaid(
            r#"gitGraph
commit tag: "v1.0.0"
commit msg: "Fix bug" tag: "v1.0.1""#,
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed.ir.nodes.len(), 2);

        // Labels should include tags
        let label1 = parsed.ir.nodes[0]
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|l| l.text.as_str());
        assert_eq!(label1, Some("[v1.0.0]"));

        let label2 = parsed.ir.nodes[1]
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|l| l.text.as_str());
        assert_eq!(label2, Some("Fix bug [v1.0.1]"));
    }

    #[test]
    fn gitgraph_parses_branch_and_checkout() {
        let parsed = parse_mermaid(
            r"gitGraph
commit
branch develop
checkout develop
commit
checkout main
commit",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        // 3 commits total
        assert_eq!(parsed.ir.nodes.len(), 3);
        // First commit links to both the develop commit and main commit
        // develop branch commit links from first commit
        // main branch commit links from first commit
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn gitgraph_parses_merge() {
        let parsed = parse_mermaid(
            r"gitGraph
commit
branch develop
checkout develop
commit
checkout main
merge develop",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        // 3 nodes: initial commit, develop commit, merge commit
        assert_eq!(parsed.ir.nodes.len(), 3);
        // Edges: initial->develop, initial->merge, develop->merge
        assert_eq!(parsed.ir.edges.len(), 3);
    }

    #[test]
    fn gitgraph_merge_preserves_explicit_id_and_tag() {
        let parsed = parse_mermaid(
            r"gitGraph
commit id: root
branch develop
checkout develop
commit id: feature
checkout main
merge develop id: merge1 tag: release",
        );

        let merge_node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "merge1")
            .expect("merge node should keep explicit id");
        let merge_label = merge_node
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|label| label.text.as_str());
        assert_eq!(merge_label, Some("release"));
        assert!(
            parsed.ir.edges.iter().any(|edge| {
                edge.label
                    .and_then(|id| parsed.ir.labels.get(id.0))
                    .is_some_and(|label| label.text == "release")
            }),
            "merge source edge should keep tag label"
        );
    }

    #[test]
    fn gitgraph_parses_cherry_pick() {
        let parsed = parse_mermaid(
            r#"gitGraph
commit id: "abc"
branch feature
checkout feature
commit id: "feat1"
checkout main
cherry-pick id: "feat1""#,
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        // Nodes: abc, feat1, cherry-pick commit
        assert_eq!(parsed.ir.nodes.len(), 3);
        // Edges: abc->feat1 (branch), abc->cherry-pick (main)
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn gitgraph_accepts_switch_alias_and_unquoted_ids() {
        let parsed = parse_mermaid(
            r"gitGraph
commit id: root
branch feature
switch feature
commit id: feat1
switch main
cherry-pick id: feat1",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert!(
            parsed
                .warnings
                .iter()
                .all(|warning| !warning.contains("unsupported gitGraph syntax"))
        );
    }

    #[test]
    fn gitgraph_direction_lr() {
        let parsed = parse_mermaid("gitGraph LR\ncommit");
        assert_eq!(parsed.ir.direction, GraphDirection::LR);
    }

    #[test]
    fn gitgraph_warns_on_unsupported_syntax() {
        let parsed = parse_mermaid("gitGraph\ncommit\nunsupported command here");
        assert!(!parsed.warnings.is_empty());
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.contains("unsupported gitGraph syntax"))
        );
    }

    #[test]
    fn gitgraph_preserves_specific_merge_parse_warning() {
        let parsed = parse_mermaid("gitGraph\ncommit\nmerge");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("merge requires a branch name"))
        );
    }

    #[test]
    fn gitgraph_preserves_specific_cherry_pick_parse_warning() {
        let parsed = parse_mermaid("gitGraph\ncommit\ncherry-pick");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("cherry-pick requires id: parameter"))
        );
    }

    #[test]
    fn gitgraph_case_insensitive_header() {
        // All these should be recognized as gitGraph
        let parsed1 = parse_mermaid("GITGRAPH\ncommit");
        assert_eq!(parsed1.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed1.ir.nodes.len(), 1);

        let parsed2 = parse_mermaid("GitGraph\ncommit");
        assert_eq!(parsed2.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed2.ir.nodes.len(), 1);

        let parsed3 = parse_mermaid("GITGRAPH LR\ncommit");
        assert_eq!(parsed3.ir.diagram_type, DiagramType::GitGraph);
        assert_eq!(parsed3.ir.direction, GraphDirection::LR);
    }

    #[test]
    fn gitgraph_commit_word_boundary() {
        // "committed" should NOT be parsed as "commit" + "ted"
        let parsed = parse_mermaid("gitGraph\ncommitted something");
        // Should have a warning about unsupported syntax
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.contains("unsupported gitGraph syntax"))
        );
        // No nodes should be created from "committed"
        assert_eq!(parsed.ir.nodes.len(), 0);
    }

    #[test]
    fn sankey_parses_nodes_edges_and_flow_values() {
        let parsed = parse_mermaid("sankey-beta\nA, B, 3\nB, C, 2.5\n");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sankey);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );

        let edge_labels = parsed
            .ir
            .edges
            .iter()
            .filter_map(|edge| edge.label)
            .filter_map(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(edge_labels, vec!["3", "2.5"]);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "A"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "B"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "C"));
    }

    #[test]
    fn sankey_warns_on_non_numeric_flow_but_keeps_edge() {
        let parsed = parse_mermaid("sankey-beta\nA, B, many\n");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sankey);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("sankey flow value is not numeric"))
        );
    }

    #[test]
    fn architecture_parses_services_groups_junctions_and_edges() {
        let parsed = parse_mermaid(
            r"architecture-beta
group platform(cloud)[Platform]
service api(server)[API] in platform
junction fan_in in platform
service db(database)[DB] in platform
api:R --> L:fan_in
fan_in:B --> T:db",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::ArchitectureBeta);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert_eq!(parsed.ir.clusters.len(), 1);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 1);
        assert!(
            parsed.warnings.is_empty(),
            "unexpected warnings: {:?}",
            parsed.warnings
        );

        let api = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "api")
            .expect("api node");
        assert!(
            api.classes
                .iter()
                .any(|class_name| class_name == "architecture-service")
        );
        assert!(
            api.classes
                .iter()
                .any(|class_name| class_name == "architecture-icon-server")
        );
        assert_eq!(api.icon.as_deref(), Some("server"));

        let junction = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "fan_in")
            .expect("junction node");
        assert_eq!(junction.shape, NodeShape::Circle);

        let platform_cluster = &parsed.ir.clusters[0];
        let title = platform_cluster
            .title
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(title, Some("Platform"));
        assert_eq!(platform_cluster.members.len(), 3);
    }

    #[test]
    fn architecture_reverse_and_bidirectional_edges_map_cleanly() {
        let parsed = parse_mermaid(
            r"architecture-beta
service api[API]
service db[DB]
db <-- api
api <--> db",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::ArchitectureBeta);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
        assert_eq!(parsed.ir.edges[1].arrow, ArrowType::Line);

        let first_from = match parsed.ir.edges[0].from {
            fm_core::IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.as_str(),
            _ => panic!("expected architecture node endpoint"),
        };
        let first_to = match parsed.ir.edges[0].to {
            fm_core::IrEndpoint::Node(node_id) => parsed.ir.nodes[node_id.0].id.as_str(),
            _ => panic!("expected architecture node endpoint"),
        };
        assert_eq!(first_from, "api");
        assert_eq!(first_to, "db");
    }

    #[test]
    fn c4_parses_people_systems_relationships_and_boundaries() {
        let parsed = parse_mermaid(
            r#"C4Context
Person(customer, "Customer")
System_Boundary(bank, "Banking System") {
    System(core, "Core Banking")
}
Rel(customer, core, "Uses", "HTTPS")"#,
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Context);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.clusters.len(), 1);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 1);

        let customer = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "customer")
            .expect("customer node");
        assert!(
            customer
                .classes
                .iter()
                .any(|class_name| class_name == "c4-person")
        );

        let core = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "core")
            .expect("core node");
        assert!(
            core.classes
                .iter()
                .any(|class_name| class_name == "c4-system")
        );

        let cluster_title = parsed.ir.clusters[0]
            .title
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(cluster_title, Some("System_Boundary(bank, Banking System)"));

        let edge_label = parsed.ir.edges[0]
            .label
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(edge_label, Some("Uses [HTTPS]"));
    }

    #[test]
    fn c4_container_preserves_technology_and_description_in_node_meta() {
        let parsed = parse_mermaid(
            r#"C4Container
Container(api, "Payments API", "Rust", "Handles payment requests")"#,
        );

        let api = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "api")
            .expect("api node");
        let c4_meta = api.c4_meta.as_ref().expect("c4 metadata");
        assert_eq!(c4_meta.element_type, "Container");
        assert_eq!(c4_meta.technology.as_deref(), Some("Rust"));
        assert_eq!(
            c4_meta.description.as_deref(),
            Some("Handles payment requests")
        );
    }

    #[test]
    fn c4_layout_and_legend_directives_update_ir_meta() {
        let parsed = parse_mermaid(
            r#"C4Context
LAYOUT_LEFT_RIGHT()
SHOW_LEGEND()
Person(user, "User")"#,
        );

        assert_eq!(parsed.ir.direction, GraphDirection::LR);
        assert_eq!(parsed.ir.meta.direction, GraphDirection::LR);
        assert!(parsed.ir.meta.c4_show_legend);
    }

    #[test]
    fn c4_birel_and_back_rel_map_to_expected_edges() {
        let parsed = parse_mermaid(
            r#"C4Dynamic
Person(user, "User")
System(app, "App")
System(db, "Database")
BiRel(user, app, "Browses")
Rel_Back(db, app, "Responds")"#,
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Dynamic);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Line);

        let back_edge = &parsed.ir.edges[1];
        let from_index = match back_edge.from {
            fm_core::IrEndpoint::Node(node_id) => node_id.0,
            _ => panic!("expected node endpoint"),
        };
        let to_index = match back_edge.to {
            fm_core::IrEndpoint::Node(node_id) => node_id.0,
            _ => panic!("expected node endpoint"),
        };
        assert_eq!(parsed.ir.nodes[from_index].id, "app");
        assert_eq!(parsed.ir.nodes[to_index].id, "db");
    }

    // ── Kanban parser tests ────────────────────────────────────────────

    #[test]
    fn kanban_detection() {
        assert_eq!(
            detect_type("kanban\n  Todo\n    task1"),
            DiagramType::Kanban
        );
    }

    #[test]
    fn kanban_basic_columns_and_cards() {
        let input =
            "kanban\n  Todo\n    task1[Design]\n    task2[Implement]\n  Done\n    task3[Deploy]";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Kanban);
        // 3 cards as nodes.
        assert_eq!(parsed.ir.nodes.len(), 3, "Should have 3 card nodes");
        // 2 columns as clusters.
        assert!(
            parsed.ir.clusters.len() >= 2,
            "Should have at least 2 clusters"
        );
        // Cards should have kanban-card class.
        assert!(
            parsed.ir.nodes[0]
                .classes
                .contains(&"kanban-card".to_string()),
            "Cards should have kanban-card class"
        );
    }

    #[test]
    fn kanban_plain_text_cards() {
        let input = "kanban\n  Backlog\n    Design mockups\n    Write tests";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Kanban);
        assert_eq!(parsed.ir.nodes.len(), 2, "Should have 2 card nodes");
    }

    #[test]
    fn kanban_empty_board() {
        let input = "kanban";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Kanban);
        assert_eq!(parsed.ir.nodes.len(), 0);
        assert!(
            !parsed.warnings.is_empty(),
            "Should warn about empty kanban"
        );
    }

    #[test]
    fn kanban_bracket_id_syntax() {
        let input = "kanban\n  Todo\n    t1[First Task]\n    t2[Second Task]";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.nodes[0].id, "t1");
        assert_eq!(parsed.ir.nodes[1].id, "t2");
        // Labels should be the bracket contents.
        let label0 = parsed.ir.nodes[0]
            .label
            .and_then(|lid| parsed.ir.labels.get(lid.0))
            .map(|l| l.text.as_str());
        assert_eq!(label0, Some("First Task"));
    }

    // ── Sequence autonumber tests ──────────────────────────────────────

    #[test]
    fn sequence_autonumber_sets_meta() {
        let input = "sequenceDiagram\n  autonumber\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sequence);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert!(meta.autonumber, "autonumber should be true");
        assert_eq!(meta.autonumber_start, 1);
        assert_eq!(meta.autonumber_increment, 1);
    }

    #[test]
    fn sequence_autonumber_start_sets_meta() {
        let input = "sequenceDiagram\n  autonumber 10\n  Alice->>Bob: Hello";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert!(meta.autonumber);
        assert_eq!(meta.autonumber_start, 10);
        assert_eq!(meta.autonumber_increment, 1);
    }

    #[test]
    fn sequence_autonumber_start_and_increment_set_meta() {
        let input = "sequenceDiagram\n  autonumber 10 5\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert!(meta.autonumber);
        assert_eq!(meta.autonumber_start, 10);
        assert_eq!(meta.autonumber_increment, 5);
    }

    #[test]
    fn sequence_without_autonumber_has_no_meta() {
        let input = "sequenceDiagram\n  Alice->>Bob: Hello";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sequence);
        // Without autonumber, sequence_meta is None
        assert!(parsed.ir.sequence_meta.is_none());
    }

    #[test]
    fn sequence_autonumber_does_not_produce_warning() {
        let input = "sequenceDiagram\n  autonumber\n  Alice->>Bob: Hello";
        let parsed = parse_mermaid(input);
        // autonumber should not generate unsupported syntax warnings
        let autonumber_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| w.contains("autonumber"))
            .collect();
        assert!(
            autonumber_warnings.is_empty(),
            "autonumber should not produce warnings, got: {autonumber_warnings:?}"
        );
    }

    #[test]
    fn sequence_parameterized_autonumber_does_not_produce_warning() {
        let input = "sequenceDiagram\n  autonumber 10 5\n  Alice->>Bob: Hello";
        let parsed = parse_mermaid(input);
        let autonumber_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| w.contains("autonumber"))
            .collect();
        assert!(
            autonumber_warnings.is_empty(),
            "parameterized autonumber should not produce warnings, got: {autonumber_warnings:?}"
        );
    }

    // ── Sequence note tests ────────────────────────────────────────────

    #[test]
    fn sequence_note_left_of() {
        let input = "sequenceDiagram\n  participant Alice\n  Note left of Alice: Important";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.notes.len(), 1);
        assert_eq!(meta.notes[0].position, fm_core::NotePosition::LeftOf);
        assert_eq!(meta.notes[0].text, "Important");
        assert_eq!(meta.notes[0].participants.len(), 1);
    }

    #[test]
    fn sequence_note_right_of() {
        let input = "sequenceDiagram\n  participant Bob\n  Note right of Bob: Side note";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.notes.len(), 1);
        assert_eq!(meta.notes[0].position, fm_core::NotePosition::RightOf);
        assert_eq!(meta.notes[0].text, "Side note");
    }

    #[test]
    fn sequence_note_over_single() {
        let input = "sequenceDiagram\n  participant Alice\n  Note over Alice: Centered";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.notes.len(), 1);
        assert_eq!(meta.notes[0].position, fm_core::NotePosition::Over);
        assert_eq!(meta.notes[0].participants.len(), 1);
    }

    #[test]
    fn sequence_note_over_span() {
        let input =
            "sequenceDiagram\n  participant Alice\n  participant Bob\n  Note over Alice,Bob: Span";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.notes.len(), 1);
        assert_eq!(meta.notes[0].participants.len(), 2);
        assert_eq!(meta.notes[0].text, "Span");
    }

    #[test]
    fn sequence_note_multiline_br() {
        let input = "sequenceDiagram\n  participant Alice\n  Note over Alice: Line 1<br/>Line 2";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.notes[0].text, "Line 1\nLine 2");
    }

    #[test]
    fn sequence_message_label_decodes_mermaid_entities() {
        let input = "sequenceDiagram\n  Alice->>Bob: I #35; Rust #59; #9829; #infin;";
        let parsed = parse_mermaid(input);
        let edge = parsed.ir.edges.first().expect("message edge");
        let label_id = edge.label.expect("message label");
        let label = &parsed.ir.labels[label_id.0].text;
        assert_eq!(label, "I # Rust ; ♥ ∞");
    }

    #[test]
    fn sequence_message_label_normalizes_line_break_markup() {
        let input = "sequenceDiagram\n  Alice->>Bob: Line 1<br/>Line 2";
        let parsed = parse_mermaid(input);
        let edge = parsed.ir.edges.first().expect("message edge");
        let label_id = edge.label.expect("message label");
        let label = &parsed.ir.labels[label_id.0].text;
        assert_eq!(label, "Line 1\nLine 2");
    }

    #[test]
    fn sequence_note_decodes_mermaid_entities_and_line_breaks() {
        let input = "sequenceDiagram\n  participant Alice\n  Note over Alice: Line #35;<br/>Line #59; #infin;";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.notes[0].text, "Line #\nLine ; ∞");
    }

    #[test]
    fn sequence_participant_alias_normalizes_line_break_markup() {
        let input = "sequenceDiagram\n  participant Alice as Alice<br/>Admin";
        let parsed = parse_mermaid(input);
        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "Alice")
            .expect("participant node");
        let label_id = node.label.expect("participant label");
        let label = &parsed.ir.labels[label_id.0].text;
        assert_eq!(label, "Alice\nAdmin");
    }

    #[test]
    fn sequence_note_does_not_produce_warning() {
        let input = "sequenceDiagram\n  participant Alice\n  Note left of Alice: Test";
        let parsed = parse_mermaid(input);
        let note_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| w.to_lowercase().contains("note"))
            .collect();
        assert!(
            note_warnings.is_empty(),
            "Note should not produce warnings, got: {note_warnings:?}"
        );
    }

    // ── Sequence activation tests ──────────────────────────────────────

    #[test]
    fn sequence_activate_deactivate_standalone() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Request\n  activate Bob\n  Bob->>Alice: Response\n  deactivate Bob";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.activations.len(), 1);
        assert_eq!(meta.activations[0].depth, 0);
    }

    #[test]
    fn sequence_quoted_participant_references_resolve_metadata() {
        let input = "sequenceDiagram\n  box Services\n    participant \"Bob Service\"\n  end\n  Note over \"Bob Service\": Warm cache\n  activate \"Bob Service\"\n  \"Bob Service\"-->>\"Bob Service\": Pong\n  deactivate \"Bob Service\"\n  destroy \"Bob Service\"";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");

        assert_eq!(meta.participant_groups.len(), 1);
        assert_eq!(meta.participant_groups[0].participants.len(), 1);
        assert_eq!(meta.notes.len(), 1);
        assert_eq!(meta.notes[0].participants.len(), 1);
        assert_eq!(meta.activations.len(), 1);
        assert_eq!(
            meta.lifecycle_events
                .iter()
                .filter(|event| event.kind == fm_core::LifecycleEventKind::Destroy)
                .count(),
            1
        );
    }

    #[test]
    fn sequence_activate_plus_minus_syntax() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>+Bob: Request\n  Bob-->>-Alice: Response";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(
            meta.activations.len(),
            1,
            "Should have one activation from +/- syntax"
        );
    }

    #[test]
    fn sequence_nested_activations() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>+Bob: First\n  Alice->>+Bob: Second\n  Bob-->>-Alice: Reply2\n  Bob-->>-Alice: Reply1";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.activations.len(), 2, "Should have two activations");
    }

    #[test]
    fn sequence_activation_no_warnings() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  activate Bob\n  Bob->>Alice: Hi\n  deactivate Bob";
        let parsed = parse_mermaid(input);
        let act_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| w.contains("activate") || w.contains("deactivate"))
            .collect();
        assert!(
            act_warnings.is_empty(),
            "activate/deactivate should not produce warnings, got: {act_warnings:?}"
        );
    }

    // ── Sequence box grouping tests ────────────────────────────────────

    #[test]
    fn sequence_box_grouping() {
        let input = "sequenceDiagram\n  box Backend Team\n    participant Alice\n    participant Bob\n  end\n  Alice->>Bob: Hello";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.participant_groups.len(), 1);
        assert_eq!(meta.participant_groups[0].label, "Backend Team");
        assert_eq!(meta.participant_groups[0].participants.len(), 2);
    }

    #[test]
    fn sequence_box_with_color() {
        let input = "sequenceDiagram\n  box #aaf Backend\n    participant API\n    participant DB\n  end\n  API->>DB: Query";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.participant_groups.len(), 1);
        assert_eq!(meta.participant_groups[0].color, Some("#aaf".to_string()));
        assert_eq!(meta.participant_groups[0].label, "Backend");
    }

    #[test]
    fn sequence_box_with_named_color() {
        let input = "sequenceDiagram\n  box Aqua Backend Team\n    participant API\n    participant DB\n  end\n  API->>DB: Query";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.participant_groups.len(), 1);
        assert_eq!(meta.participant_groups[0].color, Some("Aqua".to_string()));
        assert_eq!(meta.participant_groups[0].label, "Backend Team");
    }

    #[test]
    fn sequence_box_transparent_keeps_color_name_as_label() {
        let input = "sequenceDiagram\n  box transparent Aqua\n    participant API\n  end\n  API->>API: Self";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.participant_groups.len(), 1);
        assert_eq!(
            meta.participant_groups[0].color,
            Some("transparent".to_string())
        );
        assert_eq!(meta.participant_groups[0].label, "Aqua");
    }

    #[test]
    fn sequence_box_no_warnings() {
        let input =
            "sequenceDiagram\n  box Team\n    participant Alice\n  end\n  Alice->>Alice: Self";
        let parsed = parse_mermaid(input);
        let box_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| w.to_lowercase().contains("box") || w.to_lowercase().contains("end"))
            .collect();
        assert!(
            box_warnings.is_empty(),
            "box/end should not produce warnings, got: {box_warnings:?}"
        );
    }

    // ── Sequence create/destroy tests ──────────────────────────────────

    #[test]
    fn sequence_create_participant() {
        let input = "sequenceDiagram\n  participant Alice\n  Alice->>Bob: Hello\n  create participant Carol\n  Bob->>Carol: Welcome";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(
            meta.lifecycle_events
                .iter()
                .filter(|e| e.kind == fm_core::LifecycleEventKind::Create)
                .count(),
            1,
            "Should have one create event"
        );
    }

    #[test]
    fn sequence_create_quoted_participant_records_lifecycle_event() {
        let input = "sequenceDiagram\n  participant Alice\n  create participant \"Carol Service\"\n  Alice->>\"Carol Service\": Welcome";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(
            meta.lifecycle_events
                .iter()
                .filter(|e| e.kind == fm_core::LifecycleEventKind::Create)
                .count(),
            1,
            "Should have one create event"
        );
    }

    #[test]
    fn sequence_participant_config_inline_alias_and_type() {
        let input = "sequenceDiagram\n  participant DB {\"type\":\"database\",\"alias\":\"Primary DB\"}\n  Alice->>DB: Query";
        let parsed = parse_mermaid(input);

        let db = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "DB")
            .expect("configured participant should be present");
        let label = db
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|label| label.text.as_str());

        assert_eq!(label, Some("Primary DB"));
        assert_eq!(db.shape, NodeShape::Cylinder);
        assert!(
            db.classes
                .iter()
                .any(|class_name| class_name == "sequence-participant-database")
        );
    }

    #[test]
    fn sequence_actor_config_external_alias_overrides_inline_alias() {
        let input = "sequenceDiagram\n  actor Service {\"type\":\"queue\",\"alias\":\"Internal Queue\"} as \"External Queue\"";
        let parsed = parse_mermaid(input);

        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "Service")
            .expect("configured actor should be present");
        let label = node
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|label| label.text.as_str());

        assert_eq!(label, Some("External Queue"));
        assert_eq!(node.shape, NodeShape::HorizontalBar);
        assert!(
            node.classes
                .iter()
                .any(|class_name| class_name == "sequence-actor")
        );
        assert!(
            node.classes
                .iter()
                .any(|class_name| class_name == "sequence-participant-queue")
        );
    }

    #[test]
    fn sequence_create_actor_config_records_lifecycle_event() {
        let input = "sequenceDiagram\n  participant Alice\n  create actor Worker {\"type\":\"control\",\"alias\":\"Worker Loop\"}\n  Alice->>Worker: Start";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        let worker = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "Worker")
            .expect("created actor should be present");
        let label = worker
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|label| label.text.as_str());

        assert_eq!(label, Some("Worker Loop"));
        assert_eq!(worker.shape, NodeShape::Circle);
        assert_eq!(meta.lifecycle_events.len(), 1);
        assert_eq!(meta.lifecycle_events[0].participant, fm_core::IrNodeId(1));
    }

    #[test]
    fn sequence_link_actor_menu_adds_single_menu_entry() {
        let input =
            "sequenceDiagram\n  participant API\n  link API: Health @ https://example.com/health";
        let parsed = parse_mermaid(input);
        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "API")
            .expect("participant should be present");

        assert_eq!(node.menu_links.len(), 1);
        assert_eq!(node.menu_links[0].label, "Health");
        assert_eq!(node.menu_links[0].url, "https://example.com/health");
    }

    #[test]
    fn sequence_links_actor_menu_adds_multiple_menu_entries() {
        let input = "sequenceDiagram\n  participant API\n  links API: {\"Docs\":\"https://example.com/docs\",\"Repo\":\"https://example.com/repo\"}";
        let parsed = parse_mermaid(input);
        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "API")
            .expect("participant should be present");

        assert_eq!(node.menu_links.len(), 2);
        assert!(
            node.menu_links
                .iter()
                .any(|entry| entry.label == "Docs" && entry.url == "https://example.com/docs")
        );
        assert!(
            node.menu_links
                .iter()
                .any(|entry| entry.label == "Repo" && entry.url == "https://example.com/repo")
        );
    }

    #[test]
    fn sequence_actor_menu_blocks_unsafe_links() {
        let input = "sequenceDiagram\n  participant API\n  link API: Admin @ javascript:alert(1)";
        let parsed = parse_mermaid(input);
        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "API")
            .expect("participant should be present");

        assert!(node.menu_links.is_empty());
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("unsafe sequence actor menu link blocked"))
        );
    }

    #[test]
    fn sequence_destroy_participant() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Bye\n  destroy Bob";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(
            meta.lifecycle_events
                .iter()
                .filter(|e| e.kind == fm_core::LifecycleEventKind::Destroy)
                .count(),
            1,
            "Should have one destroy event"
        );
    }

    // ── Sequence fragment tests ────────────────────────────────────────

    #[test]
    fn sequence_loop_fragment() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  loop Every minute\n    Alice->>Bob: Heartbeat\n  end";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.fragments.len(), 1);
        assert_eq!(meta.fragments[0].kind, fm_core::FragmentKind::Loop);
        assert_eq!(meta.fragments[0].label, "Every minute");
    }

    #[test]
    fn sequence_alt_else_fragment() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Bob\n  alt Success\n    Bob->>Alice: 200 OK\n  else Failure\n    Bob->>Alice: 500 Error\n  end";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.fragments.len(), 1);
        assert_eq!(meta.fragments[0].kind, fm_core::FragmentKind::Alt);
        assert_eq!(meta.fragments[0].label, "Success");
        assert_eq!(
            meta.fragments[0].alternatives.len(),
            1,
            "Should have one else alternative"
        );
        assert_eq!(meta.fragments[0].alternatives[0].label, "Failure");
    }

    #[test]
    fn sequence_opt_fragment() {
        let input = "sequenceDiagram\n  participant Alice\n  participant Carol\n  opt Optional\n    Alice->>Carol: Forward\n  end";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.fragments.len(), 1);
        assert_eq!(meta.fragments[0].kind, fm_core::FragmentKind::Opt);
    }

    #[test]
    fn sequence_nested_fragments() {
        let input = "sequenceDiagram\n  participant A\n  participant B\n  loop Outer\n    A->>B: M1\n    alt Inner\n      B->>A: R1\n    end\n  end";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(
            meta.fragments.len(),
            2,
            "Should have two fragments (inner + outer)"
        );
        // The inner (alt) is at index 0 (closed first), outer (loop) at index 1
        assert_eq!(meta.fragments[0].kind, fm_core::FragmentKind::Alt);
        assert_eq!(meta.fragments[1].kind, fm_core::FragmentKind::Loop);
        // The outer should reference the inner as a child
        assert_eq!(meta.fragments[1].children, vec![0]);
    }

    #[test]
    fn sequence_rect_fragment() {
        let input = "sequenceDiagram\n  participant A\n  participant B\n  rect rgb(200, 220, 240)\n    A->>B: Highlighted\n  end";
        let parsed = parse_mermaid(input);
        let meta = parsed
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert_eq!(meta.fragments.len(), 1);
        assert_eq!(meta.fragments[0].kind, fm_core::FragmentKind::Rect);
        assert_eq!(meta.fragments[0].label, "");
        assert_eq!(
            meta.fragments[0].color.as_deref(),
            Some("rgb(200, 220, 240)")
        );
    }

    #[test]
    fn sequence_fragment_no_warnings() {
        let input =
            "sequenceDiagram\n  participant A\n  participant B\n  loop Test\n    A->>B: Hi\n  end";
        let parsed = parse_mermaid(input);
        let frag_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| {
                w.to_lowercase().contains("loop") || w.to_lowercase().contains("unsupported")
            })
            .collect();
        assert!(
            frag_warnings.is_empty(),
            "Fragments should not produce warnings, got: {frag_warnings:?}"
        );
    }

    // ── Class diagram member parsing tests ─────────────────────────────

    #[test]
    fn class_block_with_members() {
        let input = "classDiagram\n  class Animal {\n    +String name\n    -int age\n    +eat() void\n    -sleep()\n  }";
        let parsed = parse_mermaid(input);
        let animal = parsed.ir.nodes.iter().find(|n| n.id == "Animal");
        assert!(animal.is_some(), "Should find Animal node");
        let meta = animal
            .unwrap()
            .class_meta
            .as_ref()
            .expect("Should have class_meta");
        assert_eq!(meta.attributes.len(), 2, "Should have 2 attributes");
        assert_eq!(meta.methods.len(), 2, "Should have 2 methods");
        assert_eq!(meta.attributes[0].name, "String name");
        assert_eq!(
            meta.attributes[0].visibility,
            fm_core::ClassVisibility::Public
        );
        assert_eq!(
            meta.attributes[1].visibility,
            fm_core::ClassVisibility::Private
        );
        assert_eq!(meta.methods[0].return_type, Some("void".to_string()));
    }

    #[test]
    fn class_member_visibility_markers() {
        let input = "classDiagram\n  class Foo {\n    +pub_attr\n    -priv_attr\n    #prot_attr\n    ~pkg_attr\n  }";
        let parsed = parse_mermaid(input);
        let foo = parsed.ir.nodes.iter().find(|n| n.id == "Foo").unwrap();
        let meta = foo.class_meta.as_ref().expect("class_meta");
        assert_eq!(
            meta.attributes[0].visibility,
            fm_core::ClassVisibility::Public
        );
        assert_eq!(
            meta.attributes[1].visibility,
            fm_core::ClassVisibility::Private
        );
        assert_eq!(
            meta.attributes[2].visibility,
            fm_core::ClassVisibility::Protected
        );
        assert_eq!(
            meta.attributes[3].visibility,
            fm_core::ClassVisibility::Package
        );
    }

    #[test]
    fn class_stereotype_annotation() {
        let input = "classDiagram\n  class Duck\n  <<interface>> Duck";
        let parsed = parse_mermaid(input);
        let duck = parsed.ir.nodes.iter().find(|n| n.id == "Duck").unwrap();
        let meta = duck.class_meta.as_ref().expect("class_meta");
        assert_eq!(meta.stereotype, Some(fm_core::ClassStereotype::Interface));
    }

    // ── State diagram tests ────────────────────────────────────────────

    #[test]
    fn state_pseudo_states_in_transitions() {
        let input = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> [*]";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::State);
        let start = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "__state_start")
            .expect("state start node");
        let end = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "__state_end")
            .expect("state end node");
        assert_eq!(start.shape, NodeShape::FilledCircle);
        assert_eq!(end.shape, NodeShape::DoubleCircle);
        assert_eq!(parsed.ir.edges.len(), 2);
    }

    #[test]
    fn state_composite_creates_cluster() {
        let input = "stateDiagram-v2\n  state Active {\n    Working --> Paused\n  }";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::State);
        assert!(
            !parsed.ir.clusters.is_empty(),
            "Should have at least one cluster for composite state"
        );
    }

    #[test]
    fn state_composite_alias_and_regions_preserve_hierarchy() {
        let input = "stateDiagram-v2\n  state \"Active Mode\" as Active {\n    [*] --> Processing\n    state Worker {\n      [*] --> Busy\n    }\n    --\n    [*] --> Monitoring\n  }";
        let parsed = parse_mermaid(input);

        let active_cluster = parsed.ir.clusters.first().expect("active cluster");
        let active_title = active_cluster
            .title
            .and_then(|label| parsed.ir.labels.get(label.0))
            .map(|label| label.text.as_str());
        assert_eq!(active_title, Some("Active Mode"));
        assert_eq!(active_cluster.grid_span, 2);

        let active_subgraph = parsed
            .ir
            .graph
            .first_subgraph_by_key("Active")
            .expect("active subgraph");
        assert!(active_subgraph.parent.is_none());
        assert_eq!(active_subgraph.grid_span, 2);

        let child_keys = active_subgraph
            .children
            .iter()
            .filter_map(|child_id| parsed.ir.graph.subgraph(*child_id))
            .map(|child| child.key.as_str())
            .collect::<Vec<_>>();
        assert!(child_keys.contains(&"Worker"));
        assert_eq!(
            child_keys
                .iter()
                .filter(|key| key.starts_with("__state_region_"))
                .count(),
            2
        );
    }

    #[test]
    fn state_transition_with_label() {
        let input = "stateDiagram-v2\n  Idle --> Active : start";
        let parsed = parse_mermaid(input);
        assert!(!parsed.ir.edges.is_empty());
    }

    #[test]
    fn state_note() {
        let input = "stateDiagram-v2\n  state Active\n  note right of Active : This is active";
        let parsed = parse_mermaid(input);
        // Notes are parsed without producing warnings
        let note_warnings: Vec<_> = parsed
            .warnings
            .iter()
            .filter(|w| w.to_lowercase().contains("note"))
            .collect();
        assert!(
            note_warnings.is_empty(),
            "State notes should not produce warnings, got: {note_warnings:?}"
        );
    }

    #[test]
    fn state_declares_fork_join_choice_and_history_shapes() {
        let input = "stateDiagram-v2\n  state fork_state <<fork>>\n  state join_state <<join>>\n  state chooser <<choice>>\n  state hist <<history>>\n  state deep_hist <<deepHistory>>";
        let parsed = parse_mermaid(input);

        let fork = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "fork_state")
            .unwrap();
        let join = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "join_state")
            .unwrap();
        let choice = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "chooser")
            .unwrap();
        let history = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "hist")
            .unwrap();
        let deep_history = parsed
            .ir
            .nodes
            .iter()
            .find(|node| node.id == "deep_hist")
            .unwrap();

        assert_eq!(fork.shape, NodeShape::HorizontalBar);
        assert_eq!(join.shape, NodeShape::HorizontalBar);
        assert_eq!(choice.shape, NodeShape::Diamond);
        assert_eq!(history.shape, NodeShape::Circle);
        assert_eq!(deep_history.shape, NodeShape::DoubleCircle);

        let history_label = history
            .label
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        let deep_history_label = deep_history
            .label
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(history_label, Some("H"));
        assert_eq!(deep_history_label, Some("H*"));
    }

    #[test]
    fn state_region_separator_outside_composite_emits_warning() {
        let parsed = parse_mermaid("stateDiagram-v2\n  --\n  Idle --> Active");
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("outside a composite state")),
            "expected warning for separator outside composite state, got {:?}",
            parsed.warnings
        );
    }

    // --- Detection tests per diagram type ---

    #[test]
    fn detect_sequence_keyword() {
        let parsed = parse_mermaid("sequenceDiagram\n  Alice->>Bob: Hello");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sequence);
    }

    #[test]
    fn detect_class_keyword() {
        let parsed = parse_mermaid("classDiagram\n  class Animal");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Class);
    }

    #[test]
    fn detect_state_keyword() {
        let parsed = parse_mermaid("stateDiagram-v2\n  [*] --> Active");
        assert_eq!(parsed.ir.diagram_type, DiagramType::State);
    }

    #[test]
    fn detect_er_keyword() {
        let parsed = parse_mermaid("erDiagram\n  CUSTOMER ||--o{ ORDER : places");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Er);
    }

    #[test]
    fn detect_gantt_keyword() {
        let parsed =
            parse_mermaid("gantt\n  title A Gantt\n  section A\n  Task1 :a1, 2024-01-01, 30d");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Gantt);
    }

    #[test]
    fn detect_pie_keyword() {
        let parsed = parse_mermaid("pie\n  \"A\" : 40\n  \"B\" : 60");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Pie);
    }

    #[test]
    fn detect_mindmap_keyword() {
        let parsed = parse_mermaid("mindmap\n  root\n    Child");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Mindmap);
    }

    #[test]
    fn detect_timeline_keyword() {
        let parsed = parse_mermaid("timeline\n  title History\n  2023 : Event");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Timeline);
    }

    #[test]
    fn detect_quadrant_keyword() {
        let parsed = parse_mermaid("quadrantChart\n  x-axis Low --> High");
        assert_eq!(parsed.ir.diagram_type, DiagramType::QuadrantChart);
    }

    #[test]
    fn detect_sankey_keyword() {
        let parsed = parse_mermaid("sankey-beta\n  A,B,10");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sankey);
    }

    #[test]
    fn detect_xychart_keyword() {
        let parsed = parse_mermaid("xychart-beta\n  x-axis [a, b, c]");
        assert_eq!(parsed.ir.diagram_type, DiagramType::XyChart);
    }

    #[test]
    fn detect_requirement_keyword() {
        let parsed = parse_mermaid("requirementDiagram\n  requirement test_req { }");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Requirement);
    }

    #[test]
    fn detect_kanban_keyword() {
        let parsed = parse_mermaid("kanban\n  Todo\n    task1");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Kanban);
    }

    // --- Gantt parser tests ---

    #[test]
    fn gantt_parses_sections() {
        let input = "gantt\n  title Project\n  dateFormat YYYY-MM-DD\n  section Alpha\n  Task1 :a1, 2024-01-01, 30d\n  section Beta\n  Task2 :a2, after a1, 15d";
        let parsed = parse_mermaid(input);
        let gantt_meta = parsed.ir.gantt_meta.as_ref().expect("gantt meta");
        assert_eq!(gantt_meta.date_format.as_deref(), Some("YYYY-MM-DD"));
        assert_eq!(gantt_meta.sections.len(), 2);
        assert_eq!(gantt_meta.sections[0].name, "Alpha");
        assert_eq!(gantt_meta.sections[1].name, "Beta");
        assert_eq!(gantt_meta.tasks[0].section_idx, 0);
        assert_eq!(gantt_meta.tasks[1].section_idx, 1);
    }

    #[test]
    fn gantt_parses_milestones() {
        let input =
            "gantt\n  title Plan\n  section S1\n  Milestone1 :milestone, m1, 2024-06-01, 0d";
        let parsed = parse_mermaid(input);
        let gantt_meta = parsed.ir.gantt_meta.as_ref().expect("gantt meta");
        assert_eq!(gantt_meta.tasks[0].task_type, GanttTaskType::Milestone);
        assert_eq!(gantt_meta.tasks[0].task_id.as_deref(), Some("m1"));
        assert_eq!(gantt_meta.tasks[0].end, Some(GanttDate::DurationDays(0)));
    }

    #[test]
    fn gantt_parses_done_and_active_tasks() {
        let input = "gantt\n  section S1\n  Done task :done, d1, 2024-01-01, 10d\n  Active task :active, a1, 2024-01-11, 10d\n  Future task :f1, after a1, 5d";
        let parsed = parse_mermaid(input);
        let gantt_meta = parsed.ir.gantt_meta.as_ref().expect("gantt meta");
        assert_eq!(gantt_meta.tasks[0].task_type, GanttTaskType::Done);
        assert_eq!(gantt_meta.tasks[1].task_type, GanttTaskType::Active);
        assert_eq!(gantt_meta.tasks[2].depends_on, vec!["a1".to_string()]);
        let done_node = &parsed.ir.nodes[gantt_meta.tasks[0].node.0];
        let active_node = &parsed.ir.nodes[gantt_meta.tasks[1].node.0];
        assert!(
            done_node
                .classes
                .iter()
                .any(|class_name| class_name == "gantt-done")
        );
        assert!(
            active_node
                .classes
                .iter()
                .any(|class_name| class_name == "gantt-active")
        );
    }

    #[test]
    fn gantt_parses_extended_directives_into_typed_meta() {
        let input = "gantt\n  dateFormat YYYY-MM-DD\n  axisFormat %Y-%m-%d\n  tickInterval 1month\n  todayMarker stroke-width:5px,stroke:#0f0,opacity:0.5\n  inclusiveEndDates\n  weekday monday\n  excludes weekends, 2026-02-14, 2026-02-15\n  section Alpha\n  Ship :crit, release, 2026-02-10, 2026-02-12";
        let parsed = parse_mermaid(input);
        let gantt_meta = parsed.ir.gantt_meta.as_ref().expect("gantt meta");

        assert_eq!(gantt_meta.tick_interval, Some(GanttTickInterval::Month));
        assert_eq!(
            gantt_meta.today_marker_style.as_deref(),
            Some("stroke-width:5px,stroke:#0f0,opacity:0.5")
        );
        assert!(gantt_meta.inclusive_end_dates);
        assert_eq!(gantt_meta.weekday_start, Some(1));
        assert_eq!(
            gantt_meta.excludes,
            vec![
                GanttExclude::Weekends,
                GanttExclude::Dates(vec!["2026-02-14".to_string(), "2026-02-15".to_string()])
            ]
        );
        assert_eq!(gantt_meta.tasks[0].task_type, GanttTaskType::Critical);
        assert_eq!(
            gantt_meta.tasks[0].end,
            Some(GanttDate::Absolute("2026-02-12".to_string()))
        );
    }

    #[test]
    fn gantt_date_format_normalizes_non_iso_dates() {
        let input = "gantt\n  dateFormat DD/MM/YYYY\n  excludes weekends, 14/02/2026\n  section Alpha\n  Build :build1, 06/02/2026, 09/02/2026\n  Verify :verify1, after build1, 2d";
        let parsed = parse_mermaid(input);
        let gantt_meta = parsed.ir.gantt_meta.as_ref().expect("gantt meta");

        assert_eq!(
            gantt_meta.tasks[0].start,
            Some(GanttDate::Absolute("2026-02-06".to_string()))
        );
        assert_eq!(
            gantt_meta.tasks[0].end,
            Some(GanttDate::Absolute("2026-02-09".to_string()))
        );
        assert_eq!(
            gantt_meta.excludes,
            vec![
                GanttExclude::Weekends,
                GanttExclude::Dates(vec!["2026-02-14".to_string()])
            ]
        );
        assert_eq!(gantt_meta.tasks[1].depends_on, vec!["build1".to_string()]);
    }

    #[test]
    fn gantt_date_format_rejects_impossible_calendar_dates() {
        assert_eq!(
            super::normalize_gantt_date_with_format("29/02/2024", "DD/MM/YYYY"),
            Some("2024-02-29".to_string())
        );
        assert_eq!(
            super::normalize_gantt_date_with_format("31/02/2026", "DD/MM/YYYY"),
            None
        );
        assert_eq!(
            super::normalize_gantt_date_with_format("2025-02-29", "YYYY-MM-DD"),
            None
        );
    }

    // --- Pie parser tests ---

    #[test]
    fn pie_parses_title() {
        let input = "pie title Favorite Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85\n  \"Rats\" : 15";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Pie);
        assert!(
            parsed.ir.nodes.len() >= 3,
            "Should have 3 slices, got {}",
            parsed.ir.nodes.len()
        );
    }

    #[test]
    fn pie_handles_showdata() {
        let input = "pie showData\n  \"A\" : 40\n  \"B\" : 60";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Pie);
        assert_eq!(parsed.ir.nodes.len(), 2);
    }

    #[test]
    fn pie_ignores_empty_lines() {
        let input = "pie\n\n  \"Only\" : 100\n\n";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.nodes.len(), 1);
    }

    // --- C4 parser tests ---

    #[test]
    fn c4_context_parses_external_system() {
        let input = "C4Context\n  Person(user, \"User\")\n  System_Ext(ext, \"External\")\n  Rel(user, ext, \"Uses\")";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Context);
        assert!(
            parsed.ir.nodes.len() >= 2,
            "Should have person + system, got {}",
            parsed.ir.nodes.len()
        );
        assert!(!parsed.ir.edges.is_empty(), "Should have relationship edge");
    }

    #[test]
    fn c4_container_diagram() {
        let input = "C4Container\n  Container(api, \"API\", \"Go\")\n  ContainerDb(db, \"Database\", \"PostgreSQL\")";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Container);
        assert_eq!(parsed.ir.nodes.len(), 2);
    }

    #[test]
    fn c4_component_diagram() {
        let input = "C4Component\n  Component(svc, \"Service\", \"Handles requests\")";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Component);
        assert_eq!(parsed.ir.nodes.len(), 1);
    }

    #[test]
    fn c4_dynamic_diagram() {
        let input = "C4Dynamic\n  Person(user, \"User\")\n  System(sys, \"System\")\n  Rel(user, sys, \"1. Request\")";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Dynamic);
    }

    #[test]
    fn c4_deployment_diagram() {
        let input = "C4Deployment\n  Deployment_Node(server, \"Server\")";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::C4Deployment);
    }

    // --- Error recovery tests ---

    #[test]
    fn empty_input_does_not_panic() {
        let parsed = parse_mermaid("");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Unknown);
    }

    #[test]
    fn whitespace_only_input_does_not_panic() {
        let parsed = parse_mermaid("   \n\n  \t  \n");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Unknown);
    }

    #[test]
    fn garbage_input_does_not_panic() {
        let parsed = parse_mermaid("!!!@#$%^&*()_+\nrandom garbage\n12345");
        // Should not panic; falls back to Unknown or Flowchart.
        assert!(
            parsed.ir.diagram_type == DiagramType::Unknown
                || parsed.ir.diagram_type == DiagramType::Flowchart
        );
    }

    #[test]
    fn dangling_edge_creates_placeholder() {
        let input = "flowchart LR\n  A --> B\n  C --> ";
        let parsed = parse_mermaid(input);
        // Should still parse A --> B successfully.
        assert!(!parsed.ir.nodes.is_empty());
    }

    #[test]
    fn misspelled_keyword_detected_via_fuzzy() {
        let input = "flwchart LR\n  A --> B";
        let parsed = parse_mermaid(input);
        // Fuzzy detection should catch "flwchart" as flowchart.
        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
    }

    #[test]
    fn mixed_diagram_types_uses_first_header() {
        let input = "sequenceDiagram\n  Alice->>Bob: Hi\nflowchart LR\n  A-->B";
        let parsed = parse_mermaid(input);
        // First header wins.
        assert_eq!(parsed.ir.diagram_type, DiagramType::Sequence);
    }

    #[test]
    fn extremely_long_node_id_does_not_panic() {
        let long_id = "A".repeat(10_000);
        let input = format!("flowchart LR\n  {long_id} --> B");
        let parsed = parse_mermaid(&input);
        assert!(!parsed.ir.nodes.is_empty());
    }

    use std::fmt::Write;

    #[test]
    fn deeply_nested_subgraphs_do_not_panic() {
        let mut input = String::from("flowchart TB\n");
        for i in 0..20 {
            writeln!(input, "{}subgraph sg{i}", "  ".repeat(i)).unwrap();
        }
        for i in (0..20).rev() {
            writeln!(input, "{}end", "  ".repeat(i)).unwrap();
        }
        let parsed = parse_mermaid(&input);
        // Should parse without panicking.
        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
    }

    // --- Flowchart arrow type tests ---

    #[test]
    fn flowchart_all_arrow_types() {
        let input =
            "flowchart LR\n  A --> B\n  C --- D\n  E -.-> F\n  G ==> H\n  I --o J\n  K --x L";
        let parsed = parse_mermaid(input);
        assert!(
            parsed.ir.edges.len() >= 6,
            "Should have 6 edges for 6 arrow types, got {}",
            parsed.ir.edges.len()
        );
    }

    #[test]
    fn flowchart_thick_arrow_with_label() {
        let input = "flowchart LR\n  A ==>|label| B";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.edges.len(), 1);
    }

    #[test]
    fn flowchart_dotted_arrow_with_label() {
        let input = "flowchart LR\n  A -.->|label| B";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.edges.len(), 1);
    }

    // --- Flowchart node shape tests ---

    #[test]
    fn flowchart_node_shapes_basic() {
        let input = "flowchart TB\n  A[Rectangle]\n  B(Rounded)\n  C{Diamond}\n  D([Stadium])\n  E[[Subroutine]]\n  F[(Database)]\n  G((Circle))\n  H>Asymmetric]";
        let parsed = parse_mermaid(input);
        assert!(
            parsed.ir.nodes.len() >= 8,
            "Should have 8 shaped nodes, got {}",
            parsed.ir.nodes.len()
        );
    }

    #[test]
    fn flowchart_hexagon_and_parallelogram_shapes() {
        let input = "flowchart TB\n  A{{Hexagon}}\n  B[/Parallelogram/]\n  C[\\Parallelogram\\]";
        let parsed = parse_mermaid(input);
        assert!(
            parsed.ir.nodes.len() >= 3,
            "Should have 3 shaped nodes, got {}",
            parsed.ir.nodes.len()
        );
    }

    // --- XY chart tests ---

    #[test]
    fn xychart_with_bar_series() {
        let input = "xychart-beta\n  x-axis [Jan, Feb, Mar]\n  y-axis \"Sales\" 0 --> 100\n  bar [10, 30, 50]";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::XyChart);
        let xy_meta = parsed.ir.xy_chart_meta.as_ref().expect("xy chart meta");
        assert_eq!(xy_meta.y_axis.label.as_deref(), Some("Sales"));
        assert_eq!(xy_meta.y_axis.min, Some(0.0));
        assert_eq!(xy_meta.y_axis.max, Some(100.0));
        assert_eq!(xy_meta.series[0].values, vec![10.0, 30.0, 50.0]);
    }

    #[test]
    fn xychart_with_line_series() {
        let input = "xychart-beta\n  x-axis [A, B, C]\n  line [5, 15, 25]";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::XyChart);
        let xy_meta = parsed.ir.xy_chart_meta.as_ref().expect("xy chart meta");
        assert_eq!(xy_meta.x_axis.categories, vec!["A", "B", "C"]);
        assert_eq!(xy_meta.series[0].kind, IrXySeriesKind::Line);
        assert_eq!(xy_meta.series[0].values, vec![5.0, 15.0, 25.0]);
    }

    // --- Architecture tests ---

    #[test]
    fn architecture_parses_junction_nodes() {
        let input =
            "architecture-beta\n  service api(server)[API]\n  junction junc\n  api:R -- junc:L";
        let parsed = parse_mermaid(input);
        assert!(parsed.ir.nodes.len() >= 2, "Should have service + junction");
    }

    // --- Quadrant chart tests ---

    #[test]
    fn quadrant_parses_axis_labels() {
        let input = "quadrantChart\n  x-axis Low Reach --> High Reach\n  y-axis Low Engagement --> High Engagement\n  quadrant-1 We should expand\n  Campaign A: [0.3, 0.6]";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::QuadrantChart);
    }

    // --- Requirement diagram tests ---

    #[test]
    fn requirement_parses_requirement_block() {
        let input = "requirementDiagram\n  requirement test_req {\n    id: 1\n    text: The system shall do X\n    risk: high\n    verifymethod: test\n  }";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::Requirement);
        assert!(!parsed.ir.nodes.is_empty());
    }

    #[test]
    fn requirement_parses_element_and_relationship() {
        let input = "requirementDiagram\n  requirement test_req {\n    id: 1\n    text: test\n  }\n  element test_entity {\n    type: simulation\n  }\n  test_entity - satisfies -> test_req";
        let parsed = parse_mermaid(input);
        assert!(!parsed.ir.edges.is_empty(), "Should have relationship edge");
    }

    // --- Block-beta tests ---

    #[test]
    fn block_beta_columns_directive() {
        let input = "block-beta\n  columns 3\n  a b c\n  d e f";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::BlockBeta);
        assert!(
            parsed.ir.nodes.len() >= 6,
            "Should have 6 blocks, got {}",
            parsed.ir.nodes.len()
        );
    }

    // --- Packet-beta tests ---

    #[test]
    fn packet_parses_bit_fields() {
        let input = "packet-beta\n  0-3: \"Version\"\n  4-7: \"IHL\"\n  8-15: \"Type of Service\"";
        let parsed = parse_mermaid(input);
        assert_eq!(parsed.ir.diagram_type, DiagramType::PacketBeta);
    }

    // --- DOT format detection ---

    #[test]
    fn dot_digraph_detected() {
        let input = "digraph G {\n  a -> b;\n  b -> c;\n}";
        let parsed = parse_mermaid(input);
        // DOT graphs should be detected and parsed.
        assert!(!parsed.ir.nodes.is_empty());
    }

    #[test]
    fn dot_undirected_graph_detected() {
        let input = "graph G {\n  a -- b;\n}";
        let parsed = parse_mermaid(input);
        assert!(!parsed.ir.nodes.is_empty());
    }

    // --- Determinism tests ---

    #[test]
    fn parser_output_is_deterministic() {
        let input =
            "flowchart LR\n  A --> B\n  B --> C\n  A --> C\n  subgraph sg1\n    D --> E\n  end";
        let r1 = parse_mermaid(input);
        let r2 = parse_mermaid(input);
        assert_eq!(r1.ir.nodes.len(), r2.ir.nodes.len());
        assert_eq!(r1.ir.edges.len(), r2.ir.edges.len());
        for (n1, n2) in r1.ir.nodes.iter().zip(r2.ir.nodes.iter()) {
            assert_eq!(n1.id, n2.id, "Node IDs should match");
        }
    }

    // ── Pie chart parser tests ─────────────────────────────────────────

    #[test]
    fn pie_parser_extracts_slice_values() {
        let parsed = parse_mermaid("pie\n  \"Dogs\" : 386\n  \"Cats\" : 85\n  \"Rats\" : 15");
        let pie = parsed.ir.pie_meta.expect("pie_meta should be set");
        assert_eq!(pie.slices.len(), 3);
        assert_eq!(pie.slices[0].label, "Dogs");
        assert!((pie.slices[0].value - 386.0).abs() < f32::EPSILON);
        assert_eq!(pie.slices[1].label, "Cats");
        assert!((pie.slices[1].value - 85.0).abs() < f32::EPSILON);
        assert_eq!(pie.slices[2].label, "Rats");
        assert!((pie.slices[2].value - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pie_parser_extracts_title() {
        let parsed = parse_mermaid("pie title Pets\n  \"Dogs\" : 50\n  \"Cats\" : 50");
        let pie = parsed.ir.pie_meta.expect("pie_meta should be set");
        assert_eq!(pie.title.as_deref(), Some("Pets"));
    }

    #[test]
    fn pie_parser_extracts_show_data() {
        let parsed = parse_mermaid("pie showData\n  \"A\" : 10");
        let pie = parsed.ir.pie_meta.expect("pie_meta should be set");
        assert!(pie.show_data);
    }

    #[test]
    fn pie_parser_warns_on_invalid_value() {
        let parsed = parse_mermaid("pie\n  \"Bad\" : xyz");
        assert!(
            parsed.warnings.iter().any(|w| w.contains("invalid")),
            "should warn about invalid value: {:?}",
            parsed.warnings
        );
    }

    // ── Quadrant chart parser tests ────────────────────────────────────

    #[test]
    fn quadrant_parser_extracts_axes_and_points() {
        let input = "quadrantChart\n  x-axis Low --> High\n  y-axis Weak --> Strong\n  quadrant-1 Go\n  quadrant-2 Wait\n  A: [0.3, 0.7]\n  B: [0.8, 0.2]";
        let parsed = parse_mermaid(input);
        let quad = parsed
            .ir
            .quadrant_meta
            .expect("quadrant_meta should be set");
        assert_eq!(quad.x_axis_left.as_deref(), Some("Low"));
        assert_eq!(quad.x_axis_right.as_deref(), Some("High"));
        assert_eq!(quad.y_axis_bottom.as_deref(), Some("Weak"));
        assert_eq!(quad.y_axis_top.as_deref(), Some("Strong"));
        assert_eq!(quad.quadrant_labels.len(), 2);
        assert_eq!(quad.quadrant_labels[0], "Go");
        assert_eq!(quad.points.len(), 2);
        assert!((quad.points[0].x - 0.3).abs() < 0.01);
        assert!((quad.points[0].y - 0.7).abs() < 0.01);
    }

    // ── Accessibility directive tests ──────────────────────────────────

    #[test]
    fn acc_title_parsed_from_any_diagram() {
        let parsed = parse_mermaid("flowchart LR\n  accTitle: My Title\n  A --> B");
        assert_eq!(parsed.ir.meta.acc_title.as_deref(), Some("My Title"));
    }

    #[test]
    fn acc_descr_single_line() {
        let parsed = parse_mermaid("flowchart LR\n  accDescr: A brief description\n  A --> B");
        assert_eq!(
            parsed.ir.meta.acc_descr.as_deref(),
            Some("A brief description")
        );
    }

    #[test]
    fn acc_descr_multi_line_block() {
        let parsed =
            parse_mermaid("flowchart LR\n  accDescr {\n    Line 1\n    Line 2\n  }\n  A --> B");
        let descr = parsed.ir.meta.acc_descr.expect("acc_descr should be set");
        assert!(descr.contains("Line 1"));
        assert!(descr.contains("Line 2"));
    }

    #[test]
    fn acc_descr_unclosed_block_still_captured() {
        let parsed = parse_mermaid("flowchart LR\n  accDescr {\n    Unclosed content");
        assert!(
            parsed.ir.meta.acc_descr.is_some(),
            "unclosed accDescr block should still be captured"
        );
    }

    #[test]
    fn auto_close_delimiters_toggle_controls_unclosed_shape_recovery() {
        let baseline = parse_mermaid("flowchart LR\nA[Open");
        assert_eq!(baseline.ir.nodes.len(), 1);

        let strict_delimiters = parse_with_mode_and_config(
            "flowchart LR\nA[Open",
            MermaidParseMode::Compat,
            &ParserConfig {
                auto_close_delimiters: false,
                ..ParserConfig::default()
            },
        );
        assert_eq!(strict_delimiters.ir.nodes.len(), 0);
    }

    #[test]
    fn create_placeholder_nodes_toggle_controls_dangling_edge_recovery() {
        let baseline = parse_mermaid("flowchart LR\nA -->");
        assert_eq!(baseline.ir.edges.len(), 1);
        assert!(baseline.ir.nodes.iter().any(|node| node.implicit));

        let no_placeholders = parse_with_mode_and_config(
            "flowchart LR\nA -->",
            MermaidParseMode::Compat,
            &ParserConfig {
                create_placeholder_nodes: false,
                ..ParserConfig::default()
            },
        );
        assert_eq!(no_placeholders.ir.edges.len(), 0);
        assert!(!no_placeholders.ir.nodes.iter().any(|node| node.implicit));
    }

    #[test]
    fn auto_close_delimiters_toggle_controls_unclosed_acc_descr_blocks() {
        let baseline = parse_mermaid("flowchart LR\n  accDescr {\n    Unclosed content");
        assert_eq!(
            baseline.ir.meta.acc_descr.as_deref(),
            Some("Unclosed content")
        );

        let strict_delimiters = parse_with_mode_and_config(
            "flowchart LR\n  accDescr {\n    Unclosed content",
            MermaidParseMode::Compat,
            &ParserConfig {
                auto_close_delimiters: false,
                ..ParserConfig::default()
            },
        );
        assert!(strict_delimiters.ir.meta.acc_descr.is_none());
    }

    #[test]
    fn front_matter_title_is_promoted_to_diagram_meta() {
        let parsed = parse_mermaid("---\ntitle: Front Matter Title\n---\nflowchart LR\nA --> B");
        assert_eq!(parsed.ir.meta.title.as_deref(), Some("Front Matter Title"));
    }

    #[test]
    fn timeline_inline_title_is_promoted_to_diagram_meta() {
        let parsed = parse_mermaid("timeline\n  title Shipping History\n  2024 : Launch");
        assert_eq!(parsed.ir.meta.title.as_deref(), Some("Shipping History"));
    }

    #[test]
    fn gantt_inline_title_overrides_front_matter_title() {
        let parsed = parse_mermaid(
            "---\ntitle: Front Matter\n---\ngantt\n  title Inline Gantt\n  section Alpha\n  Task :a1, 2024-01-01, 1d",
        );
        assert_eq!(parsed.ir.meta.title.as_deref(), Some("Inline Gantt"));
        assert_eq!(
            parsed
                .ir
                .gantt_meta
                .as_ref()
                .and_then(|meta| meta.title.as_deref()),
            Some("Inline Gantt")
        );
    }

    // ── Click tooltip tests ────────────────────────────────────────────

    #[test]
    fn click_directive_extracts_tooltip() {
        let parsed = parse_mermaid(
            "flowchart LR\n  A[Node]\n  click A \"https://example.com\" \"My Tooltip\"",
        );
        let node = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(node.href.as_deref(), Some("https://example.com"));
        assert_eq!(node.tooltip.as_deref(), Some("My Tooltip"));
    }

    // ── linkStyle default tests ────────────────────────────────────────

    #[test]
    fn linkstyle_default_creates_link_default_style_ref() {
        let parsed = parse_mermaid(
            "flowchart LR\n  A --> B\n  linkStyle default stroke:#f00,stroke-width:2",
        );
        let has_default = parsed
            .ir
            .style_refs
            .iter()
            .any(|sr| sr.target == fm_core::IrStyleTarget::LinkDefault);
        assert!(has_default, "should have LinkDefault style ref");
    }

    // ── Requirement metadata tests ─────────────────────────────────────

    #[test]
    fn requirement_parser_extracts_metadata() {
        let input = "requirementDiagram\n  requirement MyReq {\n    id: REQ-001\n    text: Must do X\n    risk: High\n    verifymethod: Test\n  }";
        let parsed = parse_mermaid(input);
        let req_node = parsed.ir.nodes.iter().find(|n| n.id == "MyReq");
        assert!(req_node.is_some(), "should find MyReq node");
        let meta = req_node
            .unwrap()
            .requirement_meta
            .as_ref()
            .expect("requirement_meta should be set");
        assert_eq!(meta.req_id.as_deref(), Some("REQ-001"));
        assert_eq!(meta.text.as_deref(), Some("Must do X"));
        assert_eq!(meta.risk.as_deref(), Some("High"));
        assert_eq!(meta.verify_method.as_deref(), Some("Test"));
    }

    // ── ER notation storage tests ──────────────────────────────────────

    #[test]
    fn er_edge_stores_notation() {
        let parsed = parse_mermaid("erDiagram\n  USER ||--o{ ORDER : places");
        assert!(!parsed.ir.edges.is_empty(), "should have edges");
        let edge = &parsed.ir.edges[0];
        assert!(
            edge.er_notation.is_some(),
            "er_notation should be set on ER edges"
        );
    }

    // ── classDef parsing and structured style tests ───────────────────

    #[test]
    fn classdef_populates_style_defs() {
        let parsed = parse_mermaid(
            "flowchart LR\n  A[Node]\n  classDef important fill:#f9f,stroke:#333,stroke-width:4px",
        );
        assert!(
            !parsed.ir.style_defs.is_empty(),
            "style_defs should be populated"
        );
        let def = parsed
            .ir
            .style_defs
            .iter()
            .find(|d| d.name == "important")
            .expect("should find 'important' style def");
        assert_eq!(def.properties.get("fill").unwrap(), "#f9f");
        assert_eq!(def.properties.get("stroke").unwrap(), "#333");
        assert_eq!(def.properties.get("stroke-width").unwrap(), "4px");
    }

    #[test]
    fn classdef_applies_to_node_via_class_directive() {
        let parsed =
            parse_mermaid("flowchart LR\n  A[Node]\n  classDef cls fill:#abc\n  class A cls");
        let node = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        let style = node
            .inline_style
            .as_ref()
            .expect("node should have inline_style");
        assert_eq!(style.properties.get("fill").unwrap(), "#abc");
    }

    #[test]
    fn style_directive_applies_inline_style_to_node() {
        let parsed =
            parse_mermaid("flowchart LR\n  A[Node] --> B[Other]\n  style A fill:#f00,stroke:#0f0");
        let node = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        let style = node
            .inline_style
            .as_ref()
            .expect("node A should have inline_style");
        assert_eq!(style.properties.get("fill").unwrap(), "#f00");
        assert_eq!(style.properties.get("stroke").unwrap(), "#0f0");
    }

    #[test]
    fn style_directive_supports_comma_separated_node_ids() {
        let parsed = parse_mermaid("flowchart LR\n  A[One] --> B[Two]\n  style A,B fill:#abc");
        let node_a = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        let node_b = parsed.ir.nodes.iter().find(|n| n.id == "B").unwrap();
        assert!(
            node_a.inline_style.is_some(),
            "node A should have inline_style"
        );
        assert!(
            node_b.inline_style.is_some(),
            "node B should have inline_style"
        );
        assert_eq!(
            node_a
                .inline_style
                .as_ref()
                .unwrap()
                .properties
                .get("fill")
                .unwrap(),
            "#abc"
        );
        assert_eq!(
            node_b
                .inline_style
                .as_ref()
                .unwrap()
                .properties
                .get("fill")
                .unwrap(),
            "#abc"
        );
    }

    #[test]
    fn linkstyle_populates_edge_inline_style() {
        let parsed =
            parse_mermaid("flowchart LR\n  A --> B\n  linkStyle 0 stroke:#f00,stroke-width:3px");
        let edge = &parsed.ir.edges[0];
        let style = edge
            .inline_style
            .as_ref()
            .expect("edge should have inline_style");
        assert_eq!(style.properties.get("stroke").unwrap(), "#f00");
        assert_eq!(style.properties.get("stroke-width").unwrap(), "3px");
    }

    #[test]
    fn linkstyle_default_populates_all_edges() {
        let parsed =
            parse_mermaid("flowchart LR\n  A --> B\n  B --> C\n  linkStyle default stroke:#aaa");
        for edge in &parsed.ir.edges {
            let style = edge
                .inline_style
                .as_ref()
                .expect("all edges should have inline_style from default");
            assert_eq!(style.properties.get("stroke").unwrap(), "#aaa");
        }
    }

    #[test]
    fn classdef_and_style_cascade_correctly() {
        let parsed = parse_mermaid(
            "flowchart LR\n  A[Node]\n  classDef cls fill:#fff,stroke:#000\n  class A cls\n  style A fill:#f00",
        );
        let node = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        let style = node
            .inline_style
            .as_ref()
            .expect("node should have inline_style");
        // style directive overrides classDef for fill
        assert_eq!(style.properties.get("fill").unwrap(), "#f00");
        // classDef stroke preserved since style didn't override it
        assert_eq!(style.properties.get("stroke").unwrap(), "#000");
    }

    #[test]
    fn classdef_with_no_nodes_still_populates_style_defs() {
        let parsed = parse_mermaid("flowchart LR\n  A --> B\n  classDef unused fill:#999");
        assert_eq!(parsed.ir.style_defs.len(), 1);
        assert_eq!(parsed.ir.style_defs[0].name, "unused");
        // No nodes have this class, so no inline_style applied
        assert!(parsed.ir.nodes[0].inline_style.is_none());
    }

    #[test]
    fn classdef_emits_compatibility_warning() {
        let parsed =
            parse_mermaid("flowchart LR\n  A[Node]\n  classDef important fill:#f9f,stroke:#333");
        let diagnostic = parsed
            .ir
            .diagnostics
            .iter()
            .find(|diag| diag.message == STYLE_DIRECTIVE_DIAGNOSTIC_MESSAGE)
            .expect("expected classDef/style diagnostic warning");
        assert_eq!(diagnostic.category, DiagnosticCategory::Compatibility);
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert_eq!(
            diagnostic.rule_id.as_deref(),
            Some("style-directive-not-applied"),
            "expected rule_id for machine-readable diagnostics"
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning == STYLE_DIRECTIVE_DIAGNOSTIC_MESSAGE),
            "expected parse warning to surface in render logs"
        );
    }

    #[test]
    fn classdef_emits_warning_for_unsupported_properties() {
        let parsed = parse_mermaid(
            "flowchart LR\n  A[Node]\n  classDef bad fill:#fff,shadow:2px,color:javascript:alert(1)",
        );
        let diagnostic = parsed
            .ir
            .diagnostics
            .iter()
            .find(|diag| diag.rule_id.as_deref() == Some("classdef-unsupported-style"))
            .expect("expected unsupported classDef diagnostic");
        assert_eq!(diagnostic.category, DiagnosticCategory::Compatibility);
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert!(
            diagnostic.message.contains("shadow"),
            "expected unsupported property name in diagnostic message"
        );
    }

    // ── Class diagram cardinality and namespace tests ─────────────────

    #[test]
    fn class_cardinality_labels_parsed() {
        let parsed = parse_mermaid("classDiagram\n  Dog \"1\" --> \"*\" Cat : chases");
        assert!(!parsed.ir.edges.is_empty(), "should have edges");
        let edge = &parsed.ir.edges[0];
        assert_eq!(edge.source_cardinality.as_deref(), Some("1"));
        assert_eq!(edge.target_cardinality.as_deref(), Some("*"));
    }

    #[test]
    fn class_cardinality_complex_labels() {
        let parsed = parse_mermaid("classDiagram\n  Vehicle \"1\" *-- \"0..*\" Wheel");
        assert!(!parsed.ir.edges.is_empty(), "should have edges");
        let edge = &parsed.ir.edges[0];
        assert_eq!(edge.source_cardinality.as_deref(), Some("1"));
        assert_eq!(edge.target_cardinality.as_deref(), Some("0..*"));
    }

    #[test]
    fn class_no_cardinality_preserves_none() {
        let parsed = parse_mermaid("classDiagram\n  Dog --> Cat");
        assert!(!parsed.ir.edges.is_empty());
        let edge = &parsed.ir.edges[0];
        assert!(edge.source_cardinality.is_none());
        assert!(edge.target_cardinality.is_none());
    }

    #[test]
    fn class_cardinality_with_label() {
        let parsed = parse_mermaid("classDiagram\n  Student \"1\" --> \"*\" Course : enrolls");
        assert!(!parsed.ir.edges.is_empty(), "should have an edge");
        let edge = &parsed.ir.edges[0];
        assert_eq!(edge.source_cardinality.as_deref(), Some("1"));
        assert_eq!(edge.target_cardinality.as_deref(), Some("*"));
    }

    #[test]
    fn class_namespace_creates_subgraph() {
        let parsed =
            parse_mermaid("classDiagram\n  namespace Animals {\n    class Dog\n    class Cat\n  }");
        assert!(
            !parsed.ir.graph.subgraphs.is_empty(),
            "should have subgraphs from namespace"
        );
        let sg = &parsed.ir.graph.subgraphs[0];
        assert_eq!(sg.key, "Animals");
        assert!(
            !sg.members.is_empty(),
            "namespace subgraph should have members"
        );
    }

    #[test]
    fn class_namespace_clusters_nodes() {
        let parsed = parse_mermaid(
            "classDiagram\n  namespace Animals {\n    class Dog\n    class Cat\n  }\n  Dog --> Cat",
        );
        assert!(!parsed.ir.clusters.is_empty(), "should have clusters");
        let cluster = &parsed.ir.clusters[0];
        assert!(cluster.title.is_some());
    }

    // ── State diagram notes, guards, and actions ─────────────────────

    #[test]
    fn state_single_line_note_stored() {
        let parsed = parse_mermaid(
            "stateDiagram-v2\n  state Active\n  note right of Active: This is active",
        );
        assert_eq!(parsed.ir.state_notes.len(), 1);
        assert_eq!(parsed.ir.state_notes[0].target, "Active");
        assert_eq!(parsed.ir.state_notes[0].position, "right");
        assert_eq!(parsed.ir.state_notes[0].text, "This is active");
    }

    #[test]
    fn state_multi_line_note_stored() {
        let parsed = parse_mermaid(
            "stateDiagram-v2\n  state Active\n  note right of Active\n    Line one\n    Line two\n  end note",
        );
        assert_eq!(parsed.ir.state_notes.len(), 1);
        assert_eq!(parsed.ir.state_notes[0].target, "Active");
        assert!(parsed.ir.state_notes[0].text.contains("Line one"));
        assert!(parsed.ir.state_notes[0].text.contains("Line two"));
    }

    #[test]
    fn state_transition_without_guard() {
        let parsed = parse_mermaid("stateDiagram-v2\n  Active --> Done : complete");
        assert!(!parsed.ir.edges.is_empty());
        let edge = &parsed.ir.edges[0];
        assert!(edge.guard.is_none());
        assert!(edge.action.is_none());
    }

    #[test]
    fn extract_guard_action_both() {
        let (label, guard, action) =
            super::extract_state_guard_action(Some("complete [isValid] / cleanup()"));
        assert_eq!(label.as_deref(), Some("complete"));
        assert_eq!(guard.as_deref(), Some("isValid"));
        assert_eq!(action.as_deref(), Some("cleanup()"));
    }

    #[test]
    fn extract_guard_only() {
        let (label, guard, action) = super::extract_state_guard_action(Some("complete [isValid]"));
        assert_eq!(label.as_deref(), Some("complete"));
        assert_eq!(guard.as_deref(), Some("isValid"));
        assert!(action.is_none());
    }

    #[test]
    fn extract_action_only() {
        let (label, guard, action) =
            super::extract_state_guard_action(Some("complete / cleanup()"));
        assert_eq!(label.as_deref(), Some("complete"));
        assert!(guard.is_none());
        assert_eq!(action.as_deref(), Some("cleanup()"));
    }

    #[test]
    fn extract_no_guard_no_action() {
        let (label, guard, action) = super::extract_state_guard_action(Some("complete"));
        assert_eq!(label.as_deref(), Some("complete"));
        assert!(guard.is_none());
        assert!(action.is_none());
    }

    #[test]
    fn extract_none_label() {
        let (label, guard, action) = super::extract_state_guard_action(None);
        assert!(label.is_none());
        assert!(guard.is_none());
        assert!(action.is_none());
    }

    // ── Requirement diagram tests ─────────────────────────────────────

    #[test]
    fn requirement_relation_captures_type_label() {
        let parsed = parse_mermaid(
            "requirementDiagram\n  requirement MyReq {\n    id: REQ-001\n    text: Must do X\n    risk: High\n    verifymethod: Test\n  }\n  element MyElem {\n  }\n  MyElem - satisfies -> MyReq",
        );
        assert!(!parsed.ir.edges.is_empty(), "should have edges");
        let edge = &parsed.ir.edges[0];
        assert!(edge.label.is_some(), "edge should have relation label");
        let label_text = edge
            .label
            .and_then(|lid| parsed.ir.labels.get(lid.0))
            .map(|l| l.text.as_str());
        assert_eq!(label_text, Some("satisfies"));
    }

    #[test]
    fn requirement_relation_without_type() {
        let parsed = parse_mermaid(
            "requirementDiagram\n  requirement A {\n  }\n  element B {\n  }\n  B -> A",
        );
        assert!(!parsed.ir.edges.is_empty());
        let edge = &parsed.ir.edges[0];
        assert!(edge.label.is_none(), "plain -> should have no label");
    }

    #[test]
    fn requirement_meta_risk_preserved() {
        let parsed = parse_mermaid(
            "requirementDiagram\n  requirement MyReq {\n    id: R1\n    text: Test\n    risk: High\n    verifymethod: Analysis\n  }",
        );
        let node = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.id == "MyReq")
            .expect("should find MyReq");
        let meta = node.requirement_meta.as_ref().expect("should have meta");
        assert_eq!(meta.risk.as_deref(), Some("High"));
        assert_eq!(meta.verify_method.as_deref(), Some("Analysis"));
    }

    #[test]
    fn requirement_contains_uses_thick_arrow() {
        let parsed = parse_mermaid(
            "requirementDiagram\n  requirement A {\n  }\n  requirement B {\n  }\n  A - contains -> B",
        );
        assert!(!parsed.ir.edges.is_empty());
        assert_eq!(parsed.ir.edges[0].arrow, fm_core::ArrowType::ThickArrow);
    }

    #[test]
    fn requirement_derives_uses_dotted_arrow() {
        let parsed = parse_mermaid(
            "requirementDiagram\n  requirement A {\n  }\n  requirement B {\n  }\n  A - derives -> B",
        );
        assert!(!parsed.ir.edges.is_empty());
        assert_eq!(parsed.ir.edges[0].arrow, fm_core::ArrowType::DottedArrow);
    }

    // ── Architecture-beta service icon shape mapping ──────────────────

    #[test]
    fn architecture_database_icon_uses_cylinder_shape() {
        let parsed = parse_mermaid("architecture-beta\n  service db(database)[My DB]");
        let node = parsed.ir.nodes.iter().find(|n| n.id == "db").unwrap();
        assert_eq!(node.shape, fm_core::NodeShape::Cylinder);
    }

    #[test]
    fn architecture_cloud_icon_uses_rounded_shape() {
        let parsed = parse_mermaid("architecture-beta\n  service cdn(cloud)[CDN]");
        let node = parsed.ir.nodes.iter().find(|n| n.id == "cdn").unwrap();
        assert_eq!(node.shape, fm_core::NodeShape::Rounded);
    }

    #[test]
    fn architecture_default_service_uses_rect() {
        let parsed = parse_mermaid("architecture-beta\n  service api(server)[API]");
        let node = parsed.ir.nodes.iter().find(|n| n.id == "api").unwrap();
        assert_eq!(node.shape, fm_core::NodeShape::Rect);
    }

    // ── Cross-diagram style directive support ─────────────────────────

    #[test]
    fn block_beta_supports_classdef_directives() {
        let parsed = parse_mermaid(
            "block-beta\n  columns 2\n  A B\n  classDef highlight fill:#f9f\n  class A highlight",
        );
        assert!(
            !parsed.ir.style_defs.is_empty(),
            "block-beta should parse classDef"
        );
        let node_a = parsed.ir.nodes.iter().find(|n| n.id == "A").unwrap();
        assert!(
            node_a.inline_style.is_some(),
            "node A should have inline_style from classDef"
        );
    }

    #[test]
    fn class_diagram_supports_style_directives() {
        let parsed =
            parse_mermaid("classDiagram\n  class Dog\n  classDef pet fill:#efe\n  class Dog pet");
        assert!(
            !parsed.ir.style_defs.is_empty(),
            "class diagram should parse classDef"
        );
    }

    #[test]
    fn state_diagram_supports_style_directives() {
        let parsed = parse_mermaid(
            "stateDiagram-v2\n  state Active\n  classDef active fill:#afa\n  class Active active",
        );
        assert!(
            !parsed.ir.style_defs.is_empty(),
            "state diagram should parse classDef"
        );
    }

    // ── ER diagram attribute tests ────────────────────────────────────

    #[test]
    fn er_entity_attributes_with_keys_stored() {
        let parsed = parse_mermaid(
            "erDiagram\n  USER {\n    int id PK\n    varchar name\n    int dept_id FK\n  }",
        );
        let user = parsed.ir.nodes.iter().find(|n| n.id == "USER").unwrap();
        assert_eq!(user.members.len(), 3);
        assert_eq!(user.members[0].key, fm_core::IrAttributeKey::Pk);
        assert_eq!(user.members[0].data_type, "int");
        assert_eq!(user.members[0].name, "id");
        assert_eq!(user.members[2].key, fm_core::IrAttributeKey::Fk);
    }

    #[test]
    fn er_entity_attribute_comments_preserved() {
        let parsed = parse_mermaid("erDiagram\n  USER {\n    int id PK \"primary key\"\n  }");
        let user = parsed.ir.nodes.iter().find(|n| n.id == "USER").unwrap();
        assert_eq!(user.members.len(), 1);
        assert_eq!(user.members[0].comment.as_deref(), Some("primary key"));
    }

    // ── Mindmap branch color tests ────────────────────────────────────

    #[test]
    fn mindmap_assigns_branch_color_classes() {
        let parsed =
            parse_mermaid("mindmap\n  Root\n    Branch1\n      Leaf1\n    Branch2\n      Leaf2");
        // Root gets branch-0, Branch1 gets branch-1, Branch2 gets branch-2.
        let root = parsed.ir.nodes.iter().find(|n| n.id == "Root").unwrap();
        assert!(
            root.classes.iter().any(|c| c == "mindmap-root"),
            "root should have mindmap-root class"
        );

        let branch1 = parsed.ir.nodes.iter().find(|n| n.id == "Branch1").unwrap();
        assert!(
            branch1
                .classes
                .iter()
                .any(|c| c.starts_with("mindmap-branch-")),
            "branch nodes should have mindmap-branch-N class"
        );

        let branch2 = parsed.ir.nodes.iter().find(|n| n.id == "Branch2").unwrap();
        assert!(
            branch2
                .classes
                .iter()
                .any(|c| c.starts_with("mindmap-branch-")),
            "branch nodes should have mindmap-branch-N class"
        );

        // Branch1 and Branch2 should have different branch indices.
        let b1_class = branch1
            .classes
            .iter()
            .find(|c| c.starts_with("mindmap-branch-"))
            .unwrap();
        let b2_class = branch2
            .classes
            .iter()
            .find(|c| c.starts_with("mindmap-branch-"))
            .unwrap();
        assert_ne!(
            b1_class, b2_class,
            "different branches should have different color classes"
        );
    }

    // ── GitGraph commit type and branch color tests ───────────────────

    #[test]
    fn gitgraph_commit_type_maps_to_shape_and_class() {
        let parsed =
            parse_mermaid("gitGraph\n  commit\n  commit type: REVERSE\n  commit type: HIGHLIGHT");
        assert_eq!(parsed.ir.nodes.len(), 3);
        // Normal commit = Circle
        assert_eq!(parsed.ir.nodes[0].shape, fm_core::NodeShape::Circle);
        // REVERSE = FilledCircle
        assert_eq!(parsed.ir.nodes[1].shape, fm_core::NodeShape::FilledCircle);
        assert!(
            parsed.ir.nodes[1]
                .classes
                .iter()
                .any(|c| c == "git-commit-reverse")
        );
        // HIGHLIGHT = DoubleCircle
        assert_eq!(parsed.ir.nodes[2].shape, fm_core::NodeShape::DoubleCircle);
        assert!(
            parsed.ir.nodes[2]
                .classes
                .iter()
                .any(|c| c == "git-commit-highlight")
        );
    }

    #[test]
    fn gitgraph_branch_color_classes_assigned() {
        let parsed = parse_mermaid(
            "gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n  checkout main\n  commit",
        );
        // Commits on main should have git-branch-0
        let main_commit = &parsed.ir.nodes[0];
        assert!(
            main_commit.classes.iter().any(|c| c == "git-branch-0"),
            "main commits should have git-branch-0"
        );

        // Commits on dev should have git-branch-1
        let dev_commit = &parsed.ir.nodes[1];
        assert!(
            dev_commit.classes.iter().any(|c| c == "git-branch-1"),
            "dev commits should have git-branch-1"
        );
    }

    // ── Kanban metadata parsing tests ─────────────────────────────────

    #[test]
    fn kanban_card_priority_metadata_applied() {
        let parsed = parse_mermaid("kanban\n  Todo\n    task1[Design] @{ priority: high }");
        let card = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.classes.contains(&"kanban-card".to_string()))
            .expect("should find kanban card");
        assert!(
            card.classes.iter().any(|c| c == "kanban-priority-high"),
            "card should have kanban-priority-high class"
        );
    }

    #[test]
    fn kanban_card_assigned_metadata_applied() {
        let parsed = parse_mermaid("kanban\n  Todo\n    task1[Design] @{ assigned: alice }");
        let card = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.classes.contains(&"kanban-card".to_string()))
            .expect("should find kanban card");
        assert!(
            card.classes.iter().any(|c| c == "kanban-assigned-alice"),
            "card should have kanban-assigned-alice class"
        );
    }

    #[test]
    fn kanban_without_metadata_still_works() {
        let parsed = parse_mermaid("kanban\n  Todo\n    task1[Design]");
        let card = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.classes.contains(&"kanban-card".to_string()))
            .expect("should find kanban card");
        assert!(
            !card
                .classes
                .iter()
                .any(|c| c.starts_with("kanban-priority-")),
            "card without metadata should not have priority class"
        );
    }

    #[test]
    fn strip_kanban_metadata_unit() {
        let (text, meta) =
            super::strip_kanban_metadata("task1[Design] @{ priority: high, assigned: bob }");
        assert_eq!(text, "task1[Design]");
        assert_eq!(meta.priority.as_deref(), Some("high"));
        assert_eq!(meta.assigned.as_deref(), Some("bob"));
    }

    #[test]
    fn strip_kanban_metadata_wip() {
        let (text, meta) = super::strip_kanban_metadata("In Progress @{ wip: 3 }");
        assert_eq!(text, "In Progress");
        assert_eq!(meta.wip, Some(3));
    }

    // ── Packet-beta field parsing tests ───────────────────────────────

    #[test]
    fn packet_beta_parses_bit_range_fields() {
        let parsed = parse_mermaid(
            "packet-beta\n  0-7: \"Source Port\"\n  8-15: \"Dest Port\"\n  16-31: \"Length\"",
        );
        assert_eq!(parsed.ir.nodes.len(), 3, "should have 3 packet fields");
        let first = &parsed.ir.nodes[0];
        assert!(
            first.classes.iter().any(|c| c == "packet-field"),
            "fields should have packet-field class"
        );
    }

    #[test]
    fn packet_beta_wide_fields_get_class() {
        let parsed = parse_mermaid("packet-beta\n  0-31: \"Sequence Number\"");
        assert_eq!(parsed.ir.nodes.len(), 1);
        let node = &parsed.ir.nodes[0];
        assert!(
            node.classes.iter().any(|c| c == "packet-field-wide"),
            "32-bit field should have packet-field-wide class"
        );
        assert!(
            node.classes.iter().any(|c| c == "packet-bits-32"),
            "should have packet-bits-32 class"
        );
    }

    #[test]
    fn packet_beta_fields_linked_sequentially() {
        let parsed = parse_mermaid("packet-beta\n  0-7: \"A\"\n  8-15: \"B\"\n  16-23: \"C\"");
        assert_eq!(parsed.ir.edges.len(), 2, "3 fields should produce 2 edges");
    }
}
