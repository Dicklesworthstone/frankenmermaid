use fm_core::{ArrowType, DiagramType, GraphDirection, NodeShape, Span};

use crate::{ParseResult, ir_builder::IrBuilder};

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeToken {
    id: String,
    label: Option<String>,
    shape: NodeShape,
}

#[must_use]
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

#[must_use]
pub fn parse_mermaid(input: &str) -> ParseResult {
    let diagram_type = detect_type(input);
    let mut builder = IrBuilder::new(diagram_type);

    match diagram_type {
        DiagramType::Flowchart => parse_flowchart(input, &mut builder),
        DiagramType::Sequence => parse_sequence(input, &mut builder),
        DiagramType::Class => parse_class(input, &mut builder),
        DiagramType::State => parse_state(input, &mut builder),
        DiagramType::PacketBeta => parse_packet(input, &mut builder),
        DiagramType::Gantt => parse_gantt(input, &mut builder),
        DiagramType::Pie => parse_pie(input, &mut builder),
        DiagramType::QuadrantChart => parse_quadrant(input, &mut builder),
        DiagramType::Unknown => {
            builder
                .add_warning("Unable to detect diagram type; using best-effort flowchart parsing");
            parse_flowchart(input, &mut builder);
        }
        _ => {
            builder.add_warning(format!(
                "Diagram type '{}' is not fully supported yet; using best-effort flowchart parsing",
                diagram_type.as_str()
            ));
            parse_flowchart(input, &mut builder);
        }
    }

    if builder.node_count() == 0 && builder.edge_count() == 0 {
        builder.add_warning("No parseable nodes or edges were found");
    }

    builder.finish()
}

fn parse_flowchart(input: &str, builder: &mut IrBuilder) {
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

        if is_non_graph_statement(trimmed) {
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
                "Line {line_number}: unsupported flowchart syntax: {trimmed}"
            ));
        }
    }
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

        if let Some(declaration) = trimmed.strip_prefix("state ") {
            if register_state_declaration(declaration, line_number, line, builder) {
                continue;
            }
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

        let scoped_name = if current_section.is_empty() {
            task_name.to_string()
        } else {
            format!("{current_section}/{task_name}")
        };
        let task_id = normalize_identifier(&scoped_name);
        if task_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: task identifier could not be derived: {trimmed}"
            ));
            continue;
        }

        let span = span_for(line_number, line);
        let _ = builder.intern_node(&task_id, Some(task_name), NodeShape::Rect, span);
    }
}

fn parse_pie(input: &str, builder: &mut IrBuilder) {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if trimmed.starts_with("pie") || trimmed.starts_with("title ") || trimmed.starts_with("showData") {
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
    let from = builder.intern_node(
        &from_id,
        clean_label(Some(left)).as_deref(),
        NodeShape::Rect,
        span,
    );
    let to = builder.intern_node(
        &to_id,
        clean_label(Some(target_raw)).as_deref(),
        NodeShape::Rect,
        span,
    );

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
    let Some((operator_idx, operator, arrow)) = find_operator(statement, operators) else {
        return false;
    };

    let left_raw = statement[..operator_idx].trim();
    let right_raw = statement[operator_idx + operator.len()..].trim();
    if left_raw.is_empty() || right_raw.is_empty() {
        return false;
    }

    let (edge_label, right_without_label) = extract_pipe_label(right_raw);
    let Some(left_node) = parse_node_token(left_raw) else {
        return false;
    };
    let Some(right_node) = parse_node_token(right_without_label) else {
        return false;
    };

    let span = span_for(line_number, source_line);
    let from = builder.intern_node(
        &left_node.id,
        left_node.label.as_deref(),
        left_node.shape,
        span,
    );
    let to = builder.intern_node(
        &right_node.id,
        right_node.label.as_deref(),
        right_node.shape,
        span,
    );

    match (from, to) {
        (Some(from_node), Some(to_node)) => {
            builder.push_edge(from_node, to_node, arrow, edge_label.as_deref(), span);
            true
        }
        _ => false,
    }
}

fn find_operator<'a>(
    statement: &str,
    operators: &'a [(&'a str, ArrowType)],
) -> Option<(usize, &'a str, ArrowType)> {
    let mut selected: Option<(usize, &'a str, ArrowType)> = None;
    for (operator, arrow) in operators {
        if let Some(index) = statement.find(operator) {
            match selected {
                Some((best_index, best_operator, _))
                    if index > best_index
                        || (index == best_index && operator.len() <= best_operator.len()) => {}
                _ => {
                    selected = Some((index, operator, *arrow));
                }
            }
        }
    }
    selected
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
    if let Some(parsed) = parse_wrapped(core, '[', ']', NodeShape::Rect) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped(core, '(', ')', NodeShape::Rounded) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_wrapped(core, '{', '}', NodeShape::Diamond) {
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
        cleaned
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
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

fn parse_name_before_colon(line: &str) -> Option<&str> {
    let (left, _) = line.split_once(':')?;
    let candidate = left.trim().trim_matches('"').trim_matches('\'').trim();
    (!candidate.is_empty()).then_some(candidate)
}

fn split_statements(line: &str) -> impl Iterator<Item = &str> {
    line.split(';')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
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

fn first_significant_line(input: &str) -> Option<&str> {
    input.lines().map(str::trim).find(|line| {
        !line.is_empty() && !is_comment(line) && !line.starts_with("%%{") && !line.ends_with("}%%")
    })
}

fn is_flowchart_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("flowchart") || lower == "graph" || lower.starts_with("graph ")
}

fn is_non_graph_statement(line: &str) -> bool {
    line.starts_with("style ")
        || line.starts_with("classDef ")
        || line.starts_with("class ")
        || line.starts_with("click ")
        || line.starts_with("linkStyle ")
        || line.starts_with("subgraph ")
        || line == "end"
}

fn is_comment(line: &str) -> bool {
    line.starts_with("%%")
}

#[cfg(test)]
mod tests {
    use fm_core::{ArrowType, DiagramType, GraphDirection};

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
    fn unknown_uses_best_effort_flowchart_parser() {
        let parsed = parse_mermaid("A --> B");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Unknown);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.warnings.len(), 1);
    }
}
