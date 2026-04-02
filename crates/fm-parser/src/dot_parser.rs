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
    let body_without_comments = strip_all_comments(body);
    let expanded_groups = expand_edge_groups(&body_without_comments);
    let normalized_body = normalize_dot_body(&expanded_groups);
    let mut active_clusters: Vec<usize> = Vec::new();
    let mut active_subgraphs: Vec<usize> = Vec::new();

    for (index, line) in normalized_body.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for statement in split_dot_by(trimmed, ";") {
            let close_count = statement.chars().take_while(|ch| *ch == '}').count();
            for _ in 0..close_count {
                let _ = active_clusters.pop();
                let _ = active_subgraphs.pop();
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
                // Use the cluster_key directly for named clusters to allow merging.
                // For anonymous ones, the key already includes the line number.
                let lookup_key = cluster_key.clone();

                if let Some(cluster_index) = builder.ensure_cluster(
                    &lookup_key,
                    cluster_title.as_deref(),
                    span_for(line_number, line),
                ) {
                    let parent_subgraph = active_subgraphs.last().copied();
                    let subgraph_index = builder.ensure_subgraph(
                        &lookup_key,
                        &cluster_key,
                        cluster_title.as_deref(),
                        span_for(line_number, line),
                        parent_subgraph,
                        Some(cluster_index),
                    );
                    if opens_scope {
                        active_clusters.push(cluster_index);
                        if let Some(subgraph_index) = subgraph_index {
                            active_subgraphs.push(subgraph_index);
                        }
                    }
                }
                continue;
            }

            if parse_dot_edge_statement(
                statement,
                directed,
                line_number,
                line,
                &active_clusters,
                &active_subgraphs,
                &mut builder,
            ) {
                continue;
            }
            if parse_dot_node_statement(
                statement,
                line_number,
                line,
                &active_clusters,
                &active_subgraphs,
                &mut builder,
            ) {
                continue;
            }

            // Handle graph/edge/node default attribute statements.
            // These are valid DOT but we parse-and-skip them (no runtime behavior yet).
            let lower = statement.trim().to_ascii_lowercase();
            if lower.starts_with("graph ")
                || lower.starts_with("graph[")
                || lower.starts_with("graph\t")
                || lower.starts_with("edge ")
                || lower.starts_with("edge[")
                || lower.starts_with("edge\t")
                || lower.starts_with("node ")
                || lower.starts_with("node[")
                || lower.starts_with("node\t")
            {
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

fn strip_all_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut in_multiline_comment = false;
    let mut in_singleline_comment = false;
    let mut html_depth = 0_usize;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if in_multiline_comment {
            if c == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                in_multiline_comment = false;
                i += 2;
            } else {
                if c == '\n' {
                    output.push('\n');
                }
                i += 1;
            }
            continue;
        }

        if in_singleline_comment {
            if c == '\n' {
                in_singleline_comment = false;
                output.push('\n');
            }
            i += 1;
            continue;
        }

        if let Some(q) = in_quote {
            output.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        // Only start comments if not inside an HTML label
        if html_depth == 0 {
            if c == '/' && i + 1 < chars.len() {
                if chars[i + 1] == '/' {
                    in_singleline_comment = true;
                    i += 2;
                    continue;
                } else if chars[i + 1] == '*' {
                    in_multiline_comment = true;
                    i += 2;
                    continue;
                }
            }

            // DOT considers # a comment if it is the first non-whitespace character on a line.
            if c == '#' {
                // Check if only whitespace precedes it on this line
                let mut is_start_of_line = true;
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    if chars[j] == '\n' {
                        break;
                    }
                    if !chars[j].is_whitespace() {
                        is_start_of_line = false;
                        break;
                    }
                }
                if is_start_of_line {
                    in_singleline_comment = true;
                    i += 1;
                    continue;
                }
            }
        }

        if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c == '<' {
            html_depth = html_depth.saturating_add(1);
        } else if c == '>' {
            html_depth = html_depth.saturating_sub(1);
        }

        output.push(c);
        i += 1;
    }
    output
}

fn parse_dot_edge_statement(
    statement: &str,
    directed: bool,
    line_number: usize,
    source_line: &str,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
    builder: &mut IrBuilder,
) -> bool {
    let operator = if statement.contains("->") {
        "->"
    } else if statement.contains("--") {
        "--"
    } else {
        return false;
    };

    let mut parts: Vec<&str> = split_dot_by(statement, operator);
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

    // Edge groups (A -> {B C D}) are expanded in expand_edge_groups() before
    // normalization, so they arrive here as individual "A -> B", "A -> C" etc.
    for window in parts.windows(2) {
        let from_text = window[0].trim();
        let to_text = window[1].trim();

        let Some(from_node) = parse_dot_node_fragment(from_text) else {
            builder.add_warning(format!(
                "Line {line_number}: invalid DOT edge source: {from_text}"
            ));
            continue;
        };
        let Some(to_node) = parse_dot_node_fragment(to_text) else {
            builder.add_warning(format!(
                "Line {line_number}: invalid DOT edge target: {to_text}"
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
            add_node_to_active_groups(builder, active_clusters, active_subgraphs, from_id);
            add_node_to_active_groups(builder, active_clusters, active_subgraphs, to_id);
        }
    }

    true
}

fn parse_dot_node_statement(
    statement: &str,
    line_number: usize,
    source_line: &str,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
    builder: &mut IrBuilder,
) -> bool {
    let Some(node) = parse_dot_node_fragment(statement) else {
        return false;
    };
    let span = span_for(line_number, source_line);
    let node_id = builder.intern_node(&node.id, node.label.as_deref(), node.shape, span);
    if let Some(node_id) = node_id {
        add_node_to_active_groups(builder, active_clusters, active_subgraphs, node_id);
    }
    true
}

fn add_node_to_active_groups(
    builder: &mut IrBuilder,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
    node_id: fm_core::IrNodeId,
) {
    for &cluster_index in active_clusters {
        builder.add_node_to_cluster(cluster_index, node_id);
    }
    for &subgraph_index in active_subgraphs {
        builder.add_node_to_subgraph(subgraph_index, node_id);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DotNode {
    id: String,
    label: Option<String>,
    shape: NodeShape,
}

fn parse_dot_node_fragment(raw: &str) -> Option<DotNode> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "{" || trimmed == "}" {
        return None;
    }

    let (id_part, attrs) = split_endpoint_and_attrs(trimmed);
    // Strip DOT port/compass suffixes: "node:port:n" → "node".
    // Ports use colon syntax: id:port or id:port:compass.
    let id_without_port = id_part.split(':').next().unwrap_or(id_part);
    let id = normalize_identifier(id_without_port);
    if id.is_empty() {
        return None;
    }

    let label = attrs.and_then(parse_dot_label);
    let shape = attrs.and_then(parse_dot_shape).unwrap_or(NodeShape::Rect);

    Some(DotNode { id, label, shape })
}

fn extract_dot_attribute_raw(attributes: &str, key: &str) -> Option<String> {
    let lower_key = key.to_ascii_lowercase();
    let mut in_quote = false;
    let mut html_depth = 0;
    let mut escaped = false;
    let mut current_key = String::new();
    let mut current_val = String::new();
    let mut parsing_key = true;

    for ch in attributes.chars() {
        if escaped {
            if parsing_key {
                current_key.push(ch);
            } else {
                current_val.push(ch);
            }
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            if parsing_key {
                current_key.push(ch);
            } else {
                current_val.push(ch);
            }
            continue;
        }

        if ch == '"' && html_depth == 0 {
            in_quote = !in_quote;
            if parsing_key {
                current_key.push(ch);
            } else {
                current_val.push(ch);
            }
            continue;
        }

        if !in_quote {
            if ch == '<' {
                html_depth += 1;
            } else if ch == '>' && html_depth > 0 {
                html_depth -= 1;
            }
        }

        if !in_quote
            && html_depth == 0
            && (ch == ',' || ch == ']' || ch == '[' || ch == ' ' || ch == '\t')
        {
            if !current_key.trim().is_empty() && current_key.trim().eq_ignore_ascii_case(&lower_key)
            {
                return Some(current_val.trim().to_string());
            }
            if !parsing_key || !current_key.trim().is_empty() {
                current_key.clear();
                current_val.clear();
                parsing_key = true;
            }
            continue;
        }

        if !in_quote && html_depth == 0 && ch == '=' && parsing_key {
            parsing_key = false;
            continue;
        }

        if parsing_key {
            current_key.push(ch);
        } else {
            current_val.push(ch);
        }
    }

    if current_key.trim().eq_ignore_ascii_case(&lower_key) {
        return Some(current_val.trim().to_string());
    }
    None
}

/// Extract `shape=...` from DOT attribute list and map to `NodeShape`.
fn parse_dot_shape(attributes: &str) -> Option<NodeShape> {
    let value = extract_dot_attribute_raw(attributes, "shape")?;
    let token = value.trim().trim_matches('"').to_ascii_lowercase();
    dot_shape_to_node_shape(&token)
}

/// Map DOT shape names to frankenmermaid `NodeShape`.
fn dot_shape_to_node_shape(name: &str) -> Option<NodeShape> {
    Some(match name {
        "box" | "rect" | "rectangle" | "square" => NodeShape::Rect,
        "roundedbox" | "rounded" => NodeShape::Rounded,
        "diamond" => NodeShape::Diamond,
        "circle" | "point" | "doublecircle" => NodeShape::Circle,
        "hexagon" => NodeShape::Hexagon,
        "trapezium" => NodeShape::Trapezoid,
        "invtrapezium" => NodeShape::InvTrapezoid,
        "parallelogram" => NodeShape::Parallelogram,
        "triangle" | "invtriangle" => NodeShape::Triangle,
        "pentagon" => NodeShape::Pentagon,
        "star" => NodeShape::Star,
        "cylinder" => NodeShape::Cylinder,
        "note" | "tab" => NodeShape::Note,
        "cds" | "component" => NodeShape::Subroutine,
        "folder" | "box3d" | "house" | "invhouse" => NodeShape::Rect,
        _ => return None,
    })
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
    let value = extract_dot_attribute_raw(attributes, "label")?;
    let value = value.trim();

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

    let token = value.trim_matches('"');
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
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let (cleaned, was_quoted) = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('`') && trimmed.ends_with('`'))
    {
        if trimmed.len() < 2 {
            (trimmed, false)
        } else {
            (&trimmed[1..trimmed.len() - 1], true)
        }
    } else {
        (trimmed, false)
    };

    if cleaned.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(cleaned.len());
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/') {
            out.push(ch);
        } else if ch.is_whitespace() {
            if !out.is_empty() {
                out.push('_');
            }
        } else if matches!(ch, ':' | ';' | ',') {
            if !out.is_empty() {
                break;
            }
        } else if was_quoted {
            out.push('_');
        } else if !out.is_empty() {
            break;
        }
    }

    let mut result = out.trim_end_matches('_').to_string();
    if result.is_empty() {
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
        result = fallback.trim_matches('_').to_string();
    }
    result
}

fn is_directed_graph(input: &str) -> bool {
    let first_line = input.lines().map(str::trim).find(|line| !line.is_empty());
    first_line.is_some_and(|line| line.to_ascii_lowercase().contains("digraph"))
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

/// Pre-expand DOT edge group syntax: `A -> {B C D}` → `A -> B; A -> C; A -> D`.
/// This must run BEFORE `normalize_dot_body` which inserts semicolons around braces.
fn expand_edge_groups(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(brace_start) = rest.find('{') {
        // Check if this brace is preceded by an edge operator (-> or --)
        let before = &rest[..brace_start];
        let is_edge_group = before.trim_end().ends_with("->") || before.trim_end().ends_with("--");

        if !is_edge_group {
            // Not an edge group — might be a subgraph brace. Pass through.
            output.push_str(&rest[..=brace_start]);
            rest = &rest[brace_start + 1..];
            continue;
        }

        let Some(brace_end) = rest[brace_start..].find('}') else {
            // Unclosed brace — pass through rest.
            output.push_str(rest);
            return output;
        };
        let brace_end = brace_start + brace_end;

        // Extract source node (everything before the operator).
        let operator_end = before.trim_end().len();
        let op_len = 2; // "--" or "->"
        let source = before[..operator_end - op_len].trim();
        let operator = &before.trim_end()[operator_end - op_len..operator_end];

        // Extract group members.
        let inner = rest[brace_start + 1..brace_end].trim();
        let members: Vec<&str> = inner.split_whitespace().filter(|s| !s.is_empty()).collect();

        // Expand: emit "source -> member" for each member.
        for (i, member) in members.iter().enumerate() {
            if i > 0 {
                output.push_str("; ");
            }
            output.push_str(source);
            output.push(' ');
            output.push_str(operator);
            output.push(' ');
            output.push_str(member);
        }

        rest = &rest[brace_end + 1..];
    }

    output.push_str(rest);
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

fn split_dot_by<'a>(line: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut current_start = 0;
    let mut in_quote = false;
    let mut escaped = false;
    let mut html_depth = 0_usize;

    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (byte_idx, c) = chars[i];

        if in_quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_quote = false;
            }
        } else {
            if c == '"' {
                in_quote = true;
            } else if c == '<' {
                html_depth = html_depth.saturating_add(1);
            } else if c == '>' {
                html_depth = html_depth.saturating_sub(1);
            } else if html_depth == 0 && line[byte_idx..].starts_with(separator) {
                parts.push(line[current_start..byte_idx].trim());
                current_start = byte_idx + separator.len();
                let sep_chars = separator.chars().count();
                i += sep_chars.saturating_sub(1);
            }
        }
        i += 1;
    }

    if current_start < line.len() {
        parts.push(line[current_start..].trim());
    }
    parts.into_iter().filter(|s| !s.is_empty()).collect()
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
        assert_eq!(parsed.ir.graph.subgraphs.len(), 1);
        assert_eq!(parsed.ir.graph.clusters.len(), 1);
        assert_eq!(
            parsed.ir.graph.subgraphs[0].cluster,
            Some(fm_core::IrClusterId(0))
        );
        assert_eq!(parsed.ir.graph.subgraphs[0].members.len(), 2);
    }

    #[test]
    fn duplicate_dot_subgraph_keys_merge_into_single_group() {
        let parsed =
            parse_dot("digraph G { subgraph cluster_0 { a; } subgraph cluster_0 { b; } a -> b; }");

        // Should now only have 1 cluster and 1 subgraph entry (merged)
        assert_eq!(parsed.ir.clusters.len(), 1);
        assert_eq!(parsed.ir.graph.subgraphs.len(), 1);
        assert_eq!(parsed.ir.graph.subgraphs[0].key, "cluster_0");
        assert_eq!(parsed.ir.graph.subgraphs[0].members.len(), 2);

        let first_member = parsed.ir.nodes[parsed.ir.graph.subgraphs[0].members[0].0]
            .id
            .as_str();
        let second_member = parsed.ir.nodes[parsed.ir.graph.subgraphs[0].members[1].0]
            .id
            .as_str();
        assert_eq!(first_member, "a");
        assert_eq!(second_member, "b");
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
    fn parses_multiple_attribute_blocks() {
        let parsed = parse_dot("digraph G { a [color=red] [label=\"Double\"]; }");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.labels.len(), 1);
        assert_eq!(parsed.ir.labels[0].text, "Double");
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

#[test]
fn dot_port_syntax_stripped_from_node_ids() {
    let input = "digraph G { A:port1 -> B:port2:n; }";
    let result = parse_dot(input);
    assert_eq!(result.ir.edges.len(), 1, "should parse edge");
    let node_ids: Vec<&str> = result.ir.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(
        node_ids.contains(&"A"),
        "node A should exist (port stripped)"
    );
    assert!(
        node_ids.contains(&"B"),
        "node B should exist (port stripped)"
    );
}

#[test]
fn dot_edge_group_expands_to_multiple_edges() {
    let input = "digraph G { A -> {B C D}; }";
    let result = parse_dot(input);
    assert_eq!(
        result.ir.edges.len(),
        3,
        "A -> {{B C D}} should expand to 3 edges, got {} edges, {} nodes, warnings: {:?}",
        result.ir.edges.len(),
        result.ir.nodes.len(),
        result.warnings,
    );
    let node_ids: Vec<&str> = result.ir.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(node_ids.contains(&"A"));
    assert!(node_ids.contains(&"B"));
    assert!(node_ids.contains(&"C"));
    assert!(node_ids.contains(&"D"));
}

#[test]
fn dot_compass_points_stripped() {
    let input = "digraph G { A:n -> B:s; }";
    let result = parse_dot(input);
    assert_eq!(result.ir.edges.len(), 1);
    assert!(result.ir.nodes.iter().any(|n| n.id == "A"));
    assert!(result.ir.nodes.iter().any(|n| n.id == "B"));
}
