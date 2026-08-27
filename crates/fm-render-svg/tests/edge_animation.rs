//! mermaid 11's per-edge `edgeId@{ animate: … }` opt-in (bd-euyt4).
//!
//! THE GAP: the syntax parsed cleanly — `A e1@--> B` plus `e1@{ animate: true }` gave
//! `edge_count:1 node_count:2 warning_count:0`, no phantom node, no diagnostic — and then nothing
//! happened. The parser's own comment said why: *"we cannot attach the metadata to its edge without
//! an edge-id concept"*. That concept had since arrived (`edge_index_by_id`, built for
//! `class edgeId alert`); the metadata path had simply never been reconnected to it. So an author
//! who wrote valid mermaid 11 got a silently ordinary edge and no warning pointing at why.
//!
//! REFERENCE BEHAVIOUR, measured against the pinned mermaid 11.15.0 bundle driven over CDP into
//! Chromium 151 (`scratchpad/edge_probe.mjs`, `edge_css_probe.mjs`) — eight cases, every one of
//! which is pinned below:
//!
//! | source                                    | upstream edge class        |
//! |-------------------------------------------|----------------------------|
//! | `A e1@--> B`                              | (none)                     |
//! | + `e1@{ animate: true }`                  | `edge-animation-fast`      |
//! | + `e1@{ animation: fast }`                | `edge-animation-fast`      |
//! | + `e1@{ animation: slow }`                | `edge-animation-slow`      |
//! | + `e1@{ animate: false }`                 | (none)                     |
//! | `e1@{ … }` written BEFORE its edge        | (none)                     |
//! | `zz@{ … }` naming no edge                 | (none)                     |
//! | two edges declaring `e1`                  | first edge only            |
//!
//! and the CSS backing those names, read off the same bundle:
//!
//! ```text
//! .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;
//!                      animation:dash 20s linear infinite;stroke-linecap:round;}
//! .edge-animation-slow{ …same, 50s… }        @keyframes dash{to{stroke-dashoffset:0;}}
//! ```
//!
//! Computed style on the animated path confirmed it end to end: `stroke-dasharray: 9px, 5px`,
//! `animation-name: dash`, `duration: 20s`/`50s`, `iteration-count: infinite`, `linear`. An
//! un-animated edge read `0px` / `none` / `0s` / `1` / `ease`.
//!
//! THE NEGATIVE CASE this bead turns on is the one a silent fallback passes: an edge that did NOT
//! opt in must render DIFFERENTLY from one that did. Four of the eight rows above are
//! must-not-animate rows, and each is a distinct way to get the answer wrong — accept the metadata
//! regardless of order, resolve an unknown id to the nearest edge, apply an id to every edge
//! claiming it, or treat `animate: false` as presence-means-yes.

use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_config};

fn render_with(source: &str, a11y: &A11yConfig) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            a11y: a11y.clone(),
            ..SvgRenderConfig::default()
        },
    )
}

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// Every `class="fm-edge …"` value in document order — the edges as actually drawn.
///
/// ⚠️ THE OFFSET IS THE WHOLE HELPER. The first version advanced by 7 instead of 6 and so cut the
/// leading `f` off every result, turning each class into `m-edge …`. Every assertion then failed
/// against a value that differed from the real one only in its first byte — a shape that reads like
/// a renderer bug rather than a harness bug. `the_class_reader_reads_the_whole_class` pins it.
fn edge_classes(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    // The trailing SPACE excludes the enclosing `<g class="fm-edge">`, whose class is
    // exactly `fm-edge` with nothing after it. What is wanted is the drawn `<path>`, which
    // always carries a style class after the base one.
    while let Some(at) = rest.find("class=\"fm-edge ") {
        // `class="` is six bytes before the opening quote, so `tail` starts AT the quote.
        let tail = &rest[at + 6..];
        let end = tail[1..].find('"').expect("unterminated class attribute") + 1;
        out.push(tail[1..end].to_string());
        rest = &tail[end..];
    }
    out
}

/// The class reader returns whole class values, and reads the drawn path, not its wrapper.
#[test]
fn the_class_reader_reads_the_whole_class_of_the_drawn_path() {
    assert_eq!(
        edge_classes(
            "<g class=\"fm-edge\" id=\"fm-edge-0\">\
             <path class=\"fm-edge fm-edge-solid\" d=\"M0 0\"/></g>\
             <path class=\"fm-edge x\"/>"
        ),
        vec!["fm-edge fm-edge-solid", "fm-edge x"]
    );
}

const PLAIN: &str = "flowchart LR\n  A e1@--> B\n";
const FAST: &str = "flowchart LR\n  A e1@--> B\n  e1@{ animate: true }\n";
const SLOW: &str = "flowchart LR\n  A e1@--> B\n  e1@{ animation: slow }\n";

#[test]
fn animate_true_and_animation_fast_both_mark_the_edge_fast() {
    for source in [
        FAST,
        "flowchart LR\n  A e1@--> B\n  e1@{ animation: fast }\n",
    ] {
        assert_eq!(
            edge_classes(&render(source)),
            vec!["fm-edge fm-edge-solid fm-edge-animation-fast"],
            "not marked fast: {source}"
        );
    }
}

#[test]
fn animation_slow_marks_the_edge_slow() {
    assert_eq!(
        edge_classes(&render(SLOW)),
        vec!["fm-edge fm-edge-solid fm-edge-animation-slow"]
    );
}

/// THE NEGATIVE CASE: an edge that did not opt in must not be drawn as one that did.
///
/// Stated as an inequality between two renders rather than as "the class is absent", because the
/// absent-class form is satisfied by an implementation that never emits the class at all — which is
/// precisely the pre-existing behaviour this bead exists to change. Comparing the two renders means
/// the assertion can only pass if the opt-in actually did something.
#[test]
fn an_edge_that_did_not_opt_in_differs_from_one_that_did() {
    let plain = edge_classes(&render(PLAIN));
    assert_eq!(plain, vec!["fm-edge fm-edge-solid"], "plain edge is marked");
    assert_ne!(
        plain,
        edge_classes(&render(FAST)),
        "an un-animated edge renders identically to an animated one"
    );
    assert_ne!(
        edge_classes(&render(FAST)),
        edge_classes(&render(SLOW)),
        "the two speeds render identically"
    );
}

/// MEASURED upstream: metadata written before its edge animates nothing.
///
/// Not an arbitrary rule to reproduce — it is what an id-lookup application means. Upstream applies
/// the statement when it reads it, and at that point no edge has claimed `e1`. An implementation
/// that buffered the metadata and applied it at the end would be "more helpful" and would diverge.
#[test]
fn metadata_written_before_its_edge_animates_nothing() {
    assert_eq!(
        edge_classes(&render(
            "flowchart LR\n  e1@{ animate: true }\n  A e1@--> B\n"
        )),
        vec!["fm-edge fm-edge-solid"]
    );
}

/// MEASURED upstream: an id naming no edge is a silent no-op — no animation, no error, no node.
#[test]
fn an_id_naming_no_edge_animates_nothing_and_declares_nothing() {
    let result = fm_parser::parse("flowchart LR\n  A --> B\n  zz@{ animate: true }\n");
    assert_eq!(
        result.ir.nodes.len(),
        2,
        "the metadata statement declared a phantom node: {:?}",
        result.ir.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    assert_eq!(
        edge_classes(&fm_render_svg::render_svg(&result.ir)),
        vec!["fm-edge fm-edge-solid"]
    );
}

/// MEASURED upstream: when two edges declare the same id, only the FIRST is animated.
///
/// The discriminating half is the SECOND element — an implementation that animates every edge
/// carrying the id passes an assertion written only about the first.
#[test]
fn only_the_first_edge_claiming_an_id_is_animated() {
    assert_eq!(
        edge_classes(&render(
            "flowchart LR\n  A e1@--> B\n  B e1@--> C\n  e1@{ animate: true }\n"
        )),
        vec![
            "fm-edge fm-edge-solid fm-edge-animation-fast",
            "fm-edge fm-edge-solid",
        ]
    );
}

/// `animate: false` is a spelled-out opt-OUT, so presence of the key cannot mean yes — and it is
/// not a typo either, so it must not warn.
#[test]
fn animate_false_is_an_opt_out_and_not_a_mistake() {
    let result = fm_parser::parse("flowchart LR\n  A e1@--> B\n  e1@{ animate: false }\n");
    assert_eq!(
        edge_classes(&fm_render_svg::render_svg(&result.ir)),
        vec!["fm-edge fm-edge-solid"]
    );
    assert!(
        result.warnings.is_empty(),
        "a spelled-out opt-out warned: {:?}",
        result.warnings
    );
}

/// A speed upstream would interpolate into a class it does not style renders as a plain edge here
/// too — and says why.
///
/// Upstream measurably emits `edge-animation-turbo` for `animation: turbo`, a class no rule in its
/// stylesheet matches, so it draws an ordinary edge. We draw the same ordinary edge. The warning is
/// the only difference, and it is additive: it tells an author whose syntax was accepted why the
/// diagram did not move, which upstream leaves them to work out from an inert class name.
#[test]
fn an_unstyleable_speed_draws_a_plain_edge_and_says_so() {
    let result = fm_parser::parse("flowchart LR\n  A e1@--> B\n  e1@{ animation: turbo }\n");
    assert_eq!(
        edge_classes(&fm_render_svg::render_svg(&result.ir)),
        vec!["fm-edge fm-edge-solid"],
        "an unstyleable speed animated the edge anyway"
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("turbo")),
        "no warning named the rejected speed: {:?}",
        result.warnings
    );
    assert_eq!(result.ir.nodes.len(), 2, "a rejected speed declared a node");
}

/// The opt-in survives EVERY renderer configuration, not just the default one.
///
/// This is the test that would have caught a half-wired change. `render_edge` and
/// `render_edge_body_into` between them hold four streaming fast paths, each writing a fixed
/// two-class fragment, and each is selected by a different a11y shape. A gate added to one of them
/// leaves the others emitting a two-class edge — the animation silently dropped for exactly the
/// configurations that test did not use. Since a fast fragment is byte-identical to the slow path
/// by construction, "which path ran" is not observable from the output; "did the class survive" is,
/// and that is the property that actually matters.
#[test]
fn the_opt_in_survives_every_a11y_configuration() {
    // ⚠️ `A11yConfig::default()` IS NOT THE RENDERER'S DEFAULT. It derives to all-false, i.e. a
    // duplicate of `none()`; `SvgRenderConfig::default()` sets `A11yConfig::full()`. Listing the
    // derived default here instead of `full()` cost this test a whole configuration — and with it
    // the labeled fast path in `render_edge`, which needs all three flags on.
    let configs = [
        ("full (the renderer's default)", A11yConfig::full()),
        ("minimal", A11yConfig::minimal()),
        ("none", A11yConfig::none()),
        (
            "text-alternatives without keyboard nav",
            A11yConfig {
                aria_labels: true,
                text_alternatives: true,
                keyboard_nav: false,
                accessibility_css: false,
            },
        ),
    ];
    // Both an unlabeled and a labeled edge: the labeled fast paths are separate code from the
    // unlabeled ones and are gated separately.
    let labeled = "flowchart LR\n  A e1@-->|go| B\n  e1@{ animate: true }\n";
    for (name, a11y) in configs {
        for (shape, source) in [("unlabeled", FAST), ("labeled", labeled)] {
            let classes = edge_classes(&render_with(source, &a11y));
            assert!(
                classes.iter().any(|c| c.contains("fm-edge-animation-fast")),
                "{shape} edge lost its animation under a11y {name}: {classes:?}"
            );
            assert!(
                edge_classes(&render_with(PLAIN, &a11y))
                    .iter()
                    .all(|c| !c.contains("fm-edge-animation")),
                "an un-animated edge gained an animation under a11y {name}"
            );
        }
    }
}

/// The two speeds differ ONLY in duration, and by upstream's two numbers.
///
/// Pinned because "fast" and "slow" are the kind of pair that invites a second gratuitous
/// difference — a different dash pattern, a different easing — which would look fine and match
/// nothing.
#[test]
fn the_two_speeds_differ_only_in_duration() {
    let svg = render(FAST);
    assert!(
        svg.contains("animation: fm-edge-dash-march 20s linear infinite"),
        "fast is not upstream's 20s linear march"
    );
    assert!(
        render(SLOW).contains("animation: fm-edge-dash-march 50s linear infinite"),
        "slow is not upstream's 50s linear march"
    );
    // The shared declarations, upstream's values: 9-5 dashes marched over a 900px offset.
    for shared in [
        "stroke-dasharray: 9 5 !important",
        "stroke-dashoffset: 900",
        "@keyframes fm-edge-dash-march",
    ] {
        assert!(svg.contains(shared), "missing shared declaration: {shared}");
    }
}

/// The march rules ship only when an edge opted in.
///
/// `edgeId@{ animate: … }` is an explicit opt-in almost no diagram writes, so shipping its rules
/// unconditionally would put ~470 B of dead CSS into every SVG this renderer emits. The rationale
/// for those rules is deliberately NOT in the stylesheet for the same reason: as a CSS comment it
/// measured 1235 B, and the minifier keeps comments.
#[test]
fn the_march_rules_ship_only_when_an_edge_opted_in() {
    let plain = render(PLAIN);
    assert!(
        !plain.contains("fm-edge-animation") && !plain.contains("fm-edge-dash-march"),
        "a diagram with no animated edge shipped the march rules"
    );
    assert!(
        !plain.contains("MEASURED") && !plain.contains("upstream"),
        "explanatory prose is shipping inside the rendered SVG"
    );
    assert!(
        render(FAST).contains("fm-edge-dash-march"),
        "an animated edge shipped no march rules, so nothing moves"
    );
    assert!(
        render(FAST).len() > plain.len(),
        "the animated render is not larger, so the rules were stripped from it too"
    );
}

/// A node statement carrying `animate:` is still a node.
///
/// `A@{ shape: circle, animate: true }` addresses two namespaces at once. The id belongs to a node,
/// so no edge can be registered under it and the animation could only ever miss — but the node must
/// still be declared with its shape, which is the part a naive "animation key wins" would drop.
#[test]
fn a_node_statement_carrying_an_animate_key_is_still_a_node() {
    let result =
        fm_parser::parse("flowchart LR\n  A@{ shape: circle, animate: true }\n  A --> B\n");
    assert_eq!(result.ir.nodes.len(), 2);
    assert_eq!(result.ir.nodes[0].shape, fm_core::NodeShape::Circle);
    assert_eq!(
        edge_classes(&fm_render_svg::render_svg(&result.ir)),
        vec!["fm-edge fm-edge-solid"],
        "a node id resolved to an edge"
    );
}
