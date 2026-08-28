//! A classDiagram `namespace` draws a labelled cluster that actually contains its classes.
//!
//! THE DEFECT. `namespace Shapes { class Square }` produced NOTHING in the picture: no box, and the
//! name the author wrote never reached the SVG. The parser was not at fault — it created the
//! cluster and interned its title — but it registered the enclosed classes as SUBGRAPH members
//! only. `build_cluster_boxes` wraps `ir.clusters[i].members`, a different index it never wrote, so
//! every namespace owned zero members, no box was built for it, and the label went with the box.
//!
//! ```text
//!                                drawn clusters   namespace name in the SVG
//!   classDiagram namespace              0                  no
//!   CONTROL flowchart subgraph          1                  yes
//! ```
//!
//! The flowchart control is what made the diagnosis safe: it uses the same cluster machinery and
//! populates BOTH indices, so it drew its box correctly the whole time. Without it, "clusters do not
//! draw" would have looked like a renderer bug.
//!
//! MEASURED REFERENCE — pinned mermaid 11.15.0 rendered in Chromium 151, reading `g.cluster` and the
//! drawn text of each case:
//!
//! ```text
//!   namespace Shapes { class Square; class Circle }   1 cluster   "Shapes" "Square" "Circle"
//!   namespace A {One} + namespace B {Two}             2 clusters  "A" "One" "B" "Two"
//!   namespace N {Inside} + class Outside              1 cluster   "N" "Inside" "Outside"
//!   namespace N1 {A} + namespace N2 {B} + A --> B     2 clusters  "N1" "N2" "A" "B"
//! ```

use fm_core::MermaidDiagramIr;
use fm_layout::DiagramLayout;

const ARROW: &str = "-->";

fn parse(source: &str) -> MermaidDiagramIr {
    fm_parser::parse(source).ir
}

fn layout(ir: &MermaidDiagramIr) -> DiagramLayout {
    fm_layout::layout_diagram(ir)
}

/// The ids of the nodes a cluster box geometrically encloses, by intersecting the drawn rectangles.
///
/// Deliberately derived from GEOMETRY rather than from the membership list the fix writes: a test
/// that reads back the same list the code just wrote proves only that a field round-trips. What a
/// reader sees is which boxes sit inside which box.
fn nodes_inside(cluster: &fm_layout::LayoutClusterBox, layout: &DiagramLayout) -> Vec<String> {
    let bounds = cluster.bounds;
    let mut inside: Vec<String> = layout
        .nodes
        .iter()
        .filter(|node| {
            let center_x = node.bounds.x + node.bounds.width / 2.0;
            let center_y = node.bounds.y + node.bounds.height / 2.0;
            center_x >= bounds.x
                && center_x <= bounds.x + bounds.width
                && center_y >= bounds.y
                && center_y <= bounds.y + bounds.height
        })
        .map(|node| node.node_id.clone())
        .collect();
    inside.sort();
    inside
}

/// A namespace draws a cluster, and its name is in the picture.
#[test]
fn a_namespace_draws_a_labelled_cluster() {
    let source = "classDiagram\n  namespace Shapes {\n    class Square\n    class Circle\n  }\n";
    let ir = parse(source);
    let laid_out = layout(&ir);
    let svg = fm_render_svg::render_svg(&ir);

    assert_eq!(
        laid_out.clusters.len(),
        1,
        "the reference draws one cluster for one namespace; we drew {}",
        laid_out.clusters.len()
    );
    assert!(
        svg.contains(">Shapes<"),
        "the namespace name never reached the SVG, so the author's grouping is invisible"
    );
    // Both classes still draw — a grouping fix that loses its members is not a fix.
    assert!(svg.contains(">Square<") && svg.contains(">Circle<"));
}

/// ⚠️ THE PLANTED NEGATIVE: the cluster must ENCLOSE exactly the classes declared inside it.
///
/// The cheap repair for "the namespace does not draw" is to make a box appear — write the label, or
/// register every class as a member so the cluster is non-empty. Both produce a labelled box and
/// satisfy any assertion that the name is present. Neither is correct: a namespace that swallows a
/// class declared outside it, or that draws a box the classes do not sit in, is a wrong picture, and
/// the reference puts `Outside` outside.
///
/// Asserted by GEOMETRY — which node centres fall within the drawn cluster rectangle — so a
/// membership list that says the right thing while the box is drawn somewhere else still fails.
#[test]
fn the_cluster_encloses_its_own_classes_and_no_others() {
    let source = "classDiagram\n  namespace N {\n    class Inside\n  }\n  class Outside\n";
    let ir = parse(source);
    let laid_out = layout(&ir);

    assert_eq!(laid_out.clusters.len(), 1, "expected one namespace cluster");
    let cluster = &laid_out.clusters[0];
    assert_eq!(
        nodes_inside(cluster, &laid_out),
        vec!["Inside".to_string()],
        "the namespace box does not enclose exactly the class declared inside it — a class \
         declared OUTSIDE the braces was swallowed, or the member never landed in the box"
    );

    let svg = fm_render_svg::render_svg(&ir);
    assert!(
        svg.contains(">Outside<"),
        "the outside class stopped drawing"
    );
}

/// Two namespaces stay two, with disjoint contents.
///
/// A single shared "current namespace" — or a fix that registers members against the last namespace
/// seen — collapses these into one box or files both classes under `N2`. Each cluster is checked for
/// exactly its own class, so either mistake fails here rather than looking like success.
#[test]
fn two_namespaces_keep_their_own_classes() {
    let source = format!(
        "classDiagram\n  namespace N1 {{\n    class A\n  }}\n  namespace N2 {{\n    class B\n  }}\n  A {ARROW} B\n"
    );
    let ir = parse(&source);
    let laid_out = layout(&ir);

    assert_eq!(
        laid_out.clusters.len(),
        2,
        "the reference draws two clusters for two namespaces"
    );
    let mut contents: Vec<Vec<String>> = laid_out
        .clusters
        .iter()
        .map(|cluster| nodes_inside(cluster, &laid_out))
        .collect();
    contents.sort();
    assert_eq!(
        contents,
        vec![vec!["A".to_string()], vec!["B".to_string()]],
        "the two namespaces do not hold one class each — they were merged, or both classes were \
         filed under the same namespace"
    );

    let svg = fm_render_svg::render_svg(&ir);
    assert!(
        svg.contains(">N1<") && svg.contains(">N2<"),
        "a namespace name is missing"
    );
    assert_eq!(ir.edges.len(), 1, "the cross-namespace relation was lost");
}

/// A namespaced class keeps its members, so the grouping did not cost the class its body.
#[test]
fn a_class_inside_a_namespace_keeps_its_members() {
    let source =
        "classDiagram\n  namespace N {\n    class A {\n      +int x\n      +run()\n    }\n  }\n";
    let ir = parse(source);
    let svg = fm_render_svg::render_svg(&ir);

    assert!(svg.contains(">N<"), "the namespace name is missing");
    assert!(
        svg.contains("+int x"),
        "the attribute row vanished once the class was namespaced"
    );
    assert!(
        svg.contains("+run()"),
        "the method row vanished once the class was namespaced"
    );
}

/// CONTROL: a class diagram with NO namespace draws no cluster.
///
/// The fix writes cluster membership on a path that every class statement passes through, so the
/// case that must NOT gain a box is worth pinning: inventing a cluster around an ungrouped diagram
/// would also make the tests above pass.
#[test]
fn a_class_diagram_without_a_namespace_draws_no_cluster() {
    let ir = parse("classDiagram\n  class A\n  class B\n  A <|-- B\n");
    let laid_out = layout(&ir);
    assert!(
        laid_out.clusters.is_empty(),
        "a diagram that declares no namespace gained {} cluster(s)",
        laid_out.clusters.len()
    );
}

/// CONTROL: the flowchart subgraph this shares its machinery with is unchanged.
///
/// Flowcharts already populated both membership indices and drew their box correctly; the fix adds
/// the second write on the class path only. This is the case that would break if the shared cluster
/// code were changed instead.
#[test]
fn a_flowchart_subgraph_still_draws_its_box() {
    let source = format!("flowchart LR\n  subgraph S[Group]\n    A {ARROW} B\n  end\n");
    let ir = parse(&source);
    let laid_out = layout(&ir);

    assert_eq!(laid_out.clusters.len(), 1);
    assert_eq!(
        nodes_inside(&laid_out.clusters[0], &laid_out),
        vec!["A".to_string(), "B".to_string()]
    );
    assert!(fm_render_svg::render_svg(&ir).contains(">Group<"));
}
