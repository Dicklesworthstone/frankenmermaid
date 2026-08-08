use std::{borrow::Cow, iter::Peekable, str::CharIndices};

use fm_core::{
    ArrowType, DiagramType, GraphDirection, IrConstraint, IrStyleTarget, NodeShape, Span,
};
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
    // One record per open `{`, so a `}` pops exactly what its own brace opened. An anonymous group
    // (`{ a; b; }`, and the `{ rank=same; … }` idiom) opens no cluster and no subgraph; before this
    // stack existed, its closing brace popped the ENCLOSING cluster, and every node after it
    // silently fell outside the cluster it was written inside.
    let mut brace_scopes: Vec<DotBraceScope> = Vec::new();
    let mut defaults = DotDefaults::default();

    for (index, line) in normalized_body.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for statement in split_dot_by(trimmed, ";") {
            let close_count = statement.chars().take_while(|ch| *ch == '}').count();
            for _ in 0..close_count {
                close_dot_brace_scope(
                    &mut brace_scopes,
                    &mut active_clusters,
                    &mut active_subgraphs,
                    &mut defaults,
                    line_number,
                    line,
                    &mut builder,
                );
            }
            let statement = statement.trim_start_matches('}').trim();
            if statement.is_empty() {
                continue;
            }
            if statement == "{" {
                brace_scopes.push(DotBraceScope {
                    saved_defaults: defaults.clone(),
                    ..DotBraceScope::default()
                });
                continue;
            }
            // An anonymous group that opens on the same line as its first statement, e.g.
            // `{ rank=same` after the `;` split. The brace opens a scope that pushes nothing.
            let statement = if let Some(rest) = statement.strip_prefix('{') {
                brace_scopes.push(DotBraceScope {
                    saved_defaults: defaults.clone(),
                    ..DotBraceScope::default()
                });
                rest.trim()
            } else {
                statement
            };
            if statement.is_empty() {
                continue;
            }

            // `rankdir` is a graph attribute, so it applies to the whole diagram wherever it
            // appears. Handled before the `graph …` skip below, which would otherwise discard it.
            if let Some(rankdir) = parse_dot_rankdir(statement) {
                apply_dot_rankdir(rankdir, line_number, &mut builder);
                continue;
            }

            // `rank=same` marks the innermost group as a same-rank set. DOT expresses it as a graph
            // attribute inside the group, so it applies to the group, not to one node.
            if let Some(rank_value) = parse_dot_rank_attribute(statement) {
                match rank_value {
                    DotRankValue::Same => {
                        if let Some(scope) = brace_scopes.last_mut() {
                            scope.same_rank = Some(Vec::new());
                        } else {
                            builder.add_warning(format!(
                                "Line {line_number}: rank=same outside a group has no effect; \
                                 wrap the nodes in braces, as in {{ rank=same; a; b; }}"
                            ));
                        }
                    }
                    DotRankValue::Unsupported(name) => {
                        builder.add_warning(format!(
                            "Line {line_number}: DOT rank={name} is not supported and was ignored; \
                             only rank=same constrains layout"
                        ));
                    }
                }
                continue;
            }

            // Record the group's node references before dispatching, so the same-rank constraint
            // names them. Node references are the form real `rank=same` groups use; an edge written
            // inside the group still becomes an edge, but its endpoints are not collected.
            if brace_scopes.iter().any(|scope| scope.same_rank.is_some())
                && let Some(node) = parse_dot_node_fragment(statement)
            {
                for scope in &mut brace_scopes {
                    if let Some(members) = scope.same_rank.as_mut()
                        && !members.contains(&node.id)
                    {
                        members.push(node.id.clone());
                    }
                }
            }

            if let Some((cluster_key, cluster_title, opens_scope)) =
                parse_subgraph_start(statement, line_number)
            {
                // Use the cluster_key directly for named clusters to allow merging.
                // For anonymous ones, the key already includes the line number.
                let lookup_key = cluster_key.clone();

                let mut scope = DotBraceScope {
                    saved_defaults: defaults.clone(),
                    ..DotBraceScope::default()
                };
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
                        scope.pushed_cluster = true;
                        if let Some(subgraph_index) = subgraph_index {
                            active_subgraphs.push(subgraph_index);
                            scope.pushed_subgraph = true;
                        }
                    }
                }
                // Record the scope whenever the brace opened, even if the cluster could not be
                // created: the `}` is coming either way, and a missing record would make it unwind
                // an enclosing scope instead.
                if opens_scope {
                    brace_scopes.push(scope);
                }
                continue;
            }

            // `graph [...]`, `node [...]` and `edge [...]` set DEFAULTS for everything that follows.
            // They must be consumed BEFORE the node parser, which otherwise reads the keyword as a
            // node id and adds a phantom box labelled `graph`/`node`/`edge` to the diagram — a
            // visible stray shape in any DOT file that sets defaults, which is most of them.
            // The keyword must be followed by whitespace or `[`, so a node named `graphite` is
            // untouched, and a statement containing an edge operator is left to the edge parser
            // rather than silently dropped.
            let lower = statement.trim().to_ascii_lowercase();
            let is_graph_defaults = lower.starts_with("graph ")
                || lower.starts_with("graph[")
                || lower.starts_with("graph\t");
            let is_defaults_statement = is_graph_defaults
                || lower.starts_with("edge ")
                || lower.starts_with("edge[")
                || lower.starts_with("edge\t")
                || lower.starts_with("node ")
                || lower.starts_with("node[")
                || lower.starts_with("node\t");
            if is_defaults_statement && find_edge_operator(statement).is_none() {
                // `graph [rankdir=LR]` is the attribute-list spelling of the bare `rankdir=LR`
                // handled above, and just as common. The rest of the list is still skipped.
                if is_graph_defaults
                    && let Some(value) = extract_dot_attribute_raw(statement, "rankdir")
                {
                    let rankdir = parse_dot_rankdir_value(value.as_ref())
                        .ok_or_else(|| value.as_ref().trim().trim_matches('"').to_string());
                    apply_dot_rankdir(rankdir, line_number, &mut builder);
                }
                // `node [shape=…]` and `edge [style=…]` set defaults for everything that FOLLOWS in
                // this scope. A later statement of the same kind overrides only the attribute it
                // names, matching DOT, where each default statement updates the current set.
                if lower.starts_with("node") {
                    if let Some(shape) = extract_dot_attribute_raw(statement, "shape")
                        .as_deref()
                        .and_then(parse_dot_shape_value)
                    {
                        defaults.node_shape = Some(shape);
                    }
                    let colors = DotNodeColors::parse(statement);
                    if !colors.is_empty() {
                        defaults.node_colors = colors.resolved_over(&defaults.node_colors);
                    }
                } else if lower.starts_with("edge") {
                    if let Some(style) = parse_dot_edge_style(statement) {
                        defaults.edge_style = Some(style);
                    }
                    let visuals = DotNodeColors::parse(statement);
                    if !visuals.is_empty() {
                        defaults.edge_visuals = visuals.resolved_over(&defaults.edge_visuals);
                    }
                }
                continue;
            }

            let ctx = DotStatementContext {
                line_number,
                source_line: line,
                active_clusters: &active_clusters,
                active_subgraphs: &active_subgraphs,
                defaults: &defaults,
            };
            if parse_dot_edge_statement(statement, directed, ctx, &mut builder) {
                continue;
            }
            if parse_dot_node_statement(statement, ctx, &mut builder) {
                continue;
            }

            builder.add_warning(format!(
                "Line {line_number}: unsupported DOT statement: {statement}"
            ));

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

/// What one `{` opened, so its matching `}` pops exactly that and nothing else.
#[derive(Debug, Default)]
struct DotBraceScope {
    pushed_cluster: bool,
    pushed_subgraph: bool,
    /// `Some` once `rank=same` is seen in this group; accumulates the node ids it names.
    same_rank: Option<Vec<String>>,
    /// The defaults in force when this brace opened, restored when it closes.
    ///
    /// DOT scopes `node [...]` / `edge [...]` defaults to the subgraph that declares them: they
    /// apply to the rest of that subgraph and to nested ones, and revert on exit. Saving the
    /// outer values here is what makes exit revert instead of leaking.
    saved_defaults: DotDefaults,
}

/// `node [...]` / `edge [...]` defaults currently in force.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DotDefaults {
    node_shape: Option<NodeShape>,
    edge_style: Option<DotEdgeStyle>,
    node_colors: DotNodeColors,
    edge_visuals: DotNodeColors,
}

/// A DOT element's visual attributes, resolved together because graphviz's color rules are
/// interdependent and because defaults merge attribute by attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DotNodeColors {
    /// `color=` — the border color, or the FILL when `style=filled` and no `fillcolor` is given.
    color: Option<String>,
    /// `fillcolor=` — the interior color, which does not need `style=filled` here.
    fill: Option<String>,
    /// Whether `style` contained `filled`.
    filled: bool,
    /// `penwidth=` — stroke thickness, in points.
    penwidth: Option<String>,
    /// `fontsize=` — text size, in points.
    font_size: Option<String>,
    /// `fontname=` — font family.
    font_name: Option<String>,
    //
    // `fontcolor` is deliberately NOT here. This renderer colors text with `fill` on the text
    // element, and `fill` in a node-level style ref already means the SHAPE fill, so the two cannot
    // coexist in one declaration. Emitting `color:` instead was tried and measured: it survives the
    // IR's property allowlist and is then dropped by the renderer, i.e. CSS nothing consumes.
    // Parsing it would be work with no observable effect, so it stays unsupported and stated.
}

impl DotNodeColors {
    /// Read every visual attribute off one attribute list.
    fn parse(attributes: &str) -> Self {
        let value = |key: &str| {
            extract_dot_attribute_raw(attributes, key)
                .map(|raw| raw.as_ref().trim().trim_matches(['"', '\'']).to_string())
                .filter(|text| !text.is_empty())
        };
        let filled = extract_dot_attribute_raw(attributes, "style").is_some_and(|style| {
            style.as_ref().split(',').any(|token| {
                token
                    .trim()
                    .trim_matches(['"', '\''])
                    .eq_ignore_ascii_case("filled")
            })
        });
        Self {
            color: value("color"),
            fill: value("fillcolor"),
            filled,
            penwidth: value("penwidth"),
            font_size: value("fontsize"),
            font_name: value("fontname"),
        }
    }

    /// Whether this carries nothing, so a caller can skip emitting an empty style.
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Overlay `self` on top of `base`, attribute by attribute.
    ///
    /// Per-attribute rather than all-or-nothing because DOT resolves each independently: a node that
    /// names only `fillcolor` still inherits a default `color`.
    fn resolved_over(&self, base: &Self) -> Self {
        let pick = |mine: &Option<String>, theirs: &Option<String>| {
            mine.clone().or_else(|| theirs.clone())
        };
        Self {
            color: pick(&self.color, &base.color),
            fill: pick(&self.fill, &base.fill),
            filled: self.filled || base.filled,
            penwidth: pick(&self.penwidth, &base.penwidth),
            font_size: pick(&self.font_size, &base.font_size),
            font_name: pick(&self.font_name, &base.font_name),
        }
    }

    /// The element-independent properties: thickness and typography.
    ///
    /// DOT measures `penwidth` and `fontsize` in POINTS. `font-size` therefore emits `pt`, which is
    /// faithful and valid CSS. `stroke-width` is unitless SVG user units, so a penwidth crosses over
    /// as a bare number — an approximation, named as one here rather than presented as exact.
    fn push_shared(&self, properties: &mut Vec<String>) {
        if let Some(width) = self.penwidth.as_deref() {
            properties.push(format!("stroke-width:{width}"));
        }
        if let Some(size) = self.font_size.as_deref() {
            properties.push(format!("font-size:{size}pt"));
        }
        if let Some(family) = self.font_name.as_deref() {
            properties.push(format!("font-family:{family}"));
        }
    }

    /// The CSS an EDGE maps to. `color` is always the stroke: an edge has no interior, so the
    /// `style=filled` rule below does not apply to it.
    fn to_edge_css(&self) -> Option<String> {
        let mut properties: Vec<String> = Vec::new();
        if let Some(color) = self.color.as_deref() {
            properties.push(format!("stroke:{color}"));
        }
        self.push_shared(&mut properties);
        (!properties.is_empty()).then(|| properties.join(","))
    }

    /// The CSS this maps to, or `None` when there is nothing to say.
    ///
    /// graphviz's rule, which is the non-obvious part: `color` is the BORDER, `fillcolor` is the
    /// interior, and with `style=filled` but no `fillcolor` the `color` value fills the shape
    /// instead of outlining it. Getting that backwards would paint borders as fills on a large share
    /// of real .dot files, which lean on `style=filled, color=…`.
    fn to_css(&self) -> Option<String> {
        let mut properties: Vec<String> = Vec::new();
        match (self.filled, self.fill.as_deref(), self.color.as_deref()) {
            (true, None, Some(color)) => properties.push(format!("fill:{color}")),
            (_, fill, color) => {
                if let Some(fill) = fill {
                    properties.push(format!("fill:{fill}"));
                }
                if let Some(color) = color {
                    properties.push(format!("stroke:{color}"));
                }
            }
        }
        self.push_shared(&mut properties);
        (!properties.is_empty()).then(|| properties.join(","))
    }
}

/// DOT's edge `style`, limited to the values this engine can draw.
///
/// `dashed` and `dotted` both map to the same rendering: the IR has one non-solid line family
/// (`DottedArrow` / `DottedLine`), so distinguishing them here would promise a difference the
/// renderer cannot show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DotEdgeStyle {
    Dotted,
    Bold,
}

/// Read an edge's `style` attribute.
///
/// DOT allows a comma list (`style="bold,dashed"`); the first recognized token wins, because the IR
/// carries one line style per edge and cannot be both. Unrecognized tokens (`invis`, `tapered`, …)
/// are skipped rather than treated as an error: they are valid DOT this engine simply does not draw.
fn parse_dot_edge_style(attributes: &str) -> Option<DotEdgeStyle> {
    let value = extract_dot_attribute_raw(attributes, "style")?;
    value
        .as_ref()
        .trim()
        .trim_matches(['"', '\''])
        .split(',')
        .find_map(|token| match token.trim().to_ascii_lowercase().as_str() {
            "dashed" | "dotted" => Some(DotEdgeStyle::Dotted),
            "bold" => Some(DotEdgeStyle::Bold),
            _ => None,
        })
}

/// The arrow type for an edge, given the graph's directedness and the style in force.
const fn dot_arrow_type(directed: bool, style: Option<DotEdgeStyle>) -> ArrowType {
    match (directed, style) {
        (true, None) => ArrowType::Arrow,
        (false, None) => ArrowType::Line,
        (true, Some(DotEdgeStyle::Dotted)) => ArrowType::DottedArrow,
        (false, Some(DotEdgeStyle::Dotted)) => ArrowType::DottedLine,
        (true, Some(DotEdgeStyle::Bold)) => ArrowType::ThickArrow,
        (false, Some(DotEdgeStyle::Bold)) => ArrowType::ThickLine,
    }
}

/// The `rank` graph attribute, split into what this engine can honor and what it cannot.
#[derive(Debug, PartialEq, Eq)]
enum DotRankValue {
    /// `rank=same` — becomes an [`IrConstraint::SameRank`].
    Same,
    /// `rank=min|max|source|sink` — ordering semantics the IR has no constraint for.
    Unsupported(String),
}

/// Recognize a bare `rank=<value>` statement, the form DOT uses inside a group.
///
/// Accepts surrounding whitespace, either quote style, and any capitalization, because all of those
/// appear in real `.dot` files. `graph [rank=same]` is NOT matched here: attribute-list statements
/// are skipped wholesale by the caller, and treating one as a group marker would need the whole
/// attribute parser.
fn parse_dot_rank_attribute(statement: &str) -> Option<DotRankValue> {
    let (key, value) = statement.split_once('=')?;
    if !key.trim().eq_ignore_ascii_case("rank") {
        return None;
    }
    let value = value.trim().trim_matches(['"', '\'']).trim();
    if value.eq_ignore_ascii_case("same") {
        Some(DotRankValue::Same)
    } else if value.is_empty() {
        None
    } else {
        Some(DotRankValue::Unsupported(value.to_ascii_lowercase()))
    }
}

/// Apply a parsed `rankdir`, or warn about a value that is not one of DOT's four.
fn apply_dot_rankdir(
    rankdir: Result<GraphDirection, String>,
    line_number: usize,
    builder: &mut IrBuilder,
) {
    match rankdir {
        Ok(direction) => builder.set_direction(direction),
        Err(value) => builder.add_warning(format!(
            "Line {line_number}: DOT rankdir={value} is not recognized and was ignored; \
             expected TB, BT, LR, or RL"
        )),
    }
}

/// Close the innermost brace scope: unwind only what it opened, and emit its same-rank constraint.
fn close_dot_brace_scope(
    brace_scopes: &mut Vec<DotBraceScope>,
    active_clusters: &mut Vec<usize>,
    active_subgraphs: &mut Vec<usize>,
    defaults: &mut DotDefaults,
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) {
    // A `}` with no recorded scope is unbalanced input, not a reason to unwind a scope some other
    // brace owns — silently popping here is exactly the bug this stack replaced.
    let Some(scope) = brace_scopes.pop() else {
        return;
    };
    // Defaults declared inside the group stop applying at its `}`.
    *defaults = scope.saved_defaults;
    if scope.pushed_cluster {
        let _ = active_clusters.pop();
    }
    if scope.pushed_subgraph {
        let _ = active_subgraphs.pop();
    }
    if let Some(node_ids) = scope.same_rank {
        // One node is already on its own rank, so a single-member group constrains nothing and
        // would only inflate the solver's applied-constraint count.
        if node_ids.len() >= 2 {
            builder.add_constraint(IrConstraint::SameRank {
                node_ids,
                span: span_for(line_number, source_line),
            });
        }
    }
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

/// Everything a statement parser needs from the enclosing scope.
///
/// Bundled rather than threaded as separate parameters so that adding the next scoped attribute
/// does not touch every call site — the defaults field is already the second thing to arrive here
/// after the cluster/subgraph stacks.
#[derive(Clone, Copy)]
struct DotStatementContext<'a> {
    line_number: usize,
    source_line: &'a str,
    active_clusters: &'a [usize],
    active_subgraphs: &'a [usize],
    /// Borrowed rather than owned so the context stays `Copy` now that defaults carry color strings.
    defaults: &'a DotDefaults,
}

fn parse_dot_edge_statement(
    statement: &str,
    directed: bool,
    ctx: DotStatementContext<'_>,
    builder: &mut IrBuilder,
) -> bool {
    let DotStatementContext {
        line_number,
        source_line,
        active_clusters,
        active_subgraphs,
        defaults,
    } = ctx;
    let Some(operator) = find_edge_operator(statement) else {
        return false;
    };

    let mut parts: Vec<&str> = split_dot_by(statement, operator);
    if parts.len() < 2 {
        return false;
    }

    let span = span_for(line_number, source_line);

    // Extract shared attributes from the last part
    let Some(last_part) = parts.last_mut() else {
        return false;
    };
    let (last_fragment, shared_attrs) = split_endpoint_and_attrs(last_part);
    *last_part = last_fragment;

    let edge_label_str = shared_attrs.and_then(parse_dot_label);
    let min_len = shared_attrs.and_then(parse_dot_minlen);
    // A style on the statement wins over the `edge [style=…]` default, which is what DOT does.
    let style = shared_attrs
        .and_then(parse_dot_edge_style)
        .or(defaults.edge_style);
    let arrow = dot_arrow_type(operator == "->" || directed, style);
    // Edge visuals resolve the same way node visuals do: per attribute, statement over default.
    let edge_css = shared_attrs
        .map(DotNodeColors::parse)
        .unwrap_or_default()
        .resolved_over(&defaults.edge_visuals)
        .to_edge_css();

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

        // An endpoint mentioned only in an edge is still a node the `node [shape=…]` default
        // applies to, so it must not be hardcoded to Rect.
        let endpoint_shape = |declared: Option<NodeShape>| {
            declared.or(defaults.node_shape).unwrap_or(NodeShape::Rect)
        };
        let from = builder.intern_node(
            &from_node.id,
            from_node.label.as_deref(),
            endpoint_shape(from_node.shape),
            span,
        );
        let to = builder.intern_node(
            &to_node.id,
            to_node.label.as_deref(),
            endpoint_shape(to_node.shape),
            span,
        );

        if let (Some(from_id), Some(to_id)) = (from, to) {
            // Captured BEFORE the push, because `push_style_ref` addresses the edge by index and the
            // index of the edge about to be added is the current count.
            let edge_index = builder.edge_count();
            builder.push_edge(from_id, to_id, arrow, edge_label_str.as_deref(), span);
            if let Some(css) = edge_css.clone() {
                builder.push_style_ref(IrStyleTarget::Link(edge_index), css, span);
            }
            add_node_to_active_groups(builder, active_clusters, active_subgraphs, from_id);
            add_node_to_active_groups(builder, active_clusters, active_subgraphs, to_id);

            // `minlen` applies to every edge in the statement, including each hop of a chain like
            // `a -> b -> c [minlen=2]`, which is what graphviz does with a shared attribute list.
            match &min_len {
                Some(DotMinLen::Ranks(ranks)) => {
                    builder.add_constraint(IrConstraint::MinLength {
                        from_id: from_node.id.clone(),
                        to_id: to_node.id.clone(),
                        min_len: *ranks,
                        span,
                    });
                }
                Some(DotMinLen::SameRankUnsupported) => {
                    builder.add_warning(format!(
                        "Line {line_number}: DOT minlen=0 asks for both endpoints on one rank, \
                         which this engine cannot express for an edge; use {{ rank=same; \
                         {}; {}; }} instead",
                        from_node.id, to_node.id
                    ));
                }
                Some(DotMinLen::Invalid(text)) => {
                    builder.add_warning(format!(
                        "Line {line_number}: DOT minlen={text} is not a non-negative integer and \
                         was ignored"
                    ));
                }
                None => {}
            }
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
    ctx: DotStatementContext<'_>,
    builder: &mut IrBuilder,
) -> bool {
    let DotStatementContext {
        line_number,
        source_line,
        active_clusters,
        active_subgraphs,
        defaults,
    } = ctx;
    let Some(node) = parse_dot_node_fragment(statement) else {
        return false;
    };
    let span = span_for(line_number, source_line);
    let shape = node
        .shape
        .or(defaults.node_shape)
        .unwrap_or(NodeShape::Rect);
    let node_id = builder.intern_node(&node.id, node.label.as_deref(), shape, span);
    if let Some(node_id) = node_id {
        add_node_to_active_groups(builder, active_clusters, active_subgraphs, node_id);
        // Colors become an `IrStyleRef` on the node, the same shape `style A fill:#f9f` produces in
        // Mermaid, so the renderer needs no DOT-specific path.
        if let Some(css) = split_endpoint_and_attrs(statement.trim())
            .1
            .map(DotNodeColors::parse)
            .unwrap_or_default()
            .resolved_over(&defaults.node_colors)
            .to_css()
        {
            builder.push_style_ref(IrStyleTarget::Node(node_id), css, span);
        }
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
    /// `None` when the statement named no usable shape, so a `node [shape=…]` default can apply.
    shape: Option<NodeShape>,
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

    let (label, shape) = attrs.map_or((None, None), parse_dot_node_attributes);

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

fn parse_dot_node_attributes(attributes: &str) -> (Option<String>, Option<NodeShape>) {
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
    // `None` means the statement named no usable shape, which is DISTINCT from naming `rect`: only
    // the former should fall back to a `node [shape=…]` default. The fallback lives at the call
    // site, which is the only place that knows the defaults in force.
    let shape = shape_value.as_deref().and_then(parse_dot_shape_value);
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

/// Read DOT's `minlen` edge attribute — the minimum number of ranks an edge must span.
///
/// Becomes an [`IrConstraint::MinLength`], which both `apply_ir_constraints` and the LP solver
/// already honor. Returns `None` for a missing, unparseable, or `minlen=1` value: 1 is the DOT
/// default and every edge already spans at least one rank, so a constraint for it would only inflate
/// the solver's applied-constraint count. `minlen=0` is DOT's "same rank" spelling for an edge, which
/// [`IrConstraint::MinLength`] cannot express (its gap is a minimum, not an equality), so it is
/// rejected here and reported by the caller rather than silently rounded up to 1.
fn parse_dot_minlen(attributes: &str) -> Option<DotMinLen> {
    let value = extract_dot_attribute_raw(attributes, "minlen")?;
    let text = value.as_ref().trim().trim_matches(['"', '\'']).trim();
    match text.parse::<usize>() {
        Ok(0) => Some(DotMinLen::SameRankUnsupported),
        Ok(1) => None,
        Ok(min_len) => Some(DotMinLen::Ranks(min_len)),
        Err(_) => Some(DotMinLen::Invalid(text.to_string())),
    }
}

/// Read DOT's `rankdir` graph attribute into a [`GraphDirection`].
///
/// `rankdir=LR` is one of the most common lines in real `.dot` files and was previously dropped on
/// the floor with every other graph attribute, so a graph that asked to flow left-to-right rendered
/// top-to-bottom. DOT spells only TB/BT/LR/RL; `TD` is Mermaid's synonym for TB and is accepted here
/// because the two dialects meet in this parser.
fn parse_dot_rankdir_value(value: &str) -> Option<GraphDirection> {
    match value
        .trim()
        .trim_matches(['"', '\''])
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "TB" => Some(GraphDirection::TB),
        "TD" => Some(GraphDirection::TD),
        "LR" => Some(GraphDirection::LR),
        "RL" => Some(GraphDirection::RL),
        "BT" => Some(GraphDirection::BT),
        _ => None,
    }
}

/// Recognize a bare `rankdir=<value>` statement.
///
/// Returns `Err` with the offending text for a recognized key with an unusable value, so the caller
/// can warn instead of silently leaving the graph in its default direction.
fn parse_dot_rankdir(statement: &str) -> Option<Result<GraphDirection, String>> {
    let (key, value) = statement.split_once('=')?;
    if !key.trim().eq_ignore_ascii_case("rankdir") {
        return None;
    }
    let cleaned = value.trim().trim_matches(['"', '\'']).trim().to_string();
    Some(parse_dot_rankdir_value(value).ok_or(cleaned))
}

/// Outcome of reading `minlen`, kept distinct so the caller can report what it could not honor.
#[derive(Debug, PartialEq, Eq)]
enum DotMinLen {
    /// A usable minimum rank span (>= 2).
    Ranks(usize),
    /// `minlen=0`: DOT's request to place both endpoints on one rank.
    SameRankUnsupported,
    /// Not a non-negative integer.
    Invalid(String),
}

#[cfg(test)]
fn parse_dot_node_attributes_sequential(attributes: &str) -> (Option<String>, Option<NodeShape>) {
    let label = parse_dot_label(attributes);
    // Left as `Option` to match `parse_dot_node_attributes`, whose result this exists to
    // cross-check: "no shape named" has to stay distinguishable from "shape=rect" so a
    // `node [shape=…]` default can apply only to the former.
    let shape = if contains_ignore_ascii_case(attributes.as_bytes(), b"shape") {
        parse_dot_shape(attributes)
    } else {
        None
    };
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
    use fm_core::{ArrowType, DiagramType, NodeShape};

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
    fn anonymous_brace_group_inside_a_cluster_does_not_close_it_early() {
        // `{ ... }` is a valid anonymous DOT group and does not open a cluster, so its closing
        // brace must not pop one. Before the scope stack tracked braces explicitly, this `}` popped
        // `cluster_0`, and every node after it silently fell outside the cluster.
        let parsed = parse_dot("digraph G { subgraph cluster_0 { a; { b; } c; } a -> c; }");

        assert_eq!(parsed.ir.clusters.len(), 1, "{:?}", parsed.ir.clusters);
        let members: Vec<&str> = parsed.ir.clusters[0]
            .members
            .iter()
            .map(|member| parsed.ir.nodes[member.0].id.as_str())
            .collect();
        assert_eq!(
            members,
            ["a", "b", "c"],
            "every node inside cluster_0 must stay in it, including after the nested group"
        );
    }

    #[test]
    fn rank_same_group_becomes_a_same_rank_constraint() {
        let parsed = parse_dot("digraph G { { rank=same; b; c; } a -> b; a -> c; }");

        assert_eq!(
            parsed.ir.constraints.len(),
            1,
            "{:?}",
            parsed.ir.constraints
        );
        match &parsed.ir.constraints[0] {
            fm_core::IrConstraint::SameRank { node_ids, .. } => {
                assert_eq!(node_ids, &["b".to_string(), "c".to_string()]);
            }
            other => panic!("expected SameRank, got {other:?}"),
        }
        // The group's members are still ordinary nodes, not swallowed by the constraint.
        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert!(ids.contains(&"b") && ids.contains(&"c"), "{ids:?}");
    }

    #[test]
    fn rank_same_accepts_spacing_quotes_and_capitalization() {
        for source in [
            "digraph G { { rank=same; b; c; } }",
            "digraph G { { rank = same; b; c; } }",
            "digraph G { { RANK=SAME; b; c; } }",
            "digraph G { { rank=\"same\"; b; c; } }",
        ] {
            let parsed = parse_dot(source);
            assert_eq!(
                parsed.ir.constraints.len(),
                1,
                "{source} produced {:?}",
                parsed.ir.constraints
            );
        }
    }

    #[test]
    fn rank_same_with_one_member_constrains_nothing() {
        // A single node is already alone on its rank, so emitting a constraint would only inflate
        // the solver's applied count.
        let parsed = parse_dot("digraph G { { rank=same; b; } a -> b; }");
        assert!(
            parsed.ir.constraints.is_empty(),
            "{:?}",
            parsed.ir.constraints
        );
    }

    #[test]
    fn two_rank_same_groups_are_kept_separate() {
        let parsed =
            parse_dot("digraph G { { rank=same; a; b; } { rank=same; c; d; } a -> c; b -> d; }");
        assert_eq!(
            parsed.ir.constraints.len(),
            2,
            "{:?}",
            parsed.ir.constraints
        );
    }

    #[test]
    fn repeating_the_same_rank_group_does_not_duplicate_the_constraint() {
        let parsed = parse_dot("digraph G { { rank=same; a; b; } { rank=same; a; b; } a -> b; }");
        assert_eq!(
            parsed.ir.constraints.len(),
            1,
            "{:?}",
            parsed.ir.constraints
        );
    }

    #[test]
    fn unsupported_rank_values_warn_instead_of_constraining() {
        for value in ["min", "max", "source", "sink"] {
            let parsed = parse_dot(&format!(
                "digraph G {{ {{ rank={value}; a; b; }} a -> b; }}"
            ));
            assert!(
                parsed.ir.constraints.is_empty(),
                "rank={value} must not produce a constraint: {:?}",
                parsed.ir.constraints
            );
            assert!(
                parsed
                    .warnings
                    .iter()
                    .any(|warning| warning.contains(value) && warning.contains("not supported")),
                "rank={value} must warn: {:?}",
                parsed.warnings
            );
        }
    }

    #[test]
    fn rank_same_inside_a_cluster_keeps_both_the_cluster_and_the_constraint() {
        // The two mechanisms share the brace stack, so this is the case where a bug in one shows up
        // as a silent failure of the other.
        let parsed =
            parse_dot("digraph G { subgraph cluster_0 { a; { rank=same; b; c; } d; } a -> d; }");

        assert_eq!(
            parsed.ir.constraints.len(),
            1,
            "{:?}",
            parsed.ir.constraints
        );
        let members: Vec<&str> = parsed.ir.clusters[0]
            .members
            .iter()
            .map(|member| parsed.ir.nodes[member.0].id.as_str())
            .collect();
        assert_eq!(
            members,
            ["a", "b", "c", "d"],
            "the nested rank group must not close the cluster"
        );
    }

    #[test]
    fn rank_same_outside_any_group_warns_rather_than_silently_doing_nothing() {
        let parsed = parse_dot("digraph G { rank=same; a -> b; }");
        assert!(parsed.ir.constraints.is_empty());
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("rank=same")),
            "{:?}",
            parsed.warnings
        );
    }

    /// The style CSS attached to a node id, if any.
    fn node_style_css(parsed: &crate::ParseResult, id: &str) -> Option<String> {
        let node_index = parsed.ir.nodes.iter().position(|node| node.id == id)?;
        parsed.ir.style_refs.iter().find_map(|style| {
            matches!(style.target, fm_core::IrStyleTarget::Node(node_id) if node_id.0 == node_index)
                .then(|| style.style.clone())
        })
    }

    #[test]
    fn penwidth_and_font_attributes_become_css_on_nodes() {
        let parsed = parse_dot("digraph G { a [penwidth=3, fontsize=18, fontname=Georgia]; }");
        let css = node_style_css(&parsed, "a").expect("style for a");
        // fontsize is POINTS in DOT, so it carries a pt unit; penwidth becomes bare SVG user units.
        assert!(css.contains("stroke-width:3"), "{css}");
        assert!(css.contains("font-size:18pt"), "{css}");
        assert!(css.contains("font-family:Georgia"), "{css}");
    }

    #[test]
    fn fontcolor_is_not_emitted_because_the_renderer_would_drop_it() {
        // Measured, not assumed: mapping `fontcolor` to `color:` survives the IR allowlist and is
        // then dropped by the renderer, so it would be CSS nothing consumes. Text color here is
        // `fill` on the text element, which collides with the shape fill in one node style ref.
        let parsed = parse_dot("digraph G { a [fontcolor=green]; }");
        assert_eq!(
            node_style_css(&parsed, "a"),
            None,
            "fontcolor alone must produce no style entry rather than a dropped property"
        );
    }

    #[test]
    fn penwidth_and_font_attributes_become_css_on_edges() {
        let parsed = parse_dot("digraph G { a -> b [penwidth=2, fontsize=9, color=red]; }");
        let css = parsed
            .ir
            .style_refs
            .iter()
            .find(|style| matches!(style.target, fm_core::IrStyleTarget::Link(_)))
            .map(|style| style.style.clone())
            .expect("link style");
        assert!(css.contains("stroke:red"), "{css}");
        assert!(css.contains("stroke-width:2"), "{css}");
        assert!(css.contains("font-size:9pt"), "{css}");
    }

    #[test]
    fn an_edge_ignores_style_filled_because_it_has_no_interior() {
        // The node rule (style=filled makes `color` the fill) must NOT leak to edges: an edge has no
        // interior, so its color is always the stroke.
        let parsed = parse_dot("digraph G { a -> b [style=filled, color=red]; }");
        let css = parsed
            .ir
            .style_refs
            .iter()
            .find(|style| matches!(style.target, fm_core::IrStyleTarget::Link(_)))
            .map(|style| style.style.clone())
            .expect("link style");
        assert_eq!(css, "stroke:red");
    }

    #[test]
    fn font_and_width_defaults_apply_and_merge_with_per_element_values() {
        let parsed =
            parse_dot("digraph G { node [fontname=Georgia, penwidth=4]; a; b [penwidth=1]; }");
        let css_a = node_style_css(&parsed, "a").expect("style for a");
        assert!(
            css_a.contains("font-family:Georgia") && css_a.contains("stroke-width:4"),
            "{css_a}"
        );

        // `b` overrides only the width and must keep the inherited font.
        let css_b = node_style_css(&parsed, "b").expect("style for b");
        assert!(css_b.contains("stroke-width:1"), "{css_b}");
        assert!(
            css_b.contains("font-family:Georgia"),
            "the inherited font must survive a width override: {css_b}"
        );
    }

    #[test]
    fn edge_font_defaults_revert_at_the_end_of_a_subgraph() {
        let parsed =
            parse_dot("digraph G { subgraph cluster_0 { edge [penwidth=5]; a -> b; } c -> d; }");
        let link_styles: Vec<(usize, String)> = parsed
            .ir
            .style_refs
            .iter()
            .filter_map(|style| match style.target {
                fm_core::IrStyleTarget::Link(index) => Some((index, style.style.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            link_styles.len(),
            1,
            "only the in-scope edge may be styled: {link_styles:?}"
        );
        assert_eq!(link_styles[0].0, 0);
        assert!(link_styles[0].1.contains("stroke-width:5"));
    }

    #[test]
    fn node_fillcolor_becomes_a_fill_and_color_becomes_a_stroke() {
        let parsed = parse_dot("digraph G { a [fillcolor=red]; b [color=blue]; }");
        assert_eq!(node_style_css(&parsed, "a").as_deref(), Some("fill:red"));
        assert_eq!(node_style_css(&parsed, "b").as_deref(), Some("stroke:blue"));
    }

    #[test]
    fn style_filled_without_fillcolor_makes_color_the_fill() {
        // graphviz's rule and the whole reason these three attributes resolve together: with
        // `style=filled` and no `fillcolor`, `color` FILLS the shape instead of outlining it.
        // Reading it as a stroke would paint borders instead of fills on a large share of real
        // .dot files, which lean on exactly this spelling.
        let parsed = parse_dot("digraph G { a [style=filled, color=red]; }");
        assert_eq!(node_style_css(&parsed, "a").as_deref(), Some("fill:red"));

        // With BOTH, color returns to being the border.
        let parsed = parse_dot("digraph G { a [style=filled, color=blue, fillcolor=red]; }");
        assert_eq!(
            node_style_css(&parsed, "a").as_deref(),
            Some("fill:red,stroke:blue")
        );
    }

    #[test]
    fn a_node_with_no_color_attributes_gets_no_style_entry() {
        // The control: an unconditional style entry would make every assertion above vacuous, and
        // would also emit dead CSS for every plain node.
        let parsed = parse_dot("digraph G { a; b -> c; }");
        assert!(
            parsed.ir.style_refs.is_empty(),
            "plain nodes must not carry styles: {:?}",
            parsed.ir.style_refs
        );
    }

    #[test]
    fn node_color_defaults_apply_and_merge_attribute_by_attribute() {
        // `b` names only fillcolor, so it must still inherit the default border color rather than
        // losing it to an all-or-nothing override.
        let parsed = parse_dot("digraph G { node [color=blue]; a; b [fillcolor=red]; }");
        assert_eq!(node_style_css(&parsed, "a").as_deref(), Some("stroke:blue"));
        assert_eq!(
            node_style_css(&parsed, "b").as_deref(),
            Some("fill:red,stroke:blue")
        );
    }

    #[test]
    fn node_color_defaults_revert_at_the_end_of_a_subgraph() {
        let parsed =
            parse_dot("digraph G { subgraph cluster_0 { node [color=blue]; inner; } outer; }");
        assert_eq!(
            node_style_css(&parsed, "inner").as_deref(),
            Some("stroke:blue")
        );
        assert_eq!(node_style_css(&parsed, "outer"), None);
    }

    #[test]
    fn edge_color_becomes_a_link_style_on_the_right_edge() {
        let parsed = parse_dot("digraph G { a -> b; c -> d [color=red]; }");
        let link_styles: Vec<(usize, &str)> = parsed
            .ir
            .style_refs
            .iter()
            .filter_map(|style| match style.target {
                fm_core::IrStyleTarget::Link(index) => Some((index, style.style.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            link_styles,
            [(1, "stroke:red")],
            "only the second edge is colored, and it must be addressed by ITS index"
        );
    }

    #[test]
    fn edge_color_default_applies_and_loses_to_a_per_edge_color() {
        let parsed = parse_dot("digraph G { edge [color=blue]; a -> b; c -> d [color=red]; }");
        let styles: Vec<&str> = parsed
            .ir
            .style_refs
            .iter()
            .filter(|style| matches!(style.target, fm_core::IrStyleTarget::Link(_)))
            .map(|style| style.style.as_str())
            .collect();
        assert_eq!(styles, ["stroke:blue", "stroke:red"]);
    }

    #[test]
    fn edge_style_selects_the_line_family() {
        for (style, expected) in [
            ("dashed", ArrowType::DottedArrow),
            ("dotted", ArrowType::DottedArrow),
            ("bold", ArrowType::ThickArrow),
        ] {
            let parsed = parse_dot(&format!("digraph G {{ a -> b [style={style}]; }}"));
            assert_eq!(parsed.ir.edges[0].arrow, expected, "style={style}");
        }
        // Undirected graphs get the line forms, not the arrow forms.
        let parsed = parse_dot("graph G { a -- b [style=dashed]; }");
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::DottedLine);
        let parsed = parse_dot("graph G { a -- b [style=bold]; }");
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::ThickLine);
    }

    #[test]
    fn plain_edges_keep_their_arrow_when_no_style_is_given() {
        // The control: without `style` the arrow must be unchanged, or the assertions above could be
        // satisfied by a parser that always rewrites the arrow.
        assert_eq!(
            parse_dot("digraph G { a -> b; }").ir.edges[0].arrow,
            ArrowType::Arrow
        );
        assert_eq!(
            parse_dot("graph G { a -- b; }").ir.edges[0].arrow,
            ArrowType::Line
        );
    }

    #[test]
    fn unsupported_style_tokens_are_skipped_not_treated_as_errors() {
        // `invis` and `tapered` are valid DOT this engine cannot draw; the first RECOGNIZED token in
        // the list wins, since one edge cannot be both.
        let parsed = parse_dot("digraph G { a -> b [style=\"invis,bold\"]; }");
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::ThickArrow);

        let parsed = parse_dot("digraph G { a -> b [style=tapered]; }");
        assert_eq!(parsed.ir.edges[0].arrow, ArrowType::Arrow);
    }

    #[test]
    fn node_shape_default_applies_to_later_nodes_and_edge_endpoints() {
        let parsed = parse_dot("digraph G { node [shape=diamond]; a; b -> c; }");
        for node in &parsed.ir.nodes {
            assert_eq!(
                node.shape,
                NodeShape::Diamond,
                "{} should inherit the default shape",
                node.id
            );
        }
    }

    #[test]
    fn an_explicit_shape_overrides_the_default() {
        let parsed = parse_dot("digraph G { node [shape=diamond]; a [shape=circle]; b; }");
        let shape_of = |id: &str| {
            parsed
                .ir
                .nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| node.shape)
                .expect(id)
        };
        assert_eq!(shape_of("a"), NodeShape::Circle);
        assert_eq!(shape_of("b"), NodeShape::Diamond);
    }

    #[test]
    fn defaults_declared_before_a_node_do_not_apply_retroactively() {
        // DOT default statements affect what FOLLOWS them. A node declared earlier keeps its shape.
        let parsed = parse_dot("digraph G { a; node [shape=diamond]; b; }");
        let shape_of = |id: &str| {
            parsed
                .ir
                .nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| node.shape)
                .expect(id)
        };
        assert_eq!(shape_of("a"), NodeShape::Rect);
        assert_eq!(shape_of("b"), NodeShape::Diamond);
    }

    #[test]
    fn defaults_are_scoped_to_the_subgraph_that_declares_them() {
        // The scoping rule DOT specifies, and the reason defaults ride the brace stack: a default
        // set inside a subgraph reverts at its `}`.
        let parsed =
            parse_dot("digraph G { subgraph cluster_0 { node [shape=diamond]; inner; } outer; }");
        let shape_of = |id: &str| {
            parsed
                .ir
                .nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| node.shape)
                .expect(id)
        };
        assert_eq!(shape_of("inner"), NodeShape::Diamond);
        assert_eq!(
            shape_of("outer"),
            NodeShape::Rect,
            "a default set inside the subgraph must not leak past its closing brace"
        );
    }

    #[test]
    fn defaults_inherit_into_nested_subgraphs() {
        let parsed =
            parse_dot("digraph G { node [shape=diamond]; subgraph cluster_0 { inner; } outer; }");
        for node in &parsed.ir.nodes {
            assert_eq!(node.shape, NodeShape::Diamond, "{}", node.id);
        }
    }

    #[test]
    fn edge_style_default_applies_and_is_overridden_per_edge() {
        let parsed =
            parse_dot("digraph G { edge [style=dashed]; a -> b; c -> d [style=bold]; e -> f; }");
        let arrows: Vec<ArrowType> = parsed.ir.edges.iter().map(|edge| edge.arrow).collect();
        assert_eq!(
            arrows,
            [
                ArrowType::DottedArrow,
                ArrowType::ThickArrow,
                ArrowType::DottedArrow
            ],
            "the default applies except where the edge names its own style"
        );
    }

    #[test]
    fn rankdir_sets_the_graph_direction() {
        for (value, expected) in [
            ("TB", fm_core::GraphDirection::TB),
            ("BT", fm_core::GraphDirection::BT),
            ("LR", fm_core::GraphDirection::LR),
            ("RL", fm_core::GraphDirection::RL),
        ] {
            let parsed = parse_dot(&format!("digraph G {{ rankdir={value}; a -> b; }}"));
            assert_eq!(parsed.ir.direction, expected, "rankdir={value}");
            assert_eq!(parsed.ir.meta.direction, expected, "rankdir={value} meta");
        }
    }

    #[test]
    fn rankdir_default_is_top_to_bottom_when_absent() {
        // The control: without rankdir the direction must stay at DOT's default, or the assertions
        // above could be satisfied by a parser that always sets a direction.
        let parsed = parse_dot("digraph G { a -> b; }");
        assert_eq!(parsed.ir.direction, fm_core::GraphDirection::TB);
    }

    #[test]
    fn rankdir_accepts_quotes_spacing_and_lowercase() {
        for source in [
            "digraph G { rankdir=lr; a -> b; }",
            "digraph G { rankdir = LR; a -> b; }",
            "digraph G { rankdir=\"LR\"; a -> b; }",
            "digraph G { RANKDIR=LR; a -> b; }",
        ] {
            let parsed = parse_dot(source);
            assert_eq!(parsed.ir.direction, fm_core::GraphDirection::LR, "{source}");
        }
    }

    #[test]
    fn rankdir_in_a_graph_attribute_list_is_honored_too() {
        // `graph [rankdir=LR]` used to be discarded with the whole attribute-list statement.
        let parsed = parse_dot("digraph G { graph [rankdir=LR, bgcolor=white]; a -> b; }");
        assert_eq!(parsed.ir.direction, fm_core::GraphDirection::LR);
    }

    #[test]
    fn graph_default_attribute_statements_do_not_become_nodes() {
        // `graph [...]`, `node [...]` and `edge [...]` set defaults; they are not nodes. If the node
        // parser claims them first, the diagram grows phantom boxes labelled graph/node/edge.
        let parsed = parse_dot(
            "digraph G { graph [bgcolor=white]; node [shape=box]; edge [color=red]; a -> b; }",
        );
        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"], "default statements must not create nodes");
    }

    #[test]
    fn unrecognized_rankdir_warns_and_leaves_the_default() {
        let parsed = parse_dot("digraph G { rankdir=diagonal; a -> b; }");
        assert_eq!(parsed.ir.direction, fm_core::GraphDirection::TB);
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("rankdir=diagonal")),
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn rankdir_does_not_collide_with_the_rank_group_attribute() {
        // `rank=same` and `rankdir=LR` share a prefix; a key match on "rank" would swallow rankdir
        // and turn a direction into a same-rank group.
        let parsed = parse_dot("digraph G { rankdir=LR; { rank=same; a; b; } a -> b; }");
        assert_eq!(parsed.ir.direction, fm_core::GraphDirection::LR);
        assert_eq!(
            parsed.ir.constraints.len(),
            1,
            "{:?}",
            parsed.ir.constraints
        );
    }

    #[test]
    fn edge_minlen_becomes_a_min_length_constraint() {
        let parsed = parse_dot("digraph G { a -> b [minlen=3]; }");

        assert_eq!(
            parsed.ir.constraints.len(),
            1,
            "{:?}",
            parsed.ir.constraints
        );
        match &parsed.ir.constraints[0] {
            fm_core::IrConstraint::MinLength {
                from_id,
                to_id,
                min_len,
                ..
            } => {
                assert_eq!(from_id, "a");
                assert_eq!(to_id, "b");
                assert_eq!(*min_len, 3);
            }
            other => panic!("expected MinLength, got {other:?}"),
        }
        // The edge itself is still an ordinary edge.
        assert_eq!(parsed.ir.edges.len(), 1);
    }

    #[test]
    fn minlen_one_is_the_dot_default_and_constrains_nothing() {
        // Every edge already spans at least one rank, so emitting a constraint for minlen=1 would
        // only inflate the solver's applied count.
        let parsed = parse_dot("digraph G { a -> b [minlen=1]; }");
        assert!(
            parsed.ir.constraints.is_empty(),
            "{:?}",
            parsed.ir.constraints
        );
        assert_eq!(parsed.ir.edges.len(), 1);
    }

    #[test]
    fn minlen_zero_warns_because_min_length_cannot_express_it() {
        // minlen=0 is DOT's "put both endpoints on one rank", which a MINIMUM gap cannot say.
        // Rounding it silently up to 1 would look like support.
        let parsed = parse_dot("digraph G { a -> b [minlen=0]; }");
        assert!(
            parsed.ir.constraints.is_empty(),
            "{:?}",
            parsed.ir.constraints
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("minlen=0") && warning.contains("rank=same")),
            "the warning must point at the construct that works: {:?}",
            parsed.warnings
        );
    }

    #[test]
    fn non_numeric_minlen_warns_and_is_ignored() {
        let parsed = parse_dot("digraph G { a -> b [minlen=wide]; }");
        assert!(parsed.ir.constraints.is_empty());
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("minlen=wide")),
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn minlen_applies_to_every_hop_of_a_chain() {
        // graphviz applies a shared attribute list to each edge in the statement.
        let parsed = parse_dot("digraph G { a -> b -> c [minlen=2]; }");
        assert_eq!(parsed.ir.edges.len(), 2);
        assert_eq!(
            parsed.ir.constraints.len(),
            2,
            "{:?}",
            parsed.ir.constraints
        );
    }

    #[test]
    fn minlen_survives_alongside_a_label_and_quoting() {
        for source in [
            "digraph G { a -> b [label=\"x\", minlen=2]; }",
            "digraph G { a -> b [minlen=\"2\"]; }",
            "digraph G { a -> b [MINLEN=2]; }",
        ] {
            let parsed = parse_dot(source);
            assert_eq!(
                parsed.ir.constraints.len(),
                1,
                "{source} produced {:?}",
                parsed.ir.constraints
            );
        }
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
