//! Type-safe SVG attribute system.
//!
//! Provides a flexible way to manage SVG attributes with proper escaping.

use std::borrow::Cow;
use std::fmt;

/// A single SVG attribute.
#[derive(Debug, Clone)]
pub struct Attribute {
    /// Attribute name. Borrowed for the overwhelmingly common static-literal case
    /// (`"x"`, `"width"`, `"stroke-width"`, …) so building an element does not heap
    /// allocate per attribute; owned only for dynamic names (e.g. `data-*`).
    pub name: Cow<'static, str>,
    pub value: AttributeValue,
}

/// Value of an SVG attribute.
#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    Number(f32),
    Integer(i32),
}

impl AttributeValue {
    /// Write the value directly into `out`, bypassing the `fmt::Formatter` indirection on
    /// the hot per-attribute serialization path. [`Display`](fmt::Display) delegates here.
    pub(crate) fn write_value<W: fmt::Write>(&self, out: &mut W) -> fmt::Result {
        match self {
            Self::String(s) => write_escaped_attr(out, s),
            Self::Number(n) => {
                // Whole numbers in i32 range serialize as integers; everything else to 2
                // decimals. `*n as i32` is a saturating cast, so the round-trip compare
                // `i as f32 == *n` is *exactly* the old `n.fract() == 0.0 && n.is_finite()
                // && n in i32 range` test: a fractional value truncates and fails the
                // compare; a whole in-range value round-trips; NaN/±inf and out-of-range
                // wholes fail the compare and fall to `write_fixed2` (which itself routes
                // non-finite to the std formatter) — identical bytes in every case. This
                // avoids the `f32::fract` → `truncf` libm call, which measured ~9% of
                // coordinate-heavy SVG render (per-attribute number formatting hot path).
                let i = *n as i32;
                if i as f32 == *n {
                    write_int_into(out, i)
                } else {
                    write_fixed2(out, *n)
                }
            }
            Self::Integer(i) => write_int_into(out, *i),
        }
    }
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_value(f)
    }
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<f32> for AttributeValue {
    fn from(n: f32) -> Self {
        Self::Number(n)
    }
}

impl From<i32> for AttributeValue {
    fn from(n: i32) -> Self {
        Self::Integer(n)
    }
}

/// Collection of SVG attributes.
#[derive(Debug, Clone, Default)]
pub struct Attributes {
    attrs: Vec<Attribute>,
}

impl Attributes {
    /// Create a new empty attribute collection.
    ///
    /// Lazily allocated (empty `Vec`): attribute-LESS elements — notably every node's `<title>`, plus
    /// other wrapper/leaf elements — never push, so they must not pay for a heap `Vec` at all. The first
    /// push reserves the typical 8–12 slots ([`push_attr`]), so attribute-carrying elements keep the
    /// realloc-free build the old eager `with_capacity(12)` gave. Element construction dominates SVG
    /// render time, and ~3 attribute-less elements per node were each paying a wasted 12-slot allocation.
    #[must_use]
    pub fn new() -> Self {
        Self { attrs: Vec::new() }
    }

    /// Push an attribute, reserving the typical element capacity on the first push. Centralises the
    /// lazy-allocation policy so an empty element never allocates while a populated one never reallocs.
    #[inline]
    fn push_attr(&mut self, attr: Attribute) {
        if self.attrs.capacity() == 0 {
            self.attrs.reserve(12);
        }
        self.attrs.push(attr);
    }

    /// Add an attribute.
    #[must_use]
    pub fn set<K: Into<Cow<'static, str>>, V: Into<AttributeValue>>(
        mut self,
        name: K,
        value: V,
    ) -> Self {
        let name = name.into();
        self.attrs.retain(|attr| attr.name != name);
        self.push_attr(Attribute {
            name,
            value: value.into(),
        });
        self
    }

    /// Add a string attribute.
    #[must_use]
    pub fn str<K: Into<Cow<'static, str>>>(self, name: K, value: &str) -> Self {
        self.set(name, value)
    }

    /// Add a numeric attribute.
    #[must_use]
    pub fn num<K: Into<Cow<'static, str>>>(self, name: K, value: f32) -> Self {
        self.set(name, value)
    }

    /// Add an integer attribute.
    #[must_use]
    pub fn int<K: Into<Cow<'static, str>>>(self, name: K, value: i32) -> Self {
        self.set(name, value)
    }

    /// Add a data-* attribute.
    #[must_use]
    pub fn data(self, name: &str, value: &str) -> Self {
        if let Some(static_name) = static_data_attr_name(name) {
            self.set(static_name, value)
        } else {
            self.set(format!("data-{name}"), value)
        }
    }

    /// Add a class attribute (will be merged if multiple).
    #[must_use]
    pub fn class(mut self, class: &str) -> Self {
        // Look for existing class attribute and append
        for attr in &mut self.attrs {
            if attr.name.as_ref() == "class"
                && let AttributeValue::String(ref mut s) = attr.value
            {
                s.push(' ');
                s.push_str(class);
                return self;
            }
        }
        self.set("class", class)
    }

    /// Add a CSS class made from two string pieces without allocating a temporary
    /// formatted class name when a class attribute already exists.
    #[must_use]
    pub fn class_prefixed(mut self, prefix: &str, suffix: &str) -> Self {
        for attr in &mut self.attrs {
            if attr.name.as_ref() == "class"
                && let AttributeValue::String(ref mut s) = attr.value
            {
                s.push(' ');
                s.push_str(prefix);
                s.push_str(suffix);
                return self;
            }
        }

        let mut class = String::with_capacity(prefix.len() + suffix.len());
        class.push_str(prefix);
        class.push_str(suffix);
        self.push_attr(Attribute {
            name: Cow::Borrowed("class"),
            value: AttributeValue::String(class),
        });
        self
    }

    /// Add a CSS class made from a string prefix and integer suffix without a
    /// temporary `format!` allocation on the hot node-rendering path.
    #[must_use]
    pub fn class_prefixed_usize(mut self, prefix: &str, value: usize) -> Self {
        for attr in &mut self.attrs {
            if attr.name.as_ref() == "class"
                && let AttributeValue::String(ref mut s) = attr.value
            {
                s.push(' ');
                s.push_str(prefix);
                push_usize(s, value);
                return self;
            }
        }

        let mut class = String::with_capacity(prefix.len() + decimal_digits(value));
        class.push_str(prefix);
        push_usize(&mut class, value);
        self.push_attr(Attribute {
            name: Cow::Borrowed("class"),
            value: AttributeValue::String(class),
        });
        self
    }

    /// Add an id attribute.
    #[must_use]
    pub fn id(self, id: &str) -> Self {
        self.set("id", id)
    }

    /// Check if a specific attribute is set.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.attrs.iter().any(|a| a.name.as_ref() == name)
    }

    /// Get the value of an attribute.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue> {
        self.attrs
            .iter()
            .find(|a| a.name.as_ref() == name)
            .map(|a| &a.value)
    }

    /// Write the attributes directly into `out`, avoiding the intermediate `String`
    /// that [`render`](Self::render) allocates per element on the hot serialization path.
    pub fn write_into<W: fmt::Write>(&self, out: &mut W) {
        // Write the constant pieces and the attribute name directly instead of through
        // `write!`/`format_args!`, which routes every piece and both `Display` args through
        // the `fmt::Formatter` machinery. Byte-identical to ` {name}="{value}"`; this is
        // the per-attribute hot path (~thousands of attributes per diagram).
        for attr in &self.attrs {
            let _ = out.write_char(' ');
            let _ = out.write_str(&attr.name);
            let _ = out.write_str("=\"");
            let _ = attr.value.write_value(out);
            let _ = out.write_char('"');
        }
    }

    /// Render attributes to a string.
    #[must_use]
    pub fn render(&self) -> String {
        let mut result = String::new();
        self.write_into(&mut result);
        result
    }

    /// Get the number of attributes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.attrs.len()
    }

    /// Check if the attribute collection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }

    /// Merge another attribute collection into this one.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        for attr in other.attrs {
            if attr.name.as_ref() == "class" {
                // Merge classes
                if let AttributeValue::String(class) = &attr.value {
                    self = self.class(class);
                }
            } else {
                self.push_attr(attr);
            }
        }
        self
    }
}

fn static_data_attr_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "fm-edge-id" => "data-fm-edge-id",
        "fm-node-id" => "data-fm-node-id",
        "id" => "data-id",
        _ => return None,
    })
}

fn push_usize(out: &mut String, value: usize) {
    if value >= 10 {
        push_usize(out, value / 10);
    }
    out.push(match value % 10 {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        _ => '9',
    });
}

const fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

/// Two-digit decimal strings `"00".."99"`, indexed by value: [`PAIRS2`]`[d]` is the 2-digit group
/// for `d` in `0..100`, and [`DIGITS1`]`[d]` is the single low digit for `d` in `0..10`.
///
/// This replaces the earlier single concatenated `&str` sliced at runtime as `&DIGIT_PAIRS[d*2..d*2+2]`.
/// Slicing a `&str` by a runtime `Range` goes through `str`'s `check_range`, which re-validates UTF-8
/// char boundaries on every call — pure waste for known-ASCII digits, and measured at ~7% of
/// coordinate-heavy render (`write_fixed2`/`write_uint_into` dominate). Indexing an array of
/// already-built `&'static str` does only a bounds check (which the compiler elides where the index is
/// provably `< 100`/`< 10`), no boundary re-validation — byte-identical output, still no `unsafe`
/// (`#![forbid(unsafe_code)]`).
const PAIRS2: [&str; 100] = [
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09",
    "10", "11", "12", "13", "14", "15", "16", "17", "18", "19",
    "20", "21", "22", "23", "24", "25", "26", "27", "28", "29",
    "30", "31", "32", "33", "34", "35", "36", "37", "38", "39",
    "40", "41", "42", "43", "44", "45", "46", "47", "48", "49",
    "50", "51", "52", "53", "54", "55", "56", "57", "58", "59",
    "60", "61", "62", "63", "64", "65", "66", "67", "68", "69",
    "70", "71", "72", "73", "74", "75", "76", "77", "78", "79",
    "80", "81", "82", "83", "84", "85", "86", "87", "88", "89",
    "90", "91", "92", "93", "94", "95", "96", "97", "98", "99",
];

/// Single decimal digit strings `"0".."9"` — see [`PAIRS2`].
const DIGITS1: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Dot-prefixed two-digit fractions `".00"..".99"`, indexed by the fractional value. Lets
/// [`write_fixed2`] emit the decimal point and both fraction digits in a *single* `write_str`
/// instead of `write_str(".")` + `write_str(PAIRS2[frac])` — halving the tiny append calls (each
/// a `fmt::Write` dispatch + capacity check + memcpy) on the hot 2-decimal coordinate path.
/// `DOTPAIRS3[f]` is byte-identical to `"." + PAIRS2[f]`.
const DOTPAIRS3: [&str; 100] = [
    ".00", ".01", ".02", ".03", ".04", ".05", ".06", ".07", ".08", ".09",
    ".10", ".11", ".12", ".13", ".14", ".15", ".16", ".17", ".18", ".19",
    ".20", ".21", ".22", ".23", ".24", ".25", ".26", ".27", ".28", ".29",
    ".30", ".31", ".32", ".33", ".34", ".35", ".36", ".37", ".38", ".39",
    ".40", ".41", ".42", ".43", ".44", ".45", ".46", ".47", ".48", ".49",
    ".50", ".51", ".52", ".53", ".54", ".55", ".56", ".57", ".58", ".59",
    ".60", ".61", ".62", ".63", ".64", ".65", ".66", ".67", ".68", ".69",
    ".70", ".71", ".72", ".73", ".74", ".75", ".76", ".77", ".78", ".79",
    ".80", ".81", ".82", ".83", ".84", ".85", ".86", ".87", ".88", ".89",
    ".90", ".91", ".92", ".93", ".94", ".95", ".96", ".97", ".98", ".99",
];

/// Append `n` in decimal (no leading zeros) to `f`, two digits at a time via [`PAIRS2`]/[`DIGITS1`].
fn write_uint_into<W: fmt::Write>(f: &mut W, n: u64) -> fmt::Result {
    if n >= 100 {
        write_uint_into(f, n / 100)?;
        f.write_str(PAIRS2[(n % 100) as usize])
    } else if n >= 10 {
        f.write_str(PAIRS2[n as usize])
    } else {
        f.write_str(DIGITS1[n as usize])
    }
}

/// Append signed integer `i` in decimal to `f` via the fast [`PAIRS2`] path — byte-identical
/// to `write!(f, "{i}")` but without the `fmt::Formatter`/`pad_integral` machinery, which shows up
/// as ~8% of coordinate-heavy render (most SVG coordinates land on whole pixels and take the integer
/// branch of [`AttributeValue::write_value`]). `i64::from(i).unsigned_abs()` handles `i32::MIN`.
pub(crate) fn write_int_into<W: fmt::Write>(f: &mut W, i: i32) -> fmt::Result {
    if i < 0 {
        f.write_str("-")?;
    }
    write_uint_into(f, i64::from(i).unsigned_abs())
}

/// Write `value` to exactly two decimal places, byte-for-byte identical to
/// `write!(f, "{value:.2}")`, but without the general float-to-decimal formatting
/// machinery — which dominates SVG serialization on coordinate-heavy diagrams.
///
/// Promoting to `f64` is lossless for an `f32`, so scaling by 100 and rounding
/// (ties to even, matching `{:.2}`) reproduces the exact decimal rounding of the
/// underlying `f32`. Values too large to scale into `i64`, and any non-finite input,
/// fall back to the standard formatter so output stays identical in every case.
/// The integer part and the always-2-digit fraction are streamed straight into `f` as
/// borrowed [`PAIRS2`] entries (no stack buffer, no `from_utf8`, no `str` range revalidation).
/// Verified byte-identical against `{:.2}` over a dense value sweep in the tests.
pub(crate) fn write_fixed2<W: fmt::Write>(f: &mut W, value: f32) -> fmt::Result {
    if !value.is_finite() || value.abs() >= 9.0e15 {
        // Non-finite, or large enough that `* 100` could overflow `i64`.
        return write!(f, "{value:.2}");
    }
    let scaled = (f64::from(value) * 100.0).round_ties_even();
    let magnitude = (scaled as i64).unsigned_abs();
    let int_part = magnitude / 100;
    let frac_part = (magnitude % 100) as usize;

    // `[-]int_part.frac_part`, frac always 2 digits, in the same order the old right-to-left
    // stack-buffer build produced — byte-identical, just streamed instead of buffered+revalidated.
    if value.is_sign_negative() {
        f.write_str("-")?;
    }
    write_uint_into(f, int_part)?;
    // `".00".."99"` in one append instead of `write_str(".")` + `write_str(PAIRS2[frac])`.
    f.write_str(DOTPAIRS3[frac_part])
}

/// Write `s` into `f` with XML attribute-value escaping (`& < > " '`), copying
/// unescaped runs in bulk instead of character-by-character. Every escaped
/// character is ASCII, so scanning bytes never splits a multi-byte UTF-8 sequence
/// — the output is byte-for-byte identical to escaping each `char` individually,
/// with no intermediate allocation. This is the hot SVG-serialization path.
pub(crate) fn write_escaped_attr<W: fmt::Write>(f: &mut W, s: &str) -> fmt::Result {
    let bytes = s.as_bytes();
    // Fast path: a single scan checking whether ANY byte needs escaping. This is a simple
    // "byte ∈ small set" reduction the auto-vectorizer can lower to SIMD, and the common
    // attribute values on the hot render path — path `d` geometry, numeric coords, class/id
    // tokens — contain no special byte, so we bulk-copy the whole string with one `write_str`.
    // Byte-identical: when no byte is special the slow loop below would also emit `s` verbatim.
    if !bytes
        .iter()
        .any(|&b| matches!(b, b'&' | b'<' | b'>' | b'"' | b'\''))
    {
        return f.write_str(s);
    }
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#39;",
            _ => continue,
        };
        f.write_str(&s[start..i])?;
        f.write_str(replacement)?;
        start = i + 1;
    }
    f.write_str(&s[start..])
}

/// Write `s` into `f` with XML text-content escaping. Like [`write_escaped_attr`]
/// but only escapes `>` when it closes a `]]>` sequence (so CSS child combinators
/// such as `div > p` survive inline `<style>`), matching the prior behaviour
/// exactly. `]` is ASCII, so the byte look-back matches a `char` look-back.
pub(crate) fn write_escaped_text<W: fmt::Write>(f: &mut W, s: &str) -> fmt::Result {
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' if i >= 2 && bytes[i - 1] == b']' && bytes[i - 2] == b']' => "&gt;",
            _ => continue,
        };
        f.write_str(&s[start..i])?;
        f.write_str(replacement)?;
        start = i + 1;
    }
    f.write_str(&s[start..])
}

/// Escape special characters in XML text content.
#[cfg(test)]
#[must_use]
fn escape_xml_text(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let _ = write_escaped_text(&mut result, s);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_escape_byte_identical_to_charwise() {
        // Reference: the original character-by-character implementations.
        fn ref_attr(s: &str) -> String {
            let mut r = String::new();
            for c in s.chars() {
                match c {
                    '&' => r.push_str("&amp;"),
                    '<' => r.push_str("&lt;"),
                    '>' => r.push_str("&gt;"),
                    '"' => r.push_str("&quot;"),
                    '\'' => r.push_str("&#39;"),
                    _ => r.push(c),
                }
            }
            r
        }
        fn ref_text(s: &str) -> String {
            let mut r = String::new();
            let (mut p1, mut p2) = ('\0', '\0');
            for c in s.chars() {
                match c {
                    '&' => r.push_str("&amp;"),
                    '<' => r.push_str("&lt;"),
                    '>' if p1 == ']' && p2 == ']' => r.push_str("&gt;"),
                    _ => r.push(c),
                }
                p2 = p1;
                p1 = c;
            }
            r
        }
        let cases = [
            "",
            "abc",
            "a&b",
            "<x>",
            "\"q\"",
            "'s'",
            "]]>",
            "a]]>b",
            "]>",
            "]]",
            "café ☕",
            "<&>\"'",
            "node A & B",
            "x]]>y]]>z",
            "<<<",
            "&amp;already",
            "a > b",
            "div > p",
            "]] >",
            "] ] >",
            "résumé < β & δ > \"τ\"",
            "🚀]]>🚀",
            "tail]]",
            "lead]]>",
        ];
        for s in cases {
            let mut got_attr = String::new();
            write_escaped_attr(&mut got_attr, s).unwrap();
            assert_eq!(got_attr, ref_attr(s), "attr mismatch for {s:?}");

            let mut got_text = String::new();
            write_escaped_text(&mut got_text, s).unwrap();
            assert_eq!(got_text, ref_text(s), "text mismatch for {s:?}");
            assert_eq!(
                escape_xml_text(s),
                ref_text(s),
                "escape_xml_text mismatch for {s:?}"
            );
        }
    }

    #[test]
    fn write_fixed2_byte_identical_to_std_format() {
        let check = |v: f32| {
            let mut got = String::new();
            write_fixed2(&mut got, v).unwrap();
            assert_eq!(
                got,
                format!("{v:.2}"),
                "mismatch for {v} (bits {:#010x})",
                v.to_bits()
            );
        };
        // Dense sweep across the realistic coordinate range (fine step, both signs).
        let mut i: i32 = -3_000_000;
        while i <= 3_000_000 {
            check(i as f32 / 1000.0);
            i += 1;
        }
        // Half-way / rounding-tie cases and larger magnitudes.
        for &v in &[
            0.005f32,
            -0.005,
            0.015,
            0.025,
            0.045,
            0.125,
            0.135,
            1.005,
            2.675,
            2.685,
            -2.675,
            -0.001,
            0.001,
            12345.67,
            -88888.88,
            99999.99,
            131071.99,
            262143.5,
            1.0e7 + 0.5,
        ] {
            check(v);
        }
    }

    #[test]
    fn renders_attributes() {
        let attrs = Attributes::new()
            .set("id", "test")
            .set("width", 100.0_f32)
            .set("height", 50_i32);
        let rendered = attrs.render();
        assert!(rendered.contains("id=\"test\""));
        assert!(rendered.contains("width=\"100\""));
        assert!(rendered.contains("height=\"50\""));
    }

    #[test]
    fn escapes_special_characters() {
        let attrs = Attributes::new().set("title", "A & B < C > D \"E\" 'F'");
        let rendered = attrs.render();
        assert!(rendered.contains("&amp;"));
        assert!(rendered.contains("&lt;"));
        assert!(rendered.contains("&gt;"));
        assert!(rendered.contains("&quot;"));
        assert!(rendered.contains("&#39;"));
    }

    #[test]
    fn escapes_cdata_terminator_sequence() {
        let escaped = escape_xml_text("literal ]]> should be safe");
        assert!(escaped.contains("]]&gt;"));
        assert!(!escaped.contains("]]>"));
    }

    #[test]
    fn merges_classes() {
        let attrs = Attributes::new().class("foo").class("bar").class("baz");
        let rendered = attrs.render();
        assert!(rendered.contains("class=\"foo bar baz\""));
    }

    #[test]
    fn appends_prefixed_classes_without_changing_serialization() {
        let attrs = Attributes::new()
            .class("fm-node")
            .class_prefixed_usize("fm-node-accent-", 7)
            .class("fm-node-shape-rect")
            .class_prefixed("fm-node-user-", "selected");
        assert_eq!(
            attrs.render(),
            " class=\"fm-node fm-node-accent-7 fm-node-shape-rect fm-node-user-selected\""
        );
    }

    #[test]
    fn starts_class_attribute_from_prefixed_class() {
        let attrs = Attributes::new()
            .class_prefixed("fm-node-icon-", "server")
            .class_prefixed_usize("fm-node-accent-", 12);
        assert_eq!(
            attrs.render(),
            " class=\"fm-node-icon-server fm-node-accent-12\""
        );
    }

    #[test]
    fn adds_data_attributes() {
        let attrs = Attributes::new()
            .data("id", "node-a")
            .data("fm-node-id", "node-a")
            .data("test", "value")
            .data("count", "5");
        let rendered = attrs.render();
        assert!(rendered.contains("data-id=\"node-a\""));
        assert!(rendered.contains("data-fm-node-id=\"node-a\""));
        assert!(rendered.contains("data-test=\"value\""));
        assert!(rendered.contains("data-count=\"5\""));
    }
}
