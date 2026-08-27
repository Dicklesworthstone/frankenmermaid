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
    // keywords are case-insensitive (`dot_header_kind` matches them case-insensitively). So a real DOT file
    // ALWAYS contains "graph" somewhere in its raw text. Class/state diagrams have `{ }` braces but no
    // `graph` keyword, so this cheap substring pre-guard short-circuits the expensive
    // `strip_all_comments` (whole-input `Vec<char>` collect + rescan) that dominated their detection.
    // Output-identical: comment stripping never introduces a `graph` substring that wasn't there.
    if !contains_ignore_ascii_case(bytes, b"graph") {
        return false;
    }
    let cleaned = strip_all_comments_cow(input);
    // A leading `%%{init: …}%%` is Mermaid metadata, not the start of the document's grammar, so the
    // header scan looks past it (bd-mqmx2). Without this a DOT file opening with a directive had no
    // recognisable header and was handed to the Mermaid parser, which cost it an edge and a node.
    if dot_header_kind(strip_leading_mermaid_directives(cleaned.as_ref())).is_none() {
        return false;
    }
    // `dot_header_kind` proved the body-opening `{` sits where the grammar puts it. The body must
    // also CLOSE: a well-formed DOT file ends with the body's closing `}` (modulo whitespace).
    // This resolves the residual `graph`-header ambiguity: `graph\n  A{decision} --> B` parses as
    // a DOT header (graph named `A`) up to the brace, but a Mermaid flowchart keeps writing
    // statements after its brace-shaped node, so the text does not end on `}`.
    let trimmed = cleaned.trim_end();
    if trimmed.ends_with('}') {
        return true;
    }
    // ⚠️ A TRAILING `%%{…}%%` DIRECTIVE IS NOT A GRAPH STATEMENT (bd-pdz8z). `strip_all_comments`
    // removes DOT's own comment forms and knows nothing about Mermaid's, so any directive appended
    // to a DOT document left the text ending in `%%` and this check said "not DOT". The document
    // then went to the Mermaid parser, which is far worse than the deck leak that surfaced it:
    // measured on `digraph G { a -> b }` plus one directive, the graph came back with ONE node
    // instead of two — the edge and a node were simply gone.
    //
    // Re-checking after dropping trailing directive lines makes a metadata directive unable to
    // change WHAT KIND of graph the document is, which is the property that was actually broken.
    // It cannot loosen the disambiguation above: `graph\n  A{decision} --> B` still ends on `B`
    // once its directives are dropped, so it is still not DOT.
    //
    // Reached only when the cheap check has already failed, so the common case — a real DOT file
    // ending on its brace, or anything that is not DOT at all — pays nothing for this.
    strip_trailing_mermaid_directives(trimmed).ends_with('}')
}

/// The input with any LEADING Mermaid `%%…%%` directive lines removed.
///
/// Mermaid documents routinely open with `%%{init: …}%%`, and a DOT document pasted into the same
/// pipeline reasonably may too. Before this, such a document lost its DOT header to the scan and
/// went to the Mermaid parser — measured on `%%{init: …}%%` before `digraph G { a -> b }`: ONE node
/// and ZERO edges, the same silent destruction the trailing case caused (bd-mqmx2, bd-pdz8z).
///
/// Only whole leading directive lines are dropped, so the first real statement is untouched.
fn strip_leading_mermaid_directives(input: &str) -> &str {
    let mut rest = input.trim_start();
    loop {
        let Some(line) = rest.lines().next() else {
            return rest;
        };
        let trimmed = line.trim();
        if !(trimmed.starts_with("%%") || trimmed.ends_with("%%")) {
            return rest;
        }
        let Some(cut) = rest.find('\n') else {
            // The whole remaining text is directive.
            return "";
        };
        rest = rest[cut + 1..].trim_start();
    }
}

/// The input with Mermaid `%%…%%` directive lines removed from BOTH ends.
///
/// One helper for both ends because they are one question — which lines are Mermaid metadata rather
/// than graph text — and answering it twice is how the two ends came to behave differently.
fn strip_mermaid_directives_around(input: &str) -> &str {
    strip_leading_mermaid_directives(strip_trailing_mermaid_directives(input))
}

/// The input with any trailing Mermaid `%%…%%` directive lines removed.
///
/// ⚠️ SHARED BY DETECTION AND PARSING, deliberately. `looks_like_dot` uses it to decide the document
/// ends on its body brace, and `parse_dot` uses it so `extract_body` — which spans the FIRST `{` to
/// the LAST `}` — does not swallow the braces inside `%%{deck: {…}}%%`. Two copies of "where does
/// this document end" is exactly how detection and parsing come to disagree: with only the
/// detection half fixed, `digraph G { a -> b }` plus one directive was correctly routed to the DOT
/// parser and then came back with SEVEN nodes instead of two.
///
/// A directive is recognised by its `%%` delimiters at either end of the line: an opening line
/// starts with `%%`, and the closing line of a multi-line directive ends with `%%` without starting
/// with it (`}}%%`). Blank lines between them go too. Nothing else is dropped, so a real graph
/// statement can never be trimmed away.
fn strip_trailing_mermaid_directives(input: &str) -> &str {
    let mut rest = input.trim_end();
    loop {
        let line = rest.lines().next_back().unwrap_or("").trim();
        if !(line.starts_with("%%") || line.ends_with("%%")) {
            return rest;
        }
        let Some(cut) = rest.rfind('\n') else {
            // The whole remaining text is directive.
            return "";
        };
        rest = rest[..cut].trim_end();
    }
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
    // Mermaid init directives apply to a DOT document too (bd-mqmx2). Run against the ORIGINAL
    // text, before the directive lines are stripped below — otherwise routing a directive-carrying
    // document correctly to this parser would silently ignore the directive, which is the same
    // class of quiet drop as sending the document to the wrong parser in the first place.
    crate::mermaid_parser::parse_init_directives(input, &mut builder);
    // Trailing Mermaid directives are not graph statements, and `extract_body` below spans the
    // first `{` to the LAST `}` — so a `%%{deck: {…}}%%` tail would extend the body over its own
    // braces and invent nodes out of the directive's text (bd-pdz8z). Same helper detection used to
    // decide this is DOT at all, so the two cannot disagree about where the document ends.
    let input = strip_mermaid_directives_around(input);
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

            // A bare `label=…` statement names the enclosing group — the standard DOT spelling for a
            // cluster title — or the whole graph at top level. Handled before the node parser, which
            // would otherwise read `label` as a node id and draw a stray box.
            if let Some(text) = parse_dot_label_statement(statement) {
                apply_dot_label_statement(
                    &text,
                    &active_clusters,
                    &active_subgraphs,
                    line_number,
                    line,
                    &mut builder,
                );
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
                // `graph [bgcolor=white]` is the attribute-list spelling of the bare statement.
                if is_graph_defaults
                    && active_clusters.is_empty()
                    && let Some(value) = extract_dot_attribute_raw(statement, "bgcolor")
                {
                    let color = value.as_ref().trim().trim_matches(['"', '\'']).trim();
                    if !color.is_empty() {
                        builder.insert_theme_variable("background".to_string(), color.to_string());
                    }
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

            // `bgcolor` is the graph's background, which maps onto the Mermaid theme variable the SVG
            // renderer already honors. Handled before the generic consumer below, which would
            // otherwise ignore it with a warning.
            if let Some(color) = parse_dot_named_attribute(statement, "bgcolor") {
                // A cluster's own bgcolor is a DIFFERENT thing — the group's fill, which needs an
                // IrStyleTarget::Cluster this IR does not have — so only the graph-level one is taken,
                // and an in-cluster one is reported rather than silently applied to the whole canvas.
                if active_clusters.is_empty() {
                    builder.insert_theme_variable("background".to_string(), color);
                } else {
                    builder.add_warning(format!(
                        "Line {line_number}: DOT bgcolor inside a subgraph sets that cluster's fill, \
                         which is not supported; it was ignored rather than applied to the whole \
                         diagram"
                    ));
                }
                continue;
            }

            // `nodesep` / `ranksep` are spacing requests. graphviz measures them in INCHES, so they
            // are converted to layout units at 72 per inch — the same points-per-inch convention the
            // font-size mapping uses, chosen for consistency rather than because either is exact.
            if let Some(inches) = parse_dot_named_attribute(statement, "nodesep") {
                apply_dot_spacing(
                    &inches,
                    "nodesep",
                    line_number,
                    &mut builder,
                    |builder, units| {
                        builder.set_node_spacing(units);
                    },
                );
                continue;
            }
            if let Some(inches) = parse_dot_named_attribute(statement, "ranksep") {
                apply_dot_spacing(
                    &inches,
                    "ranksep",
                    line_number,
                    &mut builder,
                    |builder, units| {
                        builder.set_rank_spacing(units);
                    },
                );
                continue;
            }

            // `splines` chooses how edges are drawn, which the layout implements as
            // EdgeRouting::Orthogonal or ::Spline.
            if let Some(value) = parse_dot_named_attribute(statement, "splines") {
                match fm_core::MermaidEdgeRoutingHint::from_dot_splines(&value) {
                    Some(hint) => builder.set_edge_routing_hint(hint),
                    None => builder.add_warning(format!(
                        "Line {line_number}: DOT splines={value} is not recognized and was ignored; \
                         expected ortho, polyline, line, curved, spline, true, or false"
                    )),
                }
                continue;
            }

            // Every remaining bare `key=value` is a graph attribute this engine does not implement.
            // Consumed here — AHEAD of the node parser — so it cannot become a phantom node, and
            // named in a warning so an ignored attribute does not read as a supported one. The keys
            // that ARE implemented (rankdir, label, rank) are matched earlier and never reach this.
            if let Some(key) = parse_dot_graph_attribute_key(statement) {
                builder.add_warning(format!(
                    "Line {line_number}: DOT graph attribute '{key}' is not supported and was ignored"
                ));
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

/// Recognize a bare `<name>=<value>` statement and return its value.
///
/// Shares the identifier-key discipline of [`parse_dot_graph_attribute_key`], so an attribute list
/// that happens to mention `name` (`a [bgcolor=red]`) is not mistaken for the bare statement.
fn parse_dot_named_attribute(statement: &str, name: &str) -> Option<String> {
    let key = parse_dot_graph_attribute_key(statement)?;
    if !key.eq_ignore_ascii_case(name) {
        return None;
    }
    let (_, value) = statement.split_once('=')?;
    let text = value.trim().trim_matches(['"', '\'']).trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Recognize a bare `key=value` graph-attribute statement and return its key.
///
/// Per the DOT grammar an `ID = ID` at statement position sets a graph attribute. The node parser
/// accepts almost anything as an id and runs late in the dispatch chain, so without this every
/// unimplemented graph attribute became a stray box: `bgcolor=white` drew a node called `bgcolor`.
/// Consuming them generically closes that whole family rather than one attribute at a time.
///
/// The key must be a plain DOT identifier — letters, digits, underscore, or a quoted string — which
/// is what keeps `a [label="x"]` out: its first `=` sits inside an attribute list, so the text before
/// it is not an identifier. A statement carrying an edge operator is also excluded, since `a=b -> c`
/// belongs to the edge parser.
fn parse_dot_graph_attribute_key(statement: &str) -> Option<&str> {
    if find_edge_operator(statement).is_some() {
        return None;
    }
    let (key, _) = statement.split_once('=')?;
    let key = key.trim().trim_matches(['"', '\'']).trim();
    let is_identifier = !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.');
    is_identifier.then_some(key)
}

/// Recognize a bare `label=<value>` statement and return its text.
///
/// Deliberately narrow: the statement must be exactly a `label` assignment, so an edge or node
/// statement that merely carries a label attribute (`a [label="x"]`) is left to its own parser.
fn parse_dot_label_statement(statement: &str) -> Option<String> {
    let (key, value) = statement.split_once('=')?;
    if !key.trim().eq_ignore_ascii_case("label") {
        return None;
    }
    parse_dot_label_value(value)
}

/// Apply a bare `label=…` statement to the innermost group, or to the diagram at top level.
fn apply_dot_label_statement(
    text: &str,
    active_clusters: &[usize],
    active_subgraphs: &[usize],
    line_number: usize,
    source_line: &str,
    builder: &mut IrBuilder,
) {
    let span = span_for(line_number, source_line);
    match active_clusters.last() {
        Some(&cluster_index) => {
            builder.set_cluster_title(cluster_index, text, span);
            // Keep the subgraph view in step: a renderer reading subgraph titles must not see a
            // different name from one reading cluster titles.
            if let Some(&subgraph_index) = active_subgraphs.last() {
                builder.set_subgraph_title(subgraph_index, text, span);
            }
        }
        // At top level `label=` is the GRAPH's label, which is the diagram title.
        None => builder.set_title(text.to_string()),
    }
}

/// DOT measures spacing in inches; the layout works in units. 72 per inch matches the
/// points-per-inch convention used for `fontsize`.
const DOT_UNITS_PER_INCH: f64 = 72.0;

/// Convert a DOT inch measurement to whole layout units and hand it to `apply`, or warn.
///
/// `ranksep` also accepts `equally` and a list form (`"0.5 equally"`); only the leading number is
/// used, and a value with no leading number is reported rather than silently ignored.
fn apply_dot_spacing(
    value: &str,
    key: &str,
    line_number: usize,
    builder: &mut IrBuilder,
    apply: impl FnOnce(&mut IrBuilder, u32),
) {
    let leading = value.split_whitespace().next().unwrap_or_default();
    match leading.parse::<f64>() {
        Ok(inches) if inches.is_finite() && inches >= 0.0 => {
            // `round` not `as`: a truncating cast would turn nodesep=0.49 into 35 units rather than
            // the nearer 35.28 -> 35, and would silently floor every fractional request.
            let units = (inches * DOT_UNITS_PER_INCH).round();
            let units = units.clamp(0.0, f64::from(u32::MAX)) as u32;
            apply(builder, units);
        }
        _ => builder.add_warning(format!(
            "Line {line_number}: DOT {key}={value} is not a non-negative number of inches and was \
             ignored"
        )),
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

/// Classify the leading DOT header of comment-stripped input.
///
/// Returns `Some(true)` for `digraph`, `Some(false)` for `graph`, and `None` when the input does
/// not open with a DOT header. The DOT grammar is `[strict] (graph | digraph) [ID] '{' ...`, so
/// the header is recognised only when the body-opening `{` actually follows the optional graph
/// name — possibly on a later line, since DOT is whitespace-insensitive. Checking the keyword
/// alone is not enough: Mermaid's legacy `graph TD` header shares the `graph` keyword, and a
/// flowchart whose *labels* contain braces (`A["uses {binding}"]`) must not be reclassified as
/// DOT just because a brace exists somewhere in the text. Requiring the brace at the grammar's
/// position is what tells the two apart: `graph TD {` is a DOT graph named `TD`, while
/// `graph TD\nA[...]` is Mermaid.
fn dot_header_kind(cleaned_input: &str) -> Option<bool> {
    let mut cursor = cleaned_input.trim_start();
    if let Some(rest) = strip_dot_keyword(cursor, "strict") {
        // `strict` must be its own word; `strict{` / `strictdigraph` are not headers.
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        cursor = rest.trim_start();
    }
    let (directed, rest) = if let Some(rest) = strip_dot_keyword(cursor, "digraph") {
        (true, rest)
    } else {
        let rest = strip_dot_keyword(cursor, "graph")?;
        (false, rest)
    };
    let after_id = skip_dot_id(rest.trim_start())?;
    after_id.trim_start().starts_with('{').then_some(directed)
}

/// Strip a case-insensitive DOT keyword from the front of `text`, requiring a token boundary
/// after it so that `graphTD` or `digraphs` are not mistaken for `graph` / `digraph`.
fn strip_dot_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let head = text.as_bytes().get(..keyword.len())?;
    if !head.eq_ignore_ascii_case(keyword.as_bytes()) {
        return None;
    }
    let rest = &text[keyword.len()..];
    let boundary = rest
        .chars()
        .next()
        .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
    boundary.then_some(rest)
}

/// Skip an optional DOT `ID` at the front of `text` and return what follows it.
///
/// A DOT `ID` is one of: an identifier / numeral (letters, digits, `_`, plus `.` and `-` for
/// numerals), a double-quoted string (with `\"` escapes), or an HTML string delimited by balanced
/// `<` `>`. An absent `ID` returns `text` unchanged; an unterminated quoted or HTML `ID` returns
/// `None` because no well-formed DOT header can follow it.
fn skip_dot_id(text: &str) -> Option<&str> {
    let mut chars = text.char_indices();
    match chars.next() {
        Some((_, '"')) => {
            let mut remainder = skip_dot_quoted_string(text)?;
            // DOT permits `"a" + "b"` concatenation of quoted IDs; every operand must be quoted.
            while let Some(next) = remainder.trim_start().strip_prefix('+') {
                remainder = skip_dot_quoted_string(next.trim_start())?;
            }
            Some(remainder)
        }
        Some((_, '<')) => {
            let mut depth = 1_usize;
            for (idx, ch) in chars {
                match ch {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(&text[idx + 1..]);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        _ => {
            let end = text
                .char_indices()
                .find(|(_, ch)| !(ch.is_alphanumeric() || matches!(ch, '_' | '.' | '-')))
                .map_or(text.len(), |(idx, _)| idx);
            Some(&text[end..])
        }
    }
}

/// Skip one double-quoted DOT string (with `\"` escapes) at the front of `text`, returning what
/// follows the closing quote. `None` when `text` does not start with `"` or the string never ends.
fn skip_dot_quoted_string(text: &str) -> Option<&str> {
    let mut chars = text.char_indices();
    if !matches!(chars.next(), Some((_, '"'))) {
        return None;
    }
    let mut escaped = false;
    for (idx, ch) in chars {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(&text[idx + 1..]);
        }
    }
    None
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

    /// Regression for the `graph TD` + brace-in-label misdetection: a Mermaid legacy header must
    /// stay Mermaid even when a node label contains `{ }`, because the DOT body brace has to
    /// follow the header's optional graph name, not appear somewhere inside a label.
    #[test]
    fn mermaid_graph_header_with_braces_in_labels_is_not_dot() {
        assert!(!looks_like_dot(
            "graph TD\n  A[\"captures {binding} tokens\"] --> B[\"done\"]"
        ));
        assert!(!looks_like_dot("graph LR;\n  A{decision} --> B"));
        assert!(!looks_like_dot("graph\n  A{decision} --> B"));
        assert!(!looks_like_dot(
            "graph TB\n  subgraph one\n    A[\"{x}\"] --> B\n  end"
        ));
        assert!(!looks_like_dot("GRAPH TD\n  A --> B{yes?}"));
    }

    #[test]
    fn dot_header_still_detected_with_mermaid_looking_graph_names() {
        // A DOT graph may legitimately be *named* TD/LR: the body brace is what decides.
        assert!(looks_like_dot("graph TD { a -- b; }"));
        assert!(looks_like_dot("digraph LR\n{\n  a -> b;\n}"));
        assert!(looks_like_dot("strict graph\n  G\n{ a -- b; }"));
        assert!(looks_like_dot("digraph 42 { a -> b; }"));
        assert!(looks_like_dot("digraph \"name {brace}\" { a -> b; }"));
        assert!(looks_like_dot("digraph \"esc \\\" quote\" { a -> b; }"));
        assert!(looks_like_dot("digraph \"multi\" + \"part\" { a -> b; }"));
        assert!(looks_like_dot("digraph <<b>{name}</b>> { a -> b; }"));
    }

    #[test]
    fn dot_header_rejects_keyword_fragments_and_unterminated_names() {
        assert!(!looks_like_dot("graphTD { a -- b; }"));
        assert!(!looks_like_dot("strictdigraph { a -> b; }"));
        assert!(!looks_like_dot("strict{ a -> b; }"));
        assert!(!looks_like_dot("digraph \"unterminated { a -> b; }"));
        assert!(!looks_like_dot("digraph <unterminated { a -> b; }"));
        assert!(!looks_like_dot("digraph G; { a -> b; }"));
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
    fn splines_maps_to_an_edge_routing_hint() {
        use fm_core::MermaidEdgeRoutingHint;
        for (value, expected) in [
            ("ortho", MermaidEdgeRoutingHint::Orthogonal),
            ("polyline", MermaidEdgeRoutingHint::Orthogonal),
            ("line", MermaidEdgeRoutingHint::Orthogonal),
            ("false", MermaidEdgeRoutingHint::Orthogonal),
            ("curved", MermaidEdgeRoutingHint::Curved),
            ("spline", MermaidEdgeRoutingHint::Curved),
            ("true", MermaidEdgeRoutingHint::Curved),
        ] {
            let parsed = parse_dot(&format!("digraph G {{ splines={value}; a -> b; }}"));
            assert_eq!(
                parsed.ir.meta.edge_routing,
                Some(expected),
                "splines={value}"
            );
        }
    }

    #[test]
    fn absent_splines_leaves_the_routing_hint_unset() {
        // The control: `None` is what lets the caller's LayoutConfig stand.
        let parsed = parse_dot("digraph G { a -> b; }");
        assert_eq!(parsed.ir.meta.edge_routing, None);
    }

    #[test]
    fn unrecognized_splines_warns_and_sets_no_hint() {
        let parsed = parse_dot("digraph G { splines=zigzag; a -> b; }");
        assert_eq!(parsed.ir.meta.edge_routing, None);
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("splines=zigzag")),
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn nodesep_and_ranksep_convert_inches_to_layout_units() {
        // graphviz measures both in inches; 72 units per inch is the documented conversion.
        let parsed = parse_dot("digraph G { nodesep=0.5; ranksep=1.0; a -> b; }");
        assert_eq!(parsed.ir.meta.node_spacing, Some(36));
        assert_eq!(parsed.ir.meta.rank_spacing, Some(72));

        // Rounding, not truncation: 0.49in is 35.28 units, which must land on 35 rather than being
        // floored by a cast.
        let parsed = parse_dot("digraph G { nodesep=0.49; a -> b; }");
        assert_eq!(parsed.ir.meta.node_spacing, Some(35));
    }

    #[test]
    fn absent_spacing_leaves_the_hints_unset() {
        // The control: `None` is what tells the layout to keep its own default, so an unconditional
        // Some(0) would silently collapse every diagram's spacing.
        let parsed = parse_dot("digraph G { a -> b; }");
        assert_eq!(parsed.ir.meta.node_spacing, None);
        assert_eq!(parsed.ir.meta.rank_spacing, None);
    }

    #[test]
    fn ranksep_takes_the_leading_number_of_a_list_form() {
        // DOT allows `ranksep="0.75 equally"`; only the number is usable here.
        let parsed = parse_dot("digraph G { ranksep=\"0.75 equally\"; a -> b; }");
        assert_eq!(parsed.ir.meta.rank_spacing, Some(54));
    }

    #[test]
    fn non_numeric_spacing_warns_and_leaves_the_default() {
        let parsed = parse_dot("digraph G { nodesep=equally; ranksep=wide; a -> b; }");
        assert_eq!(parsed.ir.meta.node_spacing, None);
        assert_eq!(parsed.ir.meta.rank_spacing, None);
        for key in ["nodesep", "ranksep"] {
            assert!(
                parsed.warnings.iter().any(|w| w.contains(key)),
                "{key} must be reported: {:?}",
                parsed.warnings
            );
        }
    }

    #[test]
    fn spacing_statements_do_not_become_nodes() {
        let parsed = parse_dot("digraph G { nodesep=0.5; ranksep=1.0; a -> b; }");
        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"], "{ids:?}");
    }

    #[test]
    fn bgcolor_sets_the_theme_background() {
        for source in [
            "digraph G { bgcolor=lightyellow; a -> b; }",
            "digraph G { bgcolor=\"lightyellow\"; a -> b; }",
            "digraph G { graph [bgcolor=lightyellow]; a -> b; }",
        ] {
            let parsed = parse_dot(source);
            assert_eq!(
                parsed
                    .ir
                    .meta
                    .theme_overrides
                    .theme_variables
                    .get("background")
                    .map(String::as_str),
                Some("lightyellow"),
                "{source}"
            );
            let ids: Vec<&str> = parsed
                .ir
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect();
            assert_eq!(ids, ["a", "b"], "{source}");
        }
    }

    #[test]
    fn no_bgcolor_leaves_the_theme_untouched() {
        // The control: an unconditional insert would make the assertions above vacuous.
        let parsed = parse_dot("digraph G { a -> b; }");
        assert!(
            parsed.ir.meta.theme_overrides.theme_variables.is_empty(),
            "{:?}",
            parsed.ir.meta.theme_overrides.theme_variables
        );
    }

    #[test]
    fn bgcolor_inside_a_cluster_is_refused_rather_than_recolouring_the_canvas() {
        // A cluster's bgcolor is that GROUP's fill, which needs an IrStyleTarget::Cluster this IR does
        // not have. Applying it to the whole diagram would be visibly wrong, so it is reported.
        let parsed = parse_dot("digraph G { subgraph cluster_0 { bgcolor=red; a; } }");
        assert!(
            !parsed
                .ir
                .meta
                .theme_overrides
                .theme_variables
                .contains_key("background"),
            "a cluster bgcolor must not become the diagram background"
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("bgcolor")),
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn bare_graph_attributes_are_consumed_not_turned_into_nodes() {
        // Per the DOT grammar a bare `ID = ID` at statement position sets a graph attribute. The node
        // parser accepts almost anything as an id and runs late in the dispatch chain, so every such
        // attribute this engine does not implement used to become a stray box — the same family as
        // the graph/node/edge defaults and the cluster label statement.
        let parsed = parse_dot(
            "digraph G { bgcolor=white; ratio=fill; size=\"6,6\"; splines=ortho; nodesep=0.5; ranksep=1.2; a -> b; }",
        );

        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["a", "b"],
            "graph attributes must not become nodes: {ids:?}"
        );

        // Degrading silently would be worse than a stray box: it would look like support. Each
        // unimplemented attribute is named in a warning. `bgcolor`, `nodesep` and `ranksep` are
        // deliberately absent from this list — they ARE implemented now, so warning about them would
        // be the false report. `splines` joined them once EdgeRouting::Spline was made to do
        // something (bd-hfaw). `ratio` and `size` are canvas-fitting semantics with no equivalent
        // here, so they may stay on this list.
        for key in ["ratio", "size"] {
            assert!(
                parsed
                    .ir
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains(key))
                    || parsed.warnings.iter().any(|w| w.contains(key)),
                "{key} must be reported as unsupported: {:?}",
                parsed.warnings
            );
        }
    }

    #[test]
    fn the_graph_attribute_consumer_does_not_swallow_nodes_or_edges() {
        // The boundary a generic `key=value` consumer must respect. Each of these carries an `=` and
        // must still parse as what it is.
        let parsed = parse_dot(
            "digraph G { bgcolor=white; a [shape=box]; b [label=\"B\"]; a -> b [minlen=2]; }",
        );

        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"], "{ids:?}");
        assert_eq!(
            parsed.ir.nodes[0].shape,
            NodeShape::Rect,
            "shape=box → rect"
        );
        assert_eq!(parsed.ir.edges.len(), 1, "the edge must survive");
        assert_eq!(
            parsed.ir.constraints.len(),
            1,
            "minlen on the edge must still constrain: {:?}",
            parsed.ir.constraints
        );
    }

    #[test]
    fn a_quoted_attribute_key_is_still_consumed() {
        // DOT allows quoted ids, so `"bgcolor"=white` is the same statement and must not become a
        // node called `"bgcolor"`.
        let parsed = parse_dot("digraph G { \"bgcolor\"=white; a -> b; }");
        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"], "{ids:?}");
    }

    #[test]
    fn top_level_label_statement_becomes_the_diagram_title() {
        let parsed = parse_dot("digraph G { label=\"System Overview\"; a -> b; }");
        assert_eq!(
            parsed.ir.meta.title.as_deref(),
            Some("System Overview"),
            "a graph-level label is the diagram title"
        );
        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn a_node_label_attribute_is_not_mistaken_for_a_label_statement() {
        // The narrowness check: `a [label="x"]` must stay a node with a label, not be swallowed as a
        // group-naming statement.
        let parsed = parse_dot("digraph G { a [label=\"Alpha\"]; }");
        assert_eq!(parsed.ir.nodes.len(), 1);
        assert_eq!(parsed.ir.nodes[0].id, "a");
        let label = parsed.ir.nodes[0]
            .label
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|label| label.text.as_str());
        assert_eq!(label, Some("Alpha"));
        assert_eq!(parsed.ir.meta.title, None, "a node label is not a title");
    }

    #[test]
    fn nested_cluster_labels_attach_to_their_own_cluster() {
        let parsed = parse_dot(
            "digraph G { subgraph cluster_0 { label=\"Outer\"; subgraph cluster_1 { label=\"Inner\"; x; } y; } }",
        );
        let title_of = |index: usize| {
            parsed.ir.clusters[index]
                .title
                .and_then(|id| parsed.ir.labels.get(id.0))
                .map(|label| label.text.as_str())
        };
        assert_eq!(parsed.ir.clusters.len(), 2);
        assert_eq!(title_of(0), Some("Outer"));
        assert_eq!(
            title_of(1),
            Some("Inner"),
            "the inner label must not overwrite the outer cluster's title"
        );
    }

    #[test]
    fn cluster_label_statement_sets_the_title_and_makes_no_node() {
        // `label="…"` inside a subgraph body is how DOT names a cluster — it is the standard
        // spelling, not an edge case. It must set the cluster title, and it must NOT be read as a
        // node id, which would draw a stray box labelled `label`.
        let parsed = parse_dot("digraph G { subgraph cluster_0 { label=\"Backend\"; a; b; } }");

        let ids: Vec<&str> = parsed
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"], "a label statement must not become a node");
        assert_eq!(parsed.ir.clusters.len(), 1);
        let title = parsed.ir.clusters[0]
            .title
            .and_then(|label_id| parsed.ir.labels.get(label_id.0))
            .map(|label| label.text.as_str());
        assert_eq!(
            title,
            Some("Backend"),
            "the cluster must take its title from the label statement"
        );
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
