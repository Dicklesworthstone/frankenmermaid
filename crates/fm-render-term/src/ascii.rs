//! ASCII diagram detection and correction.
//!
//! Provides utilities for detecting ASCII art diagrams in text and
//! cleaning up right-border alignment issues.

/// Character classification for diagram detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    /// Box-drawing character (Unicode or ASCII).
    BoxDrawing,
    /// Arrow or directional symbol.
    Arrow,
    /// Connector junction.
    Junction,
    /// Horizontal line segment.
    HorizontalLine,
    /// Vertical line segment.
    VerticalLine,
    /// Diagonal line segment.
    DiagonalLine,
    /// Text or label character.
    Text,
    /// Whitespace.
    Whitespace,
    /// Unknown/other.
    Other,
}

/// Classify a character for diagram detection.
#[must_use]
pub fn classify_char(ch: char) -> CharClass {
    match ch {
        // Unicode box-drawing.
        '─' | '━' | '═' | '│' | '┃' | '║' => CharClass::BoxDrawing,
        '┌' | '┐' | '└' | '┘' | '┏' | '┓' | '┗' | '┛' | '╔' | '╗' | '╚' | '╝' | '╭' | '╮' | '╰'
        | '╯' | '+' => CharClass::Junction,
        '├' | '┤' | '┬' | '┴' | '┼' | '┣' | '┫' | '┳' | '┻' | '╋' | '╠' | '╣' | '╦' | '╩' | '╬' => {
            CharClass::Junction
        }

        // ASCII box-drawing.
        '-' => CharClass::HorizontalLine,
        '|' => CharClass::VerticalLine,
        '/' | '\\' => CharClass::DiagonalLine,

        // Arrows.
        '>' | '<' | '^' | 'v' | 'V' | '→' | '←' | '↑' | '↓' | '▶' | '◀' | '▲' | '▼' | '»' | '«' => {
            CharClass::Arrow
        }

        // Whitespace.
        ' ' | '\t' => CharClass::Whitespace,

        // Alphanumeric = text.
        ch if ch.is_alphanumeric() => CharClass::Text,

        // Punctuation that might be labels.
        ':' | '.' | ',' | ';' | '!' | '?' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' => {
            CharClass::Text
        }

        _ => CharClass::Other,
    }
}

/// Classification of a text line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClass {
    /// Line is mostly diagram elements.
    Diagram,
    /// Line is mostly text/labels.
    Text,
    /// Line is empty or whitespace.
    Empty,
    /// Mixed content.
    Mixed,
}

/// Classify a line of text.
#[must_use]
pub fn classify_line(line: &str) -> LineClass {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineClass::Empty;
    }

    let mut total = 0_usize;
    let mut diagram_count = 0_usize;
    let mut text_count = 0_usize;

    for ch in trimmed.chars() {
        total += 1;
        match classify_char(ch) {
            CharClass::BoxDrawing
            | CharClass::Arrow
            | CharClass::Junction
            | CharClass::HorizontalLine
            | CharClass::VerticalLine
            | CharClass::DiagonalLine => diagram_count += 1,
            CharClass::Text => text_count += 1,
            _ => {}
        }
    }

    let diagram_ratio = diagram_count as f32 / total as f32;
    let text_ratio = text_count as f32 / total as f32;

    if diagram_ratio > 0.6 {
        LineClass::Diagram
    } else if text_ratio > 0.7 {
        LineClass::Text
    } else if diagram_count > 0 && text_count > 0 {
        LineClass::Mixed
    } else {
        LineClass::Text
    }
}

/// Detected diagram block in text.
#[derive(Debug, Clone)]
pub struct DiagramBlock {
    /// Starting line index (0-based).
    pub start_line: usize,
    /// Ending line index (exclusive).
    pub end_line: usize,
    /// Starting column (0-based).
    pub start_col: usize,
    /// Ending column (exclusive).
    pub end_col: usize,
    /// The extracted lines.
    pub lines: Vec<String>,
}

/// Detect diagram blocks in a text document.
#[must_use]
pub fn detect_diagram_blocks(text: &str) -> Vec<DiagramBlock> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();

    let mut in_block = false;
    let mut block_start = 0_usize;
    let mut min_col = usize::MAX;
    let mut max_col = 0_usize;

    for (i, line) in lines.iter().enumerate() {
        let class = classify_line(line);

        match class {
            LineClass::Diagram | LineClass::Mixed => {
                if !in_block {
                    in_block = true;
                    block_start = i;
                    min_col = usize::MAX;
                    max_col = 0;
                }

                // Update column bounds.
                if let Some(first_diagram_col) = find_first_diagram_char(line) {
                    min_col = min_col.min(first_diagram_col);
                }
                if let Some(last_diagram_col) = find_last_diagram_char(line) {
                    max_col = max_col.max(last_diagram_col);
                }
            }
            LineClass::Empty | LineClass::Text => {
                // Allow one empty line within a block.
                if in_block {
                    let next_is_diagram = lines
                        .get(i + 1)
                        .map(|l| matches!(classify_line(l), LineClass::Diagram | LineClass::Mixed))
                        .unwrap_or(false);

                    if !next_is_diagram {
                        // End the block.
                        if i > block_start {
                            let block_lines: Vec<String> = lines[block_start..i]
                                .iter()
                                .map(|l| l.to_string())
                                .collect();

                            blocks.push(DiagramBlock {
                                start_line: block_start,
                                end_line: i,
                                start_col: min_col.min(max_col),
                                end_col: max_col,
                                lines: block_lines,
                            });
                        }
                        in_block = false;
                    }
                }
            }
        }
    }

    // Handle block at end of document.
    if in_block && lines.len() > block_start {
        let block_lines: Vec<String> = lines[block_start..].iter().map(|l| l.to_string()).collect();

        blocks.push(DiagramBlock {
            start_line: block_start,
            end_line: lines.len(),
            start_col: min_col.min(max_col),
            end_col: max_col,
            lines: block_lines,
        });
    }

    blocks
}

fn find_first_diagram_char(line: &str) -> Option<usize> {
    line.char_indices()
        .find(|(_, ch)| {
            matches!(
                classify_char(*ch),
                CharClass::BoxDrawing
                    | CharClass::Junction
                    | CharClass::HorizontalLine
                    | CharClass::VerticalLine
                    | CharClass::DiagonalLine
                    | CharClass::Arrow
            )
        })
        .map(|(i, _)| i)
}

fn find_last_diagram_char(line: &str) -> Option<usize> {
    line.char_indices()
        .rfind(|(_, ch)| {
            matches!(
                classify_char(*ch),
                CharClass::BoxDrawing
                    | CharClass::Junction
                    | CharClass::HorizontalLine
                    | CharClass::VerticalLine
                    | CharClass::DiagonalLine
                    | CharClass::Arrow
            )
        })
        .map(|(i, ch)| i + ch.len_utf8())
}

/// Align the right border of a diagram by padding lines.
///
/// ASCII diagrams often have ragged right edges due to varying line lengths.
/// This function pads shorter lines to align the right border.
#[must_use]
pub fn align_right_border(lines: &[String]) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    // Find maximum significant width (in characters, not bytes).
    let max_chars = lines
        .iter()
        .map(|l| l.trim_end().chars().count())
        .max()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_end();
            let char_count = trimmed.chars().count();
            if char_count < max_chars {
                let padding = max_chars - char_count;
                format!("{}{}", trimmed, " ".repeat(padding))
            } else {
                trimmed.to_string()
            }
        })
        .collect()
}

/// Clean up ASCII diagram by normalizing box-drawing characters.
///
/// Converts ASCII box-drawing (+, -, |) to Unicode equivalents and
/// fixes misaligned junctions.
#[must_use]
pub fn normalize_box_drawing(text: &str, to_unicode: bool) -> String {
    if !to_unicode {
        return text.to_string();
    }

    let mut lines: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();

    // First pass: convert basic characters.
    for line in &mut lines {
        for ch in line.iter_mut() {
            *ch = match *ch {
                '-' => '─',
                '|' => '│',
                _ => *ch,
            };
        }
    }

    // Second pass: fix junctions.
    let height = lines.len();
    for y in 0..height {
        let width = lines[y].len();
        for x in 0..width {
            if lines[y][x] == '+' {
                let junction = detect_junction(&lines, x, y);
                lines[y][x] = junction;
            }
        }
    }

    lines
        .into_iter()
        .map(|l| l.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn detect_junction(lines: &[Vec<char>], x: usize, y: usize) -> char {
    let up = y > 0
        && lines
            .get(y - 1)
            .and_then(|l| l.get(x))
            .map(|c| is_vertical_connector(*c))
            .unwrap_or(false);

    let down = lines
        .get(y + 1)
        .and_then(|l| l.get(x))
        .map(|c| is_vertical_connector(*c))
        .unwrap_or(false);

    let left = x > 0
        && lines[y]
            .get(x - 1)
            .map(|c| is_horizontal_connector(*c))
            .unwrap_or(false);

    let right = lines[y]
        .get(x + 1)
        .map(|c| is_horizontal_connector(*c))
        .unwrap_or(false);

    match (up, down, left, right) {
        (false, true, false, true) => '┌',
        (false, true, true, false) => '┐',
        (true, false, false, true) => '└',
        (true, false, true, false) => '┘',
        (true, true, false, true) => '├',
        (true, true, true, false) => '┤',
        (false, true, true, true) => '┬',
        (true, false, true, true) => '┴',
        (true, true, true, true) => '┼',
        (false, false, true, true) => '─',
        (true, true, false, false) => '│',
        _ => '+',
    }
}

fn is_vertical_connector(ch: char) -> bool {
    matches!(
        ch,
        '|' | '│'
            | '┃'
            | '║'
            | '+'
            | '┼'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '┌'
            | '┐'
            | '└'
            | '┘'
    )
}

fn is_horizontal_connector(ch: char) -> bool {
    matches!(
        ch,
        '-' | '─'
            | '━'
            | '═'
            | '+'
            | '┼'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '┌'
            | '┐'
            | '└'
            | '┘'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify_line_collect_baseline(line: &str) -> LineClass {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return LineClass::Empty;
        }

        let chars: Vec<char> = trimmed.chars().collect();
        let total = chars.len();
        let mut diagram_count = 0_usize;
        let mut text_count = 0_usize;

        for ch in chars {
            match classify_char(ch) {
                CharClass::BoxDrawing
                | CharClass::Arrow
                | CharClass::Junction
                | CharClass::HorizontalLine
                | CharClass::VerticalLine
                | CharClass::DiagonalLine => diagram_count += 1,
                CharClass::Text => text_count += 1,
                _ => {}
            }
        }

        let diagram_ratio = diagram_count as f32 / total as f32;
        let text_ratio = text_count as f32 / total as f32;

        if diagram_ratio > 0.6 {
            LineClass::Diagram
        } else if text_ratio > 0.7 {
            LineClass::Text
        } else if diagram_count > 0 && text_count > 0 {
            LineClass::Mixed
        } else {
            LineClass::Text
        }
    }

    fn line_class_code(class: LineClass) -> u64 {
        match class {
            LineClass::Diagram => 1,
            LineClass::Text => 2,
            LineClass::Empty => 3,
            LineClass::Mixed => 4,
        }
    }

    #[test]
    fn classifies_box_drawing() {
        assert_eq!(classify_char('─'), CharClass::BoxDrawing);
        assert_eq!(classify_char('│'), CharClass::BoxDrawing);
        assert_eq!(classify_char('┌'), CharClass::Junction);
        assert_eq!(classify_char('+'), CharClass::Junction);
    }

    #[test]
    fn classifies_text() {
        assert_eq!(classify_char('A'), CharClass::Text);
        assert_eq!(classify_char('1'), CharClass::Text);
        assert_eq!(classify_char(' '), CharClass::Whitespace);
    }

    #[test]
    fn classifies_diagram_line() {
        assert_eq!(classify_line("┌────────┐"), LineClass::Diagram);
        assert_eq!(classify_line("│  text  │"), LineClass::Mixed);
        assert_eq!(classify_line("This is text"), LineClass::Text);
        assert_eq!(classify_line(""), LineClass::Empty);
    }

    #[test]
    fn streaming_line_classification_matches_collect_baseline() {
        let lines = [
            "",
            "   \t",
            "+--------+",
            "┌────────┐",
            "│  text  │",
            "A-->B",
            "This is ordinary text",
            "  ╠═╦═╣  ",
            "Αβγ → 東京",
            "... ??? !!!",
            "_/\\_",
        ];

        for line in lines {
            assert_eq!(
                classify_line(line),
                classify_line_collect_baseline(line),
                "classification changed for {line:?}"
            );
        }
    }

    #[test]
    #[ignore = "release-only same-binary performance probe"]
    fn classify_line_streaming_perf_probe() {
        use std::hint::black_box;
        use std::time::Instant;

        const SAMPLE_COUNT: usize = 9;
        const SWEEPS: usize = 64;

        let patterns = [
            "+----------------------+",
            "| service --> database |",
            "┌──────────────────────┐",
            "│ Unicode label → cache │",
            "ordinary prose around a diagram block",
            "    A -- event --> B    ",
            "├── worker-01 ──┤",
            "[client] .... [gateway]",
        ];
        let lines: Vec<String> = (0..4_096)
            .map(|index| patterns[index % patterns.len()].repeat(1 + index % 3))
            .collect();

        fn measure(lines: &[String], classifier: impl Fn(&str) -> LineClass) -> (u128, u64) {
            let started = Instant::now();
            let mut digest = 0xcbf2_9ce4_8422_2325_u64;
            for _ in 0..SWEEPS {
                for line in lines {
                    let class = black_box(classifier(black_box(line.as_str())));
                    digest ^= line_class_code(class);
                    digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            (started.elapsed().as_nanos(), digest)
        }

        let (_, baseline_digest) = measure(&lines, classify_line_collect_baseline);
        let (_, candidate_digest) = measure(&lines, classify_line);
        assert_eq!(candidate_digest, baseline_digest);

        let mut baseline_samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut candidate_samples = Vec::with_capacity(SAMPLE_COUNT);
        for sample in 0..SAMPLE_COUNT {
            let ((baseline_ns, baseline_digest), (candidate_ns, candidate_digest)) =
                if sample % 2 == 0 {
                    (
                        measure(&lines, classify_line_collect_baseline),
                        measure(&lines, classify_line),
                    )
                } else {
                    let candidate = measure(&lines, classify_line);
                    let baseline = measure(&lines, classify_line_collect_baseline);
                    (baseline, candidate)
                };
            assert_eq!(candidate_digest, baseline_digest);
            baseline_samples.push(baseline_ns);
            candidate_samples.push(candidate_ns);
        }

        baseline_samples.sort_unstable();
        candidate_samples.sort_unstable();
        let baseline_median = baseline_samples[SAMPLE_COUNT / 2];
        let candidate_median = candidate_samples[SAMPLE_COUNT / 2];
        let speedup = baseline_median as f64 / candidate_median as f64;
        let improvement = (1.0 - candidate_median as f64 / baseline_median as f64) * 100.0;

        eprintln!("baseline_ns={baseline_samples:?}");
        eprintln!("candidate_ns={candidate_samples:?}");
        eprintln!(
            "baseline_median_ns={baseline_median} candidate_median_ns={candidate_median} improvement_pct={improvement:.3} speedup={speedup:.3}x digest={baseline_digest:016x}"
        );
    }

    #[test]
    fn detects_simple_diagram_block() {
        let text = r"
Some text before

+--------+
|  box   |
+--------+

Some text after
";
        let blocks = detect_diagram_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].lines.len() >= 3);
    }

    #[test]
    fn aligns_right_border() {
        let lines = vec![
            "┌────┐".to_string(),
            "│ A  │".to_string(),
            "└─┘".to_string(),
        ];
        let aligned = align_right_border(&lines);
        // Check all lines have the same character count.
        let first_len = aligned[0].chars().count();
        assert!(aligned.iter().all(|l| l.chars().count() == first_len));
    }

    #[test]
    fn normalizes_ascii_to_unicode() {
        let ascii = "+--+\n|  |\n+--+";
        let unicode = normalize_box_drawing(ascii, true);
        assert!(unicode.contains('┌'));
        assert!(unicode.contains('─'));
        assert!(unicode.contains('│'));
    }
}
