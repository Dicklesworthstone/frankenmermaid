#![forbid(unsafe_code)]

mod dot_parser;
mod ir_builder;
mod mermaid_parser;

use fm_core::{DiagramType, MermaidDiagramIr};
use serde::Serialize;
use serde_json::json;

pub use dot_parser::{looks_like_dot, parse_dot};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParseResult {
    pub ir: MermaidDiagramIr,
    pub warnings: Vec<String>,
    /// Detection confidence (0.0 to 1.0)
    pub confidence: f32,
    /// Method used for type detection
    pub detection_method: DetectionMethod,
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

    // Strategy 1: DOT format detection (high priority for interop)
    if looks_like_dot(input) {
        return DetectedType {
            diagram_type: DiagramType::Flowchart,
            confidence: 0.95,
            method: DetectionMethod::DotFormat,
            warnings: vec![],
        };
    }

    // Get first significant line
    let first_line = mermaid_parser::first_significant_line(input).unwrap_or("");
    let lower = first_line.to_ascii_lowercase();

    // Strategy 2: Exact keyword match
    if let Some(detected) = exact_keyword_match(&lower, first_line) {
        return detected;
    }

    // Strategy 3: Fuzzy keyword match
    if let Some(detected) = fuzzy_keyword_match(&lower) {
        return detected;
    }

    // Strategy 4: Content heuristics
    if let Some(detected) = content_heuristics(input) {
        return detected;
    }

    // Strategy 5: Fallback to flowchart
    DetectedType {
        diagram_type: DiagramType::Flowchart,
        confidence: 0.3,
        method: DetectionMethod::Fallback,
        warnings: vec!["Could not detect diagram type; assuming flowchart".to_string()],
    }
}

/// Exact keyword matching for diagram type detection.
fn exact_keyword_match(lower: &str, original: &str) -> Option<DetectedType> {
    let (diagram_type, confidence) =
        if lower.starts_with("flowchart") || lower == "graph" || lower.starts_with("graph ") {
            (DiagramType::Flowchart, 1.0)
        } else if lower.starts_with("sequencediagram") {
            (DiagramType::Sequence, 1.0)
        } else if lower.starts_with("classdiagram") {
            (DiagramType::Class, 1.0)
        } else if lower.starts_with("statediagram") {
            (DiagramType::State, 1.0)
        } else if lower.starts_with("gantt") {
            (DiagramType::Gantt, 1.0)
        } else if lower.starts_with("erdiagram") {
            (DiagramType::Er, 1.0)
        } else if lower.starts_with("mindmap") {
            (DiagramType::Mindmap, 1.0)
        } else if lower.starts_with("pie") {
            (DiagramType::Pie, 1.0)
        } else if lower.starts_with("gitgraph") {
            (DiagramType::GitGraph, 1.0)
        } else if lower.starts_with("journey") {
            (DiagramType::Journey, 1.0)
        } else if lower.starts_with("requirementdiagram") {
            (DiagramType::Requirement, 1.0)
        } else if lower.starts_with("timeline") {
            (DiagramType::Timeline, 1.0)
        } else if lower.starts_with("quadrantchart") {
            (DiagramType::QuadrantChart, 1.0)
        } else if lower.starts_with("sankey") {
            (DiagramType::Sankey, 1.0)
        } else if lower.starts_with("xychart") {
            (DiagramType::XyChart, 1.0)
        } else if lower.starts_with("block-beta") {
            (DiagramType::BlockBeta, 1.0)
        } else if lower.starts_with("packet-beta") {
            (DiagramType::PacketBeta, 1.0)
        } else if lower.starts_with("architecture-beta") {
            (DiagramType::ArchitectureBeta, 1.0)
        } else if original.starts_with("C4Context") {
            (DiagramType::C4Context, 1.0)
        } else if original.starts_with("C4Container") {
            (DiagramType::C4Container, 1.0)
        } else if original.starts_with("C4Component") {
            (DiagramType::C4Component, 1.0)
        } else if original.starts_with("C4Dynamic") {
            (DiagramType::C4Dynamic, 1.0)
        } else if original.starts_with("C4Deployment") {
            (DiagramType::C4Deployment, 1.0)
        } else {
            return None;
        };

    Some(DetectedType {
        diagram_type,
        confidence,
        method: DetectionMethod::ExactKeyword,
        warnings: vec![],
    })
}

/// Known diagram keywords for fuzzy matching.
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
];

/// Fuzzy keyword matching using Levenshtein distance.
fn fuzzy_keyword_match(lower: &str) -> Option<DetectedType> {
    // Extract the first word
    let first_word = lower.split_whitespace().next()?;

    // Find best fuzzy match
    let mut best_match: Option<(DiagramType, usize)> = None;

    for (keyword, diagram_type) in DIAGRAM_KEYWORDS {
        let distance = levenshtein_distance(first_word, keyword);
        // Only consider matches with distance 1-2 (non-zero but close)
        if distance > 0 && distance <= 2 {
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
        let confidence = match distance {
            1 => 0.85,
            2 => 0.7,
            _ => 0.5,
        };

        DetectedType {
            diagram_type,
            confidence,
            method: DetectionMethod::FuzzyKeyword,
            warnings: vec![format!(
                "Fuzzy match: possible typo in diagram type declaration"
            )],
        }
    })
}

/// Content-based heuristics for detecting diagram type from patterns.
fn content_heuristics(input: &str) -> Option<DetectedType> {
    let content = input.to_lowercase();

    // ER diagram patterns
    if content.contains("||--o{")
        || content.contains("}|--||")
        || content.contains("||--|{")
        || content.contains("|o--o|")
    {
        return Some(DetectedType {
            diagram_type: DiagramType::Er,
            confidence: 0.8,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected ER relationship patterns".to_string()],
        });
    }

    // Sequence diagram patterns
    if content.contains("->>")
        || content.contains("participant ")
        || content.contains("actor ")
        || content.contains("activate ")
    {
        return Some(DetectedType {
            diagram_type: DiagramType::Sequence,
            confidence: 0.75,
            method: DetectionMethod::ContentHeuristic,
            warnings: vec!["Detected sequence diagram patterns".to_string()],
        });
    }

    // Class diagram patterns
    if content.contains("<|--")
        || content.contains("--|>")
        || (content.contains("class ") && content.contains("{"))
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

    // Flowchart patterns (broad, lower confidence)
    if content.contains("-->") || content.contains("---") || content.contains("==>") {
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
            let cost = if a_char == b_char { 0 } else { 1 };
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
    detect_type_with_confidence(input).diagram_type
}

#[must_use]
pub fn parse(input: &str) -> ParseResult {
    if input.trim().is_empty() {
        return ParseResult {
            ir: MermaidDiagramIr::empty(DiagramType::Unknown),
            warnings: vec!["Input was empty; returning empty IR".to_string()],
            confidence: 0.0,
            detection_method: DetectionMethod::Fallback,
        };
    }

    // Detect type with confidence first
    let detection = detect_type_with_confidence(input);

    if detection.method == DetectionMethod::DotFormat {
        // DOT format - parse via dot parser
        let mut result = parse_dot(input);
        result.confidence = detection.confidence;
        result.detection_method = detection.method;
        return result;
    }

    mermaid_parser::parse_mermaid_with_detection(input, detection)
}

#[must_use]
pub fn parse_evidence_json(parsed: &ParseResult) -> String {
    json!({
        "diagram_type": parsed.ir.diagram_type.as_str(),
        "node_count": parsed.ir.nodes.len(),
        "edge_count": parsed.ir.edges.len(),
        "cluster_count": parsed.ir.clusters.len(),
        "label_count": parsed.ir.labels.len(),
        "warning_count": parsed.warnings.len(),
        "warnings": parsed.warnings.clone(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{detect_type, parse};
    use fm_core::{ArrowType, DiagramType, GraphDirection, IrEndpoint, MermaidDiagramIr};
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
        assert_eq!(result.confidence, 1.0);
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
        assert_eq!(result.confidence, 0.0);
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
        assert_eq!(result.confidence, 1.0);
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
            prop_assert_eq!(first.confidence, second.confidence);
            prop_assert_eq!(first.warnings, second.warnings);
        }
    }
}
