use chumsky::prelude::*;
use fm_core::{
    ArrowType, DiagramType, GraphDirection, IrAttributeKey, IrNodeId, NodeShape, Span,
    parse_mermaid_js_config_value, to_init_parse,
};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

use crate::{DetectedType, ParseResult, ir_builder::IrBuilder};

const FLOW_OPERATORS: [(&str, ArrowType); 6] = [
    ("-.->", ArrowType::DottedArrow),
    ("==>", ArrowType::ThickArrow),
    ("-->", ArrowType::Arrow),
    ("---", ArrowType::Line),
    ("--o", ArrowType::Circle),
    ("--x", ArrowType::Cross),
];

const SEQUENCE_OPERATORS: [(&str, ArrowType); 6] = [
    ("-->>", ArrowType::DottedArrow),
    ("->>", ArrowType::Arrow),
    ("-->", ArrowType::DottedArrow),
    ("->", ArrowType::Arrow),
    ("--x", ArrowType::Cross),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeToken {
    id: String,
    label: Option<String>,
    shape: NodeShape,
}

/// Simple type detection (used by tests).
#[must_use]
#[allow(dead_code)] // Used by tests
pub fn detect_type(input: &str) -> DiagramType {
    let Some(first_line) = first_significant_line(input) else {
        return DiagramType::Unknown;
    };
    let lower = first_line.to_ascii_lowercase();

    if lower.starts_with("flowchart") || lower == "graph" || lower.starts_with("graph ") {
        DiagramType::Flowchart
    } else if lower.starts_with("sequencediagram") {
        DiagramType::Sequence
    } else if lower.starts_with("classdiagram") {
        DiagramType::Class
    } else if lower.starts_with("statediagram") {
        DiagramType::State
    } else if lower.starts_with("gantt") {
        DiagramType::Gantt
    } else if lower.starts_with("erdiagram") {
        DiagramType::Er
    } else if lower.starts_with("mindmap") {
        DiagramType::Mindmap
    } else if lower.starts_with("pie") {
        DiagramType::Pie
    } else if lower.starts_with("gitgraph") {
        DiagramType::GitGraph
    } else if lower.starts_with("journey") {
        DiagramType::Journey
    } else if lower.starts_with("requirementdiagram") {
        DiagramType::Requirement
    } else if lower.starts_with("timeline") {
        DiagramType::Timeline
    } else if lower.starts_with("quadrantchart") {
        DiagramType::QuadrantChart
    } else if lower.starts_with("sankey") {
        DiagramType::Sankey
    } else if lower.starts_with("xychart") {
        DiagramType::XyChart
    } else if lower.starts_with("block-beta") {
        DiagramType::BlockBeta
    } else if lower.starts_with("packet-beta") {
        DiagramType::PacketBeta
    } else if lower.starts_with("architecture-beta") {
        DiagramType::ArchitectureBeta
    } else if first_line.starts_with("C4Context") {
        DiagramType::C4Context
    } else if first_line.starts_with("C4Container") {
        DiagramType::C4Container
    } else if first_line.starts_with("C4Component") {
        DiagramType::C4Component
    } else if first_line.starts_with("C4Dynamic") {
        DiagramType::C4Dynamic
    } else if first_line.starts_with("C4Deployment") {
        DiagramType::C4Deployment
    } else {
        DiagramType::Unknown
    }
}

/// Parse mermaid input (used by tests, delegates to parse_mermaid_with_detection).
#[must_use]
#[allow(dead_code)] // Used by tests
pub fn parse_mermaid(input: &str) -> ParseResult {
    let detection = crate::detect_type_with_confidence(input);
    parse_mermaid_with_detection(input, detection)
}

/// Parse mermaid input with pre-computed detection results.
#[must_use]
pub fn parse_mermaid_with_detection(input: &str, detection: DetectedType) -> ParseResult {
    let (content, front_matter_payload) = split_front_matter_block(input);
    let diagram_type = detection.diagram_type;
    let mut builder = IrBuilder::new(diagram_type);

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
        DiagramType::PacketBeta => parse_packet(content, &mut builder),
        DiagramType::Gantt => parse_gantt(content, &mut builder),
        DiagramType::Pie => parse_pie(content, &mut builder),
        DiagramType::QuadrantChart => parse_quadrant(content, &mut builder),
        DiagramType::GitGraph => parse_gitgraph(content, &mut builder),
        DiagramType::Unknown => {
            builder
                .add_warning("Unable to detect diagram type; using best-effort flowchart parsing");
            parse_flowchart(content, &mut builder);
        }
        _ => {
            builder.add_warning(format!(
                "Diagram type '{}' is not fully supported yet; using best-effort flowchart parsing",
                diagram_type.as_str()
            ));
            parse_flowchart(content, &mut builder);
        }
    }

    if builder.node_count() == 0 && builder.edge_count() == 0 {
        builder.add_warning("No parseable nodes or edges were found");
    }

    builder.finish(detection.confidence, detection.method)
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
    Subgraph {
        id: String,
        title: Option<String>,
        body: Vec<FlowAst>,
    },
    ClassAssign {
        nodes: Vec<String>,
        class: String,
    },
    ClickDirective {
        node: String,
        target: String,
    },
    StyleOrLinkStyle,
    ClassDef,
}

#[derive(Debug, Clone)]
struct FlowAstNode {
    id: String,
    label: Option<String>,
    shape: NodeShape,
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

    // -- Quoted string -------------------------------------------------------
    let quoted_string = {
        let double_q = just('"')
            .ignore_then(any().filter(|c: &char| *c != '"').repeated().to_slice())
            .then_ignore(just('"'));
        let single_q = just('\'')
            .ignore_then(any().filter(|c: &char| *c != '\'').repeated().to_slice())
            .then_ignore(just('\''));
        double_q.or(single_q)
    };

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
                    let label = (!trimmed.is_empty()).then(|| trimmed.to_string());
                    FlowAstNode { id, label, shape }
                }
                None => FlowAstNode {
                    id,
                    label: None,
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
            let nodes: Vec<String> = node_ids.iter().map(|s| s.to_string()).collect();
            FlowAst::ClassAssign {
                nodes,
                class: class_raw.trim().to_string(),
            }
        });

    // -- click directive: click nodeId target --------------------------------
    let click_directive = just("click")
        .then(required_ws)
        .ignore_then(ident)
        .then_ignore(required_ws)
        .then(
            quoted_string.map(|s: &str| s.to_string()).or(any()
                .repeated()
                .at_least(1)
                .to_slice()
                .map(|s: &str| s.to_string())),
        )
        .then_ignore(end())
        .map(
            |(node_id, target): (&str, String)| FlowAst::ClickDirective {
                node: node_id.to_string(),
                target,
            },
        );

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
) {
    let span = span_for(line_number, source_line);

    match ast {
        FlowAst::Direction(dir) => {
            builder.set_direction(*dir);
        }
        FlowAst::Node(n) => {
            if let Some(node_id) = builder.intern_node(&n.id, n.label.as_deref(), n.shape, span) {
                add_node_to_active_clusters(builder, active_clusters, node_id);
            }
        }
        FlowAst::Edge {
            from,
            arrow,
            label,
            to,
        } => {
            let from_id = builder.intern_node(&from.id, from.label.as_deref(), from.shape, span);
            let to_id = builder.intern_node(&to.id, to.label.as_deref(), to.shape, span);
            if let (Some(f), Some(t)) = (from_id, to_id) {
                add_node_to_active_clusters(builder, active_clusters, f);
                add_node_to_active_clusters(builder, active_clusters, t);
                builder.push_edge(f, t, *arrow, label.as_deref(), span);
            }
        }
        FlowAst::Subgraph { id, title, .. } => {
            let _cluster = builder.ensure_cluster(id, title.as_deref(), span);
        }
        FlowAst::ClassAssign { nodes, class } => {
            for class_name in class.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                for node_key in nodes {
                    builder.add_class_to_node(node_key, class_name, span);
                }
            }
        }
        FlowAst::ClickDirective { node, target } => {
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
            } else if !is_safe_click_target(cleaned) {
                builder.add_warning(format!(
                    "Line {line_number}: unsafe click link target blocked: {cleaned}"
                ));
            } else {
                builder.add_class_to_node(node, "has-link", span);
                builder.set_node_link(node, cleaned, span);
            }
        }
        FlowAst::StyleOrLinkStyle | FlowAst::ClassDef => {
            // Intentionally skipped — same as the hand-written parser
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level parse_flowchart — line-by-line with chumsky statement parser
// ---------------------------------------------------------------------------

fn parse_flowchart(input: &str, builder: &mut IrBuilder) {
    let mut active_clusters: Vec<usize> = Vec::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if is_flowchart_header(trimmed) {
            if let Some(direction) = parse_graph_direction(trimmed) {
                builder.set_direction(direction);
            }
            continue;
        }

        let uncommented_line = strip_flowchart_inline_comment(trimmed);
        if uncommented_line.is_empty() {
            continue;
        }

        let mut parsed_line = false;
        for statement in split_statements(uncommented_line) {
            let normalized_statement = statement.trim();
            if normalized_statement.is_empty() {
                parsed_line = true;
                continue;
            }

            if let Some((cluster_key, cluster_title)) =
                parse_subgraph_statement(normalized_statement)
            {
                let span = span_for(line_number, line);
                if let Some(cluster_index) =
                    builder.ensure_cluster(&cluster_key, cluster_title.as_deref(), span)
                {
                    active_clusters.push(cluster_index);
                }
                parsed_line = true;
                continue;
            }

            if normalized_statement == "end" {
                if active_clusters.pop().is_none() {
                    builder.add_warning(format!(
                        "Line {line_number}: encountered 'end' without matching 'subgraph'"
                    ));
                }
                parsed_line = true;
                continue;
            }

            // Try the chumsky statement parser first
            let (ast, errors) = flow_statement_parser()
                .parse(normalized_statement)
                .into_output_errors();
            if errors.is_empty()
                && let Some(ref ast_node) = ast
            {
                lower_flow_ast(ast_node, line_number, line, builder, &active_clusters);
                parsed_line = true;
                continue;
            }

            // Fallback: use the hand-written helpers for statements chumsky
            // couldn't parse (e.g. class/click with complex quoting)
            if parse_flowchart_directive(normalized_statement, line_number, line, builder) {
                parsed_line = true;
                continue;
            }
            if let Some(node_ids) = parse_edge_statement_with_nodes(
                normalized_statement,
                line_number,
                line,
                &FLOW_OPERATORS,
                builder,
            ) {
                for node_id in node_ids {
                    add_node_to_active_clusters(builder, &active_clusters, node_id);
                }
                parsed_line = true;
                continue;
            }
            if let Some(node) = parse_node_token(normalized_statement) {
                let span = span_for(line_number, line);
                if let Some(node_id) =
                    builder.intern_node(&node.id, node.label.as_deref(), node.shape, span)
                {
                    add_node_to_active_clusters(builder, &active_clusters, node_id);
                }
                parsed_line = true;
            }
        }

        if !parsed_line {
            builder.add_warning(format!(
                "Line {line_number}: unsupported flowchart syntax: {trimmed}"
            ));
        }
    }

    if !active_clusters.is_empty() {
        builder.add_warning(format!(
            "Flowchart ended with {} unclosed subgraph block(s)",
            active_clusters.len()
        ));
    }
}

fn parse_flowchart_directive(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    parse_class_assignment(statement, line_number, source_line, builder)
        || parse_click_directive(statement, line_number, source_line, builder)
        || is_non_graph_statement(statement)
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

fn parse_subgraph_statement(statement: &str) -> Option<(String, Option<String>)> {
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
    if has_explicit_label_delimiters && let Some(node) = parse_node_token(body) {
        let key = normalize_identifier(&node.id);
        if !key.is_empty() {
            return Some((key, node.label));
        }
    }

    // Mermaid commonly supports `subgraph <id> <title>` and
    // `subgraph <id> "<title>"`. Prefer this split form before trying
    // compact node-like forms (`id[Title]`), otherwise the title may
    // accidentally include the id token.
    if let Some(split_index) = body.find(char::is_whitespace) {
        let (candidate_key, candidate_title) = body.split_at(split_index);
        let key = normalize_identifier(candidate_key);
        let title = normalize_subgraph_title(candidate_title);
        let title_has_wrappers = matches!(
            candidate_title.trim_start().chars().next(),
            Some('[' | '(' | '{' | '"' | '\'' | '`')
        );
        let key_is_structured = key.chars().any(|ch| !ch.is_ascii_alphabetic());
        if !key.is_empty() && (title_has_wrappers || key_is_structured) {
            return Some((key, title));
        }
    }

    if let Some(node) = parse_node_token(body) {
        let key = normalize_identifier(&node.id);
        if !key.is_empty() {
            return Some((key, node.label));
        }
    }

    let key = normalize_identifier(body);
    if key.is_empty() {
        return None;
    }
    let title = normalize_subgraph_title(body).filter(|value| value != &key);
    Some((key, title))
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

        if let Some(rest) = trimmed.strip_prefix("participant ") {
            if !register_participant(rest, line_number, line, builder) {
                builder.add_warning(format!(
                    "Line {line_number}: unable to parse participant declaration: {trimmed}"
                ));
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("actor ") {
            if !register_participant(rest, line_number, line, builder) {
                builder.add_warning(format!(
                    "Line {line_number}: unable to parse actor declaration: {trimmed}"
                ));
            }
            continue;
        }

        if parse_sequence_message(trimmed, line_number, line, builder) {
            continue;
        }

        builder.add_warning(format!(
            "Line {line_number}: unsupported sequence syntax: {trimmed}"
        ));
    }
}

fn parse_class(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("classDiagram") {
            continue;
        }

        if trimmed.starts_with("class ") && trimmed.ends_with('{') {
            let class_name = trimmed
                .trim_start_matches("class")
                .trim()
                .trim_end_matches('{')
                .trim();
            if let Some(node) = parse_node_token(class_name) {
                let span = span_for(line_number, line);
                let _ = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span);
            }
            continue;
        }

        let mut parsed_line = false;
        for statement in split_statements(trimmed) {
            if parse_edge_statement(statement, line_number, line, &CLASS_OPERATORS, builder) {
                parsed_line = true;
                continue;
            }
            if let Some(node) = parse_node_token(statement) {
                let span = span_for(line_number, line);
                let _ = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span);
                parsed_line = true;
            }
        }

        if !parsed_line && !trimmed.starts_with('}') {
            builder.add_warning(format!(
                "Line {line_number}: unsupported class syntax: {trimmed}"
            ));
        }
    }
}

fn parse_state(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("stateDiagram") {
            continue;
        }

        if trimmed.starts_with("direction ") {
            if let Some(direction) = parse_graph_direction(trimmed) {
                builder.set_direction(direction);
            }
            continue;
        }

        if trimmed == "[*]" || trimmed == "{" || trimmed == "}" {
            continue;
        }

        if let Some(declaration) = trimmed.strip_prefix("state ")
            && register_state_declaration(declaration, line_number, line, builder)
        {
            continue;
        }

        let mut parsed_line = false;
        for statement in split_statements(trimmed) {
            if parse_edge_statement(statement, line_number, line, &FLOW_OPERATORS, builder) {
                parsed_line = true;
                continue;
            }
            if let Some(node) = parse_node_token(statement) {
                let span = span_for(line_number, line);
                let _ = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span);
                parsed_line = true;
            }
        }

        if !parsed_line {
            builder.add_warning(format!(
                "Line {line_number}: unsupported state syntax: {trimmed}"
            ));
        }
    }
}

fn parse_requirement(input: &str, builder: &mut IrBuilder) {
    let mut inside_requirement_block = false;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "requirementDiagram" {
            continue;
        }

        if let Some(requirement_decl) = trimmed.strip_prefix("requirement ") {
            let requirement_name = requirement_decl.trim_end_matches('{').trim();
            if let Some(node) = parse_node_token(requirement_name) {
                let span = span_for(line_number, line);
                let _ = builder.intern_node(&node.id, node.label.as_deref(), NodeShape::Rect, span);
                inside_requirement_block = trimmed.ends_with('{');
                continue;
            }
        }

        if trimmed.starts_with('{') {
            inside_requirement_block = true;
            continue;
        }
        if trimmed.starts_with('}') {
            inside_requirement_block = false;
            continue;
        }

        if inside_requirement_block
            && (trimmed.starts_with("id:")
                || trimmed.starts_with("text:")
                || trimmed.starts_with("risk:")
                || trimmed.starts_with("verifymethod:"))
        {
            continue;
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
            // Icons are visual metadata; we note them but don't store in IR yet
            // Future: could add icon field to IrNode
            continue;
        }

        // Handle class directive (:::class1 class2) - applies to last node
        if let Some(class_suffix) = trimmed.strip_prefix(":::") {
            let classes = class_suffix.trim();
            if let Some(node_id) = last_node_id {
                let span = span_for(line_number, line);
                for class in classes.split_whitespace() {
                    // Use a placeholder node key since we already have the id
                    // The add_class_to_node function will look up by key
                    if let Some(node) = builder.get_node_by_id(node_id) {
                        let key = node.id.clone();
                        builder.add_class_to_node(&key, class, span);
                    }
                }
            }
            continue;
        }

        let depth = leading_indent_width(line);
        let Some(node) = parse_mindmap_node_token(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported mindmap syntax: {trimmed}"
            ));
            continue;
        };

        let span = span_for(line_number, line);
        let Some(node_id) = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span)
        else {
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
    }
}

/// Parse a mindmap node token with mindmap-specific shapes.
/// Mindmap supports: square [], rounded (), circle (()), bang ))((, cloud )(, hexagon {{}}
fn parse_mindmap_node_token(raw: &str) -> Option<NodeToken> {
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
    if let Some(parsed) = parse_double_circle(core) {
        // For mindmap, (( )) is just a circle, not double-circle
        return Some(NodeToken {
            id: parsed.id,
            label: parsed.label,
            shape: NodeShape::Circle,
        });
    }

    // Rounded shape: id(text)
    if let Some(parsed) = parse_wrapped(core, '(', ')', NodeShape::Rounded) {
        return Some(parsed);
    }

    // Square shape: id[text]
    if let Some(parsed) = parse_wrapped(core, '[', ']', NodeShape::Rect) {
        return Some(parsed);
    }

    // Default shape: plain text (no delimiters)
    let id = normalize_identifier(core);
    if id.is_empty() {
        return None;
    }

    let label = clean_label(Some(core)).filter(|value| value != &id);
    Some(NodeToken {
        id,
        label,
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

    Some(NodeToken {
        id,
        label: clean_label(Some(label_raw)),
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

    Some(NodeToken {
        id,
        label: clean_label(Some(label_raw)),
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

    Some(NodeToken {
        id,
        label: clean_label(Some(label_raw)),
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
            if let Some(node) = parse_node_token(entity_name) {
                let span = span_for(line_number, line);
                current_entity =
                    builder.intern_node(&node.id, node.label.as_deref(), NodeShape::Rect, span);
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
        if let Some(node) = parse_node_token(trimmed) {
            let span = span_for(line_number, line);
            let _ = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span);
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

    // Split into parts, handling quoted comments
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_content = String::new();

    for ch in trimmed.chars() {
        if ch == '"' {
            if in_quote {
                // End of quoted string
                parts.push(quote_content.clone());
                quote_content.clear();
                in_quote = false;
            } else {
                // Start of quoted string - save current if any
                if !current.trim().is_empty() {
                    for part in current.split_whitespace() {
                        parts.push(part.to_string());
                    }
                    current.clear();
                }
                in_quote = true;
            }
        } else if in_quote {
            quote_content.push(ch);
        } else {
            current.push(ch);
        }
    }

    // Don't forget trailing content
    if !current.trim().is_empty() {
        for part in current.split_whitespace() {
            parts.push(part.to_string());
        }
    }
    if in_quote && !quote_content.is_empty() {
        // Unclosed quote - still include it
        parts.push(quote_content);
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
                // (especially if it was quoted or is the last element)
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
    let Some((left_raw, right_raw)) = statement.split_once("->") else {
        return false;
    };

    let left_id = left_raw
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

    let span = span_for(line_number, source_line);
    let from = builder.intern_node(&left_id, None, NodeShape::Rect, span);
    let to = builder.intern_node(&right_id, None, NodeShape::Rect, span);
    match (from, to) {
        (Some(from_id), Some(to_id)) => {
            builder.push_edge(from_id, to_id, ArrowType::Arrow, None, span);
            true
        }
        _ => false,
    }
}

fn parse_class_assignment(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let Some(rest) = statement.strip_prefix("class ") else {
        return false;
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return false;
    }

    let mut parts = rest.split_whitespace();
    let Some(node_list_raw) = parts.next() else {
        return false;
    };
    let class_list_raw = parts.collect::<Vec<_>>().join(" ");
    if class_list_raw.is_empty() {
        return false;
    }

    let classes: Vec<&str> = class_list_raw
        .split(',')
        .map(str::trim)
        .filter(|class_name| !class_name.is_empty())
        .collect();
    if classes.is_empty() {
        return false;
    }

    let span = span_for(line_number, source_line);
    let mut assigned_any = false;
    for raw_node in node_list_raw.split(',') {
        let node_id = normalize_identifier(raw_node);
        if node_id.is_empty() {
            continue;
        }
        for class_name in &classes {
            builder.add_class_to_node(&node_id, class_name, span);
            assigned_any = true;
        }
    }
    assigned_any
}

fn parse_click_directive(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let Some(rest) = statement.strip_prefix("click ") else {
        return false;
    };
    let span = span_for(line_number, source_line);

    let Some((node_token, after_node)) = take_token(rest) else {
        builder.add_warning(format!(
            "Line {line_number}: malformed click directive (missing node id): {statement}"
        ));
        return true;
    };
    let node_id = normalize_identifier(node_token);
    if node_id.is_empty() {
        builder.add_warning(format!(
            "Line {line_number}: malformed click directive (invalid node id): {statement}"
        ));
        return true;
    }

    let Some((target_token, after_target)) = take_token(after_node) else {
        builder.add_warning(format!(
            "Line {line_number}: malformed click directive (missing target): {statement}"
        ));
        return true;
    };

    let resolved_target = if target_token.eq_ignore_ascii_case("href") {
        let Some((href_target, _)) = take_token(after_target) else {
            builder.add_warning(format!(
                "Line {line_number}: malformed click directive (missing href target): {statement}"
            ));
            return true;
        };
        href_target
    } else if target_token.eq_ignore_ascii_case("call")
        || target_token.eq_ignore_ascii_case("callback")
    {
        builder.add_warning(format!(
            "Line {line_number}: click callbacks are not supported yet; keeping node without link metadata"
        ));
        return true;
    } else {
        target_token
    };

    let cleaned_target = resolved_target
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim();
    if cleaned_target.is_empty() {
        builder.add_warning(format!(
            "Line {line_number}: click directive target is empty after normalization"
        ));
        return true;
    }

    if !is_safe_click_target(cleaned_target) {
        builder.add_warning(format!(
            "Line {line_number}: unsafe click link target blocked: {cleaned_target}"
        ));
        return true;
    }

    builder.add_class_to_node(&node_id, "has-link", span);
    builder.set_node_link(&node_id, cleaned_target, span);
    true
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
    let lower = decoded.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("vbscript:")
    {
        return false;
    }

    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || decoded.starts_with('/')
        || decoded.starts_with('#')
        || !lower.contains(':')
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

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "journey" || trimmed.starts_with("title ") || trimmed.starts_with("section ")
        {
            continue;
        }

        let Some(step_name) = parse_name_before_colon(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported journey syntax: {trimmed}"
            ));
            continue;
        };
        let step_id = normalize_identifier(step_name);
        if step_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: journey step identifier could not be derived: {trimmed}"
            ));
            continue;
        }

        let span = span_for(line_number, line);
        let current_step = builder.intern_node(&step_id, Some(step_name), NodeShape::Rounded, span);
        if let (Some(prev), Some(current)) = (previous_step, current_step) {
            builder.push_edge(prev, current, ArrowType::Line, None, span);
        }
        if current_step.is_some() {
            previous_step = current_step;
        }
    }
}

fn parse_timeline(input: &str, builder: &mut IrBuilder) {
    let mut previous_period: Option<IrNodeId> = None;
    let mut current_period: Option<IrNodeId> = None;
    let mut current_section: Option<usize> = None;

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

        // Handle title (currently just skip, could store in metadata)
        if let Some(title_text) = trimmed.strip_prefix("title ") {
            // Store title in IR metadata if needed
            let _title = title_text.trim();
            continue;
        }

        // Handle section
        if let Some(section_name) = trimmed.strip_prefix("section ") {
            let section_name = section_name.trim();
            let span = span_for(line_number, line);
            current_section = builder.ensure_cluster(section_name, Some(section_name), span);
            continue;
        }

        let span = span_for(line_number, line);

        // Check if this is a continuation event (starts with :)
        if let Some(continuation) = trimmed.strip_prefix(':') {
            // This is a continuation event for the current period
            if let Some(period_id) = current_period {
                let events_text = continuation.trim();
                parse_timeline_events(events_text, period_id, line_number, line, builder);
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
                // Add to current section if any
                if let Some(section_idx) = current_section {
                    builder.add_node_to_cluster(section_idx, period_node_id);
                }

                // Link to previous period (timeline sequence)
                if let Some(prev_id) = previous_period {
                    builder.push_edge(prev_id, period_node_id, ArrowType::Arrow, None, span);
                }

                previous_period = Some(period_node_id);
                current_period = Some(period_node_id);

                // Parse events for this period
                if !events_text.is_empty() {
                    parse_timeline_events(events_text, period_node_id, line_number, line, builder);
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
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);

    // Events can be separated by colons
    for event_text in events_text.split(':') {
        let event_text = event_text.trim();
        if event_text.is_empty() {
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
            // Events are children of their time period
            builder.push_edge(period_id, event_node_id, ArrowType::Line, None, span);
        }
    }
}

fn parse_packet(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed.starts_with("packet-beta") {
            continue;
        }

        let mut parsed_line = false;
        for statement in split_statements(trimmed) {
            if parse_edge_statement(statement, line_number, line, &PACKET_OPERATORS, builder) {
                parsed_line = true;
                continue;
            }
            if let Some(node) = parse_node_token(statement) {
                let span = span_for(line_number, line);
                let _ = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span);
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

    let Some(left_node) = parse_node_token(left_raw) else {
        return false;
    };
    let Some(right_node) = parse_node_token(right_raw) else {
        return false;
    };

    let span = span_for(line_number, source_line);
    let from = builder.intern_node(
        &left_node.id,
        left_node.label.as_deref(),
        NodeShape::Rect,
        span,
    );
    let to = builder.intern_node(
        &right_node.id,
        right_node.label.as_deref(),
        NodeShape::Rect,
        span,
    );

    match (from, to) {
        (Some(from_node), Some(to_node)) => {
            builder.push_edge(from_node, to_node, arrow, label.as_deref(), span);
            true
        }
        _ => false,
    }
}

fn parse_gantt(input: &str, builder: &mut IrBuilder) {
    let mut current_section = String::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "gantt" || trimmed.starts_with("title ") {
            continue;
        }
        if trimmed.starts_with("dateFormat ")
            || trimmed.starts_with("axisFormat ")
            || trimmed.starts_with("tickInterval ")
            || trimmed.starts_with("excludes ")
        {
            continue;
        }

        if let Some(section_name) = trimmed.strip_prefix("section ") {
            current_section = section_name.trim().to_string();
            continue;
        }

        let Some(task_name) = parse_name_before_colon(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported gantt syntax: {trimmed}"
            ));
            continue;
        };

        let task_id_raw = normalize_identifier(task_name);
        if task_id_raw.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: task identifier could not be derived: {trimmed}"
            ));
            continue;
        }
        let task_id = format!("{task_id_raw}_{line_number}");

        let task_label = if current_section.is_empty() {
            task_name.to_string()
        } else {
            format!("{current_section}: {task_name}")
        };
        let span = span_for(line_number, line);
        let _ = builder.intern_node(&task_id, Some(&task_label), NodeShape::Rect, span);
    }
}

fn parse_pie(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if trimmed.starts_with("pie")
            || trimmed.starts_with("title ")
            || trimmed.starts_with("showData")
        {
            continue;
        }

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
        let span = span_for(line_number, line);
        let _ = builder.intern_node(&slice_id, Some(slice_name), NodeShape::Circle, span);
    }
}

fn parse_quadrant(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if trimmed == "quadrantChart"
            || trimmed.starts_with("x-axis ")
            || trimmed.starts_with("y-axis ")
            || trimmed.starts_with("quadrant-")
            || trimmed.starts_with("title ")
        {
            continue;
        }

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
        let span = span_for(line_number, line);
        let _ = builder.intern_node(&point_id, Some(point_name), NodeShape::Circle, span);
    }
}

/// Git graph state tracker for parsing.
struct GitGraphState {
    /// Map of branch names to their current head commit node ID
    branches: std::collections::BTreeMap<String, IrNodeId>,
    /// Current branch name
    current_branch: String,
    /// Auto-generated commit counter for unnamed commits
    commit_counter: usize,
}

impl GitGraphState {
    fn new() -> Self {
        Self {
            branches: std::collections::BTreeMap::new(),
            current_branch: "main".to_string(),
            commit_counter: 0,
        }
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

        // Parse git commands - require word boundary (space or end of line after command)
        if let Some(rest) = strip_git_command(trimmed, "commit") {
            parse_git_commit(rest, line_number, line, &mut state, builder);
            continue;
        }

        if let Some(rest) = strip_git_command(trimmed, "branch") {
            parse_git_branch(rest.trim(), line_number, line, &mut state, builder);
            continue;
        }

        if let Some(rest) = strip_git_command(trimmed, "checkout") {
            parse_git_checkout(rest.trim(), line_number, line, &mut state, builder);
            continue;
        }

        if let Some(rest) = strip_git_command(trimmed, "merge") {
            parse_git_merge(rest.trim(), line_number, line, &mut state, builder);
            continue;
        }

        if let Some(rest) = strip_git_command(trimmed, "cherry-pick") {
            parse_git_cherry_pick(rest.trim(), line_number, line, &mut state, builder);
            continue;
        }

        builder.add_warning(format!(
            "Line {line_number}: unsupported gitGraph syntax: {trimmed}"
        ));
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
    rest: &str,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);
    let options = parse_git_commit_options(rest);

    // Determine commit ID
    let commit_id = options.id.unwrap_or_else(|| state.next_commit_id());

    // Build label from message and/or tag
    let label = match (&options.msg, &options.tag) {
        (Some(msg), Some(tag)) => Some(format!("{msg} [{tag}]")),
        (Some(msg), None) => Some(msg.clone()),
        (None, Some(tag)) => Some(format!("[{tag}]")),
        (None, None) => None,
    };

    // Create the commit node
    let Some(node_id) = builder.intern_node(&commit_id, label.as_deref(), NodeShape::Circle, span)
    else {
        return;
    };

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
}

fn parse_git_commit_options(rest: &str) -> GitCommitOptions {
    let mut options = GitCommitOptions {
        id: None,
        msg: None,
        tag: None,
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
            && let Some((value, rest)) = extract_quoted_value(rest_after_id.trim_start())
        {
            options.id = Some(value);
            remaining = rest;
            continue;
        }

        // Try to match msg: "value"
        if let Some(rest_after_msg) = remaining.strip_prefix("msg:")
            && let Some((value, rest)) = extract_quoted_value(rest_after_msg.trim_start())
        {
            options.msg = Some(value);
            remaining = rest;
            continue;
        }

        // Try to match tag: "value"
        if let Some(rest_after_tag) = remaining.strip_prefix("tag:")
            && let Some((value, rest)) = extract_quoted_value(rest_after_tag.trim_start())
        {
            options.tag = Some(value);
            remaining = rest;
            continue;
        }

        // Try to match type: VALUE (we acknowledge but don't store it for now)
        if let Some(rest_after_type) = remaining.strip_prefix("type:") {
            let type_rest = rest_after_type.trim_start();
            // Skip type value (NORMAL, REVERSE, HIGHLIGHT)
            let end = type_rest
                .find(|c: char| c.is_whitespace())
                .unwrap_or(type_rest.len());
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

    let content_start = 1;
    let end_quote = trimmed[content_start..].find(quote_char)?;
    let value = trimmed[content_start..content_start + end_quote].to_string();
    let rest = &trimmed[content_start + end_quote + 1..];

    Some((value, rest))
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
    merge_spec: &str,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);

    // Parse branch name and optional tag/id
    // Syntax: merge branch_name [tag: "tag"] [id: "id"]
    let parts: Vec<&str> = merge_spec.split_whitespace().collect();
    let branch_name = match parts.first() {
        Some(name) => normalize_identifier(name),
        None => {
            builder.add_warning(format!("Line {line_number}: merge requires a branch name"));
            return;
        }
    };

    if branch_name.is_empty() {
        builder.add_warning(format!("Line {line_number}: invalid branch name in merge"));
        return;
    }

    // Get the head of the branch being merged
    let merge_source = match state.branches.get(&branch_name).copied() {
        Some(id) => id,
        None => {
            builder.add_warning(format!(
                "Line {line_number}: cannot merge non-existent branch '{branch_name}'"
            ));
            return;
        }
    };

    // Create a merge commit
    let merge_id = state.next_commit_id();
    let label = format!("merge {branch_name}");

    let Some(merge_node) = builder.intern_node(&merge_id, Some(&label), NodeShape::Circle, span)
    else {
        return;
    };

    // Create edge from merge source to merge commit
    builder.push_edge(merge_source, merge_node, ArrowType::DottedArrow, None, span);

    // Create edge from current head to merge commit
    if let Some(current_head) = state.current_head() {
        builder.push_edge(current_head, merge_node, ArrowType::Line, None, span);
    }

    // Update current branch head
    state.set_head(&state.current_branch.clone(), merge_node);
}

fn parse_git_cherry_pick(
    cherry_pick_spec: &str,
    line_number: usize,
    source_line: &str,
    state: &mut GitGraphState,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);

    // Syntax: cherry-pick id: "commit_id" [tag: "tag"]
    let id_prefix = "id:";
    let Some(id_start) = cherry_pick_spec.find(id_prefix) else {
        builder.add_warning(format!(
            "Line {line_number}: cherry-pick requires id: parameter"
        ));
        return;
    };

    let rest = cherry_pick_spec[id_start + id_prefix.len()..].trim_start();
    let Some((source_commit_id, _)) = extract_quoted_value(rest) else {
        builder.add_warning(format!(
            "Line {line_number}: cherry-pick id must be a quoted string"
        ));
        return;
    };

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

fn register_state_declaration(
    declaration: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let body = declaration.trim().trim_end_matches('{').trim();
    if body.is_empty() {
        return false;
    }

    let (raw_id, raw_label) = if let Some((label_part, id_part)) = body.split_once(" as ") {
        (id_part.trim(), Some(label_part.trim()))
    } else {
        (body, None)
    };

    let id = normalize_identifier(raw_id);
    if id.is_empty() {
        return false;
    }

    let label = raw_label
        .and_then(|value| clean_label(Some(value)))
        .or_else(|| clean_label(Some(raw_id)));
    let span = span_for(line_number, source_line);
    let _ = builder.intern_node(&id, label.as_deref(), NodeShape::Rounded, span);
    true
}

fn register_participant(
    declaration: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let trimmed = declaration.trim();
    if trimmed.is_empty() {
        return false;
    }

    let (raw_id, raw_label) = if let Some((left, right)) = trimmed.split_once(" as ") {
        (left.trim(), Some(right.trim()))
    } else {
        (trimmed, None)
    };

    let participant_id = normalize_identifier(raw_id);
    if participant_id.is_empty() {
        return false;
    }

    let label = raw_label
        .and_then(|value| clean_label(Some(value)))
        .or_else(|| clean_label(Some(raw_id)));
    let span = span_for(line_number, source_line);
    let _ = builder.intern_node(&participant_id, label.as_deref(), NodeShape::Rect, span);
    true
}

fn parse_sequence_message(
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
        (target.trim(), clean_label(Some(label)))
    } else {
        (right, None)
    };

    let from_id = normalize_identifier(left);
    let to_id = normalize_identifier(target_raw);
    if from_id.is_empty() || to_id.is_empty() {
        return false;
    }

    let span = span_for(line_number, source_line);

    let left_label = clean_label(Some(left)).filter(|l| l != &from_id);
    let from = builder.intern_node(&from_id, left_label.as_deref(), NodeShape::Rect, span);

    let right_label = clean_label(Some(target_raw)).filter(|l| l != &to_id);
    let to = builder.intern_node(&to_id, right_label.as_deref(), NodeShape::Rect, span);

    match (from, to) {
        (Some(from_node), Some(to_node)) => {
            builder.push_edge(from_node, to_node, arrow, message_label.as_deref(), span);
            true
        }
        _ => false,
    }
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
    let left_node = parse_node_token(left_raw)?;
    let mut from_node = builder.intern_node(
        &left_node.id,
        left_node.label.as_deref(),
        left_node.shape,
        span,
    )?;
    let mut touched_nodes = vec![from_node];

    let mut operator_idx = first_operator_idx;
    let mut operator = first_operator;
    let mut arrow = first_arrow;

    loop {
        let rhs_start = operator_idx + operator.len();
        let next_operator = find_operator_from_index(statement, rhs_start, operators);
        let right_segment = match next_operator {
            Some((next_idx, _, _)) => &statement[rhs_start..next_idx],
            None => &statement[rhs_start..],
        }
        .trim();

        if right_segment.is_empty() {
            return (touched_nodes.len() > 1).then_some(touched_nodes);
        }

        let (edge_label, right_without_label) = extract_pipe_label(right_segment);
        let right_node = match parse_node_token(right_without_label) {
            Some(node) => node,
            None => return (touched_nodes.len() > 1).then_some(touched_nodes),
        };
        let to_node = match builder.intern_node(
            &right_node.id,
            right_node.label.as_deref(),
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

fn parse_node_token(raw: &str) -> Option<NodeToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed == "[*]" {
        return Some(NodeToken {
            id: "__state_start_end".to_string(),
            label: Some("*".to_string()),
            shape: NodeShape::Circle,
        });
    }

    let core = trimmed.split(":::").next().unwrap_or(trimmed).trim();
    if core.is_empty() {
        return None;
    }

    if let Some(parsed) = parse_double_circle(core) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, "[(", ")]", NodeShape::Cylinder) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, "[[", "]]", NodeShape::Subroutine) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped(core, '[', ']', NodeShape::Rect) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped(core, '(', ')', NodeShape::Rounded) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped(core, '{', '}', NodeShape::Diamond) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, ">", "]", NodeShape::Asymmetric) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, "[/", "/]", NodeShape::Parallelogram) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, "[\\", "\\]", NodeShape::InvParallelogram) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, "[/", "\\]", NodeShape::Trapezoid) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped_str(core, "[\\", "/]", NodeShape::InvTrapezoid) {
        return Some(parsed);
    }

    let id = normalize_identifier(core);
    if id.is_empty() {
        return None;
    }

    let label = clean_label(Some(core)).filter(|value| value != &id);
    Some(NodeToken {
        id,
        label,
        shape: NodeShape::Rect,
    })
}

fn parse_double_circle(raw: &str) -> Option<NodeToken> {
    let start = raw.find("((")?;
    if !raw.ends_with("))") {
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

    Some(NodeToken {
        id,
        label: clean_label(Some(label_raw)),
        shape: NodeShape::DoubleCircle,
    })
}

fn parse_wrapped(raw: &str, open: char, close: char, shape: NodeShape) -> Option<NodeToken> {
    let start = raw.find(open)?;
    if !raw.ends_with(close) {
        return None;
    }

    let inner_start = start + open.len_utf8();
    let end = raw.len().saturating_sub(close.len_utf8());
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

    Some(NodeToken {
        id,
        label: clean_label(Some(label_raw)),
        shape,
    })
}

fn parse_wrapped_str(raw: &str, open: &str, close: &str, shape: NodeShape) -> Option<NodeToken> {
    let start = raw.find(open)?;
    if !raw.ends_with(close) {
        return None;
    }

    let inner_start = start + open.len();
    let end = raw.len().saturating_sub(close.len());
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

    Some(NodeToken {
        id,
        label: clean_label(Some(label_raw)),
        shape,
    })
}

fn normalize_identifier(raw: &str) -> String {
    let cleaned = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim();
    if cleaned.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(cleaned.len());
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/') {
            out.push(ch);
        } else if ch.is_whitespace() || matches!(ch, ':' | ';' | ',') {
            if !out.is_empty() {
                break;
            }
        } else if !out.is_empty() {
            break;
        }
    }

    if out.is_empty() {
        let mut fallback = String::with_capacity(cleaned.len());
        for grapheme in cleaned.graphemes(true) {
            if grapheme
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            {
                fallback.push_str(grapheme);
            } else {
                fallback.push('_');
            }
        }
        fallback.trim_matches('_').to_string()
    } else {
        out
    }
}

fn clean_label(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let cleaned = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
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
        .map(str::trim)
        .unwrap_or(&title);
    (!unwrapped.is_empty()).then_some(unwrapped.to_string())
}

fn parse_name_before_colon(line: &str) -> Option<&str> {
    let (left, _) = line.split_once(':')?;
    let candidate = left.trim().trim_matches('"').trim_matches('\'').trim();
    (!candidate.is_empty()).then_some(candidate)
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
    let yaml_value = match serde_yaml::from_str::<Value>(front_matter_payload) {
        Ok(value) => value,
        Err(error) => {
            let message = format!("Front matter: invalid YAML config: {error}");
            builder.add_warning(message.clone());
            builder.add_init_error(message, span);
            return;
        }
    };

    let config_value = yaml_value
        .get("config")
        .cloned()
        .unwrap_or_else(|| yaml_value.clone());
    apply_mermaid_config_value(config_value, "Front matter", span, builder);
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

pub(crate) fn first_significant_line(input: &str) -> Option<&str> {
    let (content, _) = split_front_matter_block(input);
    content.lines().map(str::trim).find(|line| {
        !line.is_empty() && !is_comment(line) && !line.starts_with("%%{") && !line.ends_with("}%%")
    })
}

fn is_flowchart_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("flowchart") || lower == "graph" || lower.starts_with("graph ")
}

fn is_non_graph_statement(line: &str) -> bool {
    line.starts_with("style ") || line.starts_with("classDef ") || line.starts_with("linkStyle ")
}

fn is_comment(line: &str) -> bool {
    line.starts_with("%%")
}

#[cfg(test)]
mod tests {
    use fm_core::{ArrowType, DiagramType, GraphDirection, NodeShape};

    use super::{detect_type, parse_mermaid};

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
        assert_eq!(parsed.ir.edges.len(), 1);
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "A"));
        assert!(parsed.ir.nodes.iter().any(|node| node.id == "B"));

        assert_eq!(parsed.ir.clusters.len(), 1);
        let cluster = &parsed.ir.clusters[0];
        let member_ids: std::collections::BTreeSet<String> = cluster
            .members
            .iter()
            .filter_map(|member| parsed.ir.nodes.get(member.0).map(|node| node.id.clone()))
            .collect();
        assert!(member_ids.contains("A"));
        assert!(member_ids.contains("B"));
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
                    .map(|label| label.text.as_str() == "API Layer")
                    .unwrap_or(false)
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
                    .map(|label| label.text.as_str() == "DB Layer")
                    .unwrap_or(false)
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

        let input = r#"erDiagram
    CUSTOMER {
        int id PK
        string name
        string email UK
    }
"#;
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
        let input = r#"erDiagram
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
"#;
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
        let input = r#"erDiagram
    TABLE {
        varchar(255) email
        decimal(10,2) price
        timestamp created_at
    }
"#;
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
    fn gantt_parses_tasks_as_nodes() {
        let parsed = parse_mermaid(
            "gantt\ntitle Release\nsection Phase 1\nDesign :a1, 2026-02-01, 3d\nBuild :a2, after a1, 5d",
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::Gantt);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 0);
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
        // Icons are recognized but currently not stored in IR
        let parsed = parse_mermaid("mindmap\n  Root\n    Child\n    ::icon(fa fa-book)");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert!(parsed.warnings.is_empty());
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
        let input = r#"mindmap
  root((mindmap))
    Origins
      Long history
      Popularisation
        Tony Buzan
    Research
      On effectiveness
    Tools
      Pen and paper
      Mermaid"#;
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
        let input = r#"timeline
    title History of Social Media
    2002 : LinkedIn
    2004 : Facebook
    2005 : YouTube"#;
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
        let input = r#"timeline
    2004 : Facebook
         : Google
         : Orkut"#;
        let parsed = parse_mermaid(input);
        // 1 time period + 3 events = 4 nodes
        assert_eq!(parsed.ir.nodes.len(), 4);
        // 3 edges from period to each event
        assert_eq!(parsed.ir.edges.len(), 3);
    }

    #[test]
    fn timeline_parses_sections() {
        let input = r#"timeline
    title Timeline
    section Early Days
        2002 : LinkedIn
    section Growth Era
        2004 : Facebook"#;
        let parsed = parse_mermaid(input);
        // 2 time periods + 2 events = 4 nodes
        assert_eq!(parsed.ir.nodes.len(), 4);
        // 2 clusters (sections)
        assert_eq!(parsed.ir.clusters.len(), 2);
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
            "%%{init: {\"flowchart\":{\"curve\":\"basis\",\"rankDir\":\"LR\"},\"sequence\":{\"mirrorActors\":true}}}%%\nflowchart TB\nA-->B",
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
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert!(parsed.ir.meta.init.errors.is_empty());
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
            r#"gitGraph
commit
branch develop
checkout develop
commit
checkout main
commit"#,
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
            r#"gitGraph
commit
branch develop
checkout develop
commit
checkout main
merge develop"#,
        );
        assert_eq!(parsed.ir.diagram_type, DiagramType::GitGraph);
        // 3 nodes: initial commit, develop commit, merge commit
        assert_eq!(parsed.ir.nodes.len(), 3);
        // Edges: initial->develop, initial->merge, develop->merge
        assert_eq!(parsed.ir.edges.len(), 3);
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
}
