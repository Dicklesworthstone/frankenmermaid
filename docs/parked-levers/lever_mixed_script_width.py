"""docs_site_50 lever: one non-ASCII char must not cost the whole label its fast path.

Applies to crates/fm-core/src/font_metrics.rs. Reapply with `python3 <this file>` and gate with
`cargo test -j 1 -p fm-core --lib`.
"""

p = '/data/projects/frankenmermaid/crates/fm-core/src/font_metrics.rs'
s = open(p).read()

old = '''        if self.config.preset != FontPreset::Monospace && text.is_ascii() {
            let avg = self.avg_char_width;
            return text
                .bytes()
                .map(|b| avg * ASCII_WIDTH_MULT[b as usize])
                .sum();
        }
        text.chars().map(|c| self.char_width(c)).sum()'''

new = '''        if self.config.preset != FontPreset::Monospace {
            let avg = self.avg_char_width;
            // Pure ASCII: unchanged. `bytes()` is the shape the auto-vectorizer likes, so the
            // overwhelmingly common label keeps the fastest path it already had.
            if text.is_ascii() {
                return text
                    .bytes()
                    .map(|b| avg * ASCII_WIDTH_MULT[b as usize])
                    .sum();
            }
            // MIXED SCRIPT. The `is_ascii()` gate above used to be the ONLY gate, so a label like
            // `Café` — three ASCII chars and one accented one — sent every character down the
            // `classify()` match chain. One non-ASCII byte cost the whole label its table.
            //
            // That is the wrong shape for `docs_site_50`, the worst ratio this engine actually
            // renders at (434.107779x): the ledger describes it as flowchart-dominated with
            // "escaping/non-ASCII labels", and it measures 940 node labels across 50 diagrams. A
            // label that is mostly ASCII with an accent or a dash is exactly its common case, and
            // it was taking the slow path end to end.
            //
            // BIT-IDENTICAL, and the identity is structural rather than a claim: `ASCII_WIDTH_MULT`
            // is BUILT at compile time as `classify(i as char).multiplier()` for every i < 128, so
            // for an ASCII char the table and the match chain are the same f32 by construction.
            // Every term is still `avg * multiplier(c)` and the terms are still summed
            // left-to-right over `chars()` in order, so the f32 accumulation sequence is unchanged.
            // Node sizes therefore cannot move, and no golden can shift.
            //
            // The monospace branch of `char_width` is also hoisted out of the loop: the preset
            // cannot change per character, so testing it once is the same answer with one branch
            // instead of one per char.
            return text
                .chars()
                .map(|c| {
                    let multiplier = if c.is_ascii() {
                        ASCII_WIDTH_MULT[c as usize]
                    } else {
                        CharWidthClass::classify(c).multiplier()
                    };
                    avg * multiplier
                })
                .sum();
        }
        text.chars().map(|c| self.char_width(c)).sum()'''

assert s.count(old) == 1, "estimate_width fast path not in the expected form"
s = s.replace(old, new)

# Test, appended to the file's existing test module.
anchor = '#[cfg(test)]\nmod tests {\n'
test = '''
    /// One non-ASCII character must not cost a label its width table, and the mixed-script path
    /// must agree with the per-character reference BIT for BIT.
    ///
    /// Exact equality is the right assertion, not an epsilon: node sizes are derived from this
    /// number and every committed golden encodes the resulting geometry, so a one-ulp drift here
    /// is a corpus-wide diff. `to_bits()` is what makes "bit-identical" checkable rather than
    /// asserted in a comment.
    #[test]
    fn mixed_script_width_matches_the_per_char_reference_bit_for_bit() {
        let metrics = FontMetrics::default_metrics();
        let reference = |text: &str| -> f32 {
            text.chars()
                .map(|c| metrics.avg_char_width() * CharWidthClass::classify(c).multiplier())
                .sum()
        };

        for text in [
            "Cafe\\u{301}",           // combining accent: ASCII base, non-ASCII mark
            "naïve façade",           // mostly ASCII with two accents — the docs_site_50 shape
            "日本語ラベル",             // full-width, no ASCII at all
            "Ünicode — em dash",      // leading non-ASCII plus a wide punctuation char
            "mixed 日本 and ASCII",    // alternating runs
            "ASCII only",             // pure ASCII: must still take the byte path
            "",                       // empty
            "é",                      // single non-ASCII char
        ] {
            assert_eq!(
                metrics.estimate_width(text).to_bits(),
                reference(text).to_bits(),
                "width drifted for {text:?}: got {} want {}",
                metrics.estimate_width(text),
                reference(text)
            );
        }
    }

    /// Control: the monospace preset is a different rule entirely — every character is one
    /// `avg_char_width`, including full-width ones — and the mixed-script path must not capture it.
    /// Without this, mapping monospace onto the width table would look like a simplification and
    /// would silently widen every CJK label in a monospace diagram.
    #[test]
    fn monospace_width_is_unaffected_by_the_mixed_script_path() {
        let metrics = FontMetrics::monospace(14.0);
        for text in ["日本語", "abc", "aé日"] {
            // Summed, NOT `avg * count`: repeated f32 addition and a single multiply are not
            // bit-equal in general, and this test asserts bits. The reference must mirror how the
            // implementation accumulates, or the control fails on correct code.
            let expected: f32 = text.chars().map(|_| metrics.avg_char_width()).sum();
            assert_eq!(
                metrics.estimate_width(text).to_bits(),
                expected.to_bits(),
                "monospace width changed for {text:?}"
            );
        }
    }
'''

assert s.count(anchor) == 1, "test module anchor not found"
s = s.replace(anchor, anchor + test, 1)
open(p, 'w').write(s)
print('lever applied: mixed-script labels keep the ASCII width table')
