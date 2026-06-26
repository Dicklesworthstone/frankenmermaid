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
                // Format with reasonable precision, trim trailing zeros.
                // Use integer formatting only for values that fit in i32 range
                // to avoid truncation overflow on extreme coordinates.
                if n.fract() == 0.0
                    && n.is_finite()
                    && *n >= i32::MIN as f32
                    && *n <= i32::MAX as f32
                {
                    write!(out, "{}", *n as i32)
                } else {
                    write_fixed2(out, *n)
                }
            }
            Self::Integer(i) => write!(out, "{i}"),
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
    /// Pre-sized to a typical element's attribute count (rect/text/path carry ~8–12
    /// attributes), so building an element's attribute list does not repeatedly realloc
    /// and copy as setters push — element construction dominates SVG render time.
    #[must_use]
    pub fn new() -> Self {
        Self {
            attrs: Vec::with_capacity(12),
        }
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
        self.attrs.push(Attribute {
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
        self.attrs.push(Attribute {
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
        self.attrs.push(Attribute {
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
                self.attrs.push(attr);
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

/// Write `value` to exactly two decimal places, byte-for-byte identical to
/// `write!(f, "{value:.2}")`, but without the general float-to-decimal formatting
/// machinery — which dominates SVG serialization on coordinate-heavy diagrams.
///
/// Promoting to `f64` is lossless for an `f32`, so scaling by 100 and rounding
/// (ties to even, matching `{:.2}`) reproduces the exact decimal rounding of the
/// underlying `f32`. Values too large to scale into `i64`, and any non-finite input,
/// fall back to the standard formatter so output stays identical in every case.
/// Verified byte-identical against `{:.2}` over a dense value sweep in the tests.
pub(crate) fn write_fixed2<W: fmt::Write>(f: &mut W, value: f32) -> fmt::Result {
    if !value.is_finite() || value.abs() >= 9.0e15 {
        // Non-finite, or large enough that `* 100` could overflow `i64`.
        return write!(f, "{value:.2}");
    }
    let scaled = (f64::from(value) * 100.0).round_ties_even();
    let magnitude = (scaled as i64).unsigned_abs();
    let int_part = magnitude / 100;
    let frac_part = magnitude % 100;
    if value.is_sign_negative() {
        f.write_char('-')?;
    }
    write!(f, "{int_part}.{frac_part:02}")
}

/// Write `s` into `f` with XML attribute-value escaping (`& < > " '`), copying
/// unescaped runs in bulk instead of character-by-character. Every escaped
/// character is ASCII, so scanning bytes never splits a multi-byte UTF-8 sequence
/// — the output is byte-for-byte identical to escaping each `char` individually,
/// with no intermediate allocation. This is the hot SVG-serialization path.
pub(crate) fn write_escaped_attr<W: fmt::Write>(f: &mut W, s: &str) -> fmt::Result {
    let bytes = s.as_bytes();
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
#[must_use]
pub fn escape_xml_text(s: &str) -> String {
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
