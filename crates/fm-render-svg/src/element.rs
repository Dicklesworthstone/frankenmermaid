//! SVG element primitives.
//!
//! Provides structs for creating SVG elements like rect, circle, path, etc.
//! with a fluent builder API.

use std::fmt::Write;

use crate::attributes::Attributes;

/// Types of SVG elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Rect,
    Circle,
    Ellipse,
    Line,
    Polyline,
    Polygon,
    Path,
    Text,
    Tspan,
    Group,
    Use,
    ClipPath,
    Marker,
    Defs,
    LinearGradient,
    RadialGradient,
    Stop,
    Filter,
    FeDropShadow,
    FeGaussianBlur,
    FeOffset,
    FeFlood,
    FeComposite,
    FeMerge,
    FeMergeNode,
}

impl ElementKind {
    /// Get the SVG tag name for this element kind.
    #[must_use]
    pub const fn tag_name(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Circle => "circle",
            Self::Ellipse => "ellipse",
            Self::Line => "line",
            Self::Polyline => "polyline",
            Self::Polygon => "polygon",
            Self::Path => "path",
            Self::Text => "text",
            Self::Tspan => "tspan",
            Self::Group => "g",
            Self::Use => "use",
            Self::ClipPath => "clipPath",
            Self::Marker => "marker",
            Self::Defs => "defs",
            Self::LinearGradient => "linearGradient",
            Self::RadialGradient => "radialGradient",
            Self::Stop => "stop",
            Self::Filter => "filter",
            Self::FeDropShadow => "feDropShadow",
            Self::FeGaussianBlur => "feGaussianBlur",
            Self::FeOffset => "feOffset",
            Self::FeFlood => "feFlood",
            Self::FeComposite => "feComposite",
            Self::FeMerge => "feMerge",
            Self::FeMergeNode => "feMergeNode",
        }
    }

    /// Check if this element is self-closing (has no children).
    #[must_use]
    pub const fn is_self_closing(self) -> bool {
        matches!(
            self,
            Self::Rect
                | Self::Circle
                | Self::Ellipse
                | Self::Line
                | Self::Polyline
                | Self::Polygon
                | Self::Path
                | Self::Use
                | Self::Stop
                | Self::FeDropShadow
                | Self::FeGaussianBlur
                | Self::FeOffset
                | Self::FeFlood
                | Self::FeMergeNode
        )
    }
}

/// An SVG element with attributes and optional children.
#[derive(Debug, Clone)]
pub struct Element {
    kind: ElementKind,
    attrs: Attributes,
    children: Vec<Element>,
    text_content: Option<String>,
}

impl Element {
    /// Create a new element of the given kind.
    #[must_use]
    pub fn new(kind: ElementKind) -> Self {
        Self {
            kind,
            attrs: Attributes::new(),
            children: Vec::new(),
            text_content: None,
        }
    }

    // Factory methods for common elements

    /// Create a rect element.
    #[must_use]
    pub fn rect() -> Self {
        Self::new(ElementKind::Rect)
    }

    /// Create a circle element.
    #[must_use]
    pub fn circle() -> Self {
        Self::new(ElementKind::Circle)
    }

    /// Create an ellipse element.
    #[must_use]
    pub fn ellipse() -> Self {
        Self::new(ElementKind::Ellipse)
    }

    /// Create a line element.
    #[must_use]
    pub fn line() -> Self {
        Self::new(ElementKind::Line)
    }

    /// Create a polyline element.
    #[must_use]
    pub fn polyline() -> Self {
        Self::new(ElementKind::Polyline)
    }

    /// Create a polygon element.
    #[must_use]
    pub fn polygon() -> Self {
        Self::new(ElementKind::Polygon)
    }

    /// Create a path element.
    #[must_use]
    pub fn path() -> Self {
        Self::new(ElementKind::Path)
    }

    /// Create a text element.
    #[must_use]
    pub fn text() -> Self {
        Self::new(ElementKind::Text)
    }

    /// Create a tspan element.
    #[must_use]
    pub fn tspan() -> Self {
        Self::new(ElementKind::Tspan)
    }

    /// Create a group element.
    #[must_use]
    pub fn group() -> Self {
        Self::new(ElementKind::Group)
    }

    /// Create a use element.
    #[must_use]
    pub fn use_elem() -> Self {
        Self::new(ElementKind::Use)
    }

    /// Create a clipPath element.
    #[must_use]
    pub fn clip_path() -> Self {
        Self::new(ElementKind::ClipPath)
    }

    /// Create a marker element.
    #[must_use]
    pub fn marker() -> Self {
        Self::new(ElementKind::Marker)
    }

    // Common attribute setters

    /// Set the x attribute.
    #[must_use]
    pub fn x(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("x", value);
        self
    }

    /// Set the y attribute.
    #[must_use]
    pub fn y(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("y", value);
        self
    }

    /// Set the width attribute.
    #[must_use]
    pub fn width(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("width", value);
        self
    }

    /// Set the height attribute.
    #[must_use]
    pub fn height(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("height", value);
        self
    }

    /// Set the rx (corner radius x) attribute.
    #[must_use]
    pub fn rx(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("rx", value);
        self
    }

    /// Set the ry (corner radius y) attribute.
    #[must_use]
    pub fn ry(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("ry", value);
        self
    }

    /// Set the cx (center x) attribute.
    #[must_use]
    pub fn cx(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("cx", value);
        self
    }

    /// Set the cy (center y) attribute.
    #[must_use]
    pub fn cy(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("cy", value);
        self
    }

    /// Set the r (radius) attribute.
    #[must_use]
    pub fn r(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("r", value);
        self
    }

    /// Set x1 attribute (for line).
    #[must_use]
    pub fn x1(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("x1", value);
        self
    }

    /// Set y1 attribute (for line).
    #[must_use]
    pub fn y1(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("y1", value);
        self
    }

    /// Set x2 attribute (for line).
    #[must_use]
    pub fn x2(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("x2", value);
        self
    }

    /// Set y2 attribute (for line).
    #[must_use]
    pub fn y2(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("y2", value);
        self
    }

    /// Set the d (path data) attribute.
    #[must_use]
    pub fn d(mut self, path: &str) -> Self {
        self.attrs = self.attrs.str("d", path);
        self
    }

    /// Set the points attribute (for polyline/polygon).
    #[must_use]
    pub fn points(mut self, pts: &str) -> Self {
        self.attrs = self.attrs.str("points", pts);
        self
    }

    /// Set the fill attribute.
    #[must_use]
    pub fn fill(mut self, color: &str) -> Self {
        self.attrs = self.attrs.str("fill", color);
        self
    }

    /// Set the stroke attribute.
    #[must_use]
    pub fn stroke(mut self, color: &str) -> Self {
        self.attrs = self.attrs.str("stroke", color);
        self
    }

    /// Set the stroke-width attribute.
    #[must_use]
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.attrs = self.attrs.num("stroke-width", width);
        self
    }

    /// Set the stroke-dasharray attribute.
    #[must_use]
    pub fn stroke_dasharray(mut self, pattern: &str) -> Self {
        self.attrs = self.attrs.str("stroke-dasharray", pattern);
        self
    }

    /// Set the stroke-linecap attribute.
    #[must_use]
    pub fn stroke_linecap(mut self, cap: &str) -> Self {
        self.attrs = self.attrs.str("stroke-linecap", cap);
        self
    }

    /// Set the stroke-linejoin attribute.
    #[must_use]
    pub fn stroke_linejoin(mut self, join: &str) -> Self {
        self.attrs = self.attrs.str("stroke-linejoin", join);
        self
    }

    /// Set the opacity attribute.
    #[must_use]
    pub fn opacity(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("opacity", value);
        self
    }

    /// Set the fill-opacity attribute.
    #[must_use]
    pub fn fill_opacity(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("fill-opacity", value);
        self
    }

    /// Set the stroke-opacity attribute.
    #[must_use]
    pub fn stroke_opacity(mut self, value: f32) -> Self {
        self.attrs = self.attrs.num("stroke-opacity", value);
        self
    }

    /// Set the transform attribute.
    #[must_use]
    pub fn transform(mut self, value: &str) -> Self {
        self.attrs = self.attrs.str("transform", value);
        self
    }

    /// Set the filter attribute.
    #[must_use]
    pub fn filter(mut self, value: &str) -> Self {
        self.attrs = self.attrs.str("filter", value);
        self
    }

    /// Set the clip-path attribute.
    #[must_use]
    pub fn clip_path_ref(mut self, value: &str) -> Self {
        self.attrs = self.attrs.str("clip-path", value);
        self
    }

    /// Set the marker-start attribute.
    #[must_use]
    pub fn marker_start(mut self, value: &str) -> Self {
        self.attrs = self.attrs.str("marker-start", value);
        self
    }

    /// Set the marker-mid attribute.
    #[must_use]
    pub fn marker_mid(mut self, value: &str) -> Self {
        self.attrs = self.attrs.str("marker-mid", value);
        self
    }

    /// Set the marker-end attribute.
    #[must_use]
    pub fn marker_end(mut self, value: &str) -> Self {
        self.attrs = self.attrs.str("marker-end", value);
        self
    }

    /// Set the id attribute.
    #[must_use]
    pub fn id(mut self, id: &str) -> Self {
        self.attrs = self.attrs.id(id);
        self
    }

    /// Add a CSS class.
    #[must_use]
    pub fn class(mut self, class: &str) -> Self {
        self.attrs = self.attrs.class(class);
        self
    }

    /// Set a data-* attribute.
    #[must_use]
    pub fn data(mut self, name: &str, value: &str) -> Self {
        self.attrs = self.attrs.data(name, value);
        self
    }

    /// Set a custom attribute.
    #[must_use]
    pub fn attr(mut self, name: &str, value: &str) -> Self {
        self.attrs = self.attrs.str(name, value);
        self
    }

    /// Set a custom numeric attribute.
    #[must_use]
    pub fn attr_num(mut self, name: &str, value: f32) -> Self {
        self.attrs = self.attrs.num(name, value);
        self
    }

    /// Set text content for text elements.
    #[must_use]
    pub fn content(mut self, text: impl Into<String>) -> Self {
        self.text_content = Some(text.into());
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

    /// Get the element kind.
    #[must_use]
    pub const fn kind(&self) -> ElementKind {
        self.kind
    }

    /// Render the element to a string.
    #[must_use]
    pub fn render(&self) -> String {
        let mut output = String::with_capacity(256);
        self.write_to_string(&mut output);
        output
    }

    /// Write the element to a string.
    pub fn write_to_string(&self, output: &mut String) {
        let tag = self.kind.tag_name();
        let _ = write!(output, "<{tag}");
        output.push_str(&self.attrs.render());

        if self.kind.is_self_closing() && self.children.is_empty() && self.text_content.is_none() {
            output.push_str("/>");
        } else {
            output.push('>');

            if let Some(ref text) = self.text_content {
                output.push_str(&crate::attributes::escape_xml_text(text));
            }

            for child in &self.children {
                child.write_to_string(output);
            }

            let _ = write!(output, "</{tag}>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_rect() {
        let elem = Element::rect()
            .x(10.0)
            .y(20.0)
            .width(100.0)
            .height(50.0)
            .fill("#fff")
            .stroke("#000");
        let svg = elem.render();
        assert!(svg.contains("<rect"));
        assert!(svg.contains("x=\"10\""));
        assert!(svg.contains("y=\"20\""));
        assert!(svg.contains("width=\"100\""));
        assert!(svg.contains("height=\"50\""));
        assert!(svg.contains("fill=\"#fff\""));
        assert!(svg.contains("stroke=\"#000\""));
        assert!(svg.ends_with("/>"));
    }

    #[test]
    fn creates_circle() {
        let elem = Element::circle().cx(50.0).cy(50.0).r(25.0).fill("red");
        let svg = elem.render();
        assert!(svg.contains("<circle"));
        assert!(svg.contains("cx=\"50\""));
        assert!(svg.contains("cy=\"50\""));
        assert!(svg.contains("r=\"25\""));
    }

    #[test]
    fn creates_path() {
        let elem = Element::path()
            .d("M 0 0 L 100 100")
            .fill("none")
            .stroke("blue");
        let svg = elem.render();
        assert!(svg.contains("<path"));
        assert!(svg.contains("d=\"M 0 0 L 100 100\""));
    }

    #[test]
    fn creates_line() {
        let elem = Element::line()
            .x1(0.0)
            .y1(0.0)
            .x2(100.0)
            .y2(100.0)
            .stroke("#333")
            .stroke_width(2.0);
        let svg = elem.render();
        assert!(svg.contains("<line"));
        assert!(svg.contains("x1=\"0\""));
        assert!(svg.contains("y1=\"0\""));
        assert!(svg.contains("x2=\"100\""));
        assert!(svg.contains("y2=\"100\""));
    }

    #[test]
    fn creates_group_with_children() {
        let elem = Element::group()
            .class("node")
            .child(Element::rect().x(0.0).y(0.0).width(50.0).height(50.0))
            .child(Element::circle().cx(25.0).cy(25.0).r(10.0));
        let svg = elem.render();
        assert!(svg.contains("<g"));
        assert!(svg.contains("class=\"node\""));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<circle"));
        assert!(svg.ends_with("</g>"));
    }

    #[test]
    fn creates_text_with_content() {
        let elem = Element::text().x(50.0).y(50.0).content("Hello & World");
        let svg = elem.render();
        assert!(svg.contains("<text"));
        assert!(svg.contains("Hello &amp; World"));
        assert!(svg.ends_with("</text>"));
    }
}
