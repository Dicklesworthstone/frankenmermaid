use fm_core::{ArrowType, DiagramType, NodeShape, Span};

use crate::{ParseResult, ir_builder::IrBuilder};

#[must_use]
pub fn looks_like_dot(input: &str) -> bool {
    let Some(first_line) = input.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return false;
    };
    let lower = first_line.to_ascii_lowercase();
    if !(lower.starts_with("graph ")
        || lower.starts_with("digraph ")
        || lower.starts_with("strict graph ")
        || lower.starts_with("strict digraph "))
    {
        return false;
    }
    input.contains('{') && input.contains('}')
}

#[must_use]
pub fn parse_dot(input: &str) -> ParseResult {
    let mut builder = IrBuilder::new(DiagramType::Flowchart);
    let directed = is_directed_graph(input);
    let body = extract_body(input);

    for (index, line) in body.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = strip_comments(line).trim();
        if trimmed.is_empty() {
            continue;
        }

        for statement in trimmed
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if parse_dot_edge_statement(statement, directed, line_number, line, &mut builder) {
                continue;
            }
            if parse_dot_node_statement(statement, line_number, line, &mut builder) {
                continue;
            }

            builder.add_warning(format!(
                "Line {line_number}: unsupported DOT statement: {statement}"
            ));
        }
    }

    if builder.node_count() == 0 && builder.edge_count() == 0 {
        builder.add_warning("DOT input contained no parseable nodes or edges");
    }

    builder.finish()
}

fn parse_dot_edge_statement(
    statement: &str,
    directed: bool,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let operator = if statement.contains("->") {
        "->"
    } else if statement.contains("--") {
        "--"
    } else {
        return false;
    };

    let parts: Vec<&str> = statement
        .split(operator)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if parts.len() < 2 {
        return false;
    }

    let arrow = if operator == "->" || directed {
        ArrowType::Arrow
    } else {
        ArrowType::Line
    };
    let span = span_for(line_number, source_line);

    for window in parts.windows(2) {
        let Some(from_node) = parse_dot_node_fragment(window[0]) else {
            builder.add_warning(format!(
                "Line {line_number}: invalid DOT edge source: {}",
                window[0]
            ));
            continue;
        };
        let Some(to_node) = parse_dot_node_fragment(window[1]) else {
            builder.add_warning(format!(
                "Line {line_number}: invalid DOT edge target: {}",
                window[1]
            ));
            continue;
        };

        let from = builder.intern_node(
            &from_node.id,
            from_node.label.as_deref(),
            NodeShape::Rect,
            span,
        );
        let to = builder.intern_node(&to_node.id, to_node.label.as_deref(), NodeShape::Rect, span);

        if let (Some(from_id), Some(to_id)) = (from, to) {
            builder.push_edge(from_id, to_id, arrow, None, span);
        }
    }

    true
}

fn parse_dot_node_statement(
    statement: &str,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) -> bool {
    let Some(node) = parse_dot_node_fragment(statement) else {
        return false;
    };
    let span = span_for(line_number, source_line);
    let _ = builder.intern_node(&node.id, node.label.as_deref(), NodeShape::Rect, span);
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DotNode {
    id: String,
    label: Option<String>,
}

fn parse_dot_node_fragment(raw: &str) -> Option<DotNode> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "{" || trimmed == "}" {
        return None;
    }

    if let Some(open_idx) = trimmed.find('[') {
        let close_idx = trimmed.rfind(']')?;
        let id = normalize_identifier(trimmed[..open_idx].trim());
        if id.is_empty() {
            return None;
        }
        let attrs = &trimmed[open_idx + 1..close_idx];
        return Some(DotNode {
            id,
            label: parse_dot_label(attrs),
        });
    }

    let id_token = trimmed.split_whitespace().next().unwrap_or_default();
    let id = normalize_identifier(id_token);
    if id.is_empty() {
        return None;
    }

    Some(DotNode { id, label: None })
}

fn parse_dot_label(attributes: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let label_idx = lower.find("label")?;
    let after_label = attributes[label_idx + "label".len()..].trim_start();
    let value = after_label.strip_prefix('=')?.trim_start();

    if let Some(quoted) = value.strip_prefix('"') {
        let end = quoted.find('"')?;
        let text = quoted[..end].trim();
        return (!text.is_empty()).then_some(text.to_string());
    }

    let token = value
        .split([',', ']'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches('"');
    (!token.is_empty()).then_some(token.to_string())
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
    out
}

fn strip_comments(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else if let Some(idx) = line.find('#') {
        &line[..idx]
    } else {
        line
    }
}

fn is_directed_graph(input: &str) -> bool {
    let first_line = input.lines().map(str::trim).find(|line| !line.is_empty());
    first_line
        .map(|line| line.to_ascii_lowercase().contains("digraph"))
        .unwrap_or(false)
        || input.contains("->")
}

fn extract_body(input: &str) -> &str {
    let Some(start) = input.find('{') else {
        return input;
    };
    let Some(end) = input.rfind('}') else {
        return &input[start + 1..];
    };
    if end <= start {
        return input;
    }
    &input[start + 1..end]
}

fn span_for(line_number: usize, line: &str) -> Span {
    Span::at_line(line_number, line.chars().count())
}

#[cfg(test)]
mod tests {
    use fm_core::{ArrowType, DiagramType};

    use super::{looks_like_dot, parse_dot};

    #[test]
    fn detects_dot_headers() {
        assert!(looks_like_dot("digraph G { a -> b; }"));
        assert!(looks_like_dot("graph G { a -- b; }"));
        assert!(!looks_like_dot("flowchart LR\nA-->B"));
    }

    #[test]
    fn parses_directed_dot_edges() {
        let parsed = parse_dot("digraph G { a -> b; b -> c; }");
        assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(parsed.ir.nodes.len(), 3);
        assert_eq!(parsed.ir.edges.len(), 2);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn parses_node_labels_from_attributes() {
        let parsed = parse_dot("graph G { a [label=\"Alpha\"]; a -- b; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "Alpha");
    }
}
