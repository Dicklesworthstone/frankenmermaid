use fm_core::{ArrowType, DiagramType, NodeShape, Span};
use unicode_segmentation::UnicodeSegmentation;

use crate::{DetectionMethod, ParseResult, ir_builder::IrBuilder};

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
    let normalized_body = normalize_dot_body(body);
    let mut active_clusters: Vec<usize> = Vec::new();

    for (index, line) in normalized_body.lines().enumerate() {
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
            let close_count = statement.chars().take_while(|ch| *ch == '}').count();
            for _ in 0..close_count {
                let _ = active_clusters.pop();
            }
            let statement = statement.trim_start_matches('}').trim();
            if statement.is_empty() {
                continue;
            }
            if statement == "{" {
                continue;
            }

            if let Some((cluster_key, cluster_title, opens_scope)) =
                parse_subgraph_start(statement, line_number)
            {
                if let Some(cluster_index) = builder.ensure_cluster(
                    &cluster_key,
                    cluster_title.as_deref(),
                    span_for(line_number, line),
                ) && opens_scope
                {
                    active_clusters.push(cluster_index);
                }
                continue;
            }

            if parse_dot_edge_statement(
                statement,
                directed,
                line_number,
                line,
                &active_clusters,
                &mut builder,
            ) {
                continue;
            }
            if parse_dot_node_statement(
                statement,
                line_number,
                line,
                &active_clusters,
                &mut builder,
            ) {
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

    builder.finish(0.95, DetectionMethod::DotFormat)
}

fn parse_dot_edge_statement(
    statement: &str,
    directed: bool,
    line_number: usize,
    source_line: &str,
    active_clusters: &[usize],
    builder: &mut IrBuilder,
) -> bool {
    let operator = if statement.contains("->") {
        "->"
    } else if statement.contains("--") {
        "--"
    } else {
        return false;
    };

    let mut parts: Vec<&str> = statement
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

    // Extract shared attributes from the last part
    let Some(last_part) = parts.last_mut() else {
        return false;
    };
    let (last_fragment, shared_attrs) = split_endpoint_and_attrs(last_part);
    *last_part = last_fragment;

    let edge_label_str = shared_attrs.and_then(parse_dot_label);

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
            builder.push_edge(from_id, to_id, arrow, edge_label_str.as_deref(), span);
            for &cluster_index in active_clusters {
                builder.add_node_to_cluster(cluster_index, from_id);
                builder.add_node_to_cluster(cluster_index, to_id);
            }
        }
    }

    true
}

fn parse_dot_node_statement(
    statement: &str,
    line_number: usize,
    source_line: &str,
    active_clusters: &[usize],
    builder: &mut IrBuilder,
) -> bool {
    let Some(node) = parse_dot_node_fragment(statement) else {
        return false;
    };
    let span = span_for(line_number, source_line);
    let node_id = builder.intern_node(&node.id, node.label.as_deref(), NodeShape::Rect, span);
    if let Some(node_id) = node_id {
        for &cluster_index in active_clusters {
            builder.add_node_to_cluster(cluster_index, node_id);
        }
    }
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

fn split_endpoint_and_attrs(fragment: &str) -> (&str, Option<&str>) {
    let trimmed = fragment.trim();
    let Some(open_idx) = trimmed.find('[') else {
        return (trimmed, None);
    };
    let Some(close_idx) = trimmed.rfind(']') else {
        return (trimmed, None);
    };
    if close_idx <= open_idx {
        return (trimmed, None);
    }

    let endpoint = trimmed[..open_idx].trim();
    let attrs = trimmed[open_idx + 1..close_idx].trim();
    (endpoint, Some(attrs))
}

fn parse_dot_label(attributes: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let label_idx = lower.find("label")?;
    let after_label = attributes[label_idx + "label".len()..].trim_start();
    let value = after_label.strip_prefix('=')?.trim_start();

    if let Some(quoted) = value.strip_prefix('"') {
        let end = find_unescaped_quote_end(quoted)?;
        let text = decode_escapes(quoted[..end].trim());
        return (!text.is_empty()).then_some(text);
    }

    if value.starts_with('<') {
        let end = value.rfind('>')?;
        let text = strip_html_tags(&value[..=end]);
        return (!text.is_empty()).then_some(text);
    }

    let token = value
        .split([',', ']'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches('"');
    let token = decode_escapes(token);
    (!token.is_empty()).then_some(token)
}

fn find_unescaped_quote_end(input: &str) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(idx);
        }
    }
    None
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

fn strip_comments(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        // DOT format macro preprocessor lines
        return "";
    }

    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for (i, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(quote) = in_quote {
            if ch == quote {
                in_quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
            continue;
        }
        if line[i..].starts_with("//") || line[i..].starts_with("/*") {
            return &line[..i];
        }
    }

    line
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

fn parse_subgraph_start(
    statement: &str,
    line_number: usize,
) -> Option<(String, Option<String>, bool)> {
    let body = if let Some(rest) = statement.strip_prefix("subgraph ") {
        rest
    } else if statement == "subgraph" {
        ""
    } else {
        return None;
    };
    let opens_scope = true;
    let body = body.trim().trim_end_matches('{').trim();

    let key = if body.is_empty() {
        format!("cluster_anon_line_{line_number}")
    } else {
        normalize_identifier(body)
    };

    if key.is_empty() {
        return None;
    }
    let title = clean_optional(body);
    Some((key, title, opens_scope))
}

fn normalize_dot_body(body: &str) -> String {
    let mut output = String::with_capacity(body.len().saturating_mul(2));
    let mut quote_char: Option<char> = None;
    let mut escaped = false;

    for ch in body.chars() {
        if let Some(active_quote) = quote_char {
            output.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == active_quote {
                quote_char = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote_char = Some(ch);
                output.push(ch);
            }
            '{' | '}' => {
                output.push(';');
                output.push(ch);
                output.push(';');
            }
            _ => output.push(ch),
        }
    }

    output
}

fn clean_optional(raw: &str) -> Option<String> {
    let cleaned = raw.trim().trim_matches('"').trim_matches('\'').trim();
    (!cleaned.is_empty()).then_some(cleaned.to_string())
}

fn decode_escapes(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            let decoded = match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                '\'' => '\'',
                other => other,
            };
            output.push(decoded);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
        } else {
            output.push(ch);
        }
    }

    if escaped {
        output.push('\\');
    }
    output
}

fn strip_html_tags(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut in_tag = false;

    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }

    output.trim().to_string()
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
    fn parses_edge_labels() {
        let parsed = parse_dot("digraph G { a -> b [label=\"connects\"]; }");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "connects");
    }

    #[test]
    fn parses_node_labels_from_attributes() {
        let parsed = parse_dot("graph G { a [label=\"Alpha\"]; a -- b; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "Alpha");
    }

    #[test]
    fn parses_clusters_from_subgraph_blocks() {
        let parsed = parse_dot("digraph G { subgraph cluster_0 { a; b; } a -> b; }");
        assert_eq!(parsed.ir.clusters.len(), 1);
        assert_eq!(parsed.ir.clusters[0].members.len(), 2);
    }

    #[test]
    fn parses_html_labels() {
        let parsed = parse_dot("digraph G { a [label=<b>Alpha</b>]; }");
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "Alpha");
    }

    #[test]
    fn parses_escaped_labels() {
        let parsed = parse_dot("digraph G { a [label=\"Line\\nBreak\"]; }");
        assert_eq!(parsed.ir.labels.len(), 1);
        assert!(parsed.ir.labels[0].text.contains('\n'));
    }

    #[test]
    fn does_not_strip_comment_markers_inside_quoted_labels() {
        let parsed = parse_dot("digraph G { a [label=\"Bob's // car\"]; }");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "Bob's // car");
    }

    #[test]
    fn parses_escaped_quotes_inside_labels() {
        let parsed = parse_dot("digraph G { a [label=\"a \\\"b\\\" c\"]; }");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "a \"b\" c");
    }
}

#[test]
fn parses_semicolon_in_label() {
    let input = r#"digraph G { A -> B [label="foo; bar"]; }"#;
    let result = parse_dot(input);
    let edge = &result.ir.edges[0];
    let label = result.ir.labels[edge.label.unwrap().0].text.clone();
    assert_eq!(label, "foo; bar");
}
