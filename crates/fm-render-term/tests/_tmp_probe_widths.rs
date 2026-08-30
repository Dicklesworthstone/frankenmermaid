//! TEMPORARY PROBE — reports rendered row widths; not a committed test.
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| if fm_core::is_east_asian_wide(c) { 2 } else { 1 })
        .sum()
}

#[test]
fn probe() {
    for (name, src) in [
        ("ascii", "flowchart LR\n  A[\"Hello World\"]\n"),
        ("cjk", "flowchart LR\n  A[\"日本語のラベル\"]\n"),
        ("mixed", "flowchart LR\n  A[\"Data 日本語 text\"]\n"),
        ("emoji", "flowchart LR\n  A[\"push 🚀 now\"]\n"),
    ] {
        let ir = fm_parser::parse(src).ir;
        let out = fm_render_term::render_term(&ir);
        let widths: Vec<usize> = out.lines().map(display_width).collect();
        let chars: Vec<usize> = out.lines().map(|l| l.chars().count()).collect();
        let uniq: std::collections::BTreeSet<_> = widths.iter().collect();
        println!(
            "{name}: rows={} distinct_display_widths={:?} char_counts_distinct={:?}",
            widths.len(),
            uniq,
            chars.iter().collect::<std::collections::BTreeSet<_>>()
        );
    }
}
