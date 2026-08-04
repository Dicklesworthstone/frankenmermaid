use std::{borrow::Cow, iter::Peekable, str::CharIndices};

use fm_core::{ArrowType, DiagramType, NodeShape, Span};
use memchr::memchr2;
use unicode_segmentation::UnicodeSegmentation;

use crate::{DetectionMethod, ParseResult, ir_builder::IrBuilder};

#[must_use]
pub fn looks_like_dot(input: &str) -> bool {
    // DOT graphs are brace-delimited, so an input with no braces cannot be DOT. Bail
    // before `strip_all_comments`, which collects the whole input into a `Vec<char>` and
    // rescans it — wasteful on every parse of the common Mermaid flowchart (no braces).
    // Output-identical: comment stripping only removes characters, so a brace in the
    // cleaned text implies a brace in the raw input.
    let bytes = input.as_bytes();
    if !bytes.contains(&b'{') || !bytes.contains(&b'}') {
        return false;
    }
    // Every DOT header is `graph` / `digraph` / `strict [di]graph` — all contain "graph", and DOT
    // keywords are case-insensitive (`dot_header_kind` lowercases the first line). So a real DOT file
    // ALWAYS contains "graph" somewhere in its raw text. Class/state diagrams have `{ }` braces but no
    // `graph` keyword, so this cheap substring pre-guard short-circuits the expensive
    // `strip_all_comments` (whole-input `Vec<char>` collect + rescan) that dominated their detection.
    // Output-identical: comment stripping never introduces a `graph` substring that wasn't there.
    if !contains_ignore_ascii_case(bytes, b"graph") {
        return false;
    }
    let cleaned = strip_all_comments_cow(input);
    if dot_header_kind(cleaned.as_ref()).is_none() {
        return false;
    }
    cleaned.contains('{') && cleaned.contains('}')
}

/// Case-insensitive ASCII substring test (`needle` is a short ASCII literal). Scans byte windows,
/// short-circuiting on the first match; each window compare rejects on the first differing byte.
fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[must_use]
pub fn parse_dot(input: &str) -> ParseResult {
    let mut builder = IrBuilder::new(DiagramType::Flowchart);
    let cleaned = strip_all_comments_cow(input);
    let cleaned = cleaned.as_ref();
    let directed = is_directed_graph_cleaned(cleaned);
    let body = extract_body(cleaned);
    let normalized_body_storage;
    let normalized_body = if dot_body_needs_normalization(body) {
        let expanded_groups = expand_edge_groups(body);
        normalized_body_storage = normalize_dot_body(&expanded_groups);
        normalized_body_storage.as_str()
    } else {
        body
    };
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

fn strip_all_comments_cow(input: &str) -> Cow<'_, str> {
    if memchr2(b'/', b'#', input.as_bytes()).is_none() {
        return Cow::Borrowed(input);
    }
    Cow::Owned(strip_all_comments_slow(input))
}

fn strip_all_comments_slow(input: &str) -> String {
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

fn dot_header_kind(cleaned_input: &str) -> Option<bool> {
    let first_line = cleaned_input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let lower = first_line.to_ascii_lowercase();
    let mut cursor = lower.as_str();
    if let Some(rest) = cursor.strip_prefix("strict") {
        if rest.is_empty() || !rest.chars().next().is_some_and(char::is_whitespace) {
            return None;
        }
        cursor = rest.trim_start();
    }
    if starts_with_keyword(cursor, "digraph") {
        return Some(true);
    }
    if starts_with_keyword(cursor, "graph") {
        return Some(false);
    }
    None
}

fn starts_with_keyword(line: &str, keyword: &str) -> bool {
    let Some(rest) = line.strip_prefix(keyword) else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    rest.chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace() || ch == '{')
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
    let Some(operator) = find_edge_operator(statement) else {
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

fn find_edge_operator(statement: &str) -> Option<&'static str> {
    let mut in_quote: Option<u8> = None;
    let mut escaped = false;
    let mut html_depth = 0_usize;

    let bytes = statement.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = bytes[i];

        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        if c == b'"' || c == b'\'' {
            in_quote = Some(c);
            i += 1;
            continue;
        }

        if c == b'<' {
            html_depth = html_depth.saturating_add(1);
            i += 1;
            continue;
        }
        if c == b'>' {
            html_depth = html_depth.saturating_sub(1);
            i += 1;
            continue;
        }

        if html_depth == 0 && c == b'-' {
            match bytes[i + 1] {
                b'>' => return Some("->"),
                b'-' => return Some("--"),
                _ => {}
            }
        }

        i += 1;
    }

    None
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

    let (label, shape) = attrs.map_or((None, NodeShape::Rect), parse_dot_node_attributes);

    Some(DotNode { id, label, shape })
}

struct DotAttributeIter<'a> {
    attributes: &'a str,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> DotAttributeIter<'a> {
    fn new(attributes: &'a str) -> Self {
        Self {
            attributes,
            chars: attributes.char_indices().peekable(),
        }
    }
}

impl<'a> Iterator for DotAttributeIter<'a> {
    type Item = (&'a str, Cow<'a, str>);

    fn next(&mut self) -> Option<Self::Item> {
        let attributes = self.attributes;

        loop {
            let (key_start, ch) = self.chars.next()?;
            if ch.is_whitespace() || ch == '[' || ch == ']' || ch == ',' {
                continue;
            }

            let mut key_end = attributes.len();
            while let Some(&(idx, c)) = self.chars.peek() {
                if c == '=' || c.is_whitespace() || c == '[' || c == ']' || c == ',' {
                    key_end = idx;
                    break;
                }
                self.chars.next();
            }
            let current_key = &attributes[key_start..key_end];

            while let Some(&(_, c)) = self.chars.peek() {
                if c.is_whitespace() {
                    self.chars.next();
                } else {
                    break;
                }
            }

            let mut has_eq = false;
            if let Some(&(_, '=')) = self.chars.peek() {
                has_eq = true;
                self.chars.next();
                while let Some(&(_, c)) = self.chars.peek() {
                    if c.is_whitespace() {
                        self.chars.next();
                    } else {
                        break;
                    }
                }
            }

            let mut current_val: Cow<'_, str> = Cow::Borrowed("");
            if has_eq && let Some(&(val_start, c)) = self.chars.peek() {
                if c == '"' {
                    self.chars.next();
                    let mut escaped = false;
                    let mut close_idx = None;
                    while let Some(&(idx, vc)) = self.chars.peek() {
                        if escaped {
                            escaped = false;
                            self.chars.next();
                        } else if vc == '\\' {
                            escaped = true;
                            self.chars.next();
                        } else if vc == '"' {
                            close_idx = Some(idx);
                            self.chars.next();
                            break;
                        } else {
                            self.chars.next();
                        }
                    }
                    current_val = match close_idx {
                        Some(ci) => Cow::Borrowed(&attributes[val_start..ci + 1]),
                        None => Cow::Owned(format!("{}\"", &attributes[val_start..])),
                    };
                } else {
                    let mut html_depth = 0;
                    let mut val_end = attributes.len();
                    while let Some(&(idx, vc)) = self.chars.peek() {
                        if vc == '<' {
                            html_depth += 1;
                        } else if vc == '>' && html_depth > 0 {
                            html_depth -= 1;
                        }

                        if html_depth == 0 && (vc.is_whitespace() || vc == ',' || vc == ']') {
                            val_end = idx;
                            break;
                        }
                        self.chars.next();
                    }
                    current_val = Cow::Borrowed(&attributes[val_start..val_end]);
                }
            }

            return Some((current_key, current_val));
        }
    }
}

fn parse_dot_node_attributes(attributes: &str) -> (Option<String>, NodeShape) {
    let mut label_value = None;
    let mut shape_value = None;

    for (key, value) in DotAttributeIter::new(attributes) {
        if label_value.is_none() && key.eq_ignore_ascii_case("label") {
            label_value = Some(value);
        } else if shape_value.is_none() && key.eq_ignore_ascii_case("shape") {
            shape_value = Some(value);
        }
        if label_value.is_some() && shape_value.is_some() {
            break;
        }
    }

    let label = label_value.as_deref().and_then(parse_dot_label_value);
    let shape = shape_value
        .as_deref()
        .and_then(parse_dot_shape_value)
        .unwrap_or(NodeShape::Rect);
    (label, shape)
}

fn parse_dot_shape_value(value: &str) -> Option<NodeShape> {
    let shape_name = value.trim().trim_matches('"').to_ascii_lowercase();
    dot_shape_to_node_shape(&shape_name)
}

fn extract_dot_attribute_raw<'a>(attributes: &'a str, key: &str) -> Option<Cow<'a, str>> {
    DotAttributeIter::new(attributes)
        .find_map(|(current_key, value)| current_key.eq_ignore_ascii_case(key).then_some(value))
}

#[cfg(test)]
fn parse_dot_shape(attributes: &str) -> Option<NodeShape> {
    let value = extract_dot_attribute_raw(attributes, "shape")?;
    parse_dot_shape_value(value.as_ref())
}

/// Map DOT shape names to frankenmermaid `NodeShape`.
fn dot_shape_to_node_shape(name: &str) -> Option<NodeShape> {
    Some(match name {
        "box" | "rect" | "rectangle" | "square" | "folder" | "box3d" | "house" | "invhouse" => {
            NodeShape::Rect
        }
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
        _ => return None,
    })
}

fn split_endpoint_and_attrs(fragment: &str) -> (&str, Option<&str>) {
    let trimmed = fragment.trim();
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut html_depth = 0_usize;
    let mut open_idx: Option<usize> = None;
    let mut close_idx: Option<usize> = None;

    for (idx, ch) in trimmed.char_indices() {
        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                in_quote = None;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
            continue;
        }

        if ch == '<' {
            html_depth = html_depth.saturating_add(1);
            continue;
        }
        if ch == '>' {
            html_depth = html_depth.saturating_sub(1);
            continue;
        }

        if html_depth == 0 {
            if ch == '[' && open_idx.is_none() {
                open_idx = Some(idx);
                continue;
            }
            if ch == ']' && open_idx.is_some() {
                close_idx = Some(idx);
            }
        }
    }

    let Some(open_idx) = open_idx else {
        return (trimmed, None);
    };
    let Some(close_idx) = close_idx else {
        return (trimmed, None);
    };
    if close_idx <= open_idx {
        return (trimmed, None);
    }

    let endpoint = trimmed[..open_idx].trim();
    let attrs = trimmed[open_idx + 1..close_idx].trim();
    (endpoint, Some(attrs))
}

fn parse_dot_label_value(value: &str) -> Option<String> {
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

    let raw_label = value.trim_matches('"');
    let decoded_label = decode_escapes(raw_label);
    (!decoded_label.is_empty()).then_some(decoded_label)
}

fn parse_dot_label(attributes: &str) -> Option<String> {
    let value = extract_dot_attribute_raw(attributes, "label")?;
    parse_dot_label_value(value.as_ref())
}

#[cfg(test)]
fn parse_dot_node_attributes_sequential(attributes: &str) -> (Option<String>, NodeShape) {
    let label = parse_dot_label(attributes);
    let shape = if contains_ignore_ascii_case(attributes.as_bytes(), b"shape") {
        parse_dot_shape(attributes)
    } else {
        None
    }
    .unwrap_or(NodeShape::Rect);
    (label, shape)
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

    // Fast path (parity with the canonical `lib.rs::normalize_identifier`): an identifier already made
    // up entirely of the bytes the loop below keeps verbatim (ASCII alphanumerics + `_ - . /`) with no
    // trailing `_` (so `trim_end_matches('_')` is a no-op) normalizes to ITSELF — the overwhelmingly
    // common case for generated/most DOT node ids. Return one owned copy and skip the char-by-char
    // rebuild. Byte-identical: the loop pushes each such char unchanged and the trim/fallback leave it
    // as-is; a non-ASCII byte fails `is_ascii_alphanumeric`, correctly deferring to the slow path.
    let cleaned_bytes = cleaned.as_bytes();
    if cleaned_bytes[cleaned_bytes.len() - 1] != b'_'
        && cleaned_bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/'))
    {
        return cleaned.to_owned();
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

    // Drop trailing `_` in place instead of `out.trim_end_matches('_').to_string()`, which
    // allocates a second String and copies the whole id. `_` is single-byte ASCII, so the trimmed
    // byte length is a valid truncation boundary — byte-identical, and a no-op when there is no
    // trailing `_` (the common case for well-formed DOT ids), reusing `out`'s allocation.
    let trimmed_len = out.trim_end_matches('_').len();
    out.truncate(trimmed_len);
    let mut result = out;
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
    if result.is_empty() {
        result = format!("id_{:x}", fnv1a_hash(cleaned.as_bytes()));
    }
    result
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn is_directed_graph_cleaned(cleaned_input: &str) -> bool {
    if let Some(is_directed) = dot_header_kind(cleaned_input) {
        return is_directed;
    }

    let body = extract_body(cleaned_input);
    contains_directed_edge_operator(body)
}

fn contains_directed_edge_operator(input: &str) -> bool {
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut html_depth = 0_usize;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i + 1 < chars.len() {
        let c = chars[i];

        if let Some(q) = in_quote {
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

        if c == '"' || c == '\'' {
            in_quote = Some(c);
            i += 1;
            continue;
        }

        if c == '<' {
            html_depth = html_depth.saturating_add(1);
            i += 1;
            continue;
        }
        if c == '>' {
            html_depth = html_depth.saturating_sub(1);
            i += 1;
            continue;
        }

        if html_depth == 0 && c == '-' && chars[i + 1] == '>' {
            return true;
        }

        i += 1;
    }

    false
}

fn extract_body(input: &str) -> &str {
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut html_depth = 0_usize;

    for (idx, ch) in input.char_indices() {
        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                in_quote = None;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
            continue;
        }
        if ch == '<' {
            html_depth = html_depth.saturating_add(1);
            continue;
        }
        if ch == '>' {
            html_depth = html_depth.saturating_sub(1);
            continue;
        }
        if html_depth > 0 {
            continue;
        }

        if ch == '{' {
            start.get_or_insert(idx);
        } else if ch == '}' && start.is_some() {
            end = Some(idx);
        }
    }

    let Some(start_idx) = start else {
        return input;
    };
    let end_idx = end.unwrap_or(input.len());
    if end_idx <= start_idx {
        return input;
    }
    &input[start_idx + 1..end_idx]
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

fn dot_body_needs_normalization(body: &str) -> bool {
    memchr2(b'{', b'}', body.as_bytes()).is_some()
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

/// Split a string on whitespace while respecting quoted sections.
///
/// Handles both double and single quotes. Quoted strings are preserved intact.
/// For example: `"node 1" B 'node 2'` → `["\"node 1\"", "B", "'node 2'"]`
fn split_whitespace_respecting_quotes(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut in_quote: Option<char> = None;
    let bytes = input.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        let ch = b as char;

        match in_quote {
            Some(quote_char) => {
                if ch == quote_char {
                    in_quote = None;
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    in_quote = Some(ch);
                } else if ch.is_ascii_whitespace() {
                    if i > start {
                        let token = &input[start..i];
                        if !token.trim().is_empty() {
                            result.push(token.trim());
                        }
                    }
                    start = i + 1;
                }
            }
        }
    }

    // Don't forget the last token
    if start < input.len() {
        let token = &input[start..];
        if !token.trim().is_empty() {
            result.push(token.trim());
        }
    }

    result
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
        let operator = &before.trim_end()[operator_end - op_len..operator_end];

        let mut last_idx = 0;
        let bytes = &before.as_bytes()[..operator_end - op_len];
        for i in 0..bytes.len() {
            let is_sep =
                bytes[i] == b';' || bytes[i] == b'\n' || bytes[i] == b'{' || bytes[i] == b'}';
            let is_edge_op =
                i > 0 && bytes[i - 1] == b'-' && (bytes[i] == b'>' || bytes[i] == b'-');
            if is_sep || is_edge_op {
                last_idx = i + 1;
            }
        }

        let prefix = &before[..last_idx];
        let source = before[last_idx..operator_end - op_len].trim();

        output.push_str(prefix);

        // Extract group members (respecting quotes).
        let inner = rest[brace_start + 1..brace_end].trim();
        let members = split_whitespace_respecting_quotes(inner);

        // Expand: emit "source -> member" for each member.
        if members.is_empty() {
            output.push_str(source);
        } else {
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
    // Fast path: no backslash means no escape sequence, so the loop below pushes every char unchanged and
    // returns `raw` verbatim — replace the char-by-char rebuild with one memcpy. Byte-identical: without a
    // `\`, `escaped` never flips, so every char takes the `else { output.push(ch) }` branch and the trailing
    // `if escaped` is false. `\` is single-byte ASCII (never a UTF-8 continuation byte), so the byte scan is
    // correct. Called on every DOT node/edge label; the overwhelming majority carry no escapes.
    if !raw.as_bytes().contains(&b'\\') {
        return raw.to_owned();
    }

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
    let mut in_quote: Option<u8> = None;
    let mut escaped = false;
    let mut html_depth = 0_usize;

    let bytes = line.as_bytes();
    let separator_bytes = separator.as_bytes();
    let separator_len = separator_bytes.len();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];

        if let Some(quote_char) = in_quote {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == quote_char {
                in_quote = None;
            }
        } else {
            if c == b'"' || c == b'\'' {
                in_quote = Some(c);
            } else if c == b'<' {
                html_depth = html_depth.saturating_add(1);
            } else if c == b'>' {
                html_depth = html_depth.saturating_sub(1);
            } else if html_depth == 0 && bytes[i..].starts_with(separator_bytes) {
                // Skip empty (post-trim) parts at push time rather than materializing them and
                // then dropping them with `into_iter().filter().collect()`, which allocated a
                // whole second `Vec`. Byte-identical: same non-empty parts in the same order.
                let part = line[current_start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                current_start = i + separator_len;
                i = current_start;
                continue;
            }
        }
        i += 1;
    }

    if current_start < line.len() {
        let part = line[current_start..].trim();
        if !part.is_empty() {
            parts.push(part);
        }
    }
    parts
}

fn span_for(line_number: usize, line: &str) -> Span {
    let width = if line.is_ascii() {
        line.len()
    } else {
        line.chars().count()
    };
    Span::at_line(line_number, width)
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
    fn detects_dot_headers_with_leading_comments() {
        assert!(looks_like_dot("// comment\ndigraph G { a -> b; }"));
        assert!(looks_like_dot("/* comment */\nstrict graph G { a -- b; }"));
    }

    #[test]
    fn detects_dot_headers_without_space_before_brace() {
        assert!(looks_like_dot("digraph{ a -> b; }"));
        assert!(looks_like_dot("strict digraph{ a -> b; }"));
        assert!(looks_like_dot("graph{ a -- b; }"));
    }

    #[test]
    fn directed_detection_ignores_leading_comments() {
        let parsed = parse_dot("// comment\n digraph{ a -> b; }");
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);

        let parsed = parse_dot("/* comment */ graph{ a -- b; }");
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Line);
    }

    #[test]
    fn parses_dot_when_leading_comment_contains_brace() {
        let parsed = parse_dot("// { comment\n digraph G { a -> b; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
    }

    #[test]
    fn parses_dot_when_block_comment_contains_brace() {
        let parsed = parse_dot("/* { comment */ graph G { a -- b; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Line);
    }

    #[test]
    fn parses_dot_when_graph_name_contains_brace_in_quotes() {
        let parsed = parse_dot("digraph \"name {brace}\" { a -> b; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
    }

    #[test]
    fn parses_dot_when_graph_name_contains_brace_in_html() {
        let parsed = parse_dot("digraph <<b>{name}</b>> { a -> b; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
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
    fn undirected_graph_label_arrows_do_not_force_directed_edges() {
        let parsed = parse_dot("graph G { a -- b [label=\"a->b\"]; }");
        assert_eq!(parsed.ir.edges.len(), 1);
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Line);
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
    fn quoted_node_ids_with_brackets_do_not_start_attribute_blocks() {
        let parsed = parse_dot("digraph G { \"node[a]\" -> b; }");
        let ids: Vec<&str> = parsed.ir.nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"node_a"));
    }

    #[test]
    fn symbol_only_node_ids_fall_back_to_hashed_ids() {
        let parsed = parse_dot("digraph G { \"***\" -> \"$$$\"; }");
        assert_eq!(parsed.ir.nodes.len(), 2);
        let first = parsed.ir.nodes[0].id.as_str();
        let second = parsed.ir.nodes[1].id.as_str();
        assert!(first.starts_with("id_"));
        assert!(second.starts_with("id_"));
        assert_ne!(first, second);
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
fn dot_edge_group_with_quoted_nodes() {
    // Quoted nodes with spaces in edge groups
    // Spaces are normalized to underscores by normalize_identifier()
    let input = r#"digraph G { A -> {"node 1" "node 2"}; }"#;
    let result = parse_dot(input);
    let node_ids: Vec<&str> = result.ir.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        result.ir.edges.len(),
        2,
        "Expected 2 edges, got {} with nodes {:?}",
        result.ir.edges.len(),
        node_ids
    );
    assert!(
        node_ids.contains(&"A"),
        "Missing source node A, got: {:?}",
        node_ids
    );
    // Spaces in quoted IDs are normalized to underscores
    assert!(
        node_ids.contains(&"node_1"),
        "Missing 'node_1', got: {:?}",
        node_ids
    );
    assert!(
        node_ids.contains(&"node_2"),
        "Missing 'node_2', got: {:?}",
        node_ids
    );
}

#[test]
fn dot_edge_group_with_single_quoted_nodes() {
    // Single-quoted nodes in edge groups should also work
    let input = "digraph G { A -> {'node 1' 'node 2'}; }";
    let result = parse_dot(input);
    let node_ids: Vec<&str> = result.ir.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        result.ir.edges.len(),
        2,
        "Expected 2 edges, got {} with nodes {:?}",
        result.ir.edges.len(),
        node_ids
    );
    assert!(
        node_ids.contains(&"node_1"),
        "Missing 'node_1', got: {:?}",
        node_ids
    );
    assert!(
        node_ids.contains(&"node_2"),
        "Missing 'node_2', got: {:?}",
        node_ids
    );
}

#[test]
fn dot_compass_points_stripped() {
    let input = "digraph G { A:n -> B:s; }";
    let result = parse_dot(input);
    assert_eq!(result.ir.edges.len(), 1);
    assert!(result.ir.nodes.iter().any(|n| n.id == "A"));
    assert!(result.ir.nodes.iter().any(|n| n.id == "B"));
}

#[test]
fn extract_attribute_with_spaces() {
    let attr = "shape = box";
    assert_eq!(
        extract_dot_attribute_raw(attr, "shape").as_deref(),
        Some("box")
    );
    let attr2 = "shape= box";
    assert_eq!(
        extract_dot_attribute_raw(attr2, "shape").as_deref(),
        Some("box")
    );
}

#[test]
fn dot_node_attribute_single_pass_matches_sequential_reference() {
    for attributes in [
        r#"label="Node", shape=diamond"#,
        r#"SHAPE = "circle", color=red, LABEL = <b>Alpha</b>"#,
        r#"color=red, label="Line\nBreak", style=filled, shape=roundedbox"#,
        r#"label="", xlabel="shape", myshape=star"#,
        r#"shape=unknown, shape=diamond, label="first", label="second""#,
        r#"color=red, label="unterminated"#,
        r#"tooltip="shape=diamond", label="Tooltip only""#,
        r#"shape=hexagon"#,
        "",
    ] {
        assert_eq!(
            parse_dot_node_attributes(attributes),
            parse_dot_node_attributes_sequential(attributes),
            "attribute list: {attributes:?}"
        );
    }
}

#[test]
fn single_quoted_identifiers_with_semicolons() {
    // Single-quoted identifiers containing semicolons should not be split
    let input = "digraph G { 'foo;bar' -> B; }";
    let result = parse_dot(input);
    // Should have 2 nodes, not more (semicolon inside quotes should not split)
    assert_eq!(
        result.ir.nodes.len(),
        2,
        "expected 2 nodes, got {}: {:?}",
        result.ir.nodes.len(),
        result.ir.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    assert_eq!(result.ir.edges.len(), 1, "expected 1 edge");
}
