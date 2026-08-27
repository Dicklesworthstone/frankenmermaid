//! `journey` and `timeline`: the interaction directives the reference REFUSES (bd-umqc6).
//!
//! bd-rnc6l stopped seven families drawing `click`/`link`/`callback`/`cssClass` as content and left
//! these two alone, because mermaid draws some of those lines here too and silencing them would
//! have traded agreement for divergence. This finishes the per-cell decision that bead deferred.
//!
//! ⚠️ THE AGREEMENT IS REAL, AND PROVING THAT NEEDED A SHARPER INSTRUMENT THAN THE ONE THAT FOUND
//! IT. The earlier probe asked `drawn.includes("click A")` of both engines and called a double yes
//! agreement — but two different manglings of one input both contain that substring. Printing the
//! EXACT drawn text settled it:
//!
//! ```text
//!   journey + click A "https://example.com"
//!     reference  ["Me","click A \"https","M","Wake","D"]   (each text doubled by its shadow node)
//!     ours       ["D","Me","M","click A \"https","Wake"]
//! ```
//!
//! Both engines take the line as a TASK and caption it `click A "https`, truncated at the URL's own
//! colon, because a journey line is `text: score: actors`. Same for `link`. mermaid's timeline draws
//! `cssClass "A" mine` as a PERIOD for the mirror-image reason — no colon at all.
//!
//! ⚠️ SO THE TWO FAMILIES DISAGREE WITH EACH OTHER, AND THAT IS THE REFERENCE'S DOING. journey keeps
//! `click`/`link` and guards `callback`/`cssClass`; timeline does the opposite. There is no rule
//! behind it to implement — only two grammars' accidents — so each family carries a MEASURED list.
//! Sniffing the line for a colon to derive the split would be reproducing the accident itself.
//!
//! ⚠️ ONE CELL IS DELIBERATELY LEFT DRAWING, AND IT IS THE PRICE OF THE AGREEMENT. journey must keep
//! `click` to match the reference on `click A "url"`, so `click A callback "tip"` — which mermaid
//! refuses — is still taken as a task. Narrowing that would mean matching on the line's shape rather
//! than its keyword. It is asserted below rather than left unstated, so the residue is visible.

/// Drawn text only: `<text>` content with `<title>`/`<desc>` removed first.
fn drawn_text(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut body = svg;
    for (open, close) in [("<title>", "</title>"), ("<desc>", "</desc>")] {
        while let Some(start) = body.find(open) {
            let Some(end) = body[start..].find(close) else {
                break;
            };
            body.replace_range(start..start + end + close.len(), "");
        }
    }
    let mut out = Vec::new();
    let mut rest = body.as_str();
    while let Some(at) = rest.find("<text") {
        rest = &rest[at..];
        let Some(gt) = rest.find('>') else { break };
        rest = &rest[gt + 1..];
        let Some(end) = rest.find("</text>") else {
            break;
        };
        let mut text = String::new();
        let mut in_tag = false;
        for ch in rest[..end].chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        out.push(text.trim().to_string());
        rest = &rest[end + 7..];
    }
    out.retain(|t| !t.is_empty());
    out
}

fn draws(source: &str, needle: &str) -> bool {
    drawn_text(source).iter().any(|t| t.contains(needle))
}

const JOURNEY_HEAD: &str = "journey\n  title D\n";
const JOURNEY_BODY: &str = "  section M\n    Wake: 3: Me\n";
const TIMELINE_HEAD: &str = "timeline\n  title T\n";
const TIMELINE_BODY: &str = "  2024 : x\n";

/// The six cells this closes: the reference refuses the line, and we were drawing it.
fn guarded_cells() -> Vec<(&'static str, String, &'static str)> {
    vec![
        (
            "journey callback",
            format!("{JOURNEY_HEAD}  callback A myFunc \"tip\"\n{JOURNEY_BODY}"),
            "callback A",
        ),
        (
            "journey cssClass",
            format!("{JOURNEY_HEAD}  cssClass \"A\" mine\n{JOURNEY_BODY}"),
            "cssClass",
        ),
        (
            "timeline click",
            format!("{TIMELINE_HEAD}  click A \"https://example.com\"\n{TIMELINE_BODY}"),
            "click A",
        ),
        (
            "timeline click callback",
            format!("{TIMELINE_HEAD}  click A callback \"tip\"\n{TIMELINE_BODY}"),
            "click A callback",
        ),
        (
            "timeline link",
            format!("{TIMELINE_HEAD}  link A \"https://example.com\"\n{TIMELINE_BODY}"),
            "link A",
        ),
        (
            "timeline callback",
            format!("{TIMELINE_HEAD}  callback A myFunc \"tip\"\n{TIMELINE_BODY}"),
            "callback A",
        ),
    ]
}

/// ⚠️ THE NEGATIVE CASE: the refused directive is not drawn, and the diagram still is.
///
/// Both halves. "Nothing drawn" satisfies the first assertion on its own and would mean the guard
/// ate the diagram — which in these two families is a real risk, because their rule is that almost
/// every line IS content.
#[test]
fn the_refused_directives_are_no_longer_drawn() {
    let mut checked = 0;
    for (name, source, needle) in guarded_cells() {
        let texts = drawn_text(&source);
        assert!(
            !texts.iter().any(|t| t.contains(needle)),
            "{name} still draws the directive: {texts:?}"
        );
        assert!(
            !texts.is_empty(),
            "{name} drew nothing at all, so the guard swallowed the diagram"
        );
        checked += 1;
    }
    assert_eq!(checked, 6, "the cell table lost an entry");
}

/// The diagram's own content survives, item for item.
#[test]
fn the_diagram_is_unchanged_apart_from_the_directive() {
    let journey_base = drawn_text(&format!("{JOURNEY_HEAD}{JOURNEY_BODY}"));
    let timeline_base = drawn_text(&format!("{TIMELINE_HEAD}{TIMELINE_BODY}"));
    for (name, source, _) in guarded_cells() {
        let baseline = if name.starts_with("journey") {
            &journey_base
        } else {
            &timeline_base
        };
        let with_directive = drawn_text(&source);
        assert_eq!(
            &with_directive, baseline,
            "{name}: the guarded line changed what the diagram draws"
        );
    }
}

/// ⚠️ THE THREE AGREEMENTS STILL AGREE, AND ON THE EXACT CAPTION.
///
/// `draws(.., "click A")` would pass on any mangling that happens to contain those characters —
/// which is precisely the check that could not tell agreement from coincidence in the first place.
/// The captions asserted here are the strings mermaid itself produced, read out of a Chromium 151
/// render of the pinned 11.15.0 bundle.
#[test]
fn the_cells_the_reference_draws_are_still_drawn_with_the_same_caption() {
    for (name, source, caption) in [
        (
            "journey click",
            format!("{JOURNEY_HEAD}  click A \"https://example.com\"\n{JOURNEY_BODY}"),
            "click A \"https",
        ),
        (
            "journey link",
            format!("{JOURNEY_HEAD}  link A \"https://example.com\"\n{JOURNEY_BODY}"),
            "link A \"https",
        ),
        (
            "timeline cssClass",
            format!("{TIMELINE_HEAD}  cssClass \"A\" mine\n{TIMELINE_BODY}"),
            "cssClass \"A\" mine",
        ),
    ] {
        let texts = drawn_text(&source);
        assert!(
            texts.iter().any(|t| t == caption),
            "{name}: expected the reference's own caption {caption:?}, drew {texts:?}"
        );
    }
}

/// ⚠️ THE RESIDUE, STATED RATHER THAN HIDDEN.
///
/// journey keeps `click` so that `click A "url"` still matches the reference, and the keyword is the
/// whole mechanism — so `click A callback "tip"`, which mermaid refuses, is still taken as a task.
/// Narrowing it would mean matching on the line's SHAPE, which is the grammar accident this
/// deliberately does not chase.
///
/// Asserted, so that a future change which does fix it fails here and has to say so, rather than
/// silently absorbing a case nobody recorded.
#[test]
fn the_one_cell_left_drawing_is_the_one_named_in_the_notes() {
    let source = format!("{JOURNEY_HEAD}  click A callback \"tip\"\n{JOURNEY_BODY}");
    assert!(
        draws(&source, "click A callback"),
        "journey stopped drawing `click A callback \"tip\"`. That may be an improvement, but it is \
         a divergence from the reference in a cell this bead deliberately left alone — update the \
         notes in this file and in bd-umqc6 rather than deleting this test"
    );
}

/// ⚠️ A NAME THAT MERELY BEGINS WITH A KEYWORD IS STILL CONTENT.
///
/// Both families take almost any line as content, so an over-broad guard is more damaging here than
/// anywhere else: it silently deletes the author's data rather than a directive.
#[test]
fn a_name_that_only_starts_with_a_keyword_is_still_content() {
    for (name, source, needle) in [
        (
            "journey callbacks",
            format!("{JOURNEY_HEAD}  section M\n    callbacks: 3: Me\n"),
            "callbacks",
        ),
        (
            "timeline clickthrough",
            format!("{TIMELINE_HEAD}  2024 : clickthrough\n"),
            "clickthrough",
        ),
        (
            "timeline linkage",
            format!("{TIMELINE_HEAD}  2024 : linkage\n"),
            "linkage",
        ),
    ] {
        assert!(
            draws(&source, needle),
            "{name}: `{needle}` was swallowed, so the guard matches on prefix rather than keyword"
        );
    }
}

/// The guarded line adds no node, which drawn text alone cannot see.
#[test]
fn the_guarded_directive_adds_no_node() {
    let journey_base = fm_parser::parse(&format!("{JOURNEY_HEAD}{JOURNEY_BODY}"))
        .ir
        .nodes
        .len();
    let timeline_base = fm_parser::parse(&format!("{TIMELINE_HEAD}{TIMELINE_BODY}"))
        .ir
        .nodes
        .len();
    for (name, source, _) in guarded_cells() {
        let baseline = if name.starts_with("journey") {
            journey_base
        } else {
            timeline_base
        };
        let count = fm_parser::parse(&source).ir.nodes.len();
        assert_eq!(count, baseline, "{name}: the directive added a node");
    }
}
