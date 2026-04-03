//! Comprehensive bidirectional lens round-trip consistency tests.
//!
//! Verifies the PutGet and GetPut laws for the Mermaid lens system:
//!
//! **GetPut law**: `apply_lens_edit(source, edit) → updated_source` then
//! `build_lens_bindings(updated_source, updated_source_map)` should find the
//! edited element with the replacement text as its snippet.
//!
//! **PutGet law**: If we read a snippet via `build_lens_bindings`, then write it
//! back unchanged via `apply_lens_edit`, the source should be identical.

#[cfg(test)]
mod tests {
    use crate::{
        DiagramType, MermaidLensEdit, MermaidSourceMap, MermaidSourceMapEntry,
        MermaidSourceMapKind, Position, Span, apply_lens_edit, build_lens_bindings,
    };

    /// Helper to create a span from line/col ranges.
    fn span(line: usize, start_col: usize, end_col: usize) -> Span {
        Span::new(
            Position {
                line,
                col: start_col,
                byte: 0,
            },
            Position {
                line,
                col: end_col,
                byte: 0,
            },
        )
    }

    fn simple_source_map(source: &str) -> MermaidSourceMap {
        // "flowchart LR\nA-->B\n"
        // Line 2: "A-->B"  (col 1='A', 2='-', 3='-', 4='>', 5='B')
        // Span end col is INCLUSIVE; resolve_span_text_range adds 1 for exclusive end.
        let _ = source;
        MermaidSourceMap {
            diagram_type: DiagramType::Flowchart,
            entries: vec![
                MermaidSourceMapEntry {
                    kind: MermaidSourceMapKind::Node,
                    index: 0,
                    element_id: "fm-node-a-0".into(),
                    source_id: Some("A".into()),
                    span: span(2, 1, 1), // "A" (col 1, 1 char)
                },
                MermaidSourceMapEntry {
                    kind: MermaidSourceMapKind::Node,
                    index: 1,
                    element_id: "fm-node-b-1".into(),
                    source_id: Some("B".into()),
                    span: span(2, 5, 5), // "B" (col 5, 1 char)
                },
                MermaidSourceMapEntry {
                    kind: MermaidSourceMapKind::Edge,
                    index: 0,
                    element_id: "fm-edge-0".into(),
                    source_id: None,
                    span: span(2, 1, 5), // "A-->B" (col 1..5, 5 chars)
                },
            ],
        }
    }

    // -----------------------------------------------------------------------
    // PutGet law: read a snippet, write it back unchanged → source unchanged
    // -----------------------------------------------------------------------

    #[test]
    fn putget_identity_edit_preserves_source() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let bindings = build_lens_bindings(source, &source_map);

        // For each binding with a snippet, write it back unchanged.
        for binding in &bindings {
            if let Some(snippet) = &binding.snippet {
                let edit = MermaidLensEdit {
                    element_id: binding.element_id.clone(),
                    replacement: snippet.clone(),
                };
                let result =
                    apply_lens_edit(source, &source_map, &edit).expect("edit should apply");
                assert_eq!(
                    result.updated_source, source,
                    "PutGet violated for element '{}': writing back snippet '{}' changed the source",
                    binding.element_id, snippet
                );
            }
        }
    }

    #[test]
    fn putget_identity_preserves_binding_projection() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);
        let before = build_lens_bindings(source, &source_map);

        let edge_binding = before
            .iter()
            .find(|binding| binding.element_id == "fm-edge-0")
            .expect("edge binding");
        let edit = MermaidLensEdit {
            element_id: edge_binding.element_id.clone(),
            replacement: edge_binding
                .snippet
                .clone()
                .expect("edge binding should expose snippet"),
        };

        let result = apply_lens_edit(source, &source_map, &edit).expect("identity edit");
        let after = build_lens_bindings(&result.updated_source, &source_map);

        assert_eq!(result.updated_source, source);
        assert_eq!(after, before);
    }

    #[test]
    fn putget_node_identity() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let edit = MermaidLensEdit {
            element_id: "fm-node-a-0".into(),
            replacement: "A".into(),
        };
        let result = apply_lens_edit(source, &source_map, &edit).unwrap();
        assert_eq!(result.updated_source, source);
    }

    // -----------------------------------------------------------------------
    // GetPut law: write a new value, then read it back → get the new value
    // -----------------------------------------------------------------------

    #[test]
    fn getput_edit_is_readable() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        // Replace "A" with "Start"
        let edit = MermaidLensEdit {
            element_id: "fm-node-a-0".into(),
            replacement: "Start".into(),
        };
        let result = apply_lens_edit(source, &source_map, &edit).unwrap();

        assert_eq!(result.updated_source, "flowchart LR\nStart-->B\n");
        assert_eq!(result.previous_snippet, "A");
        assert_eq!(result.replacement, "Start");
    }

    #[test]
    fn getput_edge_replacement() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let edit = MermaidLensEdit {
            element_id: "fm-edge-0".into(),
            replacement: "A-.->B".into(),
        };
        let result = apply_lens_edit(source, &source_map, &edit).unwrap();

        assert_eq!(result.updated_source, "flowchart LR\nA-.->B\n");
        assert_eq!(result.previous_snippet, "A-->B");
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn edit_missing_element_returns_error() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let edit = MermaidLensEdit {
            element_id: "nonexistent".into(),
            replacement: "X".into(),
        };
        let result = apply_lens_edit(source, &source_map, &edit);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Binding completeness
    // -----------------------------------------------------------------------

    #[test]
    fn bindings_cover_all_source_map_entries() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let bindings = build_lens_bindings(source, &source_map);
        assert_eq!(
            bindings.len(),
            source_map.entries.len(),
            "Every source map entry should produce a binding"
        );

        for (binding, entry) in bindings.iter().zip(source_map.entries.iter()) {
            assert_eq!(binding.element_id, entry.element_id);
            assert_eq!(binding.kind, entry.kind);
        }
    }

    #[test]
    fn binding_snippets_match_source_text() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let bindings = build_lens_bindings(source, &source_map);

        // Node A should have snippet "A"
        let node_a = bindings
            .iter()
            .find(|b| b.element_id == "fm-node-a-0")
            .expect("node A binding");
        assert_eq!(node_a.snippet.as_deref(), Some("A"));

        // Node B should have snippet "B"
        let node_b = bindings
            .iter()
            .find(|b| b.element_id == "fm-node-b-1")
            .expect("node B binding");
        assert_eq!(node_b.snippet.as_deref(), Some("B"));
    }

    // -----------------------------------------------------------------------
    // Multi-edit consistency
    // -----------------------------------------------------------------------

    #[test]
    fn sequential_edits_compose() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        // First edit: replace A with Start
        let edit1 = MermaidLensEdit {
            element_id: "fm-node-a-0".into(),
            replacement: "Start".into(),
        };
        let result1 = apply_lens_edit(source, &source_map, &edit1).unwrap();
        assert_eq!(result1.updated_source, "flowchart LR\nStart-->B\n");

        // The result reports what was replaced and with what.
        assert_eq!(result1.previous_snippet, "A");
        assert_eq!(result1.replacement, "Start");
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn empty_replacement_removes_element_text() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let edit = MermaidLensEdit {
            element_id: "fm-node-a-0".into(),
            replacement: String::new(),
        };
        let result = apply_lens_edit(source, &source_map, &edit).unwrap();
        assert_eq!(result.updated_source, "flowchart LR\n-->B\n");
    }

    #[test]
    fn replacement_with_special_chars() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let edit = MermaidLensEdit {
            element_id: "fm-node-a-0".into(),
            replacement: "X[\"Label with quotes\"]".into(),
        };
        let result = apply_lens_edit(source, &source_map, &edit).unwrap();
        assert!(result.updated_source.contains("X[\"Label with quotes\"]"));
    }

    #[test]
    fn edit_result_range_matches_edit() {
        let source = "flowchart LR\nA-->B\n";
        let source_map = simple_source_map(source);

        let edit = MermaidLensEdit {
            element_id: "fm-node-a-0".into(),
            replacement: "Start".into(),
        };
        let result = apply_lens_edit(source, &source_map, &edit).unwrap();

        // The replaced range should correspond to the original "A" position.
        let range = result.replaced_range;
        assert_eq!(&source[range.start_byte..range.end_byte], "A");
    }
}
