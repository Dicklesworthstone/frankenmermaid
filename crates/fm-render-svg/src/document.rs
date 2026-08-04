//! SVG document root element.
//!
//! Provides the `SvgDocument` struct for building complete SVG documents
//! with proper namespace, viewBox, and accessibility support.

use std::fmt::{self, Write};
use std::io;

use crate::attributes::{Attributes, write_escaped_attr, write_escaped_text};
use crate::defs::DefsBuilder;
use crate::element::Element;

/// SVG document builder.
#[derive(Debug, Clone)]
pub struct SvgDocument {
    attrs: Attributes,
    viewbox: Option<(f32, f32, f32, f32)>,
    width: Option<String>,
    height: Option<String>,
    title: Option<String>,
    desc: Option<String>,
    defs: Option<DefsBuilder>,
    children: Vec<Element>,
    style: Option<String>,
}

impl SvgDocument {
    /// Create a new SVG document.
    #[must_use]
    pub fn new() -> Self {
        Self {
            attrs: Attributes::new(),
            viewbox: None,
            width: None,
            height: None,
            title: None,
            desc: None,
            defs: None,
            children: Vec::new(),
            style: None,
        }
    }

    /// Set the viewBox attribute.
    #[must_use]
    pub fn viewbox(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.viewbox = Some((x, y, width, height));
        self
    }

    /// Set `font-family` on the root `<svg>`. `font-family` is inherited, so every descendant
    /// `<text>` picks it up — letting the per-label inline `font-family` (a long ~90-byte string)
    /// be dropped when the theme CSS is embedded.
    #[must_use]
    pub fn font_family(mut self, family: &str) -> Self {
        self.attrs = self.attrs.set("font-family", family);
        self
    }

    /// Set explicit width.
    #[must_use]
    pub fn width(mut self, w: &str) -> Self {
        self.width = Some(w.to_string());
        self
    }

    /// Set explicit height.
    #[must_use]
    pub fn height(mut self, h: &str) -> Self {
        self.height = Some(h.to_string());
        self
    }

    /// Make the SVG responsive (width/height set to 100%).
    #[must_use]
    pub fn responsive(mut self) -> Self {
        self.width = Some(String::from("100%"));
        self.height = Some(String::from("100%"));
        self
    }

    /// Set preserveAspectRatio attribute.
    #[must_use]
    pub fn preserve_aspect_ratio(mut self, value: &str) -> Self {
        self.attrs = self.attrs.set("preserveAspectRatio", value);
        self
    }

    /// Add accessibility attributes (title and description).
    #[must_use]
    pub fn accessible(mut self, title: impl Into<String>, desc: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self.desc = Some(desc.into());
        self.attrs = self.attrs.set("role", "img");
        self
    }

    /// Add a CSS class to the root SVG element.
    #[must_use]
    pub fn class(mut self, class: &str) -> Self {
        self.attrs = self.attrs.class(class);
        self
    }

    /// Add an id to the root SVG element.
    #[must_use]
    pub fn id(mut self, id: &str) -> Self {
        self.attrs = self.attrs.id(id);
        self
    }

    /// Add a data-* attribute.
    #[must_use]
    pub fn data(mut self, name: &str, value: &str) -> Self {
        self.attrs = self.attrs.data(name, value);
        self
    }

    /// Set a custom attribute.
    #[must_use]
    pub fn attr<V: Into<String>>(mut self, name: &str, value: V) -> Self {
        self.attrs = self.attrs.set(name.to_string(), value.into());
        self
    }

    /// Add a defs section.
    #[must_use]
    pub fn defs(mut self, defs: DefsBuilder) -> Self {
        self.defs = Some(defs);
        self
    }

    /// Add inline CSS styles.
    #[must_use]
    pub fn style(mut self, css: impl Into<String>) -> Self {
        self.style = Some(css.into());
        self
    }

    /// Add a child element.
    #[must_use]
    pub fn child(mut self, elem: Element) -> Self {
        self.children.push(elem);
        self
    }

    /// Add multiple child elements.
    #[must_use]
    pub fn children<I: IntoIterator<Item = Element>>(mut self, elems: I) -> Self {
        self.children.extend(elems);
        self
    }

    /// Write the SVG document to a string.
    pub fn write_to_string(&self, output: &mut String) {
        self.write_prelude(output);
        output.push_str("</svg>");
    }

    /// Serialize everything up to (but not including) the closing `</svg>`: the open tag with all
    /// root attributes, `<title>`/`<desc>`/`<style>`/`<defs>`, and every child in order. Split out
    /// of [`write_to_string`] so a caller can stream extra body content (the node/edge fragments)
    /// straight into the final buffer at the child position instead of materializing them as
    /// intermediate `String`s and copying them a second time. See [`to_string_with_body`].
    fn write_prelude(&self, output: &mut String) {
        output.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");

        // Add viewBox (guard against NaN/Infinity producing invalid SVG)
        if let Some((x, y, w, h)) = self.viewbox
            && x.is_finite()
            && y.is_finite()
            && w.is_finite()
            && h.is_finite()
        {
            let _ = write!(output, " viewBox=\"{x} {y} {w} {h}\"");
        }

        // Add width/height
        if let Some(ref w) = self.width {
            output.push_str(" width=\"");
            let _ = write_escaped_attr(output, w);
            output.push('"');
        }
        if let Some(ref h) = self.height {
            output.push_str(" height=\"");
            let _ = write_escaped_attr(output, h);
            output.push('"');
        }

        // Add other attributes
        output.push_str(&self.attrs.render());

        output.push('>');

        // Add title for accessibility
        if let Some(ref title) = self.title {
            output.push_str("<title>");
            let _ = write_escaped_text(output, title);
            output.push_str("</title>");
        }

        // Add description for accessibility
        if let Some(ref desc) = self.desc {
            output.push_str("<desc>");
            let _ = write_escaped_text(output, desc);
            output.push_str("</desc>");
        }

        // Add inline style
        if let Some(ref css) = self.style {
            output.push_str("<style>");
            let _ = write_escaped_text(output, css);
            output.push_str("</style>");
        }

        // Add defs section
        if let Some(ref defs) = self.defs {
            defs.write_to_string(output);
        }

        // Add children
        for child in &self.children {
            child.write_to_string(output);
        }
    }

    /// Render the SVG document into a string with caller-provided capacity.
    ///
    /// Large diagrams are dominated by the final contiguous SVG buffer. Letting
    /// layout/render callers provide a size hint avoids repeated growth copies
    /// while preserving the exact same serialization path.
    #[must_use]
    pub fn to_string_with_capacity(&self, capacity: usize) -> String {
        let mut output = String::with_capacity(capacity.max(4096));
        self.write_to_string(&mut output);
        output
    }

    /// Serialize the prelude (open tag + meta + defs + this document's children), then invoke
    /// `body` to stream additional content directly into the final buffer at the child position,
    /// then close with `</svg>`. Byte-identical to appending `body`'s content as the trailing
    /// children — but without materializing that content as an intermediate `String` and copying
    /// it in a second time. The render fast path uses this to write the node/edge fragments
    /// straight into the output.
    #[must_use]
    pub fn to_string_with_body(&self, capacity: usize, body: impl FnOnce(&mut String)) -> String {
        let mut output = String::with_capacity(capacity.max(4096));
        self.write_prelude(&mut output);
        body(&mut output);
        output.push_str("</svg>");
        output
    }

    /// Write the SVG document to an io::Write implementor.
    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let svg = self.to_string();
        writer.write_all(svg.as_bytes())
    }
}

impl Default for SvgDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SvgDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_with_capacity(4096))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_basic_svg() {
        let doc = SvgDocument::new().viewbox(0.0, 0.0, 100.0, 100.0);
        let svg = doc.to_string();
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("viewBox=\"0 0 100 100\""));
    }

    #[test]
    fn capacity_hint_preserves_serialization() {
        let doc = SvgDocument::new()
            .viewbox(0.0, 0.0, 100.0, 100.0)
            .responsive()
            .accessible("Title", "Description")
            .child(Element::rect().x(1.0).y(2.0).width(3.0).height(4.0));

        assert_eq!(doc.to_string(), doc.to_string_with_capacity(64 * 1024));
    }

    #[test]
    fn creates_responsive_svg() {
        let doc = SvgDocument::new().responsive();
        let svg = doc.to_string();
        assert!(svg.contains("width=\"100%\""));
        assert!(svg.contains("height=\"100%\""));
    }

    #[test]
    fn adds_accessibility() {
        let doc = SvgDocument::new().accessible("My Title", "My Description");
        let svg = doc.to_string();
        assert!(svg.contains("role=\"img\""));
        assert!(svg.contains("<title>My Title</title>"));
        assert!(svg.contains("<desc>My Description</desc>"));
    }

    #[test]
    fn adds_data_attributes() {
        let doc = SvgDocument::new()
            .data("type", "flowchart")
            .data("nodes", "5");
        let svg = doc.to_string();
        assert!(svg.contains("data-type=\"flowchart\""));
        assert!(svg.contains("data-nodes=\"5\""));
    }

    #[test]
    fn escapes_title_and_desc() {
        let doc = SvgDocument::new().accessible("A & B", "X < Y > Z");
        let svg = doc.to_string();
        assert!(svg.contains("<title>A &amp; B</title>"));
        assert!(svg.contains("<desc>X &lt; Y > Z</desc>"));
    }

    #[test]
    fn escapes_width_and_height_attributes() {
        let doc = SvgDocument::new()
            .width("100\" onload=\"alert(1)")
            .height("200&300");
        let svg = doc.to_string();
        assert!(svg.contains("width=\"100&quot; onload=&quot;alert(1)\""));
        assert!(svg.contains("height=\"200&amp;300\""));
    }

    #[test]
    fn escapes_inline_style_content() {
        let doc = SvgDocument::new().style("g{fill:red;} </style><script>alert(1)</script>");
        let svg = doc.to_string();
        assert!(
            svg.contains("<style>g{fill:red;} &lt;/style>&lt;script>alert(1)&lt;/script></style>")
        );
        assert!(!svg.contains("</style><script>"));
    }
}
