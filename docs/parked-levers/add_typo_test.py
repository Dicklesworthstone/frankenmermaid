p = '/data/projects/frankenmermaid/crates/fm-parser/src/mermaid_parser.rs'
s = open(p).read()

anchor = '''    #[test]
    fn operator_tables_are_longest_prefix_first() {'''

test = r'''    /// A one-letter typo in the header must not produce a diagram containing a box captioned with
    /// the typo (bd-ec1t).
    ///
    /// The parser already KNEW: detection fuzzy-matches `flowchat` to `flowchart` and warns
    /// "possible typo in diagram type declaration", and then the same line was interned as a node
    /// by the generic fallback. Every control below exists because the guard could easily eat real
    /// content instead.
    #[test]
    fn a_mistyped_diagram_header_is_not_interned_as_a_node() {
        let ids = |result: &crate::ParseResult| -> Vec<String> {
            result.ir.nodes.iter().map(|node| node.id.clone()).collect()
        };

        let typo = parse_mermaid("flowchat LR\n  A-->B-->C");
        assert_eq!(
            ids(&typo),
            vec!["A", "B", "C"],
            "the mistyped header must not appear as a node"
        );

        // The DIAGNOSIS must survive the fix. Removing the phantom node by suppressing the
        // detection warning would trade a visible defect for a silent one.
        assert!(
            typo.warnings
                .iter()
                .any(|warning| warning.contains("Fuzzy match")),
            "the typo warning must survive: {:?}",
            typo.warnings
        );

        // CONTROL 1: the correct header, same graph. This is the shape the typo case must match.
        let correct = parse_mermaid("flowchart LR\n  A-->B-->C");
        assert_eq!(ids(&correct), vec!["A", "B", "C"]);

        // CONTROL 2: a node legitimately NAMED after a diagram keyword, under a correct header,
        // must still be declared. A guard written as a keyword blacklist fails here.
        let keyword_node = parse_mermaid("flowchart LR\n  graph[Box] --> A");
        assert!(
            ids(&keyword_node).iter().any(|id| id == "graph"),
            "a node called `graph` must survive: {:?}",
            ids(&keyword_node)
        );

        // CONTROL 3: a HEADERLESS diagram whose genuine first line is a node. Content heuristics
        // exist precisely for this input, and it must not lose its first node — which is what a
        // guard keyed on "blank line one" rather than on the fuzzy-match signal would do.
        let headerless = parse_mermaid("A-->B\n  B-->C");
        assert_eq!(
            ids(&headerless),
            vec!["A", "B", "C"],
            "a headerless diagram must keep the node on its first line"
        );
    }

'''

assert s.count(anchor) == 1
s = s.replace(anchor, test + anchor, 1)
open(p, 'w').write(s)
print('typo-header test added')
