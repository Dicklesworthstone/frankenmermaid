//! SVG document root element.
//!
//! Provides the `SvgDocument` struct for building complete SVG documents
//! with proper namespace, viewBox, and accessibility support.

use std::fmt::{self, Write};
use std::io;

use crate::attributes::{Attributes, escape_xml_text};
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
        self.attrs = self.attrs.set(name, value.into());
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
        output.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");

        // Add viewBox
        if let Some((x, y, w, h)) = self.viewbox {
            let _ = write!(output, " viewBox=\"{x} {y} {w} {h}\"");
        }

        // Add width/height
        if let Some(ref w) = self.width {
            let _ = write!(output, " width=\"{}\"", escape_xml_attr_value(w));
        }
        if let Some(ref h) = self.height {
            let _ = write!(output, " height=\"{}\"", escape_xml_attr_value(h));
        }

        // Add other attributes
        output.push_str(&self.attrs.render());

        output.push('>');

        // Add title for accessibility
        if let Some(ref title) = self.title {
            let _ = write!(output, "<title>{}</title>", escape_xml_text(title));
        }

        // Add description for accessibility
        if let Some(ref desc) = self.desc {
            let _ = write!(output, "<desc>{}</desc>", escape_xml_text(desc));
        }

        // Add inline style
        if let Some(ref css) = self.style {
            let _ = write!(output, "<style>{}</style>", escape_xml_text(css));
        }

        // Add defs section
        if let Some(ref defs) = self.defs {
            defs.write_to_string(output);
        }

        // Add children
        for child in &self.children {
            child.write_to_string(output);
        }

        output.push_str("</svg>");
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
        let mut output = String::with_capacity(4096);
        self.write_to_string(&mut output);
        f.write_str(&output)
    }
}

fn escape_xml_attr_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
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
