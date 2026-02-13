use fm_core::{ArrowType, DiagramType, GraphDirection, IrAttributeKey, IrNodeId, NodeShape, Span};
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
    let diagram_type = detection.diagram_type;
    let mut builder = IrBuilder::new(diagram_type);

    // Add detection warnings to builder
    for warning in &detection.warnings {
        builder.add_warning(warning.clone());
    }

    parse_init_directives(input, &mut builder);

    match diagram_type {
        DiagramType::Flowchart => parse_flowchart(input, &mut builder),
        DiagramType::Sequence => parse_sequence(input, &mut builder),
        DiagramType::Class => parse_class(input, &mut builder),
        DiagramType::State => parse_state(input, &mut builder),
        DiagramType::Requirement => parse_requirement(input, &mut builder),
        DiagramType::Mindmap => parse_mindmap(input, &mut builder),
        DiagramType::Er => parse_er(input, &mut builder),
        DiagramType::Journey => parse_journey(input, &mut builder),
        DiagramType::Timeline => parse_timeline(input, &mut builder),
        DiagramType::PacketBeta => parse_packet(input, &mut builder),
        DiagramType::Gantt => parse_gantt(input, &mut builder),
        DiagramType::Pie => parse_pie(input, &mut builder),
        DiagramType::QuadrantChart => parse_quadrant(input, &mut builder),
        DiagramType::GitGraph => parse_gitgraph(input, &mut builder),
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

    builder.finish(detection.confidence, detection.method)
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

        if parse_flowchart_directive(trimmed, line_number, line, builder) {
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

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if trimmed == "mindmap" {
            continue;
        }

        let depth = leading_indent_width(line);
        let Some(node) = parse_node_token(trimmed) else {
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
        if let Some(entity_id) = current_entity {
            if let Some(attr) = parse_er_attribute(trimmed) {
                builder.add_entity_attribute(
                    entity_id,
                    &attr.data_type,
                    &attr.name,
                    attr.key,
                    attr.comment.as_deref(),
                );
                continue;
            }
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
    let from = builder.intern_node(&left_id, Some(&left_id), NodeShape::Rect, span);
    let to = builder.intern_node(&right_id, Some(&right_id), NodeShape::Rect, span);
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
    let mut previous_event = None;

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }

        if trimmed == "timeline" || trimmed.starts_with("title ") || trimmed.starts_with("section ")
        {
            continue;
        }

        let Some(event_name) = parse_name_before_colon(trimmed) else {
            builder.add_warning(format!(
                "Line {line_number}: unsupported timeline syntax: {trimmed}"
            ));
            continue;
        };
        let event_id = normalize_identifier(event_name);
        if event_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: timeline event identifier could not be derived: {trimmed}"
            ));
            continue;
        }

        let span = span_for(line_number, line);
        let current_event = builder.intern_node(&event_id, Some(event_name), NodeShape::Rect, span);
        if let (Some(prev), Some(current)) = (previous_event, current_event) {
            builder.push_edge(prev, current, ArrowType::Line, None, span);
        }
        if current_event.is_some() {
            previous_event = current_event;
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

        let task_id = normalize_identifier(task_name);
        if task_id.is_empty() {
            builder.add_warning(format!(
                "Line {line_number}: task identifier could not be derived: {trimmed}"
            ));
            continue;
        }

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
        if let Some(rest_after_id) = remaining.strip_prefix("id:") {
            if let Some((value, rest)) = extract_quoted_value(rest_after_id.trim_start()) {
                options.id = Some(value);
                remaining = rest;
                continue;
            }
        }

        // Try to match msg: "value"
        if let Some(rest_after_msg) = remaining.strip_prefix("msg:") {
            if let Some((value, rest)) = extract_quoted_value(rest_after_msg.trim_start()) {
                options.msg = Some(value);
                remaining = rest;
                continue;
            }
        }

        // Try to match tag: "value"
        if let Some(rest_after_tag) = remaining.strip_prefix("tag:") {
            if let Some((value, rest)) = extract_quoted_value(rest_after_tag.trim_start()) {
                options.tag = Some(value);
                remaining = rest;
                continue;
            }
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

        apply_init_value(parsed_value, line_number, span, builder);
    }
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

fn apply_init_value(value: Value, line_number: usize, span: Span, builder: &mut IrBuilder) {
    let Some(init_object) = value.as_object() else {
        let message = format!("Line {line_number}: init directive must be a JSON object");
        builder.add_warning(message.clone());
        builder.add_init_error(message, span);
        return;
    };

    if let Some(theme) = init_object.get("theme").and_then(Value::as_str) {
        builder.set_init_theme(theme.to_string());
    }

    if let Some(theme_variables) = init_object.get("themeVariables") {
        if let Some(theme_variables_obj) = theme_variables.as_object() {
            for (key, raw_value) in theme_variables_obj {
                if let Some(parsed_value) = json_value_to_string(raw_value) {
                    builder.insert_theme_variable(key.clone(), parsed_value);
                } else {
                    let message = format!(
                        "Line {line_number}: init.themeVariables.{key} must be a string, number, or boolean"
                    );
                    builder.add_warning(message.clone());
                    builder.add_init_warning(message, span);
                }
            }
        } else {
            let message = format!("Line {line_number}: init.themeVariables must be an object");
            builder.add_warning(message.clone());
            builder.add_init_warning(message, span);
        }
    }

    if let Some(flowchart_value) = init_object.get("flowchart") {
        if let Some(flowchart_obj) = flowchart_value.as_object() {
            let direction = flowchart_obj
                .get("direction")
                .or_else(|| flowchart_obj.get("rankDir"))
                .and_then(Value::as_str);
            if let Some(direction_token) = direction {
                if let Some(parsed_direction) = parse_direction_token(direction_token) {
                    builder.set_init_flowchart_direction(parsed_direction);
                } else {
                    let message = format!(
                        "Line {line_number}: unsupported init.flowchart.direction value: {direction_token}"
                    );
                    builder.add_warning(message.clone());
                    builder.add_init_warning(message, span);
                }
            }
        } else {
            let message = format!("Line {line_number}: init.flowchart must be an object");
            builder.add_warning(message.clone());
            builder.add_init_warning(message, span);
        }
    }
}

fn json_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_direction_token(token: &str) -> Option<GraphDirection> {
    match token.trim().to_ascii_uppercase().as_str() {
        "LR" => Some(GraphDirection::LR),
        "RL" => Some(GraphDirection::RL),
        "TB" => Some(GraphDirection::TB),
        "TD" => Some(GraphDirection::TD),
        "BT" => Some(GraphDirection::BT),
        _ => None,
    }
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

pub(crate) fn first_significant_line(input: &str) -> Option<&str> {
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
    fn timeline_parses_events_as_linear_edges() {
        let parsed = parse_mermaid("timeline\n2025 : kickoff\n2026 : launch");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Timeline);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
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
