#![forbid(unsafe_code)]

mod dot_parser;
mod ir_builder;
mod mermaid_parser;

use std::sync::Arc;

use fm_core::{
    DiagramType, MermaidDiagramIr, MermaidLensBinding, MermaidLensEdit, MermaidLensEditResult,
    MermaidLensError, MermaidParseMode, MermaidSourceMap, MermaidSourceMapKind, MermaidTextRange,
    Position, Span, apply_lens_edit, build_lens_bindings, resolve_span_text_range,
};
use serde::Serialize;
use serde_json::json;
use unicode_segmentation::UnicodeSegmentation;

pub use dot_parser::{looks_like_dot, parse_dot};
pub use mermaid_parser::first_significant_line;

/// Normalize a Mermaid identifier by trimming, stripping quotes, and replacing unsafe characters
/// with underscores -- BORROWING when the input already normalizes to itself.
///
/// This ensures consistent node identity across the engine and safe identifiers for backend layout
/// engines and rendering formats.
///
/// The ledger has carried this as a deferred lever since 2026-07-12: "`normalize_identifier`'s
/// remaining self-time is its `to_owned()` alloc on the fast path -- a structural Cow/borrow lever
/// (return `&str`/`Cow` when unchanged so the interner clones once instead of twice), deferred as
/// it touches ~30 callers."
///
/// This lands the borrow WITHOUT touching those callers. `normalize_identifier` keeps its `String`
/// signature and delegates here, so all 49 existing call sites are byte-identical and pay exactly
/// what they paid before. A call site that only reads or compares the result can switch to this one
/// and pay nothing.
///
/// The fast path -- an id already made of `[A-Za-z0-9_./-]` with no trailing `_` -- is the
/// overwhelmingly common case for generated node ids, and returns `Cow::Borrowed` into the caller's
/// own input rather than a fresh allocation.
#[must_use]
pub fn normalize_identifier_cow(raw: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let trimmed = crate::mermaid_parser::trim_fast(raw);
    if trimmed.is_empty() {
        return Cow::Borrowed("");
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
        return Cow::Borrowed("");
    }

    // THE LEVER. This branch previously ended in `cleaned.to_owned()` -- one heap allocation per
    // identifier, on the hottest path in the parser. It now borrows.
    let cleaned_bytes = cleaned.as_bytes();
    if cleaned_bytes[cleaned_bytes.len() - 1] != b'_'
        && cleaned_bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/'))
    {
        return Cow::Borrowed(cleaned);
    }

    Cow::Owned(normalize_identifier_rebuild(cleaned, was_quoted))
}

/// Owning slow path: the char-by-char rebuild, its `_`-trim, the grapheme fallback and the hashed
/// last resort. Unchanged from the original function body; only its home moved.
fn normalize_identifier_rebuild(cleaned: &str, was_quoted: bool) -> String {
    let mut out = String::with_capacity(cleaned.len());
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/') {
            out.push(ch);
        } else if ch.is_whitespace() {
            // Replace spaces with underscores for all identifiers to ensure they are safe
            // for layout engines and other backends, while preserving the intent of
            // multi-word identifiers (especially quoted ones).
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

    // `out` is a fresh owned String; the old `out.trim_end_matches('_').to_string()` reallocated a
    // second buffer on EVERY identifier even when nothing is trimmed (no trailing `_` — the common
    // case, e.g. "Event_0"). `trim_end_matches` only removes from the tail, so the kept text is the
    // `out[..k]` prefix — truncate in place and move `out` instead of copying. Byte-identical.
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
        let has_alphanumeric = cleaned.chars().any(|ch| ch.is_alphanumeric());
        if has_alphanumeric {
            result = format!("id_{:x}", fnv1a_hash(cleaned.as_bytes()));
        }
    }

    result
}

/// Normalize an identifier to an owned `String`.
///
/// Delegates to [`normalize_identifier_cow`]; byte-identical to the previous implementation. Kept
/// so the existing call sites need no change.
#[must_use]
pub fn normalize_identifier(raw: &str) -> String {
    normalize_identifier_cow(raw).into_owned()
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParseResult {
    pub ir: MermaidDiagramIr,
    pub warnings: Vec<String>,
    /// Detection confidence (0.0 to 1.0)
    pub confidence: f32,
    /// Method used for type detection
    pub detection_method: DetectionMethod,
    /// Raw-format trivia captured alongside the parsed IR.
    pub format_complement: MermaidFormatComplement,
}

impl ParseResult {
    #[must_use]
    pub const fn parse_mode(&self) -> MermaidParseMode {
        self.ir.meta.parse_mode
    }
}

/// Exact immutable prefix certified by a batch parser for downstream layout/render reuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowchartBatchPrefix {
    /// Pointer-stable identity of the exact source prefix compiled by this plan.
    pub identity: Arc<str>,
    /// Number of leading IR nodes proven unchanged after suffix lowering.
    pub node_count: usize,
    /// Number of leading IR edges proven unchanged after suffix lowering.
    pub edge_count: usize,
}

/// Borrowed parse result backed by a caller-owned reusable batch slot.
#[derive(Debug, Clone, Copy)]
pub struct FlowchartBatchParseRef<'a> {
    pub ir: &'a MermaidDiagramIr,
    pub warnings: &'a [String],
    pub confidence: f32,
    pub detection_method: DetectionMethod,
    pub reusable_prefix: Option<&'a FlowchartBatchPrefix>,
}

/// Per-worker storage for [`FlowchartBatchParsePlan::with_parse_scratch`].
///
/// Keep one beside each batch renderer. Repeated suffix parses then overwrite the same builder
/// slot, reusing prefix vector and string allocations without locks or cross-worker ownership.
#[derive(Default)]
pub struct FlowchartBatchParseScratch {
    inner: mermaid_parser::CompiledFlowchartScratch,
}

/// Work eliminated by a [`FlowchartBatchParsePlan`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct FlowchartBatchParseStats {
    /// Distinct exact subgraph prefixes compiled once for two or more inputs.
    pub shared_prefix_groups: usize,
    /// Inputs served from a compiled prefix, including each group's compiling owner.
    pub shared_prefix_inputs: usize,
    /// Repeated prefix parses removed (`group_len - 1` summed across groups).
    pub reused_prefix_parses: usize,
    /// Source bytes that no longer pass through tokenization/lowering.
    pub reused_prefix_bytes: usize,
}

/// Immutable batch compilation plan for diagrams with repeated prefix subgraphs.
///
/// The ordinary parser remains the fallback for every ungrouped input. When two or more explicit
/// flowcharts begin with the same complete, closed subgraph block, the plan lowers that prefix once
/// and seeds a caller-owned builder slot before parsing each suffix. Callers may invoke
/// [`Self::parse`] concurrently; callers pursuing allocation reuse give each worker an independent
/// [`FlowchartBatchParseScratch`] through [`Self::with_parse_scratch`].
pub struct FlowchartBatchParsePlan {
    compiled: Vec<mermaid_parser::CompiledFlowchartPrefix>,
    assignment: Vec<Option<usize>>,
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    stats: FlowchartBatchParseStats,
}

impl FlowchartBatchParsePlan {
    #[must_use]
    pub fn new(inputs: &[&str], parse_mode: MermaidParseMode, config: &ParserConfig) -> Self {
        if let Some(prefix) = inputs
            .first()
            .and_then(|input| mermaid_parser::reusable_flowchart_prefix(input))
            && inputs.len() >= 2
            && inputs
                .iter()
                .all(|input| mermaid_parser::can_reuse_flowchart_prefix(input, prefix))
            && let Some(prefix_parser) =
                mermaid_parser::CompiledFlowchartPrefix::new(prefix, parse_mode, config)
        {
            return Self {
                compiled: vec![prefix_parser],
                assignment: vec![Some(0); inputs.len()],
                parse_mode,
                parser_config: *config,
                stats: FlowchartBatchParseStats {
                    shared_prefix_groups: 1,
                    shared_prefix_inputs: inputs.len(),
                    reused_prefix_parses: inputs.len() - 1,
                    reused_prefix_bytes: prefix.len().saturating_mul(inputs.len() - 1),
                },
            };
        }

        // The largest reusable prefix can differ because later leading subgraphs are unique even
        // when an earlier, expensive subgraph is shared by the entire batch. Find the byte LCP in
        // one streaming pass, then snap it back to the last complete subgraph boundary. This keeps
        // grouping O(total shared bytes), avoiding ordered comparisons of every long prefix.
        if let Some(first) = inputs.first()
            && inputs.len() >= 2
        {
            let first_bytes = first.as_bytes();
            let common_len = inputs
                .iter()
                .skip(1)
                .fold(first_bytes.len(), |limit, input| {
                    let input_bytes = input.as_bytes();
                    let compared = limit.min(input_bytes.len());
                    first_bytes[..compared]
                        .iter()
                        .zip(&input_bytes[..compared])
                        .position(|(left, right)| left != right)
                        .unwrap_or(compared)
                });
            if let Some(prefix) =
                mermaid_parser::reusable_flowchart_prefix_at_or_before(first, common_len)
                && inputs
                    .iter()
                    .all(|input| mermaid_parser::can_reuse_flowchart_prefix(input, prefix))
                && let Some(prefix_parser) =
                    mermaid_parser::CompiledFlowchartPrefix::new(prefix, parse_mode, config)
            {
                return Self {
                    compiled: vec![prefix_parser],
                    assignment: vec![Some(0); inputs.len()],
                    parse_mode,
                    parser_config: *config,
                    stats: FlowchartBatchParseStats {
                        shared_prefix_groups: 1,
                        shared_prefix_inputs: inputs.len(),
                        reused_prefix_parses: inputs.len() - 1,
                        reused_prefix_bytes: prefix.len().saturating_mul(inputs.len() - 1),
                    },
                };
            }
        }

        let mut groups: std::collections::BTreeMap<&str, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (index, input) in inputs.iter().enumerate() {
            if let Some(prefix) = mermaid_parser::reusable_flowchart_prefix(input) {
                groups.entry(prefix).or_default().push(index);
            }
        }

        let mut compiled = Vec::new();
        let mut assignment = vec![None; inputs.len()];
        let mut stats = FlowchartBatchParseStats::default();
        for (prefix, indexes) in groups {
            if indexes.len() < 2 {
                continue;
            }
            let Some(prefix_parser) =
                mermaid_parser::CompiledFlowchartPrefix::new(prefix, parse_mode, config)
            else {
                continue;
            };
            let compiled_index = compiled.len();
            compiled.push(prefix_parser);
            for &index in &indexes {
                assignment[index] = Some(compiled_index);
            }
            stats.shared_prefix_groups += 1;
            stats.shared_prefix_inputs += indexes.len();
            stats.reused_prefix_parses += indexes.len() - 1;
            stats.reused_prefix_bytes += prefix.len().saturating_mul(indexes.len() - 1);
        }

        Self {
            compiled,
            assignment,
            parse_mode,
            parser_config: *config,
            stats,
        }
    }

    /// Parse one input at the same index used to construct the plan.
    ///
    /// A mismatched index/input or an input outside a reusable group takes the standard full parser
    /// path, preserving the public parser's behavior instead of turning cache eligibility into a
    /// correctness requirement.
    #[must_use]
    pub fn parse(&self, index: usize, input: &str) -> ParseResult {
        self.assignment
            .get(index)
            .and_then(|entry| *entry)
            .and_then(|compiled_index| self.compiled.get(compiled_index))
            .and_then(|compiled| compiled.parse(input))
            .unwrap_or_else(|| {
                parse_with_mode_and_config(input, self.parse_mode, &self.parser_config)
            })
    }

    /// Parse one input into a caller-owned reusable slot and borrow the result for `consume`.
    ///
    /// The borrowed result cannot escape `consume`, which lets the next diagram reuse its backing
    /// allocations. Inputs outside a certified group transparently use the ordinary parser.
    pub fn with_parse_scratch<R>(
        &self,
        index: usize,
        input: &str,
        scratch: &mut FlowchartBatchParseScratch,
        consume: impl FnOnce(FlowchartBatchParseRef<'_>) -> R,
    ) -> R {
        let compiled = self
            .assignment
            .get(index)
            .and_then(|entry| *entry)
            .and_then(|compiled_index| self.compiled.get(compiled_index));
        if let Some(compiled) = compiled
            && let Some(parsed) = compiled.parse_with_scratch(input, &mut scratch.inner)
        {
            return consume(parsed);
        }

        let parsed = parse_with_mode_and_config(input, self.parse_mode, &self.parser_config);
        consume(FlowchartBatchParseRef {
            ir: &parsed.ir,
            warnings: &parsed.warnings,
            confidence: parsed.confidence,
            detection_method: parsed.detection_method,
            reusable_prefix: None,
        })
    }

    #[must_use]
    pub const fn stats(&self) -> FlowchartBatchParseStats {
        self.stats
    }

    /// Return the plan-local reusable-prefix group assigned to `index`.
    ///
    /// Batch executors can use this opaque index to keep one downstream layout/render state per
    /// compiled prefix. The value is meaningful only for this plan and deliberately exposes no
    /// parser internals; `None` means the input takes the ordinary full-parser path.
    #[must_use]
    pub fn reusable_prefix_group(&self, index: usize) -> Option<usize> {
        self.assignment.get(index).copied().flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MermaidLineEndingStyle {
    #[default]
    None,
    Lf,
    Crlf,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MermaidWhitespaceKind {
    Indent,
    InterToken,
    Trailing,
    BlankLine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MermaidWhitespaceSpan {
    pub kind: MermaidWhitespaceKind,
    pub span: Span,
    pub text_range: MermaidTextRange,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MermaidCommentSpan {
    pub span: Span,
    pub text_range: MermaidTextRange,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MermaidDirectiveSpan {
    pub span: Span,
    pub text_range: MermaidTextRange,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MermaidQuoteStyle {
    Single,
    Double,
    Backtick,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MermaidQuotedSpan {
    pub style: MermaidQuoteStyle,
    pub span: Span,
    pub text_range: MermaidTextRange,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MermaidFormatComplement {
    pub line_ending: MermaidLineEndingStyle,
    pub trailing_newline: bool,
    pub whitespace: Vec<MermaidWhitespaceSpan>,
    pub comments: Vec<MermaidCommentSpan>,
    pub directives: Vec<MermaidDirectiveSpan>,
    pub quoted_literals: Vec<MermaidQuotedSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseLensSnapshot {
    pub parsed: ParseResult,
    pub source_map: MermaidSourceMap,
    pub bindings: Vec<MermaidLensBinding>,
    #[serde(skip)]
    source: String,
}

impl ParseLensSnapshot {
    /// Returns the exact source that produced this snapshot's spans and bindings.
    #[must_use]
    pub fn original_source(&self) -> &str {
        &self.source
    }

    /// Consumes the snapshot and returns its exact source without cloning it.
    #[must_use]
    pub fn into_original_source(self) -> String {
        self.source
    }

    /// Applies an edit against the source map captured by this snapshot.
    ///
    /// Keeping the source and map together prevents applying byte ranges from one
    /// parse to unrelated caller-provided text.
    pub fn apply_edit(
        &self,
        edit: &MermaidLensEdit,
    ) -> Result<MermaidLensEditResult, MermaidLensError> {
        apply_lens_edit(&self.source, &self.source_map, edit)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseLensEditResponse {
    pub result: MermaidLensEditResult,
    pub snapshot: ParseLensSnapshot,
}

/// A flowchart-specific text/IR lens.
///
/// The parser owns the original source as the format complement. [`Self::put`] accepts an IR with
/// node-label text changed and splices only those label bytes back into that original source. This
/// deliberately refuses graph-structure edits: changing an identifier without changing all of its
/// edge references is not a faithful text-to-IR round trip.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowchartParseLens {
    snapshot: ParseLensSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowchartParseLensError {
    NotFlowchart,
    UnsupportedIrChange,
    MissingSourceLabel(String),
}

impl std::fmt::Display for FlowchartParseLensError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFlowchart => {
                formatter.write_str("ParseLens currently supports flowcharts only")
            }
            Self::UnsupportedIrChange => formatter.write_str(
                "ParseLens can re-emit node-label text changes only; graph structure is unchanged",
            ),
            Self::MissingSourceLabel(node_id) => {
                write!(
                    formatter,
                    "could not locate the source label for flowchart node '{node_id}'"
                )
            }
        }
    }
}

impl std::error::Error for FlowchartParseLensError {}

impl FlowchartParseLens {
    /// Parses `input` and retains its exact text as the formatting complement.
    pub fn parse(input: &str) -> Result<Self, FlowchartParseLensError> {
        let snapshot = build_parse_lens(input);
        if snapshot.parsed.ir.diagram_type != DiagramType::Flowchart {
            return Err(FlowchartParseLensError::NotFlowchart);
        }
        Ok(Self { snapshot })
    }

    /// Returns the immutable IR view obtained from the original text.
    #[must_use]
    pub fn ir(&self) -> &MermaidDiagramIr {
        &self.snapshot.parsed.ir
    }

    /// Returns the source whose whitespace, comments, directives, quotes, and line endings are
    /// retained by this lens.
    #[must_use]
    pub fn original_source(&self) -> &str {
        self.snapshot.original_source()
    }

    /// Re-emits `edited` by replacing only changed flowchart node-label text.
    pub fn put(&self, edited: &MermaidDiagramIr) -> Result<String, FlowchartParseLensError> {
        if edited.diagram_type != DiagramType::Flowchart {
            return Err(FlowchartParseLensError::NotFlowchart);
        }

        let original = self.ir();
        if original.labels.len() != edited.labels.len() {
            return Err(FlowchartParseLensError::UnsupportedIrChange);
        }

        // First prove that every IR field except label text stayed unchanged. This keeps the
        // initial lens honest: a node/edge/style mutation cannot be rendered as though it had
        // round-tripped when only label source spans are available.
        let mut labels_only = original.clone();
        for (expected, actual) in labels_only.labels.iter_mut().zip(&edited.labels) {
            expected.text.clone_from(&actual.text);
        }
        if &labels_only != edited {
            return Err(FlowchartParseLensError::UnsupportedIrChange);
        }

        let changed_labels: Vec<usize> = original
            .labels
            .iter()
            .zip(&edited.labels)
            .enumerate()
            .filter_map(|(index, (before, after))| (before.text != after.text).then_some(index))
            .collect();
        if changed_labels.is_empty() {
            return Ok(self.original_source().to_string());
        }

        let mut replacements = Vec::new();
        for (node_index, node) in original.nodes.iter().enumerate() {
            let Some(label_id) = node.label else {
                continue;
            };
            if !changed_labels.contains(&label_id.0) {
                continue;
            }

            let source_entry = self.snapshot.source_map.entries.iter().find(|entry| {
                entry.kind == MermaidSourceMapKind::Node && entry.index == node_index
            });
            let Some(source_entry) = source_entry else {
                return Err(FlowchartParseLensError::MissingSourceLabel(node.id.clone()));
            };
            let Some(line_range) =
                resolve_span_text_range(self.original_source(), source_entry.span)
            else {
                return Err(FlowchartParseLensError::MissingSourceLabel(node.id.clone()));
            };
            let old_label = &original.labels[label_id.0].text;
            let label_range = find_flowchart_label_text_range(
                self.original_source(),
                line_range,
                &node.id,
                old_label,
            )
            .ok_or_else(|| FlowchartParseLensError::MissingSourceLabel(node.id.clone()))?;
            replacements.push((label_range, edited.labels[label_id.0].text.as_str()));
        }

        if replacements.len() != changed_labels.len()
            && changed_labels.iter().any(|label_id| {
                !original
                    .nodes
                    .iter()
                    .any(|node| node.label.is_some_and(|id| id.0 == *label_id))
            })
        {
            return Err(FlowchartParseLensError::UnsupportedIrChange);
        }

        replacements.sort_unstable_by_key(|(range, _)| std::cmp::Reverse(range.start_byte));
        let mut updated = self.original_source().to_string();
        for (range, replacement) in replacements {
            updated.replace_range(range.start_byte..range.end_byte, replacement);
        }
        Ok(updated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ParserConfig {
    pub intent_inference: bool,
    pub fuzzy_keyword_distance: usize,
    pub auto_close_delimiters: bool,
    pub create_placeholder_nodes: bool,
    /// Maximum nesting depth for block-structured containers (`subgraph … end`,
    /// `block:… end`).
    ///
    /// These are parsed by recursive descent into a recursively-nested document-item
    /// tree, so nesting depth is stack depth — for the parse itself, for the lowering
    /// walk over the tree, and for the tree's own `Drop`. Input nesting is *not* bounded
    /// by input size: `subgraph S\n` … `end\n` costs a constant ~14 bytes per level with
    /// no indentation, so under 1 MB of input can request tens of thousands of levels and
    /// overflow the stack, which aborts the process and cannot be caught by any caller.
    ///
    /// Containers nested deeper than this are flattened into the deepest accepted
    /// container with a warning diagnostic. Their nodes and edges are preserved; only the
    /// surplus grouping is dropped.
    pub max_nesting_depth: usize,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            intent_inference: true,
            fuzzy_keyword_distance: 2,
            auto_close_delimiters: true,
            create_placeholder_nodes: true,
            // Real diagrams nest a handful of levels; 256 is far above any legible
            // diagram while staying safe on a small (1 MiB) spawned-thread stack even in
            // an unoptimized build, where a `parse_flowchart_document_items` frame
            // measures ~2.7 KB.
            max_nesting_depth: 256,
        }
    }
}

/// Method used to detect diagram type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DetectionMethod {
    /// Exact keyword match (highest confidence)
    ExactKeyword,
    /// Fuzzy keyword match with small edit distance
    FuzzyKeyword,
    /// Content-based heuristics (patterns like -->)
    ContentHeuristic,
    /// DOT format detection
    DotFormat,
    /// Fallback to flowchart (lowest confidence)
    Fallback,
}

impl DetectionMethod {
    /// Get a human-readable description of the detection method.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactKeyword => "explicit keyword match",
            Self::FuzzyKeyword => "fuzzy keyword match",
            Self::ContentHeuristic => "content heuristics",
            Self::DotFormat => "DOT format detected",
            Self::Fallback => "fallback to flowchart",
        }
    }
}

/// Result of diagram type detection with confidence information.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetectedType {
    /// The detected diagram type
    pub diagram_type: DiagramType,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// Method used for detection
    pub method: DetectionMethod,
    /// Any warnings generated during detection
    pub warnings: Vec<String>,
}

/// Detect diagram type with confidence information.
///
/// Uses multiple detection strategies in order of precedence:
/// 1. Exact keyword match
/// 2. Fuzzy keyword match (edit distance <= 2)
/// 3. Content heuristics (characteristic patterns)
/// 4. DOT format detection
/// 5. Fallback to flowchart
#[must_use]
pub fn detect_type_with_confidence(input: &str) -> DetectedType {
    detect_type_with_confidence_and_config(input, &ParserConfig::default())
}

/// Detect diagram type with explicit parser-behavior settings.
#[must_use]
pub fn detect_type_with_confidence_and_config(input: &str, config: &ParserConfig) -> DetectedType {
    let trimmed = input.trim();

    // Empty input
    if trimmed.is_empty() {
        return DetectedType {
            diagram_type: DiagramType::Unknown,
            confidence: 0.0,
            method: DetectionMethod::Fallback,
            warnings: vec!["Empty input".to_string()],
        };
    }

    // Get the first significant line up front. An UNAMBIGUOUS mermaid keyword — anything but a bare
    // `graph` / `digraph` / `strict` DOT header — lets us return WITHOUT running the DOT probe
    // (Strategy 1), whose comment-strip + whole-input "graph" scan is pure overhead for
    // class/er/state/sequence/… (and even the brace scan for a plain flowchart). Only
    // `graph`/`digraph`/`strict`/comment/unknown first lines can actually be DOT, so those still run
    // `looks_like_dot`. Byte-identical: a real DOT header always starts with one of those prefixes.
    let first_line = mermaid_parser::first_significant_line(input).unwrap_or("");
    let keyword = exact_keyword_match(first_line);
    let could_be_dot = ["graph", "digraph", "strict"].iter().any(|prefix| {
        first_line
            .as_bytes()
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
    });

    // Strategy 1: DOT format detection (high priority for interop).
    if (keyword.is_none() || could_be_dot) && looks_like_dot(input) {
        return DetectedType {
            diagram_type: DiagramType::Flowchart,
            confidence: 0.95,
            method: DetectionMethod::DotFormat,
            warnings: vec![],
        };
    }

    // Strategy 2: Exact keyword match
    if let Some(detected) = keyword {
        return detected;
    }

    if config.intent_inference {
        // Strategy 3: Fuzzy keyword match
        let lower = first_line.to_ascii_lowercase();
        if let Some(detected) = fuzzy_keyword_match(&lower, config.fuzzy_keyword_distance) {
            return detected;
        }

        // Strategy 4: Content heuristics
        if let Some(detected) = content_heuristics(input) {
            return detected;
        }
    }

    // Strategy 4.9: a header the INCUMBENT supports and we do not.
    //
    // Found by reading mermaid 11.15.0's own bundle against our `DiagramType`: it ships `radar` and
    // `treemap`, and we have neither. Today such a document falls through to Strategy 5 and is
    // reported as "could not detect diagram type" — which sends the author to check their syntax,
    // when their syntax is fine and the type is simply unimplemented here. Those are different
    // problems with different fixes, and the generic message names the wrong one.
    //
    // Behaviour is otherwise UNCHANGED: same Flowchart fallback, same confidence, same
    // `Fallback` method — so strict mode still refuses it exactly as before (see the
    // `MermaidParseMode::Strict` check on `DetectionMethod::Fallback`). Only the message improves.
    if let Some(kind) = unsupported_upstream_keyword(first_line) {
        return DetectedType {
            diagram_type: DiagramType::Flowchart,
            confidence: 0.3,
            method: DetectionMethod::Fallback,
            warnings: vec![format!(
                "'{kind}' is a mermaid diagram type this renderer does not implement yet. \
                 Rendering it as a flowchart will not be meaningful."
            )],
        };
    }

    // Strategy 5: Fallback to flowchart
    DetectedType {
        diagram_type: DiagramType::Flowchart,
        confidence: 0.3,
        method: DetectionMethod::Fallback,
        warnings: vec!["Could not detect diagram type; assuming flowchart".to_string()],
    }
}

/// Diagram headers that upstream mermaid supports and this renderer does not.
///
/// Every entry is taken from the incumbent's OWN detector table, not from guesswork: the pinned
/// 11.15.0 bundle carries 31 start-anchored detectors, and these are the ones with no `DiagramType`
/// behind them. Guessing at plausible names would produce a confident "not implemented yet" for a
/// typo, which is worse than the generic message this replaces.
///
/// ⚠️ Extract that table with a pattern that tolerates OPTIONAL SUFFIXES. My first pass matched
/// only `/^\s*name/` and returned 23 detectors, silently missing every
/// `/^\s*name(-beta)?\b/` form -- including `ishikawa`, which I then had to be told about. The
/// widened pattern found 31, and the eight it added were all real.
///
/// Matched on the header TOKEN, splitting on whitespace or `-`, so `radar-beta` and `treemap-beta`
/// reach the same entries as their bare spellings without needing four rows.
///
/// ⚠️ Matching a bare spelling does NOT mean upstream accepts it. Measured against the pinned
/// 11.15.0 grammar (BeigeHill, cross-checked with two different bodies): `radar-beta` parses and a
/// bare `radar` is a SYNTAX ERROR. An earlier version of this comment claimed the bare form worked,
/// which is exactly the sort of sentence a later reader treats as verified.
///
/// That is also why the warning below no longer tells the author their input "was not misspelled".
/// For `radar-beta` that is true; for a bare `radar` it is false, because mermaid rejects it too —
/// and confidently absolving a malformed document is worse than the generic message this replaced.
/// The message now says only what is certain: this renderer does not implement the type.
///
/// When one of these lands as a real `DiagramType`, DELETE its entry rather than leave a message
/// claiming the feature is missing.
fn unsupported_upstream_keyword(first_line: &str) -> Option<&'static str> {
    let head = first_line
        .split(|c: char| c.is_whitespace() || c == '-')
        .find(|token| !token.is_empty())?
        .to_ascii_lowercase();

    // ⚠️ THE RETURNED NAME IS THE SPELLING THE INCUMBENT ACCEPTS, WHICH IS NOT ALWAYS THE ONE THE
    // AUTHOR TYPED. The `-` split above means `venn-beta` and `venn` both arrive here as `venn`, so
    // matching is spelling-agnostic — but the string returned goes into a message telling the
    // author this is real mermaid, and naming a spelling mermaid REJECTS teaches them a syntax
    // error. Every name below was probed against the pinned 11.15.0 bundle with
    // `scripts/headtohead/parse_probe.mjs`:
    //
    //   header          bare        with content   `-beta`
    //   ishikawa        rejected    ACCEPTED       --
    //   treemap         PARSED      --             PARSED
    //   info            PARSED      --             --
    //   eventmodeling   PARSED      --             --
    //   radar           rejected    rejected       ACCEPTED
    //   venn            rejected    rejected       PARSED
    //   wardley         rejected    rejected       PARSED
    //   treeView        rejected    rejected       PARSED
    //
    // ⚠️ PROBE WITH CONTENT, NOT A BARE HEADER, and this nearly cost a correct entry. A bare header
    // conflates "this type does not exist" with "this type exists and a header alone is not a valid
    // document": bare `ishikawa` reports "grammar rejected" and `ishikawa` plus one line reports
    // "grammar ACCEPTED" (the execution throw after it is a missing DOMPurify shim, not a parse
    // failure). Reading the bare result alone, I concluded ishikawa was fictional and was about to
    // delete it. `notatypeatall` is rejected in both forms, which is the control that says the probe
    // discriminates at all.
    match head.as_str() {
        // Accepted bare AND with `-beta`, so the bare spelling is the honest name.
        "treemap" => Some("treemap"),
        "info" => Some("info"),
        "eventmodeling" => Some("eventmodeling"),
        // Accepted bare only, once it has content. Reported by BeigeHill and re-verified here.
        "ishikawa" => Some("ishikawa"),
        // ONLY the `-beta` spelling is real mermaid. Naming the bare form here would tell an author
        // that `radar` is a diagram type they can use, and the incumbent rejects it.
        "radar" => Some("radar-beta"),
        "venn" => Some("venn-beta"),
        "wardley" => Some("wardley-beta"),
        "treeview" => Some("treeView-beta"),
        _ => None,
    }
}

/// Exact keyword matching for diagram type detection.
fn exact_keyword_match(line: &str) -> Option<DetectedType> {
    let diagram_type = exact_diagram_type_with(line, matches_keyword_header_ci)?;

    Some(DetectedType {
        diagram_type,
        confidence: 1.0,
        method: DetectionMethod::ExactKeyword,
        warnings: vec![],
    })
}

#[inline]
fn exact_diagram_type_with(
    line: &str,
    matches: impl Fn(&str, &str) -> bool + Copy,
) -> Option<DiagramType> {
    if matches(line, "flowchart") || matches(line, "graph") {
        Some(DiagramType::Flowchart)
    } else if matches(line, "sequencediagram") {
        Some(DiagramType::Sequence)
    } else if matches(line, "classdiagram") {
        Some(DiagramType::Class)
    } else if matches(line, "statediagram") {
        Some(DiagramType::State)
    } else if matches(line, "gantt") {
        Some(DiagramType::Gantt)
    } else if matches(line, "erdiagram") {
        Some(DiagramType::Er)
    } else if matches(line, "mindmap") {
        Some(DiagramType::Mindmap)
    } else if matches(line, "pie") {
        Some(DiagramType::Pie)
    } else if matches(line, "gitgraph") {
        Some(DiagramType::GitGraph)
    } else if matches(line, "journey") {
        Some(DiagramType::Journey)
    // Bare `requirement`, `packet` and `architecture` are accepted by the incumbent -- its detector
    // table spells them `/^\s*requirement(Diagram)?/`, `/^\s*packet(-beta)?/` and
    // `/^\s*architecture/`. Our matcher requires the LINE to be at least as long as the keyword, so
    // a keyword of `requirementdiagram` cannot match a line of `requirement`; the bare spellings
    // were undetectable and fell through to the flowchart fallback. The reverse direction already
    // worked, which is why this was easy to miss: `sankey` matches `sankey-beta` because the
    // matcher accepts a following `-`.
    } else if matches(line, "requirementdiagram") || matches(line, "requirement") {
        Some(DiagramType::Requirement)
    } else if matches(line, "timeline") {
        Some(DiagramType::Timeline)
    } else if matches(line, "quadrantchart") {
        Some(DiagramType::QuadrantChart)
    } else if matches(line, "sankey") {
        Some(DiagramType::Sankey)
    } else if matches(line, "xychart") {
        Some(DiagramType::XyChart)
    } else if matches(line, "block-beta") || matches(line, "block") {
        Some(DiagramType::BlockBeta)
    } else if matches(line, "packet-beta") || matches(line, "packet") {
        Some(DiagramType::PacketBeta)
    } else if matches(line, "architecture-beta") || matches(line, "architecture") {
        Some(DiagramType::ArchitectureBeta)
    } else if matches(line, "c4context") {
        Some(DiagramType::C4Context)
    } else if matches(line, "c4container") {
        Some(DiagramType::C4Container)
    } else if matches(line, "c4component") {
        Some(DiagramType::C4Component)
    } else if matches(line, "c4dynamic") {
        Some(DiagramType::C4Dynamic)
    } else if matches(line, "c4deployment") {
        Some(DiagramType::C4Deployment)
    } else if matches(line, "kanban") {
        Some(DiagramType::Kanban)
    } else {
        None
    }
}

/// Known diagram keywords for fuzzy matching.
///
/// Kept in step with the exact-match chain above. It had drifted: block, packet, architecture and
/// all five C4 headers were reachable exactly but had no fuzzy entry, so a typo in those eight was
/// the only kind this parser could not offer to correct. Nothing documents that as deliberate and
/// the exact chain lists them, so the omission reads as a table that stopped being updated as types
/// were added.
///
/// ⚠️ ORDER IS SIGNIFICANT ON A TIE. `fuzzy_keyword_match` keeps the first candidate at the minimum
/// distance (`distance < best_distance`, strictly), so two keywords equidistant from a typo resolve
/// to whichever appears FIRST here. That is deterministic but arbitrary, and it matters most for the
/// five C4 headers, which differ from each other by only a few characters. They are listed
/// shortest-first so a tie favours the shorter, more general name rather than a longer one the
/// author is less likely to have meant.
const DIAGRAM_KEYWORDS: &[(&str, DiagramType)] = &[
    ("flowchart", DiagramType::Flowchart),
    ("graph", DiagramType::Flowchart),
    ("sequencediagram", DiagramType::Sequence),
    ("classdiagram", DiagramType::Class),
    ("statediagram", DiagramType::State),
    ("gantt", DiagramType::Gantt),
    ("erdiagram", DiagramType::Er),
    ("mindmap", DiagramType::Mindmap),
    ("pie", DiagramType::Pie),
    ("gitgraph", DiagramType::GitGraph),
    ("journey", DiagramType::Journey),
    ("requirementdiagram", DiagramType::Requirement),
    ("timeline", DiagramType::Timeline),
    ("quadrantchart", DiagramType::QuadrantChart),
    ("sankey", DiagramType::Sankey),
    ("xychart", DiagramType::XyChart),
    ("kanban", DiagramType::Kanban),
    ("block", DiagramType::BlockBeta),
    ("packet", DiagramType::PacketBeta),
    ("architecture", DiagramType::ArchitectureBeta),
    ("c4context", DiagramType::C4Context),
    ("c4dynamic", DiagramType::C4Dynamic),
    ("c4component", DiagramType::C4Component),
    ("c4container", DiagramType::C4Container),
    ("c4deployment", DiagramType::C4Deployment),
];

pub(crate) fn is_sankey_header(line: &str) -> bool {
    matches_keyword_header_ci(line, "sankey") || matches_keyword_header_ci(line, "sankey-beta")
}

pub(crate) fn matches_keyword_header(line: &str, keyword: &str) -> bool {
    line == keyword
        || line
            .strip_prefix(keyword)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_whitespace() || c == '-')
}

/// ASCII-case-insensitive [`matches_keyword_header`] that avoids the caller's per-line
/// `to_ascii_lowercase()` heap allocation. Byte-identical to
/// `matches_keyword_header(&line.to_ascii_lowercase(), keyword)` for a lowercase-ASCII `keyword`:
/// the prefix compare folds ASCII case (matching the lowercasing), and `to_ascii_lowercase` never
/// changes a char's whitespace-ness or the `'-'` byte, so the post-keyword char test is unaffected.
/// `keyword` is ASCII, so `keyword.len()` is a char boundary once the prefix matches.
pub(crate) fn matches_keyword_header_ci(line: &str, keyword: &str) -> bool {
    let kb = keyword.as_bytes();
    if line.len() < kb.len() || !line.as_bytes()[..kb.len()].eq_ignore_ascii_case(kb) {
        return false;
    }
    match line[kb.len()..].chars().next() {
        None => true,
        Some(c) => c.is_whitespace() || c == '-',
    }
}

/// Fuzzy keyword matching using Levenshtein distance.
fn fuzzy_keyword_match(lower: &str, max_distance: usize) -> Option<DetectedType> {
    if max_distance == 0 {
        return None;
    }

    // Extract the first word
    let first_word = lower.split_whitespace().next()?;

    // Find best fuzzy match
    let mut best_match: Option<(DiagramType, usize)> = None;

    for (keyword, diagram_type) in DIAGRAM_KEYWORDS {
        let distance = levenshtein_distance(first_word, keyword);
        // Only consider non-exact matches within the configured threshold.
        if distance > 0 && distance <= max_distance {
            let is_better_match = match best_match {
                Some((_, best_distance)) => distance < best_distance,
                None => true,
            };
            if is_better_match {
                best_match = Some((*diagram_type, distance));
            }
        }
    }

    best_match.map(|(diagram_type, distance)| {
        // Confidence decreases with distance
        let confidence = (0.85 - (distance.saturating_sub(1)) as f32 * 0.15).max(0.4);

        DetectedType {
            diagram_type,
            confidence,
            method: DetectionMethod::FuzzyKeyword,
            warnings: vec!["Fuzzy match: possible typo in diagram type declaration".to_string()],
        }
    })
}

/// Content-based heuristics for detecting diagram type from patterns.
/// Whether the content contains an ER relationship, recognised STRUCTURALLY.
///
/// This replaced four hardcoded spellings -- `||--o{`, `}|--||`, `||--|{`, `|o--o|` -- which is four
/// of the thirty-two the grid actually admits, and none of the `..` non-identifying forms at all. A
/// many-to-many `}o--o{`, the most common relationship anyone draws, was not detected.
///
/// An ER relationship is a LEFT cardinality token, a separator, then a RIGHT one. The two sides use
/// different alphabets because the notation is mirrored: `}o` opens to the left, `o{` to the right.
/// Checking the flanking pairs rather than whole spellings covers all thirty-two combinations and
/// both separators without enumerating any of them.
///
/// Cheap and specific enough not to fire on other diagrams: a flowchart `-->` has a space and `>`
/// around its `--`, a class dependency `..>` likewise, and neither pair is a cardinality token.
fn looks_like_er_relationship(content: &str) -> bool {
    // Lowercased upstream, so only these forms occur.
    const LEFT: [&str; 4] = ["|o", "||", "}o", "}|"];
    const RIGHT: [&str; 4] = ["o|", "||", "o{", "|{"];

    for separator in ["--", ".."] {
        let mut rest = content;
        let mut base = 0_usize;
        while let Some(offset) = rest.find(separator) {
            let at = base + offset;
            let after = at + separator.len();
            let left_ok = at >= 2
                && content
                    .get(at - 2..at)
                    .is_some_and(|pair| LEFT.contains(&pair));
            let right_ok = content
                .get(after..after + 2)
                .is_some_and(|pair| RIGHT.contains(&pair));
            if left_ok && right_ok {
                return true;
            }
            base = at + separator.len();
            rest = &content[base..];
        }
    }
    false
}

/// Does this content carry an actual sequence MESSAGE -- an arrow followed by its `:` separator?
///
/// The sequence arm used to fire on "contains an arrow" AND "contains a colon" as two independent
/// facts about the whole document, which is a much weaker claim than it looks. A flowchart arrow
/// `-->` contains `->`, and a colon appears in any `style a fill:#f00`, so a headerless flowchart
/// with one style line was read as a sequence diagram -- and because the sequence arm runs BEFORE
/// the flowchart arm, widening the flowchart table could not rescue it.
///
/// A mermaid sequence message is `A->>B: text`: the colon is a SEPARATOR that follows the arrow on
/// the same line. Requiring that shape is not a heuristic tightening so much as reading the grammar
/// the two facts were standing in for.
///
/// Bracket depth is tracked so a flowchart label supplies neither half: in `A[Start] --> B[Step: two]`
/// the colon is inside a node label, and in `A[a->b] --> C` the arrow is. Both are depth 1, and only
/// a depth-0 arrow followed by a depth-0 colon counts.
fn has_sequence_message_separator(content: &str) -> bool {
    content.lines().any(|line| {
        let bytes = line.as_bytes();
        let mut depth = 0_usize;
        let mut seen_arrow = false;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'[' | b'(' | b'{' => depth += 1,
                b']' | b')' | b'}' => depth = depth.saturating_sub(1),
                b'-' if depth == 0 && bytes.get(i + 1) == Some(&b'>') => {
                    seen_arrow = true;
                    i += 1;
                }
                b':' if depth == 0 && seen_arrow => return true,
                _ => {}
            }
            i += 1;
        }
        false
    })
}

/// Is `[*]` used here as a state START/END MARKER rather than as a node label?
///
/// `[*]` is decisive for state diagrams -- no other mermaid type uses it -- but only when it sits
/// next to a transition arrow. `A[*]` on its own is a flowchart node whose label is `*`.
///
/// The existing checks were the literal strings `"[*] -->"` and `"--> [*]"`, which are
/// SPACE-SENSITIVE: mermaid accepts `[*]-->Idle` with no space at all, and that spelling fell
/// through to the flowchart fallback. Scanning around the marker instead of matching a literal is
/// the same fix applied to the operator tables -- match the shape, not one of its spellings.
fn looks_like_state_transition(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut from = 0_usize;
    while let Some(offset) = content[from..].find("[*]") {
        let at = from + offset;
        let after = at + 3;

        // `[*]-->` / `[*] --> `
        let mut ahead = after;
        while matches!(bytes.get(ahead), Some(b' ' | b'\t')) {
            ahead += 1;
        }
        if bytes.get(ahead) == Some(&b'-') {
            return true;
        }

        // `-->[*]` / `--> [*]`
        let mut behind = at;
        while behind > 0 && matches!(bytes[behind - 1], b' ' | b'\t') {
            behind -= 1;
        }
        if behind > 0 && matches!(bytes[behind - 1], b'>' | b'-') {
            return true;
        }

        from = after;
    }
    false
}

fn content_heuristics(input: &str) -> Option<DetectedType> {
    // Strip comments to avoid false positives in metadata
    let lines: Vec<&str> = input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("%%"))
        .collect();
    let content = lines.join("\n").to_lowercase();

    // ER diagram patterns
    if looks_like_er_relationship(&content) {
        return Some(DetectedType {
            diagram_type: DiagramType::Er,
            confidence: 0.8,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected ER relationship patterns".to_string()],
        });
    }

    // State transitions come BEFORE the sequence arm, and the ordering is the whole point.
    //
    // `[*]` is mermaid's start/end state marker and appears in no other diagram type, so a line
    // carrying it next to an arrow is decisive. But a labelled transition -- `[*] --> Idle: boot` --
    // is also an arrow followed by a colon, which is exactly what the sequence arm matches, and the
    // sequence arm ran first. Every headerless state diagram with labelled transitions was reported
    // as a sequence diagram. Tightening the sequence rule to a real message separator did not fix
    // this and could not: the state line genuinely IS arrow-then-colon. Only precedence decides it,
    // and the decisive marker must get to speak first.
    if looks_like_state_transition(&content) {
        return Some(DetectedType {
            diagram_type: DiagramType::State,
            confidence: 0.8,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected state diagram patterns".to_string()],
        });
    }

    // Sequence diagram patterns.
    //
    // Our parser accepts ten arrow forms; this tested three. The async arrows `-)` / `--)` and the
    // bidirectional `<<->>` / `<<-->>` are sequence-only, so they are matched unguarded -- testing
    // `-)` covers `--)` by substring, and `<<->>` covers `<<-->>`.
    //
    // `-x` and `--x` are EXCLUDED for the same reason `o--` is excluded from the class branch: a
    // flowchart cross edge is spelled `A --x B`, and `-x` is a substring of it, so neither can be
    // tested here without reclassifying flowcharts. The colon guard would not save it either --
    // a headerless flowchart carrying any `fill:#f00` supplies the colon.
    if content.contains("->>")
        || content.contains("-->>")
        || content.contains("-)")
        || content.contains("<<->>")
        || content.contains("participant ")
        || content.contains("actor ")
        || content.contains("activate ")
        || content.contains("note ")
        || has_sequence_message_separator(&content)
    {
        return Some(DetectedType {
            diagram_type: DiagramType::Sequence,
            confidence: 0.75,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected sequence diagram patterns".to_string()],
        });
    }

    // Class diagram patterns.
    //
    // Our own parser accepts ten class relations -- *--, --*, --o, --|>, ..>, ..|>, <.., <|--,
    // <|.., o-- -- and this heuristic recognised two of them. A diagram whose only relation was a
    // composition (`Order *-- LineItem`) was not detected, so an unheaded document fell through to
    // the flowchart fallback.
    //
    // `o--` and `--o` are deliberately EXCLUDED despite being class relations: a flowchart circle
    // edge is spelled `A --o B` and `A o--o B`, so matching them here would reclassify flowcharts.
    // They are the two the parser can only disambiguate from a header, and guessing from content
    // would trade a missed class diagram for a corrupted flowchart.
    if content.contains("<|--")
        || content.contains("--|>")
        || content.contains("<|..")
        || content.contains("..|>")
        || content.contains("*--")
        || content.contains("--*")
        || content.contains("..>")
        || content.contains("<..")
        || (content.contains("class ") && content.contains('{'))
    {
        return Some(DetectedType {
            diagram_type: DiagramType::Class,
            confidence: 0.75,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected class diagram patterns".to_string()],
        });
    }

    // State diagram patterns
    if content.contains("[*] -->") || content.contains("--> [*]") || content.contains("state ") {
        return Some(DetectedType {
            diagram_type: DiagramType::State,
            confidence: 0.7,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected state diagram patterns".to_string()],
        });
    }

    // Flowchart patterns (broad, lower confidence).
    //
    // FLOW_OPERATORS has fourteen entries and this tested three. The miss is quieter here than in the
    // branches above, which is why it survived: this arm is last, so a flowchart that fails it still
    // REACHES `DiagramType::Flowchart` through the Strategy 5 fallback. What changes is `method` --
    // `ContentHeuristic` becomes `Fallback` -- and strict mode REFUSES `Fallback` (see the
    // `MermaidParseMode::Strict` check on the detection method). So today a headerless `A --x B` is
    // rejected outright while `A --> B` parses, for no reason an author could infer.
    //
    // Testing `-.-` covers the dotted arrow `-.->` and the bidirectional `<-.->` by substring, as
    // `-->` already covers `<-->` and `==>` covers `<==>`.
    //
    // `--o` and `--x` belong HERE, and they are the completing half of a decision made in the two
    // branches above: both were excluded from the class and sequence arms precisely so that a
    // flowchart circle or cross edge would not be reclassified. Excluding them there while never
    // matching them here left them belonging to no branch at all.
    //
    // The remaining four -- `--`, `==`, `-.`, `..` -- are deliberately NOT tested. Each is a
    // two-character sequence that occurs inside ordinary prose and inside diagram types this
    // function cannot detect, and matching them would shadow the unsupported-upstream-keyword
    // message in Strategy 4.9: a `radar` document containing an ellipsis would be reported as a
    // detected flowchart instead of as an unimplemented type. A missed heuristic costs confidence;
    // a shadowed diagnostic costs the author the one message that tells them the truth.
    if content.contains("-->")
        || content.contains("---")
        || content.contains("==>")
        || content.contains("-.-")
        || content.contains("--o")
        || content.contains("--x")
    {
        return Some(DetectedType {
            diagram_type: DiagramType::Flowchart,
            confidence: 0.6,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected flowchart arrow patterns".to_string()],
        });
    }

    None
}

/// Simple Levenshtein distance implementation.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    // Use two rows for space efficiency
    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row: Vec<usize> = vec![0; b_len + 1];

    for (i, a_char) in a_chars.iter().enumerate() {
        curr_row[0] = i + 1;

        for (j, b_char) in b_chars.iter().enumerate() {
            let cost = usize::from(a_char != b_char);
            curr_row[j + 1] = (prev_row[j + 1] + 1) // deletion
                .min(curr_row[j] + 1) // insertion
                .min(prev_row[j] + cost); // substitution
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

/// Simple diagram type detection (for backwards compatibility).
#[must_use]
pub fn detect_type(input: &str) -> DiagramType {
    detect_type_with_confidence_and_config(input, &ParserConfig::default()).diagram_type
}

#[must_use]
pub fn build_parse_lens(input: &str) -> ParseLensSnapshot {
    let mut parsed = parse(input);
    // The lens needs the format complement for round-trip editing; `parse` no longer
    // captures it on the hot path, so capture it explicitly here.
    parsed.format_complement = capture_format_complement(input);
    let source_map = parsed.ir.source_map();
    let bindings = build_lens_bindings(input, &source_map);
    ParseLensSnapshot {
        parsed,
        source_map,
        bindings,
        source: input.to_owned(),
    }
}

/// Finds the exact bytes of one node label inside its source line. The source map identifies the
/// line; matching the node id before the label keeps repeated label text on a shared edge line from
/// being mistaken for the target node.
fn find_flowchart_label_text_range(
    source: &str,
    line_range: MermaidTextRange,
    node_id: &str,
    label: &str,
) -> Option<MermaidTextRange> {
    if label.is_empty() || line_range.end_byte > source.len() {
        return None;
    }
    let line = source.get(line_range.start_byte..line_range.end_byte)?;
    let mut search_from = 0;
    while let Some(relative_id_start) = line.get(search_from..)?.find(node_id) {
        let id_start = search_from + relative_id_start;
        let id_end = id_start + node_id.len();
        search_from = id_end;

        let before_is_identifier = line[..id_start]
            .chars()
            .next_back()
            .is_some_and(is_flowchart_identifier_char);
        let after_is_identifier = line[id_end..]
            .chars()
            .next()
            .is_some_and(is_flowchart_identifier_char);
        if before_is_identifier || after_is_identifier {
            continue;
        }

        let Some(open) = line[id_end..].chars().next() else {
            continue;
        };
        if !matches!(open, '[' | '(' | '{') {
            continue;
        }
        let closing = match open {
            '[' => ']',
            '(' => ')',
            '{' => '}',
            _ => unreachable!("the opening delimiter is filtered above"),
        };
        let content_start = id_end + open.len_utf8();
        let Some(relative_close) = line[content_start..].find(closing) else {
            continue;
        };
        let content_end = content_start + relative_close;
        let content = &line[content_start..content_end];
        let Some(relative_label) = content.find(label) else {
            continue;
        };
        let start_byte = line_range.start_byte + content_start + relative_label;
        return Some(MermaidTextRange {
            start_byte,
            end_byte: start_byte + label.len(),
        });
    }
    None
}

fn is_flowchart_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/')
}

pub fn apply_parse_lens_edit(
    input: &str,
    edit: &MermaidLensEdit,
) -> Result<ParseLensEditResponse, MermaidLensError> {
    let snapshot = build_parse_lens(input);
    let result = snapshot.apply_edit(edit)?;
    let updated_snapshot = build_parse_lens(&result.updated_source);
    Ok(ParseLensEditResponse {
        result,
        snapshot: updated_snapshot,
    })
}

/// Delete the element `element_id` addresses, returning the result plus a snapshot of the source
/// that remains.
///
/// The re-snapshot is the reason this belongs beside [`apply_parse_lens_edit`] rather than being
/// left to callers: element ids and spans are derived from the source, so every one of them shifts
/// after a delete. A caller reusing the pre-delete snapshot would address the wrong bytes on its
/// next edit.
pub fn apply_parse_lens_delete(
    input: &str,
    element_id: &str,
) -> Result<ParseLensEditResponse, MermaidLensError> {
    let snapshot = build_parse_lens(input);
    let result =
        fm_core::apply_lens_delete(snapshot.original_source(), &snapshot.source_map, element_id)?;
    let updated_snapshot = build_parse_lens(&result.updated_source);
    Ok(ParseLensEditResponse {
        result,
        snapshot: updated_snapshot,
    })
}

/// Insert `text` as a new line after the line holding `element_id`, returning the result plus a
/// snapshot of the updated source. Re-snapshots for the same reason as [`apply_parse_lens_delete`].
pub fn apply_parse_lens_insert_line_after(
    input: &str,
    element_id: &str,
    text: &str,
) -> Result<ParseLensEditResponse, MermaidLensError> {
    let snapshot = build_parse_lens(input);
    let result = fm_core::apply_lens_insert_line_after(
        snapshot.original_source(),
        &snapshot.source_map,
        element_id,
        text,
    )?;
    let updated_snapshot = build_parse_lens(&result.updated_source);
    Ok(ParseLensEditResponse {
        result,
        snapshot: updated_snapshot,
    })
}

#[must_use]
pub fn capture_format_complement(input: &str) -> MermaidFormatComplement {
    let (offsets, line_ending) = line_offsets_and_ending_style(input);
    let mut whitespace = Vec::new();
    let mut comments = Vec::new();
    let mut directives = Vec::new();
    let mut quoted_literals = Vec::new();

    let mut offset = 0_usize;
    for raw_line in input.split_inclusive('\n') {
        let line_body = raw_line.trim_end_matches(['\r', '\n']);
        let body_start = offset;
        let body_end = body_start + line_body.len();
        let trimmed = line_body.trim();

        let leading_ws_len = line_body.len() - line_body.trim_start_matches([' ', '\t']).len();
        if leading_ws_len > 0 {
            push_whitespace_span(
                input,
                &mut whitespace,
                MermaidWhitespaceKind::Indent,
                body_start,
                body_start + leading_ws_len,
                &offsets,
            );
        }

        let trailing_ws_len = line_body.len() - line_body.trim_end_matches([' ', '\t']).len();
        let content_start = body_start + leading_ws_len;
        let content_end = body_end.saturating_sub(trailing_ws_len);
        if content_end > content_start {
            collect_inter_token_whitespace(
                input,
                &mut whitespace,
                &line_body[leading_ws_len..line_body.len().saturating_sub(trailing_ws_len)],
                content_start,
                &offsets,
            );
        }

        if trailing_ws_len > 0 && body_end >= trailing_ws_len {
            push_whitespace_span(
                input,
                &mut whitespace,
                MermaidWhitespaceKind::Trailing,
                body_end - trailing_ws_len,
                body_end,
                &offsets,
            );
        }

        if trimmed.is_empty() {
            let blank_end = if body_end > body_start {
                body_end
            } else {
                body_start + raw_line.len()
            };
            push_whitespace_span(
                input,
                &mut whitespace,
                MermaidWhitespaceKind::BlankLine,
                body_start,
                blank_end,
                &offsets,
            );
        } else if trimmed.starts_with("%%{") && trimmed.ends_with("}%%") {
            push_directive_span(input, &mut directives, body_start, body_end, &offsets);
        } else if trimmed.starts_with("%%") {
            push_comment_span(input, &mut comments, body_start, body_end, &offsets);
        }

        offset += raw_line.len();
    }

    collect_quoted_literals(input, &mut quoted_literals, &offsets);

    MermaidFormatComplement {
        line_ending,
        trailing_newline: input.ends_with('\n'),
        whitespace,
        comments,
        directives,
        quoted_literals,
    }
}

#[must_use]
pub fn parse(input: &str) -> ParseResult {
    parse_with_mode_and_config(input, MermaidParseMode::Compat, &ParserConfig::default())
}

#[must_use]
pub fn parse_with_mode(input: &str, parse_mode: MermaidParseMode) -> ParseResult {
    parse_with_mode_and_config(input, parse_mode, &ParserConfig::default())
}

#[must_use]
pub fn parse_with_mode_and_config(
    input: &str,
    parse_mode: MermaidParseMode,
    config: &ParserConfig,
) -> ParseResult {
    if input.trim().is_empty() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Unknown);
        ir.meta.parse_mode = parse_mode;
        return ParseResult {
            ir,
            warnings: vec!["Input was empty; returning empty IR".to_string()],
            confidence: 0.0,
            detection_method: DetectionMethod::Fallback,
            format_complement: MermaidFormatComplement::default(),
        };
    }

    // Detect type with confidence first
    let mut detection = detect_type_with_confidence_and_config(input, config);
    if parse_mode == MermaidParseMode::Strict && detection.method == DetectionMethod::Fallback {
        detection.diagram_type = DiagramType::Unknown;
    }

    if detection.method == DetectionMethod::DotFormat {
        // DOT format - parse via dot parser
        let mut result = parse_dot(input);
        result.confidence = detection.confidence;
        result.detection_method = detection.method;
        result.ir.meta.parse_mode = parse_mode;
        result.format_complement = MermaidFormatComplement::default();
        return result;
    }

    let mut result = mermaid_parser::parse_mermaid_with_detection_and_config(
        input, detection, parse_mode, config,
    );
    // The format complement (whitespace/comment/directive/quoted-literal spans) is
    // only needed for round-trip editing (`build_parse_lens`) and evidence output —
    // never by the parse → layout → render hot path. Capturing it costs ~10-22% of
    // parse time, so it is left empty here and captured explicitly by the consumers
    // that need it (see `capture_format_complement`).
    result.format_complement = MermaidFormatComplement::default();
    result
}

#[must_use]
pub fn parse_evidence_json(parsed: &ParseResult) -> String {
    json!({
        "diagram_type": parsed.ir.diagram_type.as_str(),
        "parse_mode": parsed.parse_mode().as_str(),
        "support_level": parsed.ir.meta.support_level,
        "node_count": parsed.ir.nodes.len(),
        "edge_count": parsed.ir.edges.len(),
        "cluster_count": parsed.ir.clusters.len(),
        "label_count": parsed.ir.labels.len(),
        "diagnostic_count": parsed.ir.diagnostics.len(),
        "warning_count": parsed.warnings.len(),
        "warnings": parsed.warnings.clone(),
        "format_complement": {
            "line_ending": parsed.format_complement.line_ending,
            "trailing_newline": parsed.format_complement.trailing_newline,
            "whitespace_count": parsed.format_complement.whitespace.len(),
            "comment_count": parsed.format_complement.comments.len(),
            "directive_count": parsed.format_complement.directives.len(),
            "quoted_literal_count": parsed.format_complement.quoted_literals.len(),
        },
    })
    .to_string()
}

fn collect_inter_token_whitespace(
    source: &str,
    whitespace: &mut Vec<MermaidWhitespaceSpan>,
    line_slice: &str,
    absolute_offset: usize,
    offsets: &[usize],
) {
    let mut run_start: Option<usize> = None;
    for (offset, ch) in line_slice.char_indices() {
        if ch.is_whitespace() {
            run_start.get_or_insert(offset);
            continue;
        }
        if let Some(start) = run_start.take() {
            push_whitespace_span(
                source,
                whitespace,
                MermaidWhitespaceKind::InterToken,
                absolute_offset + start,
                absolute_offset + offset,
                offsets,
            );
        }
    }
}

fn collect_quoted_literals(
    source: &str,
    quoted_literals: &mut Vec<MermaidQuotedSpan>,
    offsets: &[usize],
) {
    let bytes = source.as_bytes();
    let mut cursor = 0_usize;

    while cursor < bytes.len() {
        let Some(relative_start) = memchr::memchr3(b'"', b'\'', b'`', &bytes[cursor..]) else {
            break;
        };
        let start_byte = cursor + relative_start;
        let (style, terminator) = match bytes[start_byte] {
            b'"' => (MermaidQuoteStyle::Double, b'"'),
            b'\'' => (MermaidQuoteStyle::Single, b'\''),
            _ => (MermaidQuoteStyle::Backtick, b'`'),
        };
        cursor = start_byte + 1;

        let end_byte = if terminator == b'`' {
            memchr::memchr(terminator, &bytes[cursor..]).map(|relative| cursor + relative + 1)
        } else {
            let mut end_byte = None;
            while cursor < bytes.len() {
                let Some(relative) = memchr::memchr2(terminator, b'\\', &bytes[cursor..]) else {
                    break;
                };
                let delimiter = cursor + relative;
                if bytes[delimiter] == terminator {
                    end_byte = Some(delimiter + 1);
                    break;
                }

                cursor = delimiter + 1;
                if let Some(escaped) = source[cursor..].chars().next() {
                    cursor += escaped.len_utf8();
                }
            }
            end_byte
        };

        let Some(end_byte) = end_byte else {
            break;
        };
        push_quoted_span(
            source,
            quoted_literals,
            style,
            start_byte,
            end_byte,
            offsets,
        );
        cursor = end_byte;
    }
}

fn push_whitespace_span(
    source: &str,
    whitespace: &mut Vec<MermaidWhitespaceSpan>,
    kind: MermaidWhitespaceKind,
    start_byte: usize,
    end_byte: usize,
    offsets: &[usize],
) {
    if start_byte >= end_byte {
        return;
    }
    let Some(text) = source.get(start_byte..end_byte) else {
        return;
    };
    whitespace.push(MermaidWhitespaceSpan {
        kind,
        span: span_for_range(source, start_byte, end_byte, offsets),
        text_range: MermaidTextRange {
            start_byte,
            end_byte,
        },
        text: text.to_string(),
    });
}

fn push_comment_span(
    source: &str,
    comments: &mut Vec<MermaidCommentSpan>,
    start_byte: usize,
    end_byte: usize,
    offsets: &[usize],
) {
    if start_byte >= end_byte {
        return;
    }
    let Some(text) = source.get(start_byte..end_byte) else {
        return;
    };
    comments.push(MermaidCommentSpan {
        span: span_for_range(source, start_byte, end_byte, offsets),
        text_range: MermaidTextRange {
            start_byte,
            end_byte,
        },
        text: text.to_string(),
    });
}

fn push_directive_span(
    source: &str,
    directives: &mut Vec<MermaidDirectiveSpan>,
    start_byte: usize,
    end_byte: usize,
    offsets: &[usize],
) {
    if start_byte >= end_byte {
        return;
    }
    let Some(text) = source.get(start_byte..end_byte) else {
        return;
    };
    directives.push(MermaidDirectiveSpan {
        span: span_for_range(source, start_byte, end_byte, offsets),
        text_range: MermaidTextRange {
            start_byte,
            end_byte,
        },
        text: text.to_string(),
    });
}

fn push_quoted_span(
    source: &str,
    quoted_literals: &mut Vec<MermaidQuotedSpan>,
    style: MermaidQuoteStyle,
    start_byte: usize,
    end_byte: usize,
    offsets: &[usize],
) {
    if start_byte >= end_byte {
        return;
    }
    let Some(text) = source.get(start_byte..end_byte) else {
        return;
    };
    quoted_literals.push(MermaidQuotedSpan {
        style,
        span: span_for_range(source, start_byte, end_byte, offsets),
        text_range: MermaidTextRange {
            start_byte,
            end_byte,
        },
        text: text.to_string(),
    });
}

fn line_offsets_and_ending_style(source: &str) -> (Vec<usize>, MermaidLineEndingStyle) {
    let bytes = source.as_bytes();
    let mut offsets = vec![0];
    let mut has_crlf = false;
    let mut has_lf = false;
    for newline in memchr::memchr_iter(b'\n', bytes) {
        offsets.push(newline + 1);
        if newline > 0 && bytes[newline - 1] == b'\r' {
            has_crlf = true;
        } else {
            has_lf = true;
        }
    }
    let line_ending = match (has_crlf, has_lf) {
        (false, false) => MermaidLineEndingStyle::None,
        (false, true) => MermaidLineEndingStyle::Lf,
        (true, false) => MermaidLineEndingStyle::Crlf,
        (true, true) => MermaidLineEndingStyle::Mixed,
    };
    (offsets, line_ending)
}

fn span_for_range(source: &str, start_byte: usize, end_byte: usize, offsets: &[usize]) -> Span {
    let start = position_for_byte(source, start_byte, offsets);
    if end_byte <= start_byte {
        return Span::new(start, start);
    }

    let end_inclusive = source[..end_byte]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(start_byte);
    Span::new(start, position_for_byte(source, end_inclusive, offsets))
}

fn position_for_byte(source: &str, byte_index: usize, offsets: &[usize]) -> Position {
    let clamped = byte_index.min(source.len());
    let line = offsets.partition_point(|&offset| offset <= clamped);
    let line_start = offsets[line.saturating_sub(1)];
    let col = source[line_start..clamped].chars().count() + 1;
    Position {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        col: u32::try_from(col).unwrap_or(u32::MAX),
        byte: u32::try_from(clamped).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::{
        FlowchartBatchParsePlan, FlowchartBatchParseScratch, MermaidLineEndingStyle,
        MermaidWhitespaceKind, ParserConfig, apply_parse_lens_edit, build_parse_lens,
        capture_format_complement, detect_type, normalize_identifier, normalize_identifier_cow, parse,
        parse_with_mode,
    };

    #[test]
    fn batch_plan_reuses_complete_prefix_subgraphs_exactly() {
        let prefix = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared ingestion platform\"]\n",
            "    S0[\"Receive & validate events\"]\n",
            "    S1[\"Normalize payload safely\"]\n",
            "    S2[\"Publish canonical records\"]\n",
            "    S0-->S1\n",
            "    S1-->S2\n",
            "  end\n",
        );
        let inputs = [
            format!("{prefix}  S2-->A[\"Analytics consumer\"]"),
            format!("{prefix}  S2-->B[\"Billing consumer\"]"),
            "flowchart TD\nX[Independent]-->Y[Diagram]".to_owned(),
        ];
        let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan =
            FlowchartBatchParsePlan::new(&refs, MermaidParseMode::Compat, &ParserConfig::default());

        assert_eq!(plan.stats().shared_prefix_groups, 1);
        assert_eq!(plan.stats().shared_prefix_inputs, 2);
        assert_eq!(plan.stats().reused_prefix_parses, 1);
        assert_eq!(plan.stats().reused_prefix_bytes, prefix.len());
        for (index, input) in inputs.iter().enumerate() {
            assert_eq!(plan.parse(index, input), parse(input));
        }
    }

    #[test]
    fn batch_plan_reuses_common_early_subgraph_when_later_blocks_diverge() {
        let shared = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared ingestion platform\"]\n",
            "    S0[\"Receive & validate events\"]\n",
            "    S1[\"Normalize payload safely\"]\n",
            "    S2[\"Publish canonical records\"]\n",
            "    S0-->S1\n",
            "    S1-->S2\n",
            "  end\n",
        );
        let inputs = [
            format!(
                "{shared}  subgraph Analytics\n    A0[Warehouse]-->A1[Dashboard]\n  end\n  S2-->A0"
            ),
            format!("{shared}  subgraph Billing\n    B0[Ledger]-->B1[Invoice]\n  end\n  S2-->B0"),
        ];
        let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan =
            FlowchartBatchParsePlan::new(&refs, MermaidParseMode::Compat, &ParserConfig::default());

        assert_eq!(plan.stats().shared_prefix_groups, 1);
        assert_eq!(plan.stats().shared_prefix_inputs, 2);
        assert_eq!(plan.stats().reused_prefix_parses, 1);
        assert_eq!(plan.stats().reused_prefix_bytes, shared.len());
        for (index, input) in inputs.iter().enumerate() {
            assert_eq!(plan.parse(index, input), parse(input));
        }
    }

    #[test]
    fn batch_plan_reuses_builder_allocations_without_changing_parse_output() {
        let shared = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared ingestion platform\"]\n",
            "    S0[\"Receive & validate events\"]\n",
            "    S1[\"Normalize payload safely\"]\n",
            "    S2[\"Publish canonical records\"]\n",
            "    S0-->S1\n",
            "    S1-->S2\n",
            "  end\n",
        );
        let inputs = [
            format!("{shared}  S2-->A[\"Analytics consumer\"]"),
            format!("{shared}  S2-->B[\"Billing consumer\"]"),
        ];
        let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan =
            FlowchartBatchParsePlan::new(&refs, MermaidParseMode::Compat, &ParserConfig::default());
        let mut scratch = FlowchartBatchParseScratch::default();
        let mut first_prefix_id_allocation = None;

        for (index, input) in inputs.iter().enumerate() {
            plan.with_parse_scratch(index, input, &mut scratch, |actual| {
                let expected = parse(input);
                assert_eq!(actual.ir, &expected.ir);
                assert_eq!(actual.warnings, expected.warnings);
                assert_eq!(actual.confidence, expected.confidence);
                assert_eq!(actual.detection_method, expected.detection_method);
                assert!(actual.reusable_prefix.is_some());

                let prefix_id_allocation = actual.ir.nodes[0].id.as_ptr();
                if let Some(first) = first_prefix_id_allocation {
                    assert_eq!(prefix_id_allocation, first);
                } else {
                    first_prefix_id_allocation = Some(prefix_id_allocation);
                }
            });
        }
    }

    #[test]
    fn batch_scratch_restores_full_prefix_after_a_mutating_suffix() {
        let shared = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared ingestion platform\"]\n",
            "    S0[\"Receive events\"]\n",
            "    S1[\"Publish records\"]\n",
            "    S0-->S1\n",
            "  end\n",
        );
        let inputs = [
            format!("{shared}  S0((Changed shape))-->A"),
            format!("{shared}  S1-->B[\"Independent suffix\"]"),
        ];
        let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan =
            FlowchartBatchParsePlan::new(&refs, MermaidParseMode::Compat, &ParserConfig::default());
        let mut scratch = FlowchartBatchParseScratch::default();

        plan.with_parse_scratch(0, &inputs[0], &mut scratch, |actual| {
            assert_eq!(actual.ir, &parse(&inputs[0]).ir);
            assert!(actual.reusable_prefix.is_none());
        });
        plan.with_parse_scratch(1, &inputs[1], &mut scratch, |actual| {
            assert_eq!(actual.ir, &parse(&inputs[1]).ir);
            assert!(actual.reusable_prefix.is_some());
        });
    }

    #[test]
    fn batch_scratch_invalidates_every_flowchart_prefix_mutation_family() {
        let shared = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared ingestion platform\"]\n",
            "    S0[\"Receive events\"]\n",
            "    S1[\"Publish records\"]\n",
            "    S0-->S1\n",
            "  end\n",
        );
        let inputs = [
            format!("{shared}  direction TB\n  S1-->A"),
            format!("{shared}  class S0 hot\n  S1-->B"),
            format!("{shared}  click S0 \"https://example.com\"\n  S1-->C"),
            format!("{shared}  subgraph Shared[\"Shared ingestion platform\"]\n    S1-->D\n  end"),
        ];
        let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan =
            FlowchartBatchParsePlan::new(&refs, MermaidParseMode::Compat, &ParserConfig::default());
        let mut scratch = FlowchartBatchParseScratch::default();

        for (index, input) in inputs.iter().enumerate() {
            plan.with_parse_scratch(index, input, &mut scratch, |actual| {
                assert_eq!(actual.ir, &parse(input).ir);
                assert!(actual.reusable_prefix.is_none());
            });
        }
    }

    #[test]
    fn batch_plan_falls_back_when_global_directives_cross_the_prefix_boundary() {
        let prefix = concat!(
            "flowchart LR\n",
            "  subgraph Shared[\"Shared platform with enough source to clear the cache floor\"]\n",
            "    S0[Receive]-->S1[Normalize]\n",
            "    S1-->S2[Publish]\n",
            "  end\n",
        );
        let inputs = [
            format!("{prefix}  style S0 fill:#fff\n  S2-->A"),
            format!("{prefix}  style S0 fill:#fff\n  S2-->B"),
        ];
        let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan =
            FlowchartBatchParsePlan::new(&refs, MermaidParseMode::Compat, &ParserConfig::default());

        assert_eq!(plan.stats(), super::FlowchartBatchParseStats::default());
        for (index, input) in inputs.iter().enumerate() {
            assert_eq!(plan.parse(index, input), parse(input));
        }
    }

    #[test]
    fn matches_keyword_header_ci_is_byte_identical() {
        // Pin the alloc-free CI matcher to `matches_keyword_header(&line.to_ascii_lowercase(), kw)`.
        let lines = [
            "sankey",
            "Sankey",
            "SANKEY-BETA",
            "sankey-beta",
            "sankey ",
            "Sankey-Beta title",
            "sankeyx",
            "sankey_beta",
            "san",
            "",
            "sankey\t",
            "sankey-",
            "title x",
            "TITLE",
            "sÄnkey",
            "sankeyÄ",
            "  sankey",
            "block-beta",
            "sankey beta extra",
        ];
        for kw in ["sankey", "sankey-beta", "title", "block-beta"] {
            for l in lines {
                let want = super::matches_keyword_header(&l.to_ascii_lowercase(), kw);
                assert_eq!(
                    super::matches_keyword_header_ci(l, kw),
                    want,
                    "line={l:?} kw={kw:?}"
                );
            }
        }
    }

    fn exact_keyword_match_lower_reference(line: &str) -> Option<super::DetectedType> {
        let lower = line.to_ascii_lowercase();
        let diagram_type = super::exact_diagram_type_with(&lower, super::matches_keyword_header)?;
        Some(super::DetectedType {
            diagram_type,
            confidence: 1.0,
            method: super::DetectionMethod::ExactKeyword,
            warnings: Vec::new(),
        })
    }

    #[test]
    fn exact_keyword_match_ci_matches_lowercase_reference() {
        let headers = [
            "flowchart",
            "graph",
            "sequenceDiagram",
            "classDiagram",
            "stateDiagram",
            "gantt",
            "erDiagram",
            "mindmap",
            "pie",
            "gitGraph",
            "journey",
            "requirementDiagram",
            "timeline",
            "quadrantChart",
            "sankey",
            "xychart",
            "block-beta",
            "block",
            "packet-beta",
            "architecture-beta",
            "C4Context",
            "C4Container",
            "C4Component",
            "C4Dynamic",
            "C4Deployment",
            "kanban",
        ];

        for header in headers {
            for line in [
                header.to_string(),
                header.to_ascii_uppercase(),
                format!("{header} LR"),
                format!("{header}-beta"),
                format!("{header}x"),
            ] {
                assert_eq!(
                    super::exact_keyword_match(&line),
                    exact_keyword_match_lower_reference(&line),
                    "line={line:?}"
                );
            }
        }

        for line in [
            "",
            " flowchart",
            "sequence_Diagram",
            "not-a-diagram",
            "Ägraph",
        ] {
            assert_eq!(
                super::exact_keyword_match(line),
                exact_keyword_match_lower_reference(line),
                "line={line:?}"
            );
        }
    }

    use fm_core::{
        ArrowType, DiagnosticCategory, DiagramType, GraphDirection, IrEndpoint, MermaidDiagramIr,
        MermaidLensEdit, MermaidParseMode,
    };
    use proptest::prelude::*;

    #[test]
    fn detects_flowchart_keyword() {
        assert_eq!(detect_type("flowchart LR\nA-->B"), DiagramType::Flowchart);
    }

    #[test]
    fn detects_sequence_keyword() {
        assert_eq!(
            detect_type("sequenceDiagram\nAlice->>Bob: Hello"),
            DiagramType::Sequence
        );
    }

    #[test]
    fn detects_dot_inputs_as_flowchart() {
        assert_eq!(detect_type("digraph G { a -> b; }"), DiagramType::Flowchart);
    }

    #[test]
    fn empty_input_returns_warning() {
        let result = parse("");
        assert_eq!(result.ir.diagram_type, DiagramType::Unknown);
        assert_eq!(result.warnings.len(), 1);
    }

    /// The five ir_builder sites converted to the borrowing form must behave identically.
    ///
    /// `add_sequence_note`, `activate_participant`, `deactivate_participant`,
    /// `add_lifecycle_create` and `add_lifecycle_destroy` each allocated a `String` purely to
    /// borrow it for one lookup and then dropped it. They now take `normalize_identifier_cow`.
    /// Every one of them resolves a participant by NAME, so a conversion error would show up as a
    /// lookup that silently misses — a note attached to nothing, an activation that never opens,
    /// a destroy marker that never lands. This drives all five through one document and asserts
    /// the resolved effects, not merely that parsing succeeded.
    #[test]
    fn sequence_participant_lookups_survive_the_borrowing_conversion() {
        let src = "sequenceDiagram\n  participant Alice\n  participant Bob\n  \
Note over Alice,Bob: setup\n  Alice->>+Bob: request\n  Bob-->>-Alice: reply\n  \
create participant Carol\n  Bob->>Carol: spawn\n  destroy Carol\n  Carol->>Bob: bye\n";
        let ir = parse(src).ir;

        // Participants resolved by name, including the one created mid-diagram.
        let ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
        for want in ["Alice", "Bob", "Carol"] {
            assert!(ids.contains(&want), "participant {want} missing: {ids:?}");
        }

        // add_sequence_note resolved its target rather than dropping the note.
        let notes = ir
            .sequence_meta
            .as_ref()
            .map_or(0, |m| m.notes.len());
        assert!(notes >= 1, "the note did not resolve to a participant");

        // activate/deactivate resolved Bob: an unresolved name leaves no activation at all.
        let activations = ir
            .sequence_meta
            .as_ref()
            .map_or(0, |m| m.activations.len());
        assert!(activations >= 1, "activation did not resolve its participant");

        // add_lifecycle_create / add_lifecycle_destroy resolved Carol; an unresolved name would
        // leave no lifecycle event at all, which is precisely how a bad conversion would present.
        let lifecycle = ir
            .sequence_meta
            .as_ref()
            .map_or(0, |m| m.lifecycle_events.len());
        assert!(lifecycle >= 1, "create/destroy did not resolve their participant");

        // The messages themselves are unaffected.
        assert!(ir.edges.len() >= 4, "expected four messages, got {}", ir.edges.len());
    }

    /// The Cow variant must be BYTE-IDENTICAL to the owning one on every shape of input.
    ///
    /// This is the whole safety argument for the lever: `normalize_identifier` now delegates to
    /// `normalize_identifier_cow`, so if the two ever disagree, 49 call sites change behaviour at
    /// once. The cases below walk every branch — fast path, each quote style, whitespace joining,
    /// the `:;,` break, the trailing-`_` trim, the grapheme fallback and the hashed last resort.
    #[test]
    fn normalize_identifier_cow_matches_the_owning_form_exactly() {
        let cases = [
            "",
            "   ",
            "A",
            "P0",
            "Node_1",
            "a-b.c/d",
            "trailing_",
            "\"quoted id\"",
            "'single quoted'",
            "`backtick quoted`",
            "with spaces here",
            "break:here",
            "break;here",
            "break,here",
            "leading break:x",
            "___",
            "  padded  ",
            "\"\"",
            "naïve",
            "日本語",
            "emoji 🎉 here",
            "!!!",
            "!!!abc",
        ];
        for case in cases {
            assert_eq!(
                normalize_identifier_cow(case).as_ref(),
                normalize_identifier(case).as_str(),
                "cow and owning forms disagree on {case:?}"
            );
        }
    }

    /// The lever itself: the common case must BORROW, not allocate.
    ///
    /// Asserted on the pointer, because `Cow::Borrowed` is the entire point — checking only the
    /// string value would pass just as happily if the fast path still called `to_owned()`, which is
    /// exactly the regression this guards.
    #[test]
    fn a_clean_identifier_borrows_instead_of_allocating() {
        let raw = String::from("P0");
        match normalize_identifier_cow(&raw) {
            std::borrow::Cow::Borrowed(borrowed) => assert!(
                std::ptr::eq(borrowed.as_ptr(), raw.as_ptr()),
                "borrowed, but not from the caller's own buffer"
            ),
            std::borrow::Cow::Owned(_) => panic!("clean id allocated; the lever is not firing"),
        }

        // A trimmed-but-otherwise-clean id still borrows, pointing INTO the input.
        assert!(matches!(
            normalize_identifier_cow("  Node_1  "),
            std::borrow::Cow::Borrowed(_)
        ));

        // And an id that genuinely needs rebuilding must still own.
        assert!(matches!(
            normalize_identifier_cow("with spaces"),
            std::borrow::Cow::Owned(_)
        ));
        assert!(matches!(
            normalize_identifier_cow("trailing_"),
            std::borrow::Cow::Owned(_)
        ));
    }

    #[test]
    fn normalize_identifier_falls_back_to_hashed_id_for_non_ascii() {
        let first = normalize_identifier("你好");
        let second = normalize_identifier("你好");
        let other = normalize_identifier("こんにちは");

        assert!(!first.is_empty());
        assert!(first.starts_with("id_"));
        assert_eq!(first, second);
        assert_ne!(first, other);
        assert!(
            first
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        );
    }

    #[test]
    fn format_complement_captures_directives_comments_quotes_and_line_endings() {
        let input = "%%{init: {\"theme\":\"dark\"}}%%\r\n  %% comment\r\nflowchart LR\r\n  A[\"Alpha\"] --> B[`Beta`]  \r\n\r\n";
        let complement = capture_format_complement(input);

        assert_eq!(complement.line_ending, MermaidLineEndingStyle::Crlf);
        assert!(complement.trailing_newline);
        assert_eq!(complement.directives.len(), 1);
        assert_eq!(complement.comments.len(), 1);
        assert!(
            complement
                .quoted_literals
                .iter()
                .any(|quoted| quoted.text == "\"theme\"")
        );
        assert!(
            complement
                .quoted_literals
                .iter()
                .any(|quoted| quoted.text == "\"Alpha\"")
        );
        assert!(
            complement
                .quoted_literals
                .iter()
                .any(|quoted| quoted.text == "`Beta`")
        );
        assert!(
            complement
                .whitespace
                .iter()
                .any(|whitespace| whitespace.kind == MermaidWhitespaceKind::Indent)
        );
        assert!(
            complement
                .whitespace
                .iter()
                .any(|whitespace| whitespace.kind == MermaidWhitespaceKind::Trailing)
        );
        assert!(
            complement
                .whitespace
                .iter()
                .any(|whitespace| whitespace.kind == MermaidWhitespaceKind::BlankLine)
        );
    }

    fn newline_metadata_scalar_reference(source: &str) -> (Vec<usize>, MermaidLineEndingStyle) {
        let mut offsets = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                offsets.push(index + 1);
            }
        }

        let mut crlf = 0_usize;
        let mut lf = 0_usize;
        let bytes = source.as_bytes();
        let mut index = 0_usize;
        while index < bytes.len() {
            match bytes[index] {
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    crlf += 1;
                    index += 2;
                }
                b'\n' => {
                    lf += 1;
                    index += 1;
                }
                _ => index += 1,
            }
        }

        let style = match (crlf > 0, lf > 0) {
            (false, false) => MermaidLineEndingStyle::None,
            (false, true) => MermaidLineEndingStyle::Lf,
            (true, false) => MermaidLineEndingStyle::Crlf,
            (true, true) => MermaidLineEndingStyle::Mixed,
        };
        (offsets, style)
    }

    #[test]
    fn fused_newline_metadata_matches_scalar_reference() {
        for source in [
            "",
            "no newline",
            "\n",
            "\r\n",
            "lone carriage return\r",
            "one\n",
            "one\r\n",
            "one\ntwo\r\nthree",
            "α\r\nβ\n中文",
            "\n\n",
            "\r\n\r\n",
            "prefix\r\r\nsuffix",
        ] {
            assert_eq!(
                super::line_offsets_and_ending_style(source),
                newline_metadata_scalar_reference(source),
                "source={source:?}"
            );
        }
    }

    fn quoted_literals_scalar_reference(
        source: &str,
        offsets: &[usize],
    ) -> Vec<super::MermaidQuotedSpan> {
        let mut quoted_literals = Vec::new();
        let mut active: Option<(super::MermaidQuoteStyle, usize, char)> = None;
        let mut escaped = false;

        for (byte_index, ch) in source.char_indices() {
            if let Some((style, start_byte, terminator)) = active {
                if escaped {
                    escaped = false;
                    continue;
                }
                if terminator != '`' && ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == terminator {
                    super::push_quoted_span(
                        source,
                        &mut quoted_literals,
                        style,
                        start_byte,
                        byte_index + ch.len_utf8(),
                        offsets,
                    );
                    active = None;
                }
                continue;
            }

            let style = match ch {
                '"' => Some(super::MermaidQuoteStyle::Double),
                '\'' => Some(super::MermaidQuoteStyle::Single),
                '`' => Some(super::MermaidQuoteStyle::Backtick),
                _ => None,
            };
            if let Some(style) = style {
                active = Some((style, byte_index, ch));
                escaped = false;
            }
        }

        quoted_literals
    }

    #[test]
    fn delimiter_indexed_quoted_literals_match_scalar_reference() {
        for source in [
            "",
            "flowchart LR\nA --> B\n",
            "A[\"double\"] B['single'] C[`backtick`]",
            r#"A["escaped \" quote"] B['escaped \' quote']"#,
            "A[`backslash does not escape \\` tail `]",
            "A[\"Unicode αβ 中文 🚀\"] --> B['café']",
            "A[\"line one\nline two\"]\r\nB[`multi\nline`]",
            "\"outer 'single' `tick` outer\" 'outer \"double\" outer'",
            "dangling \"quote",
            "dangling 'escape\\",
            "%%{init: {\"theme\":\"dark\", \"label\":\"a\\\"b\"}}%%",
            "'' \"\" `` '\\\\' \"\\\\\"",
        ] {
            let (offsets, _) = super::line_offsets_and_ending_style(source);
            let expected = quoted_literals_scalar_reference(source, &offsets);
            let mut actual = Vec::new();
            super::collect_quoted_literals(source, &mut actual, &offsets);
            assert_eq!(actual, expected, "source={source:?}");
        }
    }

    #[test]
    fn parse_result_exposes_format_complement() {
        let input =
            "%%{init: {\"theme\":\"dark\"}}%%\n%% comment\nflowchart LR\nA[Alpha] --> B[Beta]\n";
        // `parse` no longer captures the format complement on the hot path; consumers
        // (here and `build_parse_lens`) capture it explicitly when needed.
        let mut result = parse(input);
        result.format_complement = capture_format_complement(input);

        assert_eq!(result.format_complement.directives.len(), 1);
        assert_eq!(result.format_complement.comments.len(), 1);
        assert_eq!(
            result.format_complement.line_ending,
            MermaidLineEndingStyle::Lf
        );
        assert!(
            result
                .format_complement
                .quoted_literals
                .iter()
                .any(|quoted| quoted.text == "\"theme\"")
        );
    }

    #[test]
    fn build_parse_lens_collects_bindings_source_map_and_format_complement() {
        let input = "%% comment\nflowchart LR\nA[Alpha] --> B[Beta]\n";
        let lens = build_parse_lens(input);

        assert_eq!(lens.original_source(), input);
        assert_eq!(lens.parsed.format_complement.comments.len(), 1);
        assert_eq!(lens.source_map.entries.len(), 3);
        assert!(
            lens.bindings
                .iter()
                .any(|binding| binding.snippet.as_deref() == Some("A[Alpha] --> B[Beta]"))
        );
    }

    #[test]
    fn apply_parse_lens_edit_rebuilds_snapshot_after_edit() {
        let input = "%% comment\nflowchart LR\nA[Alpha] --> B[Beta]\n";
        let response = apply_parse_lens_edit(
            input,
            &MermaidLensEdit {
                element_id: "fm-edge-0".to_string(),
                replacement: "A[Alpha] -.-> B[Beta]".to_string(),
            },
        )
        .expect("parse lens edit should succeed");

        assert!(response.result.updated_source.contains("-.->"));
        assert_eq!(response.snapshot.parsed.format_complement.comments.len(), 1);
        assert!(
            response
                .snapshot
                .bindings
                .iter()
                .any(|binding| binding.snippet.as_deref() == Some("A[Alpha] -.-> B[Beta]"))
        );
    }

    #[test]
    fn parse_lens_snapshot_applies_edits_to_its_preserved_crlf_source() {
        let input = "%% comment\r\nflowchart LR\r\nA[Alpha] --> B[Beta]\r\n";
        let lens = build_parse_lens(input);

        let result = lens
            .apply_edit(&MermaidLensEdit {
                element_id: "fm-edge-0".to_string(),
                replacement: "A[Alpha] -.-> B[Beta]".to_string(),
            })
            .expect("snapshot-bound lens edit should succeed");

        assert_eq!(lens.original_source(), input);
        assert_eq!(
            result.updated_source,
            "%% comment\r\nflowchart LR\r\nA[Alpha] -.-> B[Beta]\r\n"
        );
    }

    #[test]
    fn parse_lens_snapshot_returns_owned_original_source_without_normalizing_it() {
        let input = "%% comment\r\nflowchart LR\r\nA[\"Alpha\"] --> B[Beta]\r\n";

        assert_eq!(build_parse_lens(input).into_original_source(), input);
    }

    #[test]
    fn parse_flowchart_deduplicates_identical_labels() {
        let input = "flowchart TD\nA[Same Label]\nB[Same Label]";
        let result = parse(input);

        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(
            result.ir.labels.len(),
            1,
            "Identical label text should be deduplicated"
        );

        let label_id_a = result.ir.nodes[0].label.expect("A should have label");
        let label_id_b = result.ir.nodes[1].label.expect("B should have label");
        assert_eq!(
            label_id_a, label_id_b,
            "Both nodes should point to the same label entry"
        );
    }

    #[test]
    fn parse_flowchart_reopened_subgraph_does_not_duplicate_ir_entries() {
        let input = "flowchart TD\nsubgraph S1\n  A\nend\nsubgraph S1\n  B\nend";
        let result = parse(input);

        // Should only have 1 cluster and 1 subgraph entry
        assert_eq!(result.ir.clusters.len(), 1, "Should only have 1 cluster");
        assert_eq!(
            result.ir.graph.subgraphs.len(),
            1,
            "Should only have 1 subgraph"
        );

        let cluster = &result.ir.clusters[0];
        assert_eq!(
            cluster.members.len(),
            2,
            "Cluster should have 2 members (A and B)"
        );
    }

    #[test]
    fn parse_flowchart_extracts_nodes_edges_and_direction() {
        let result = parse("flowchart LR\nA[Start] --> B(End)");
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.ir.direction, GraphDirection::LR);
        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(result.ir.edges.len(), 1);
        assert!(result.warnings.is_empty());

        let edge = &result.ir.edges[0];
        assert_eq!(edge.arrow, ArrowType::Arrow);
        assert_eq!(edge.from, IrEndpoint::Node(fm_core::IrNodeId(0)));
        assert_eq!(edge.to, IrEndpoint::Node(fm_core::IrNodeId(1)));
    }

    #[test]
    fn parse_routes_dot_inputs_through_dot_parser() {
        let result = parse("digraph G { a -> b; }");
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(result.ir.edges.len(), 1);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn strict_mode_accepts_architecture_diagram_family_without_fallback() {
        let result = parse_with_mode(
            "architecture-beta\nservice api[API]\nservice db[DB]\napi --> db\n",
            MermaidParseMode::Strict,
        );
        assert_eq!(result.ir.diagram_type, DiagramType::ArchitectureBeta);
        assert_eq!(result.parse_mode(), MermaidParseMode::Strict);
        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(result.ir.edges.len(), 1);
        assert!(!result.ir.has_errors());
    }

    #[test]
    fn compat_mode_parses_architecture_without_compatibility_diagnostic() {
        let result = parse_with_mode(
            "architecture-beta\nservice api[API]\nservice db[DB]\napi --> db\n",
            MermaidParseMode::Compat,
        );
        assert_eq!(result.ir.diagram_type, DiagramType::ArchitectureBeta);
        assert_eq!(result.parse_mode(), MermaidParseMode::Compat);
        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(result.ir.edges.len(), 1);
        assert!(
            !result
                .ir
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.category == DiagnosticCategory::Compatibility })
        );
    }

    #[test]
    fn recover_mode_marks_unknown_detection_as_recovery() {
        let detection = super::DetectedType {
            diagram_type: DiagramType::Unknown,
            confidence: 0.1,
            method: super::DetectionMethod::Fallback,
            warnings: vec!["forced unknown detection for contract coverage".to_string()],
        };
        let result = crate::mermaid_parser::parse_mermaid_with_detection(
            "???\nthis is not mermaid\n",
            detection,
            MermaidParseMode::Recover,
        );
        assert_eq!(result.parse_mode(), MermaidParseMode::Recover);
        assert_eq!(result.ir.diagram_type, DiagramType::Unknown);
        assert!(result.ir.diagnostics.iter().any(|diagnostic| {
            diagnostic.category == DiagnosticCategory::Recovery
                && diagnostic
                    .message
                    .contains("falling back to flowchart-style recovery")
        }));
    }

    #[test]
    fn evidence_json_contains_counts_and_type() {
        let result = parse("flowchart LR\nA-->B");
        let evidence = super::parse_evidence_json(&result);
        assert!(evidence.contains("\"diagram_type\":\"flowchart\""));
        assert!(evidence.contains("\"node_count\":2"));
        assert!(evidence.contains("\"edge_count\":1"));
    }

    // Detection tests
    use super::{DetectionMethod, detect_type_with_confidence};

    #[test]
    fn detection_exact_keyword_high_confidence() {
        let result = detect_type_with_confidence("flowchart LR\nA-->B");
        assert_eq!(result.diagram_type, DiagramType::Flowchart);
        assert!((result.confidence - 1.0).abs() < f32::EPSILON);
        assert_eq!(result.method, DetectionMethod::ExactKeyword);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn detection_fuzzy_keyword_with_typo() {
        // "flwochart" has edit distance 2 from "flowchart" (transposed letters)
        // This won't match starts_with("flowchart") so it exercises fuzzy matching
        let result = detect_type_with_confidence("flwochart LR\nA-->B");
        assert_eq!(result.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.method, DetectionMethod::FuzzyKeyword);
        assert!(result.confidence > 0.5 && result.confidence < 1.0);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn detection_content_heuristic_er_patterns() {
        // No header, but ER relationship patterns
        let result = detect_type_with_confidence("CUSTOMER ||--o{ ORDER : places");
        assert_eq!(result.diagram_type, DiagramType::Er);
        assert_eq!(result.method, DetectionMethod::ContentHeuristic);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn detection_content_heuristic_sequence_patterns() {
        // No header, but sequence diagram patterns
        let result = detect_type_with_confidence("Alice ->> Bob: Hello\nBob ->> Alice: Hi");
        assert_eq!(result.diagram_type, DiagramType::Sequence);
        assert_eq!(result.method, DetectionMethod::ContentHeuristic);
    }

    #[test]
    fn detection_dot_format() {
        let result = detect_type_with_confidence("digraph G { a -> b; }");
        assert_eq!(result.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.method, DetectionMethod::DotFormat);
        assert!(result.confidence > 0.9);
    }

    #[test]
    fn detection_fallback_for_unknown() {
        let result = detect_type_with_confidence("some random text\nmore text");
        assert_eq!(result.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.method, DetectionMethod::Fallback);
        assert!(result.confidence < 0.5);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn detection_empty_input() {
        let result = detect_type_with_confidence("");
        assert_eq!(result.diagram_type, DiagramType::Unknown);
        assert_eq!(result.method, DetectionMethod::Fallback);
        assert!(result.confidence.abs() < f32::EPSILON);
    }

    #[test]
    fn detection_various_diagram_types() {
        let test_cases = [
            ("sequenceDiagram\nAlice->>Bob: Hi", DiagramType::Sequence),
            ("classDiagram\nAnimal <|-- Dog", DiagramType::Class),
            ("stateDiagram-v2\n[*] --> State1", DiagramType::State),
            ("erDiagram\nA ||--o{ B : has", DiagramType::Er),
            ("gantt\ntitle Project", DiagramType::Gantt),
            ("pie\ntitle Pie", DiagramType::Pie),
            ("gitGraph\ncommit", DiagramType::GitGraph),
            ("mindmap\nroot", DiagramType::Mindmap),
            ("timeline\ntitle History", DiagramType::Timeline),
            ("journey\ntitle Journey", DiagramType::Journey),
            ("block-beta\nA", DiagramType::BlockBeta),
            ("block\nA", DiagramType::BlockBeta),
        ];

        for (input, expected_type) in test_cases {
            let result = detect_type_with_confidence(input);
            assert_eq!(
                result.diagram_type,
                expected_type,
                "Failed for: {}",
                input.lines().next().unwrap_or(input)
            );
            assert_eq!(result.method, DetectionMethod::ExactKeyword);
        }
    }

    #[test]
    fn block_alias_requires_word_boundary() {
        let result = detect_type_with_confidence("blockquote\nalpha[Alpha]");
        assert_ne!(result.diagram_type, DiagramType::BlockBeta);
    }

    #[test]
    fn levenshtein_distance_basic() {
        assert_eq!(super::levenshtein_distance("cat", "cat"), 0);
        assert_eq!(super::levenshtein_distance("cat", "bat"), 1);
        assert_eq!(super::levenshtein_distance("cat", "cart"), 1);
        assert_eq!(super::levenshtein_distance("cat", "cats"), 1);
        assert_eq!(super::levenshtein_distance("cat", "dog"), 3);
        assert_eq!(super::levenshtein_distance("", "abc"), 3);
        assert_eq!(super::levenshtein_distance("abc", ""), 3);
    }

    #[test]
    fn parse_result_includes_confidence() {
        let result = parse("flowchart LR\nA-->B");
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert!((result.confidence - 1.0).abs() < f32::EPSILON);
        assert_eq!(result.detection_method, DetectionMethod::ExactKeyword);
    }

    #[test]
    fn parse_result_content_heuristic_has_lower_confidence() {
        // No explicit header, detected via content heuristics
        let result = parse("Alice ->> Bob: Hello");
        assert_eq!(result.ir.diagram_type, DiagramType::Sequence);
        assert!(result.confidence > 0.5 && result.confidence < 1.0);
        assert_eq!(result.detection_method, DetectionMethod::ContentHeuristic);
    }

    #[test]
    fn parse_result_dot_format_has_high_confidence() {
        let result = parse("digraph G { a -> b; }");
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert!(result.confidence > 0.9);
        assert_eq!(result.detection_method, DetectionMethod::DotFormat);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_parse_is_total_and_confidence_bounded(input in ".{0,256}") {
            let result = parse(&input);
            prop_assert!((0.0..=1.0).contains(&result.confidence));

            let encoded = serde_json::to_string(&result.ir).expect("serialize parser IR");
            let decoded: MermaidDiagramIr =
                serde_json::from_str(&encoded).expect("deserialize parser IR");
            prop_assert_eq!(decoded, result.ir);
        }

        #[test]
        fn prop_detect_type_with_confidence_is_deterministic(input in ".{0,256}") {
            let first = detect_type_with_confidence(&input);
            let second = detect_type_with_confidence(&input);

            prop_assert_eq!(first.diagram_type, second.diagram_type);
            prop_assert_eq!(first.method, second.method);

            prop_assert!((first.confidence - second.confidence).abs() < f32::EPSILON);
            prop_assert_eq!(first.warnings, second.warnings);
        }

        #[test]
        fn prop_flowchart_with_random_edges_never_panics(
            node_count in 1usize..10,
            edge_seed in 0u64..500,
        ) {
            let mut input = String::from("flowchart LR\n");
            for i in 0..node_count {
                writeln!(input, "  N{i}[Node {i}]").unwrap();
            }
            let mut val = edge_seed;
            for _ in 0..node_count {
                val = val.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let from = usize::try_from(val).unwrap_or(0) % node_count;
                val = val.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let to = usize::try_from(val).unwrap_or(0) % node_count;
                if from != to {
                    writeln!(input, "  N{from} --> N{to}").unwrap();
                }
            }

            let result = parse(&input);
            prop_assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
            prop_assert!(result.ir.nodes.len() >= node_count);
        }

        #[test]
        fn prop_parse_ir_is_deterministic(input in ".{0,128}") {
            let r1 = parse(&input);
            let r2 = parse(&input);
            prop_assert_eq!(r1.ir, r2.ir);
            prop_assert!((r1.confidence - r2.confidence).abs() < f32::EPSILON);
        }

        #[test]
        fn prop_parse_node_count_matches_edge_endpoints(
            node_count in 2usize..8,
        ) {
            let mut input = String::from("flowchart TB\n");
            for i in 0..node_count {
                writeln!(input, "  N{i} --> N{}", (i + 1) % node_count).unwrap();
            }
            let result = parse(&input);
            // All edge endpoints should reference existing nodes.
            for edge in &result.ir.edges {
                if let fm_core::IrEndpoint::Node(id) = edge.from {
                    prop_assert!(
                        id.0 < result.ir.nodes.len(),
                        "Edge source {} out of range (nodes={})",
                        id.0,
                        result.ir.nodes.len()
                    );
                }
                if let fm_core::IrEndpoint::Node(id) = edge.to {
                    prop_assert!(
                        id.0 < result.ir.nodes.len(),
                        "Edge target {} out of range (nodes={})",
                        id.0,
                        result.ir.nodes.len()
                    );
                }
            }
        }

        // ── Parser roundtrip invariant tests (bd-3ac.7) ──────────────

        #[test]
        fn prop_ir_serde_roundtrip_is_idempotent(input in ".{0,256}") {
            // parse(input) -> IR -> serialize -> deserialize -> IR' => IR == IR'
            let result = parse(&input);
            let json = serde_json::to_string(&result.ir).expect("serialize");
            let roundtripped: MermaidDiagramIr =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(&result.ir, &roundtripped);
        }

        #[test]
        fn prop_flowchart_roundtrip_preserves_structure(
            node_count in 2usize..12,
            edge_seed in 0u64..200,
        ) {
            let mut input = String::from("flowchart TD\n");
            for i in 0..node_count {
                writeln!(input, "  N{i}[Node {i}]").unwrap();
            }
            let mut val = edge_seed;
            for _ in 0..node_count.min(8) {
                val = val.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let from = usize::try_from(val).unwrap_or(0) % node_count;
                val = val.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let to = usize::try_from(val).unwrap_or(0) % node_count;
                if from != to {
                    writeln!(input, "  N{from} --> N{to}").unwrap();
                }
            }

            let result = parse(&input);
            let json = serde_json::to_string(&result.ir).expect("serialize");
            let roundtripped: MermaidDiagramIr =
                serde_json::from_str(&json).expect("deserialize");

            prop_assert_eq!(result.ir.diagram_type, roundtripped.diagram_type);
            prop_assert_eq!(result.ir.direction, roundtripped.direction);
            prop_assert_eq!(result.ir.nodes.len(), roundtripped.nodes.len());
            prop_assert_eq!(result.ir.edges.len(), roundtripped.edges.len());
            prop_assert_eq!(result.ir.labels.len(), roundtripped.labels.len());

            for (orig, rt) in result.ir.nodes.iter().zip(roundtripped.nodes.iter()) {
                prop_assert_eq!(&orig.id, &rt.id);
                prop_assert_eq!(orig.shape, rt.shape);
                prop_assert_eq!(orig.implicit, rt.implicit);
            }
            for (orig, rt) in result.ir.edges.iter().zip(roundtripped.edges.iter()) {
                prop_assert_eq!(orig.from, rt.from);
                prop_assert_eq!(orig.to, rt.to);
                prop_assert_eq!(orig.arrow, rt.arrow);
            }
        }

        #[test]
        fn prop_sequence_roundtrip_preserves_participants(
            participant_count in 2usize..6,
        ) {
            let names: Vec<String> = (0..participant_count)
                .map(|i| format!("P{i}"))
                .collect();
            let mut input = String::from("sequenceDiagram\n");
            for name in &names {
                writeln!(input, "  participant {name}").unwrap();
            }
            for i in 0..participant_count.saturating_sub(1) {
                writeln!(input, "  {}->>{}:msg{i}", names[i], names[i + 1]).unwrap();
            }

            let result = parse(&input);
            prop_assert_eq!(result.ir.diagram_type, DiagramType::Sequence);

            let json = serde_json::to_string(&result.ir).expect("serialize");
            let roundtripped: MermaidDiagramIr =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(result.ir.nodes.len(), roundtripped.nodes.len());
            prop_assert_eq!(result.ir.edges.len(), roundtripped.edges.len());
        }

        #[test]
        fn prop_class_diagram_roundtrip(class_count in 2usize..6) {
            let mut input = String::from("classDiagram\n");
            for i in 0..class_count {
                writeln!(input, "  class C{i}").unwrap();
            }
            for i in 1..class_count {
                writeln!(input, "  C0 <|-- C{i}").unwrap();
            }

            let result = parse(&input);
            prop_assert_eq!(result.ir.diagram_type, DiagramType::Class);

            let json = serde_json::to_string(&result.ir).expect("serialize");
            let roundtripped: MermaidDiagramIr =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(&result.ir, &roundtripped);
        }

        #[test]
        fn prop_state_diagram_roundtrip(state_count in 2usize..8) {
            let mut input = String::from("stateDiagram-v2\n");
            input.push_str("  [*] --> S0\n");
            for i in 1..state_count {
                writeln!(input, "  S{} --> S{i}", i - 1).unwrap();
            }
            writeln!(input, "  S{} --> [*]", state_count - 1).unwrap();

            let result = parse(&input);
            prop_assert_eq!(result.ir.diagram_type, DiagramType::State);

            let json = serde_json::to_string(&result.ir).expect("serialize");
            let roundtripped: MermaidDiagramIr =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(&result.ir, &roundtripped);
        }

        #[test]
        fn prop_multi_type_detection_is_stable(diagram_index in 0usize..7) {
            let inputs = [
                "flowchart LR\n  A-->B",
                "sequenceDiagram\n  A->>B:hi",
                "classDiagram\n  A <|-- B",
                "stateDiagram-v2\n  [*]-->S1",
                "erDiagram\n  A ||--o{ B : has",
                "gantt\n  section S\n  T1 :a1, 2024-01-01, 3d",
                "pie\n  \"A\":50\n  \"B\":50",
            ];
            let input = inputs[diagram_index];

            let r1 = parse(input);
            let r2 = parse(input);
            prop_assert_eq!(r1.ir.diagram_type, r2.ir.diagram_type);
            prop_assert_eq!(r1.ir.nodes.len(), r2.ir.nodes.len());
            prop_assert_eq!(r1.ir.edges.len(), r2.ir.edges.len());

            let json1 = serde_json::to_string(&r1.ir).expect("ser1");
            let json2 = serde_json::to_string(&r2.ir).expect("ser2");
            prop_assert_eq!(json1, json2, "Serialized IR must be identical");
        }

        #[test]
        fn prop_diagnostics_survive_roundtrip(input in ".{0,128}") {
            let result = parse(&input);
            let json = serde_json::to_string(&result.ir).expect("serialize");
            let roundtripped: MermaidDiagramIr =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(
                result.ir.diagnostics.len(),
                roundtripped.diagnostics.len(),
                "Diagnostic count must survive roundtrip"
            );
            for (orig, rt) in result.ir.diagnostics.iter().zip(roundtripped.diagnostics.iter()) {
                prop_assert_eq!(orig.severity, rt.severity);
                prop_assert_eq!(&orig.message, &rt.message);
            }
        }
    }

    // ── Input sanitization and security hardening tests (bd-116l) ──────

    #[test]
    fn adversarial_deeply_nested_subgraphs_does_not_panic() {
        let depth = 200;
        let mut input = String::from("flowchart TD\n");
        for i in 0..depth {
            writeln!(input, "{}subgraph sg{i}", "  ".repeat(i + 1)).unwrap();
        }
        for i in (0..depth).rev() {
            writeln!(input, "{}end", "  ".repeat(i + 1)).unwrap();
        }
        let result = parse(&input);
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
    }

    #[test]
    fn adversarial_extremely_long_single_line_does_not_panic() {
        let long_label = "A".repeat(100_000);
        let input = format!("flowchart LR\n  X[{long_label}] --> Y");
        let result = parse(&input);
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert!(!result.ir.nodes.is_empty());
    }

    #[test]
    fn adversarial_many_nodes_does_not_panic() {
        let count = 1000;
        let mut input = String::from("flowchart TD\n");
        for i in 0..count {
            let _ = writeln!(input, "  N{i}[Node {i}]");
        }
        for i in 1..count {
            let _ = writeln!(input, "  N{} --> N{i}", i - 1);
        }
        let result = parse(&input);
        assert!(result.ir.nodes.len() >= count);
    }

    #[test]
    fn adversarial_many_edges_between_same_pair_does_not_panic() {
        let mut input = String::from("flowchart LR\n  A --> B\n");
        for _ in 0..500 {
            input.push_str("  A --> B\n");
        }
        let result = parse(&input);
        assert!(!result.ir.nodes.is_empty());
        assert!(!result.ir.edges.is_empty());
    }

    #[test]
    fn adversarial_null_bytes_in_input_does_not_panic() {
        let input = "flowchart LR\n  A\0B --> C\0D";
        let result = parse(input);
        // Should handle gracefully — type detection still works.
        assert_ne!(result.ir.diagram_type, DiagramType::Unknown);
    }

    #[test]
    fn adversarial_control_characters_does_not_panic() {
        let input = "flowchart\x01 LR\n  A\x02 --> B\x03\n  B\x1b[31m --> C";
        let _result = parse(input);
        // No panic is the success condition.
    }

    #[test]
    fn adversarial_unicode_bom_does_not_panic() {
        let input = "\u{FEFF}flowchart LR\n  A --> B";
        let result = parse(input);
        assert!(!result.ir.nodes.is_empty());
    }

    #[test]
    fn adversarial_mixed_line_endings_does_not_panic() {
        let input = "flowchart LR\r\n  A --> B\r  B --> C\n  C --> D\r\n";
        let result = parse(input);
        assert!(!result.ir.nodes.is_empty());
    }

    #[test]
    fn adversarial_empty_and_whitespace_only_inputs() {
        for input in ["", " ", "\n", "\t", "\n\n\n", "   \n  \t  \n  "] {
            let result = parse(input);
            // Should not panic, should return something.
            assert_eq!(result.ir.diagram_type, DiagramType::Unknown);
        }
    }

    #[test]
    fn adversarial_repeated_keywords_does_not_panic() {
        let input = "flowchart flowchart flowchart LR\n  A --> B";
        let _result = parse(input);
    }

    #[test]
    fn adversarial_nested_brackets_does_not_panic() {
        let depth = 100;
        let open: String = "[".repeat(depth);
        let close: String = "]".repeat(depth);
        let input = format!("flowchart LR\n  A{open}deep{close} --> B");
        let _result = parse(&input);
    }

    #[test]
    fn adversarial_very_long_node_id_does_not_panic() {
        let long_id = "N".repeat(10_000);
        let input = format!("flowchart LR\n  {long_id} --> B");
        let _result = parse(&input);
    }

    #[test]
    fn adversarial_many_diagram_type_keywords_does_not_confuse() {
        // Input that mentions multiple diagram types — first keyword wins.
        let input =
            "flowchart LR\n  A --> B\nsequenceDiagram\n  C->>D: hi\nclassDiagram\n  E <|-- F";
        let result = parse(input);
        assert_eq!(
            result.ir.diagram_type,
            DiagramType::Flowchart,
            "First keyword should win"
        );
    }

    #[test]
    fn adversarial_only_edges_no_declarations_does_not_panic() {
        let input = "flowchart TD\n  --> --> --> --> -->";
        let _result = parse(input);
    }

    #[test]
    fn adversarial_init_directive_with_bad_json_does_not_panic() {
        let input = "%%{init: {{{invalid json}}}%%\nflowchart LR\n  A --> B";
        let result = parse(input);
        assert!(!result.ir.nodes.is_empty());
    }

    #[test]
    fn adversarial_binary_content_does_not_panic() {
        // Simulate feeding binary data to the parser.
        let input: String = (0..=255_u8).map(char::from).collect();
        let _result = parse(&input);
    }

    #[test]
    fn adversarial_massive_whitespace_padding_does_not_panic() {
        let padding = " ".repeat(50_000);
        let input = format!("{padding}flowchart LR\n{padding}A --> B{padding}");
        let result = parse(&input);
        assert!(!result.ir.nodes.is_empty());
    }

    // ── Adversarial parser-only tests ────────────────────────────────
    // Cross-crate adversarial tests (SVG injection, etc.) are in
    // fm-cli/tests/integration_test.rs.

    #[test]
    fn adversarial_deeply_nested_subgraphs_no_stack_overflow() {
        let mut input = String::from("flowchart LR\n");
        for i in 0..200 {
            let _ = writeln!(input, "  subgraph S{i}");
        }
        input.push_str("    A --> B\n");
        for _ in 0..200 {
            input.push_str("  end\n");
        }
        let result = parse(&input);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn adversarial_extremely_long_node_id_no_panic() {
        let long_id: String = "A".repeat(100_000);
        let input = format!("flowchart LR\n  {long_id} --> B");
        let result = parse(&input);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn adversarial_null_bytes_in_input_no_panic() {
        let input = "flowchart LR\n  A\0B --> C\0D";
        let result = parse(input);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn adversarial_unicode_bidi_override_no_panic() {
        let input = "flowchart LR\n  A[\u{202e}reversed\u{202c}] --> B";
        let result = parse(input);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn adversarial_many_parallel_edges_no_quadratic_blowup() {
        let mut input = String::from("flowchart LR\n");
        for _ in 0..500 {
            input.push_str("  A --> B\n");
        }
        let result = parse(&input);
        assert!(!result.ir.edges.is_empty());
    }

    #[test]
    fn adversarial_empty_and_whitespace_inputs() {
        for input in ["", " ", "\n", "\t", "\r\n", "   \n   \n   "] {
            let result = parse(input);
            assert!(result.confidence >= 0.0);
        }
    }

    #[test]
    fn adversarial_javascript_url_in_click_blocked() {
        let input = "flowchart LR\n  A[Node]\n  click A \"javascript:alert(document.cookie)\"";
        let result = parse(input);
        let node = result.ir.nodes.iter().find(|n| n.id == "A");
        if let Some(node) = node {
            assert!(
                node.href().is_none() || !node.href().unwrap().contains("javascript:"),
                "javascript: URLs must be blocked"
            );
        }
    }

    /// A diagram type mermaid supports and we do not must SAY so.
    ///
    /// The old message ("could not detect diagram type") sends the author to check syntax that is
    /// perfectly correct. Naming the type is the difference between "you typed it wrong" and "we
    /// have not built this yet", which have different fixes.
    #[test]
    fn an_unimplemented_upstream_type_is_named_rather_than_blamed_on_syntax() {
        for (source, expected) in [
            ("radar\n  title Skills\n  ds1 [10, 20, 30]\n", "radar"),
            ("radar-beta\n  axis a, b\n", "radar"),
            ("treemap\n  root\n    child 5\n", "treemap"),
            ("ishikawa\n  Problem\n", "ishikawa"),
            ("eventmodeling\n  x\n", "eventmodeling"),
            ("info\n", "info"),
            ("treeView-beta\n  root\n", "treeView"),
            ("venn-beta\n  a\n", "venn"),
            ("wardley-beta\n  a\n", "wardley"),
        ] {
            let detected = super::detect_type_with_confidence(source);
            let joined = detected.warnings.join(" | ");
            assert!(
                joined.contains(expected),
                "{source:?} should name {expected:?}; warnings were {joined:?}"
            );
            assert!(
                !joined.contains("Could not detect"),
                "{source:?} still reports the generic message: {joined:?}"
            );
        }
    }

    /// CONTROL: a genuine typo must STILL get the generic message.
    ///
    /// This is the assertion that keeps the feature honest. A table that matched loosely would
    /// answer "not supported yet" for a misspelling, which is a confident lie and strictly worse
    /// than the vague message it replaced -- the author would stop looking for their typo.
    #[test]
    fn a_typo_still_gets_the_generic_message() {
        for source in [
            "flowchrt TD\n  a --> b\n",
            "radarr\n  x\n",
            "treemapp\n  y\n",
            "ishikawaa\n  x\n",
        ] {
            let detected = super::detect_type_with_confidence(source);
            let joined = detected.warnings.join(" | ");
            assert!(
                !joined.contains("does not implement yet"),
                "{source:?} was reported as unimplemented rather than as an unrecognised header: \
                 {joined:?}"
            );
        }
    }

    /// CONTROL: a SUPPORTED type must be unaffected.
    ///
    /// The new check runs before the final fallback, so a header that already detects must never
    /// reach it. If one did, this renderer would start calling its own supported diagrams
    /// unsupported.
    #[test]
    fn supported_diagram_types_are_untouched_by_the_new_check() {
        for (source, want) in [
            ("flowchart TD\n  a --> b\n", fm_core::DiagramType::Flowchart),
            ("sequenceDiagram\n  A->>B: hi\n", fm_core::DiagramType::Sequence),
            ("mindmap\n  root\n", fm_core::DiagramType::Mindmap),
            ("kanban\n  Todo\n", fm_core::DiagramType::Kanban),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(detected.diagram_type, want, "{source:?} changed detection");
            assert!(
                !detected.warnings.join(" ").contains("does not implement yet"),
                "{source:?} was called unimplemented"
            );
        }
    }

    /// The bare spellings the incumbent accepts must detect as their own type.
    ///
    /// `requirement`, `packet` and `architecture` are all valid mermaid headers -- its detectors are
    /// `/^\\s*requirement(Diagram)?/`, `/^\\s*packet(-beta)?/` and `/^\\s*architecture/`. Our matcher
    /// needs the LINE to be at least as long as the keyword, so `packet-beta` as a keyword could
    /// never match a line reading `packet`, and these fell through to the flowchart fallback.
    #[test]
    fn bare_spellings_the_incumbent_accepts_are_detected() {
        for (source, want) in [
            ("requirement\n  requirement r {\n  }\n", fm_core::DiagramType::Requirement),
            ("packet\n  0-7: \"a\"\n", fm_core::DiagramType::PacketBeta),
            ("architecture\n  group a\n", fm_core::DiagramType::ArchitectureBeta),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(detected.diagram_type, want, "{source:?} was not detected");
        }
    }

    /// CONTROL: the suffixed spellings must keep working.
    ///
    /// Widening a keyword chain is exactly where an existing arm gets shadowed -- `packet` placed
    /// before `packet-beta` would still match, but a careless reorder elsewhere could route a
    /// suffixed header to the wrong type, and nothing else in this file would notice.
    #[test]
    fn suffixed_spellings_still_detect_as_before() {
        for (source, want) in [
            ("requirementDiagram\n  requirement r {\n  }\n", fm_core::DiagramType::Requirement),
            ("packet-beta\n  0-7: \"a\"\n", fm_core::DiagramType::PacketBeta),
            ("architecture-beta\n  group a\n", fm_core::DiagramType::ArchitectureBeta),
            ("sankey-beta\n  a,b,1\n", fm_core::DiagramType::Sankey),
            ("xychart-beta\n  bar [1,2]\n", fm_core::DiagramType::XyChart),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(detected.diagram_type, want, "{source:?} changed detection");
        }
    }

    /// A typo in the eight newly-covered headers is now correctable.
    ///
    /// Before this, block/packet/architecture and the five C4 headers were the only types a
    /// misspelling could never be corrected for -- they matched exactly or not at all.
    #[test]
    fn typos_in_the_late_added_types_are_corrected() {
        let config = super::ParserConfig::default();
        if config.fuzzy_keyword_distance == 0 {
            return; // fuzzy correction disabled by default configuration; nothing to assert
        }

        for (source, want) in [
            ("architectur\n  group a\n", fm_core::DiagramType::ArchitectureBeta),
            ("packt\n  0-7: \"a\"\n", fm_core::DiagramType::PacketBeta),
            ("kanbon\n  Todo\n", fm_core::DiagramType::Kanban),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(detected.diagram_type, want, "{source:?} was not corrected");
        }
    }

    /// THE ONE THAT MATTERS: a C4 typo must correct to the RIGHT C4 variant.
    ///
    /// Adding five keywords that differ from each other by a few characters is exactly how a fuzzy
    /// table starts cross-correcting. `c4contaner` is one edit from `c4container` and three from
    /// `c4context`, so nearest-wins should hold -- but nothing enforced that before these entries
    /// existed, and a tie would silently resolve to whichever is listed first.
    #[test]
    fn a_c4_typo_corrects_to_its_own_variant() {
        let config = super::ParserConfig::default();
        if config.fuzzy_keyword_distance == 0 {
            return;
        }

        for (source, want) in [
            ("c4contaner\n  Person(a, \"A\")\n", fm_core::DiagramType::C4Container),
            ("c4contex\n  Person(a, \"A\")\n", fm_core::DiagramType::C4Context),
            ("c4deploymnt\n  Person(a, \"A\")\n", fm_core::DiagramType::C4Deployment),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(
                detected.diagram_type, want,
                "{source:?} cross-corrected to another C4 variant"
            );
        }
    }

    /// CONTROL: an EXACT header must never be routed through fuzzy matching.
    ///
    /// The matcher requires distance > 0, so an exact spelling cannot be "corrected" to a
    /// neighbour. Widening the table makes that guard load-bearing: `block` and `packet` are now
    /// close enough to real words that a bug admitting distance 0 would start reclassifying valid
    /// documents.
    #[test]
    fn exact_headers_are_never_fuzzy_corrected() {
        for (source, want) in [
            ("block-beta\n  a\n", fm_core::DiagramType::BlockBeta),
            ("packet-beta\n  0-7: \"a\"\n", fm_core::DiagramType::PacketBeta),
            ("c4context\n  Person(a, \"A\")\n", fm_core::DiagramType::C4Context),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(detected.diagram_type, want, "{source:?} was rerouted");
            assert_eq!(
                detected.method,
                super::DetectionMethod::ExactKeyword,
                "{source:?} did not take the exact path"
            );
        }
    }

    /// The ER heuristic must cover the whole cardinality grid, not four spellings.
    ///
    /// The four it used to hardcode are included so the fix cannot regress the cases that already
    /// worked; the rest are combinations it silently missed -- including `}o--o{`, a plain
    /// many-to-many, and every `..` non-identifying form.
    #[test]
    fn the_er_heuristic_covers_the_cardinality_grid() {
        for relationship in [
            "||--o{", "}|--||", "||--|{", "|o--o|", // previously hardcoded
            "}o--o{", "}o--|{", "|o--|{", "||--||", // missed
            "}o..o{", "||..||", "|o..o|", "}|..|{", // non-identifying, all missed
        ] {
            let source = format!("CUSTOMER {relationship} ORDER : places\n");
            assert!(
                super::looks_like_er_relationship(&source.to_lowercase()),
                "{relationship} was not recognised as an ER relationship"
            );
        }
    }

    /// CONTROL: other diagrams' operators must NOT read as ER.
    ///
    /// The check scans for `--` and `..`, which appear in flowchart links and class dependencies
    /// too. It is the flanking cardinality pairs that make it specific, and without this control a
    /// looser version would classify every flowchart as an ER diagram.
    #[test]
    fn ordinary_operators_are_not_mistaken_for_er() {
        for source in [
            "flowchart TD\n  a --> b\n",
            "flowchart TD\n  a --- b\n",
            "flowchart TD\n  a ==> b\n",
            "classDiagram\n  A ..> B\n",
            "classDiagram\n  A <|-- B\n",
            "sequenceDiagram\n  A-->>B: hi\n",
        ] {
            assert!(
                !super::looks_like_er_relationship(&source.to_lowercase()),
                "{source:?} was misread as an ER relationship"
            );
        }
    }

    /// The class heuristic must recognise the relations our own parser accepts.
    ///
    /// It knew two of ten. A diagram whose only relation is a composition or a realization was
    /// invisible to it, and an unheaded document then fell through to the flowchart fallback.
    #[test]
    fn the_class_heuristic_covers_the_relations_the_parser_accepts() {
        for relation in ["<|--", "--|>", "<|..", "..|>", "*--", "--*", "..>", "<.."] {
            let source = format!("Order {relation} LineItem\n");
            let detected = super::detect_type_with_confidence(&source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::Class,
                "{relation} was not recognised as a class relation"
            );
        }
    }

    /// CONTROL: a flowchart circle edge must NOT be read as a class diagram.
    ///
    /// `o--` and `--o` are class relations AND flowchart circle edges, so they are excluded from
    /// the heuristic on purpose. This pins that decision: without it, someone completing the list
    /// "for consistency" would trade a missed class diagram for a corrupted flowchart, which is the
    /// worse direction -- the flowchart renders, just as the wrong type.
    #[test]
    fn flowchart_circle_edges_are_not_read_as_class_relations() {
        for source in ["A --o B\n", "A o--o B\n", "flowchart TD\n  A --o B\n"] {
            let detected = super::detect_type_with_confidence(source);
            assert_ne!(
                detected.diagram_type,
                fm_core::DiagramType::Class,
                "{source:?} was misread as a class diagram"
            );
        }
    }

    /// The sequence heuristic must recognise the async and bidirectional arrows.
    ///
    /// Our parser accepts ten arrow forms and the heuristic tested three, so a headerless exchange
    /// using only `-)` or `<<->>` fell through to the flowchart fallback.
    #[test]
    fn the_sequence_heuristic_covers_async_and_bidirectional_arrows() {
        for arrow in ["->>", "-->>", "-)", "--)", "<<->>", "<<-->>"] {
            let source = format!("Alice {arrow} Bob: hello\n");
            let detected = super::detect_type_with_confidence(&source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::Sequence,
                "{arrow} was not recognised as a sequence arrow"
            );
        }
    }

    /// CONTROL: a flowchart cross edge must NOT be read as a sequence diagram.
    ///
    /// `--x` is both a sequence dotted-cross and a flowchart cross edge, and `-x` is a substring of
    /// it, so both are deliberately excluded. The second case is the one that matters: a headerless
    /// flowchart carrying a `fill:#f00` supplies the colon that guards the `->` rule, so a
    /// colon-guarded `-x` would still have misfired.
    #[test]
    fn flowchart_cross_edges_are_not_read_as_sequence() {
        for source in ["A --x B\n", "A --x B\n  style A fill:#f00\n"] {
            let detected = super::detect_type_with_confidence(source);
            assert_ne!(
                detected.diagram_type,
                fm_core::DiagramType::Sequence,
                "{source:?} was misread as a sequence diagram"
            );
        }
    }

    /// The flowchart heuristic must recognise dotted, circle and cross edges.
    ///
    /// Asserted on `method`, not on `diagram_type`: this arm is last, so BOTH the heuristic and the
    /// Strategy 5 fallback yield `Flowchart` and a type assertion here would pass without the fix.
    /// The observable difference is that strict mode refuses `Fallback`.
    #[test]
    fn the_flowchart_heuristic_covers_dotted_circle_and_cross_edges() {
        for edge in ["-.->", "<-.->", "-.-", "--o", "--x", "-->", "<-->", "---", "==>", "<==>"] {
            let source = format!("A {edge} B\n");
            let detected = super::detect_type_with_confidence(&source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::Flowchart,
                "{edge} was not read as a flowchart edge"
            );
            assert_eq!(
                detected.method,
                super::DetectionMethod::ContentHeuristic,
                "{edge} reached Flowchart only through the fallback, which strict mode refuses"
            );
        }
    }

    /// CONTROL: the two-character forms must NOT shadow the unsupported-type diagnostic.
    ///
    /// `--`, `==`, `-.` and `..` are real flow operators that this heuristic deliberately declines,
    /// because they occur in prose. A `radar` document carrying an ellipsis must still report that
    /// radar is unimplemented rather than claim a flowchart was detected.
    #[test]
    fn prose_punctuation_does_not_shadow_the_unsupported_type_warning() {
        let detected = super::detect_type_with_confidence("radar\n  title Scores...\n");

        assert_eq!(detected.method, super::DetectionMethod::Fallback);
        assert!(
            detected.warnings.iter().any(|w| w.contains("does not implement")),
            "the unimplemented-type message was replaced: {:?}",
            detected.warnings
        );
    }

    /// A headerless sequence message is still detected through the plain arrow.
    #[test]
    fn a_plain_arrow_message_is_still_a_sequence_diagram() {
        for source in ["alice->bob: hello\n", "alice -> bob : hello\n", "a-->b: hi\n"] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::Sequence,
                "{source:?} stopped being detected as a sequence diagram"
            );
        }
    }

    /// A LABELLED state transition is a state diagram, not a sequence diagram.
    ///
    /// `[*] --> Idle: boot` is an arrow followed by a colon on one line, which is exactly the shape
    /// the sequence arm matches — and the sequence arm ran first, so every headerless state diagram
    /// with labelled transitions was reported as a sequence. No tightening of the sequence rule can
    /// fix this, because the state line genuinely IS arrow-then-colon. Only precedence decides it.
    #[test]
    fn a_labelled_state_transition_is_not_a_sequence_diagram() {
        for source in [
            "[*] --> Idle: boot\n  Idle --> Busy: work\n",
            "[*] --> Idle\n",
            "Idle --> [*]\n",
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::State,
                "{source:?} was not read as a state diagram"
            );
        }
    }

    /// The state marker is recognised WITHOUT surrounding spaces.
    ///
    /// The checks were the literals `"[*] -->"` and `"--> [*]"`; mermaid accepts `[*]-->Idle`, which
    /// matched neither and fell through to the flowchart fallback — the same
    /// literal-instead-of-shape drift as the operator tables.
    #[test]
    fn the_state_marker_does_not_require_spaces() {
        for source in ["[*]-->Idle\n", "Idle-->[*]\n", "[*]\t--> Idle\n"] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::State,
                "{source:?} was not read as a state diagram"
            );
        }
    }

    /// CONTROL: a bare `[*]` with no arrow is NOT a state diagram.
    ///
    /// `A[*]` is a flowchart node whose label is `*`. Treating the marker as decisive wherever it
    /// appears would reclassify that, which is why the scan looks for an adjacent arrow rather than
    /// for the marker alone.
    #[test]
    fn a_bracketed_star_label_alone_is_not_a_state_diagram() {
        let detected = super::detect_type_with_confidence("a[*]\n  b[Plain]\n");

        assert_ne!(
            detected.diagram_type,
            fm_core::DiagramType::State,
            "a node labelled `*` was misread as a state marker"
        );
    }

    /// CONTROL: a headerless flowchart with a colon anywhere is NOT a sequence diagram.
    ///
    /// This is the ordering defect the message-separator rule exists for. The sequence arm runs
    /// before the flowchart arm and used to fire on "some arrow" AND "some colon" as two independent
    /// facts about the whole document, so one `style` line reclassified the diagram. Widening the
    /// flowchart table could not have fixed it -- control never reached that arm.
    #[test]
    fn a_flowchart_with_a_style_colon_is_not_a_sequence_diagram() {
        for source in [
            "a --> b\n  style a fill:#f00\n",
            "a[start] --> b[step: two]\n",
            "a[a->b] --> c\n  style a fill:#f00\n",
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert_eq!(
                detected.diagram_type,
                fm_core::DiagramType::Flowchart,
                "{source:?} was misread as a sequence diagram"
            );
        }
    }

    /// The unimplemented-type message must name a spelling the INCUMBENT ACCEPTS.
    ///
    /// The point of this message is to tell an author their syntax is fine and the renderer is
    /// behind. Naming a spelling mermaid rejects inverts that: it teaches a syntax error while
    /// sounding authoritative. Probed against the pinned 11.15.0 bundle -- `radar`, `venn`,
    /// `wardley` and `treeView` are rejected bare AND with content, and only their `-beta` forms
    /// parse, while `treemap`, `info`, `eventmodeling` and `ishikawa` are accepted bare.
    #[test]
    fn the_unimplemented_type_message_names_a_spelling_mermaid_accepts() {
        for (source, expected) in [
            ("radar-beta\n  Item\n", "radar-beta"),
            ("radar\n  Item\n", "radar-beta"),
            ("venn-beta\n  Item\n", "venn-beta"),
            ("wardley-beta\n  Item\n", "wardley-beta"),
            ("treeView-beta\n  Item\n", "treeView-beta"),
            ("treemap\n  Item\n", "treemap"),
            ("ishikawa\n  Item\n", "ishikawa"),
        ] {
            let detected = super::detect_type_with_confidence(source);
            assert!(
                detected.warnings.iter().any(|w| w.contains(expected)),
                "{source:?} should name {expected:?}; warnings: {:?}",
                detected.warnings
            );
        }
    }

    /// CONTROL: the bare spelling is NOT offered back for a `-beta`-only type.
    ///
    /// Without this, returning the author's own input would satisfy the test above for every case
    /// and the correction would be untested. `radar` typed bare must still be answered with
    /// `radar-beta` -- which the case above asserts -- and must never be echoed as `'radar'`.
    #[test]
    fn a_beta_only_type_is_never_named_by_its_bare_spelling() {
        let detected = super::detect_type_with_confidence("radar\n  Item\n");
        assert!(
            !detected.warnings.iter().any(|w| w.contains("'radar'")),
            "the bare spelling was offered back to the author: {:?}",
            detected.warnings
        );
    }
}
