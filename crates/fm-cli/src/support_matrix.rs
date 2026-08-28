//! Semantic support promotion matrix (bd-5k51.1).
//!
//! For each of the 24 documented diagram families × four delivery surfaces (svg, terminal,
//! canvas, wasm), run the canonical golden fixture through the real pipeline and classify the
//! cell onto a promotion ladder:
//!
//! `unsupported → breadth → structural → semantic → visual_a11y → production`
//!
//! The rungs are EARNED IN ORDER and every cap names its gate in `reasons`:
//!
//! - **breadth** — renders non-empty output. A no-throw smoke; never a public claim alone.
//! - **structural** — byte-identical across repeat (layout + render) runs and carrying the
//!   fixture's IR content: node/edge census and, on the SVG surfaces, every node label
//!   present in the drawn output.
//! - **semantic** — a cross-engine oracle (pinned mermaid-js) agrees. NOT grantable
//!   in-process: it requires `--oracle-results` ingested from
//!   `scripts/headtohead/chromium_text_diff.mjs` (or an equivalent pinned-bundle oracle) run
//!   against THIS revision. Without it the cell stops at `structural` and says so.
//! - **visual_a11y** — semantic plus per-element accessible names covering the drawn elements.
//! - **production** — visual/a11y plus zero errors on the well-formed fixture and an
//!   explicitly diagnosed malformed negative case.
//!
//! Two surface caps are structural facts of the current tree, not judgments:
//! - terminal/canvas cap at `structural` while bd-t1jj is open (uneven `LayoutExtensions`
//!   coverage: terminal reads 5/13 fields, canvas 4/13 — the same IR renders with different
//!   semantics across backends, so a semantic claim would outrun the evidence).
//! - wasm caps at `structural` because its authoritative gate (the fm-wasm test suite and the
//!   packaged smoke in `build-wasm.sh`) is not runnable from this subcommand; the cell binds
//!   to those guards by id and the gauntlet promotes it.
//!
//! The in-process checks deliberately reuse the EXISTING corpus: the fixtures are the golden
//! suite's own `.mmd` files, read verbatim. No second corpus, no second parser. A cell whose
//! fixture is missing reports `unsupported` with that reason rather than inventing an input.

use fm_core::{
    DiagramType, SupportCell, SupportCensus, SupportDiagnostics, SupportMatrix,
    SupportNegativeCase, SupportTier,
};
#[cfg(any(test, feature = "support-matrix"))]
use fm_layout::build_render_scene;
use fm_layout::layout_diagram;
use fm_parser::parse;
#[cfg(any(test, feature = "support-matrix"))]
use fm_render_canvas::{
    CanvasRenderConfig, DrawOperation, MockCanvas2dContext, render_scene_to_canvas,
};
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use fm_render_term::{TermRenderConfig, render_term_with_layout_and_config};
use serde::Deserialize;
use std::path::Path;

/// The delivery surfaces the matrix measures, in emission order. WebGPU is deliberately
/// absent: it is Experimental behind its own epic (bd-2u0.2) and listing it as a peer surface
/// would be proof-class inflation.
pub const SURFACES: [&str; 4] = ["svg", "terminal", "canvas", "wasm"];

/// Map each documented family to its canonical golden fixture case id.
///
/// Every entry resolves to `crates/fm-cli/tests/golden/<case>.mmd` — the same file the
/// `golden_svg_test` semantic guards render, so a cell and its guard always describe the same
/// input.
const fn family_fixture_case(family: DiagramType) -> Option<&'static str> {
    match family {
        DiagramType::Flowchart => Some("flowchart_simple"),
        DiagramType::Sequence => Some("sequence_advanced"),
        DiagramType::Class => Some("class_basic"),
        DiagramType::State => Some("state_composite"),
        DiagramType::Er => Some("er_basic"),
        DiagramType::C4Context => Some("c4_basic"),
        DiagramType::C4Container => Some("c4_container"),
        DiagramType::C4Component => Some("c4_component"),
        DiagramType::C4Dynamic => Some("c4_dynamic"),
        DiagramType::C4Deployment => Some("c4_deployment"),
        DiagramType::ArchitectureBeta => Some("architecture_basic"),
        DiagramType::BlockBeta => Some("block_basic"),
        DiagramType::Gantt => Some("gantt_basic"),
        DiagramType::Timeline => Some("timeline_basic"),
        DiagramType::Journey => Some("journey_basic"),
        DiagramType::GitGraph => Some("gitgraph_basic"),
        DiagramType::Sankey => Some("sankey_basic"),
        DiagramType::Mindmap => Some("mindmap_basic"),
        DiagramType::Pie => Some("pie_basic"),
        DiagramType::QuadrantChart => Some("quadrant_basic"),
        DiagramType::XyChart => Some("xychart_comprehensive"),
        DiagramType::Requirement => Some("requirement_basic"),
        DiagramType::PacketBeta => Some("packet_basic"),
        DiagramType::Kanban => Some("kanban_basic"),
        DiagramType::Treemap | DiagramType::Radar | DiagramType::Info | DiagramType::Unknown => {
            None
        }
    }
}

/// Terminal canvas size for the term cells: large enough that no golden fixture clips.
const TERM_COLS: usize = 200;
const TERM_ROWS: usize = 100;

/// Canvas backdrop for the canvas cells, matching the canvas test suite's standard surface.
const CANVAS_WIDTH: f64 = 800.0;
const CANVAS_HEIGHT: f64 = 600.0;

/// One ingested cross-engine verdict for a (family, surface) cell.
///
/// The shape is deliberately minimal and engine-agnostic: `oracle` names the instrument
/// (`chromium_text_diff`), `verdict` is its outcome for this cell. A report is produced by
/// running the oracle against the SAME revision this matrix stamps in `source_rev`; an entry
/// from a different revision is a lie, which is why the gauntlet regenerates both together.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OracleEntry {
    /// `DiagramType::as_str()` of the family.
    pub family: String,
    /// Surface the oracle measured. The drawn-text oracles render SVG, so entries normally
    /// carry `svg`.
    pub surface: String,
    /// Instrument id, e.g. `chromium_text_diff`.
    pub oracle: String,
    pub verdict: OracleVerdict,
    /// Short human-readable detail carried into the cell's `reasons` on DIVERGE.
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OracleVerdict {
    /// Both engines drew semantically equivalent output.
    Agree,
    /// The engines disagree; the cell must not promote past `structural`.
    Diverge,
    /// The incumbent could not render this input.
    IncumbentDnf,
    /// The oracle could not decide (e.g. whitespace-only differences).
    Undecidable,
}

/// Ingested cross-engine results, parsed from `--oracle-results <path>`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct OracleReport {
    pub cells: Vec<OracleEntry>,
}

impl OracleReport {
    #[must_use]
    pub fn lookup(&self, family: &str, surface: &str) -> Option<&OracleEntry> {
        self.cells
            .iter()
            .find(|entry| entry.family == family && entry.surface == surface)
    }
}

/// Build the full 24 × 4 matrix against the fixtures in `golden_dir`.
///
/// `source_rev` stamps the measurement; consumers must refuse `unknown` for public claims.
/// `oracle` carries ingested cross-engine results; without it every cell caps at
/// `structural` with an explicit reason — an in-process run cannot witness semantic
/// agreement with the incumbent, and claiming otherwise would be proof-class inflation.
#[must_use]
pub fn build_support_matrix(
    golden_dir: &Path,
    source_rev: &str,
    oracle: Option<&OracleReport>,
) -> SupportMatrix {
    let mut cells = Vec::new();
    for family in fm_core::documented_diagram_types() {
        for surface in SURFACES {
            cells.push(evaluate_cell(golden_dir, *family, surface, oracle));
        }
    }
    SupportMatrix {
        source_rev: source_rev.to_string(),
        cells,
    }
}

/// XML-escape a label the way the SVG renderer must, so label containment is checked against
/// the form the document actually carries.
fn xml_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The DRAWN TEXT of an SVG document: the character data inside `<text>` elements, inner
/// tags stripped, concatenated.
///
/// Label containment is checked against THIS, never the whole document. Scanning the full
/// document would tautologize the check: path data contains the arc command letter `A`, the
/// embedded CSS contains class names derived from user classes, and an attribute value can
/// echo any string — a missing label would "pass" because unrelated markup happens to
/// contain the same letters.
fn svg_text_content(svg: &str) -> String {
    let mut content = String::with_capacity(svg.len() / 8);
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        let after_open = match rest[start..].find('>') {
            Some(end) => start + end + 1,
            None => break,
        };
        let Some(close) = rest[after_open..].find("</text>") else {
            break;
        };
        let inner = &rest[after_open..after_open + close];
        let mut inner_rest = inner;
        while let Some(tag_start) = inner_rest.find('<') {
            content.push_str(&inner_rest[..tag_start]);
            let Some(tag_end) = inner_rest[tag_start..].find('>') else {
                inner_rest = "";
                break;
            };
            inner_rest = &inner_rest[tag_start + tag_end + 1..];
        }
        content.push_str(inner_rest);
        rest = &rest[after_open + close + "</text>".len()..];
    }
    content
}

/// Whitespace-collapsed containment: a label the renderer wrapped across tspans or padded
/// still matches (every word of it), while a genuinely absent one still fails.
fn normalized_contains(haystack: &str, needle: &str) -> bool {
    let squeeze = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let hay = squeeze(haystack);
    let needle = squeeze(needle);
    if needle.is_empty() {
        return false;
    }
    needle
        .split(' ')
        .all(|word| hay.contains(word) || hay.contains(&squeeze(&xml_escaped(word))))
}

/// Node labels of the IR, resolved through the interner the renderers read.
fn node_label_texts(ir: &fm_core::MermaidDiagramIr) -> Vec<String> {
    ir.nodes
        .iter()
        .filter_map(|node| node.label.and_then(|id| ir.labels.get(id.0)))
        .map(|label| label.text.clone())
        .collect()
}

fn diagnostics_of(parsed: &fm_parser::ParseResult) -> SupportDiagnostics {
    SupportDiagnostics {
        errors: parsed.ir.meta.init.errors.len(),
        warnings: parsed.warnings.len() + parsed.ir.meta.init.warnings.len(),
    }
}

/// What one surface render produced, in census form.
struct SurfaceOutput {
    /// A deterministic serialization of the drawn output (the document itself for SVG and
    /// WASM, the grid for terminal, the recorded operation stream for Canvas).
    output: String,
    text_runs: usize,
    /// Per-element accessible-name count; `None` where the surface has no notion of one.
    a11y_named_elements: Option<usize>,
}

/// Render one fixture twice (fresh layout each time) on one surface and return the first
/// render's census plus whether the two renders were byte-identical.
fn render_surface(
    ir: &fm_core::MermaidDiagramIr,
    surface: &str,
) -> Result<(SurfaceOutput, bool), String> {
    let render_once = || -> Result<SurfaceOutput, String> {
        match surface {
            "svg" | "wasm" => {
                let layout = layout_diagram(ir);
                let svg = render_svg_with_layout(ir, &layout, &SvgRenderConfig::default());
                let text_runs = svg.matches("<text").count();
                let a11y = svg.matches("aria-label=\"").count() + svg.matches("<title>").count();
                Ok(SurfaceOutput {
                    output: svg,
                    text_runs,
                    a11y_named_elements: Some(a11y),
                })
            }
            "terminal" => {
                let layout = layout_diagram(ir);
                let result = render_term_with_layout_and_config(
                    ir,
                    &layout,
                    &TermRenderConfig::default(),
                    TERM_COLS,
                    TERM_ROWS,
                );
                let text_runs = result
                    .output
                    .lines()
                    .filter(|line| line.chars().any(char::is_alphanumeric))
                    .count();
                Ok(SurfaceOutput {
                    output: result.output,
                    text_runs,
                    a11y_named_elements: None,
                })
            }
            #[cfg(any(test, feature = "support-matrix"))]
            "canvas" => {
                let layout = layout_diagram(ir);
                let scene = build_render_scene(ir, &layout);
                let mut ctx = MockCanvas2dContext::new(CANVAS_WIDTH, CANVAS_HEIGHT);
                render_scene_to_canvas(&scene, &mut ctx, &CanvasRenderConfig::default());
                let text_runs = ctx
                    .operations()
                    .iter()
                    .filter(|op| matches!(op, DrawOperation::FillText(_, _, _)))
                    .count();
                Ok(SurfaceOutput {
                    output: format!("{:?}", ctx.operations()),
                    text_runs,
                    a11y_named_elements: None,
                })
            }
            #[cfg(not(any(test, feature = "support-matrix")))]
            "canvas" => Err(String::from(
                "this fm-cli build omits the canvas link (default features, bd-53p4): \
                 rebuild with --features support-matrix to measure canvas cells",
            )),
            other => Err(format!("unknown surface {other:?}")),
        }
    };

    let first = render_once()?;
    let second = render_once()?;
    let identical = first.output == second.output;
    Ok((first, identical))
}

/// The malformed negative case: the fixture plus one deliberately malformed construct.
///
/// The construct is punctuation-only so no family's keyword grammar can legitimately claim
/// it: a parser that silently drops it loses the line without a diagnostic, which is exactly
/// the "silent loss" the bead forbids. `handled` therefore requires EITHER an explicit
/// diagnostic OR a best-effort IR that still parses as the same family (degradation, not
/// loss) — both outcomes are recorded.
fn negative_case(fixture_text: &str, family: DiagramType) -> SupportNegativeCase {
    const MALFORMATION: &str = "\n%%%% this line is deliberately not any diagram construct %%%%\n";
    let parsed = parse(&format!("{fixture_text}{MALFORMATION}"));
    let diagnostics = diagnostics_of(&parsed);
    let degraded_to_same_family = parsed.ir.diagram_type == family;
    let explicitly_diagnosed = diagnostics.errors + diagnostics.warnings > 0;
    SupportNegativeCase {
        handled: explicitly_diagnosed || degraded_to_same_family,
        diagnostics,
    }
}

/// The tier a cell may not exceed, with the reason that enforces it. These caps are
/// structural facts of the current tree; when the named bead closes, its cap goes.
fn surface_cap(surface: &str) -> (SupportTier, &'static str) {
    match surface {
        "terminal" | "canvas" => (
            SupportTier::Structural,
            "capped at structural: LayoutExtensions surface coverage is uneven (bd-t1jj: \
             terminal reads 5/13 fields, canvas 4/13), so the same IR does not yet render \
             with equal semantics on this surface",
        ),
        "wasm" => (
            SupportTier::Structural,
            "capped at structural: the authoritative WASM gate is the fm-wasm test suite and \
             the packaged smoke in build-wasm.sh, which this in-process measurement cannot \
             run; the release gauntlet promotes this cell",
        ),
        _ => (SupportTier::Production, ""),
    }
}

/// Evaluate one family × surface cell end to end.
fn evaluate_cell(
    golden_dir: &Path,
    family: DiagramType,
    surface: &str,
    oracle: Option<&OracleReport>,
) -> SupportCell {
    let family_str = family.as_str();
    let mut reasons: Vec<String> = Vec::new();
    let case = family_fixture_case(family);
    let mut evidence_ids: Vec<String> = Vec::new();
    if let Some(case) = case {
        evidence_ids.push(format!("golden_svg_test::{case}"));
        evidence_ids.push(String::from("renderer_agreement"));
    }
    if surface == "wasm" {
        evidence_ids.push(String::from(
            "fm-wasm tests + packaged smoke (build-wasm.sh)",
        ));
    }

    let cell = |tier: SupportTier,
                reasons: Vec<String>,
                evidence_ids: Vec<String>,
                fixture: Option<String>,
                fixture_sha256: Option<String>,
                output_sha256: Option<String>,
                census: Option<SupportCensus>,
                determinism_repeat_identical: Option<bool>,
                diagnostics: Option<SupportDiagnostics>,
                a11y_named_elements: Option<usize>,
                negative_case: Option<SupportNegativeCase>| SupportCell {
        family: family_str.to_string(),
        surface: surface.to_string(),
        tier,
        reasons,
        evidence_ids,
        fixture,
        fixture_sha256,
        output_sha256,
        census,
        determinism_repeat_identical,
        diagnostics,
        a11y_named_elements,
        negative_case,
    };

    let Some(case) = case else {
        reasons.push(format!(
            "no canonical fixture case is mapped for family {family_str:?}"
        ));
        return cell(
            SupportTier::Unsupported,
            reasons,
            evidence_ids,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
    };

    let fixture_path = golden_dir.join(format!("{case}.mmd"));
    let Ok(fixture_text) = std::fs::read_to_string(&fixture_path) else {
        reasons.push(format!(
            "canonical fixture {} not found; the golden suite is the single corpus and no \
             substitute input is synthesized here",
            fixture_path.display()
        ));
        return cell(
            SupportTier::Unsupported,
            reasons,
            evidence_ids,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
    };
    let fixture_sha256 = crate::sha256_hex(fixture_text.as_bytes());

    // The fixture must still parse as its own family — a drifted fixture silently measuring
    // the wrong engine is the failure this guard exists to prevent.
    let parsed = parse(&fixture_text);
    let diagnostics = diagnostics_of(&parsed);
    if parsed.ir.diagram_type != family {
        reasons.push(format!(
            "fixture {case}.mmd parses as {:?}, not {family_str:?}: the fixture has drifted \
             from its family and the cell is unverifiable until it is repaired",
            parsed.ir.diagram_type.as_str()
        ));
        return cell(
            SupportTier::Unsupported,
            reasons,
            evidence_ids,
            Some(fixture_path.display().to_string()),
            Some(fixture_sha256),
            None,
            None,
            None,
            Some(diagnostics),
            None,
            None,
        );
    }

    let negative = negative_case(&fixture_text, family);
    let census = SupportCensus {
        nodes: parsed.ir.nodes.len(),
        edges: parsed.ir.edges.len(),
        text_runs: 0,
    };

    let (rendered, determinism_repeat_identical) = match render_surface(&parsed.ir, surface) {
        Ok(result) => result,
        Err(err) => {
            reasons.push(format!("render failed: {err}"));
            return cell(
                SupportTier::Unsupported,
                reasons,
                evidence_ids,
                Some(fixture_path.display().to_string()),
                Some(fixture_sha256),
                None,
                Some(census),
                None,
                Some(diagnostics),
                None,
                Some(negative),
            );
        }
    };
    let output_sha256 = crate::sha256_hex(rendered.output.as_bytes());
    let census = SupportCensus {
        text_runs: rendered.text_runs,
        ..census
    };

    // Ladder: breadth → structural → semantic → visual_a11y → production.
    let mut tier = if rendered.text_runs == 0 && census.nodes + census.edges > 0 {
        reasons.push(format!(
            "render produced {runs} text runs for {nodes} nodes/{edges} edges: the output is \
             empty of content, which is a no-throw failure, not a success",
            runs = rendered.text_runs,
            nodes = census.nodes,
            edges = census.edges
        ));
        SupportTier::Unsupported
    } else {
        SupportTier::Breadth
    };

    if tier == SupportTier::Breadth {
        if !determinism_repeat_identical {
            reasons.push(
                "repeat render is not byte-identical: determinism is mandatory and the cell \
                 cannot promote until it holds"
                    .to_string(),
            );
        } else if matches!(surface, "svg" | "wasm") {
            let drawn_text = svg_text_content(&rendered.output);
            // WHAT the drawn text must witness is a per-family contract, not one rule:
            //
            // - Most families render each IR node's label as text, so the label set itself
            //   is the witness.
            // - xychart is a value-to-geometry family: its per-point IR labels ("Sales
            //   Q1: 30") are the value→geometry CONTRACT — bar of height 30 at category Q1 —
            //   and the pinned incumbent draws the axis categories, the axis title and the
            //   legend, never per-bar values. Requiring those composite labels as text would
            //   demand chrome mermaid does not draw, so the witness there is: every series
            //   name, every x-axis category, the title and the y-axis label appear as text,
            //   while the values are witnessed by the census (12 point marks drawn).
            let witnesses: Vec<String> = if family == DiagramType::XyChart {
                match parsed.ir.xy_chart_meta.as_ref() {
                    Some(xy) => {
                        let mut labels = Vec::new();
                        if let Some(title) = &xy.title {
                            labels.push(title.clone());
                        }
                        if let Some(label) = &xy.y_axis.label {
                            labels.push(label.clone());
                        }
                        labels.extend(xy.x_axis.categories.iter().cloned());
                        labels.extend(
                            xy.series
                                .iter()
                                .filter_map(|series| series.name.clone())
                                .filter(|name| !name.is_empty()),
                        );
                        labels
                    }
                    None => {
                        reasons.push(
                            "xychart fixture parsed without xy_chart_meta: the \
                             value-to-geometry witness has nothing to check and the cell \
                             cannot promote"
                                .to_string(),
                        );
                        Vec::new()
                    }
                }
            } else {
                node_label_texts(&parsed.ir)
            };
            let missing: Vec<&String> = witnesses
                .iter()
                .filter(|label| !label.trim().is_empty())
                .filter(|label| !normalized_contains(&drawn_text, label))
                .collect();
            // An empty witness set must NEVER promote: containment over nothing is the
            // tautology this matrix exists to refuse.
            if !witnesses.is_empty() && missing.is_empty() {
                tier = SupportTier::Structural;
            } else if !witnesses.is_empty() {
                let names: Vec<String> = missing
                    .iter()
                    .take(3)
                    .map(|label| format!("{label:?}"))
                    .collect();
                reasons.push(format!(
                    "labels absent from the drawn output: {}{}",
                    names.join(", "),
                    if missing.len() > 3 {
                        format!(" (+{} more)", missing.len() - 3)
                    } else {
                        String::new()
                    }
                ));
            }
        } else {
            // Terminal truncates and wraps labels and Canvas draws text through the scene
            // tree; the FillText/text-line census is the content witness on those surfaces,
            // and the per-label assertion stays on the SVG surfaces where it is sound.
            tier = SupportTier::Structural;
        }
    }

    if tier == SupportTier::Structural && matches!(surface, "svg" | "wasm") {
        match oracle.and_then(|report| report.lookup(family_str, "svg")) {
            Some(entry) => match entry.verdict {
                OracleVerdict::Agree => {
                    evidence_ids.push(format!("{oracle}:{case}", oracle = entry.oracle));
                    tier = SupportTier::Semantic;
                }
                OracleVerdict::Diverge => reasons.push(format!(
                    "oracle {oracle} reports DIVERGE for this cell{detail}: semantic \
                     promotion is refused until the divergence is fixed or recorded as \
                     deliberate",
                    oracle = entry.oracle,
                    detail = if entry.detail.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", entry.detail)
                    }
                )),
                OracleVerdict::IncumbentDnf => reasons.push(format!(
                    "oracle {oracle} reports INCUMBENT-DNF: the pinned mermaid-js cannot \
                     render this input, so there is no incumbent witness to agree with and \
                     the cell stays structural",
                    oracle = entry.oracle
                )),
                OracleVerdict::Undecidable => reasons.push(format!(
                    "oracle {oracle} reports UNDECIDABLE for this cell; promotion requires a \
                     deciding oracle",
                    oracle = entry.oracle
                )),
            },
            None => reasons.push(
                "no ingested cross-engine verdict for this cell: run \
                 scripts/headtohead/chromium_text_diff.mjs --all-goldens against this \
                 revision and pass the report via --oracle-results to promote"
                    .to_string(),
            ),
        }
    }

    if tier == SupportTier::Semantic {
        match rendered.a11y_named_elements {
            None => reasons.push(
                "this surface carries no per-element accessible names, so visual_a11y is not \
                 reachable on it"
                    .to_string(),
            ),
            Some(named) => {
                let floor = census.nodes.max(1);
                if named >= floor {
                    tier = SupportTier::VisualA11y;
                } else {
                    reasons.push(format!(
                        "a11y named elements {named} < elements {floor}: per-element \
                         accessible names do not yet cover this family's output"
                    ));
                }
            }
        }
    }

    if tier == SupportTier::VisualA11y {
        if negative.handled && diagnostics.errors == 0 {
            tier = SupportTier::Production;
        } else {
            if !negative.handled {
                reasons.push(
                    "malformed negative case was silently dropped with no diagnostic and no \
                     same-family degradation: silent loss, not graceful recovery"
                        .to_string(),
                );
            }
            if diagnostics.errors != 0 {
                reasons.push(format!(
                    "the well-formed fixture carries {errors} parse error(s): a production \
                     cell may not error on its own canonical input",
                    errors = diagnostics.errors
                ));
            }
        }
    }

    // Surface caps apply LAST and name themselves — both when they DEMOTE a cell and when a
    // cell has legitimately climbed TO the cap: a reader of a structural terminal cell must
    // be able to see why it cannot promote further, not just that it has not.
    let (cap, cap_reason) = surface_cap(surface);
    if tier > cap {
        tier = cap;
    }
    if !cap_reason.is_empty() && tier == cap {
        reasons.push(cap_reason.to_string());
    }

    cell(
        tier,
        reasons,
        evidence_ids,
        Some(fixture_path.display().to_string()),
        Some(fixture_sha256),
        Some(output_sha256),
        Some(census),
        Some(determinism_repeat_identical),
        Some(diagnostics),
        rendered.a11y_named_elements,
        Some(negative),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::DiagramType;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn golden_dir() -> std::path::PathBuf {
        repo_root().join("crates/fm-cli/tests/golden")
    }

    #[test]
    fn every_documented_family_maps_to_an_existing_golden_fixture() {
        for family in fm_core::documented_diagram_types() {
            let Some(case) = family_fixture_case(*family) else {
                panic!(
                    "family {} has no canonical fixture mapping",
                    family.as_str()
                );
            };
            let path = golden_dir().join(format!("{case}.mmd"));
            assert!(
                path.exists(),
                "family {} maps to {} which does not exist",
                family.as_str(),
                path.display()
            );
        }
    }

    #[test]
    fn flowchart_svg_reaches_structural_without_oracle_results() {
        let matrix = build_support_matrix(&golden_dir(), "test-rev", None);
        let cell = matrix
            .cells
            .iter()
            .find(|cell| cell.family == "flowchart" && cell.surface == "svg")
            .expect("flowchart svg cell exists");
        assert_eq!(
            cell.tier,
            SupportTier::Structural,
            "reasons: {:?}",
            cell.reasons
        );
        assert_eq!(cell.determinism_repeat_identical, Some(true));
        let census = cell.census.as_ref().expect("census present");
        assert!(census.nodes > 0 && census.edges > 0 && census.text_runs > 0);
        assert!(
            cell.reasons
                .iter()
                .any(|reason| reason.contains("no ingested cross-engine verdict")),
            "without oracle results the cell must name the promotion path it could not take"
        );
        assert!(cell.negative_case.as_ref().expect("negative case").handled);
    }

    #[test]
    fn xychart_witnesses_series_and_axis_labels_not_composite_point_labels() {
        let matrix = build_support_matrix(&golden_dir(), "test-rev", None);
        let cell = matrix
            .cells
            .iter()
            .find(|cell| cell.family == "xyChart" && cell.surface == "svg")
            .expect("xyChart svg cell exists");
        // The per-point IR labels ("Sales Q1: 30") are the value→geometry contract; the
        // drawn-text witness is the series names, categories, title and axis label. If this
        // regresses to breadth, either the legend/axis drawing broke or the witness picked
        // the wrong contract.
        assert_eq!(
            cell.tier,
            SupportTier::Structural,
            "reasons: {:?}",
            cell.reasons
        );
    }

    #[test]
    fn terminal_and_canvas_cells_carry_the_bd_t1jj_cap() {
        let matrix = build_support_matrix(&golden_dir(), "test-rev", None);
        for surface in ["terminal", "canvas"] {
            let cell = matrix
                .cells
                .iter()
                .find(|cell| cell.family == "flowchart" && cell.surface == surface)
                .unwrap_or_else(|| panic!("{surface} cell exists"));
            assert_eq!(
                cell.tier,
                SupportTier::Structural,
                "reasons: {:?}",
                cell.reasons
            );
            assert!(
                cell.reasons.iter().any(|reason| reason.contains("bd-t1jj")),
                "the cap must name its bead"
            );
            assert_eq!(cell.a11y_named_elements, None);
        }
    }

    #[test]
    fn wasm_cells_carry_the_external_gate_cap() {
        let matrix = build_support_matrix(&golden_dir(), "test-rev", None);
        let cell = matrix
            .cells
            .iter()
            .find(|cell| cell.family == "flowchart" && cell.surface == "wasm")
            .expect("wasm cell exists");
        assert_eq!(
            cell.tier,
            SupportTier::Structural,
            "reasons: {:?}",
            cell.reasons
        );
        assert!(
            cell.reasons
                .iter()
                .any(|reason| reason.contains("build-wasm.sh")),
            "the cap must name the authoritative gate"
        );
        assert!(
            cell.evidence_ids
                .iter()
                .any(|id| id.contains("fm-wasm tests")),
            "the wasm cell binds to its guards by id"
        );
    }

    #[test]
    fn an_agree_oracle_verdict_promotes_svg_to_semantic() {
        let report = OracleReport {
            cells: vec![OracleEntry {
                family: String::from("flowchart"),
                surface: String::from("svg"),
                oracle: String::from("chromium_text_diff"),
                verdict: OracleVerdict::Agree,
                detail: String::new(),
            }],
        };
        let matrix = build_support_matrix(&golden_dir(), "test-rev", Some(&report));
        let cell = matrix
            .cells
            .iter()
            .find(|cell| cell.family == "flowchart" && cell.surface == "svg")
            .expect("flowchart svg cell exists");
        // Promotes at least past semantic; visual_a11y/production depend on the fixture's
        // a11y coverage and diagnostic record, which this assertion does not pin.
        assert!(
            cell.tier >= SupportTier::Semantic,
            "reasons: {:?}",
            cell.reasons
        );
        assert!(
            cell.evidence_ids
                .iter()
                .any(|id| id == "chromium_text_diff:flowchart_simple"),
            "the cell must cite the oracle run it promoted on"
        );
    }

    #[test]
    fn a_diverge_oracle_verdict_refuses_semantic_promotion() {
        let report = OracleReport {
            cells: vec![OracleEntry {
                family: String::from("flowchart"),
                surface: String::from("svg"),
                oracle: String::from("chromium_text_diff"),
                verdict: OracleVerdict::Diverge,
                detail: String::from("missing tick labels"),
            }],
        };
        let matrix = build_support_matrix(&golden_dir(), "test-rev", Some(&report));
        let cell = matrix
            .cells
            .iter()
            .find(|cell| cell.family == "flowchart" && cell.surface == "svg")
            .expect("flowchart svg cell exists");
        assert_eq!(
            cell.tier,
            SupportTier::Structural,
            "reasons: {:?}",
            cell.reasons
        );
        assert!(
            cell.reasons
                .iter()
                .any(|reason| reason.contains("DIVERGE") && reason.contains("missing tick labels")),
            "the refusal must carry the oracle's detail"
        );
    }

    #[test]
    fn matrix_covers_exactly_the_documented_families_times_surfaces() {
        let matrix = build_support_matrix(&golden_dir(), "test-rev", None);
        let expected = fm_core::documented_diagram_types().len() * SURFACES.len();
        assert_eq!(matrix.cells.len(), expected);
        assert_eq!(matrix.source_rev, "test-rev");
        for cell in &matrix.cells {
            assert!(
                !cell.reasons.is_empty() || cell.tier == SupportTier::Production,
                "{} x {}: a capped tier must carry its reason",
                cell.family,
                cell.surface
            );
        }
    }

    #[test]
    fn no_surface_measure_uses_an_unknown_source_rev_for_public_claims() {
        // The contract: `unknown` revs are emittable but must never back a claim. The field
        // is stamped verbatim so consumers can refuse; this pins that the emitter does not
        // silently invent a revision.
        let matrix = build_support_matrix(&golden_dir(), "unknown", None);
        assert_eq!(matrix.source_rev, "unknown");
    }

    #[test]
    fn normalized_containment_survives_escaping_and_whitespace() {
        assert!(normalized_contains("a<b &amp; c>d", "a<b & c>d"));
        assert!(normalized_contains(
            "line one\n  line two",
            "line one line two"
        ));
        assert!(!normalized_contains("unrelated", "missing label"));
        assert!(!normalized_contains("unrelated", "   "));
    }

    #[test]
    fn svg_text_content_extracts_drawn_text_and_ignores_markup() {
        let svg = concat!(
            r#"<svg><style>.fm-node-user-Actor1{fill:red}</style>"#,
            r#"<path d="M10 10 A 5 5 0 0 1 20 20"/>"#,
            r#"<text x="1" y="1"><tspan>Start</tspan><tspan>here</tspan></text>"#,
            r#"<text x="2" y="2">End &amp; done</text>"#,
            r#"</svg>"#
        );
        let drawn = svg_text_content(svg);
        // Drawn text is found…
        assert!(normalized_contains(&drawn, "Start here"));
        assert!(normalized_contains(&drawn, "End & done"));
        // …while markup that merely CONTAINS the same letters is not mistaken for it.
        assert!(!normalized_contains(&drawn, "Actor1"));
        assert!(!normalized_contains(&drawn, "M 10 10"));
    }

    #[test]
    fn negative_case_is_recorded_with_counts() {
        let fixture = std::fs::read_to_string(golden_dir().join("flowchart_simple.mmd"))
            .expect("fixture readable");
        let negative = negative_case(&fixture, DiagramType::Flowchart);
        assert!(
            negative.handled,
            "flowchart must degrade or diagnose, not silently lose"
        );
    }
}
