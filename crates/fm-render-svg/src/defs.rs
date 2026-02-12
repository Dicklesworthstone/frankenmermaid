//! SVG defs section: markers, gradients, and filters.
//!
//! Provides builders for creating reusable definitions like arrowhead markers,
//! linear/radial gradients, and filter effects (drop shadow, blur).

use crate::element::Element;
use crate::path::PathBuilder;

/// Marker orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerOrient {
    /// Auto-orient to match the path direction.
    Auto,
    /// Auto-orient, but start rotated 180 degrees.
    AutoStartReverse,
    /// Fixed angle in degrees.
    Angle(f32),
}

impl MarkerOrient {
    /// Get the SVG attribute value.
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::Auto => String::from("auto"),
            Self::AutoStartReverse => String::from("auto-start-reverse"),
            Self::Angle(angle) => format!("{angle}"),
        }
    }
}

/// Arrowhead marker definition.
#[derive(Debug, Clone)]
pub struct ArrowheadMarker {
    /// Unique identifier for this marker.
    pub id: String,
    /// Width of the marker viewport.
    pub marker_width: f32,
    /// Height of the marker viewport.
    pub marker_height: f32,
    /// X coordinate of the reference point.
    pub ref_x: f32,
    /// Y coordinate of the reference point.
    pub ref_y: f32,
    /// Orientation of the marker.
    pub orient: MarkerOrient,
    /// The path data for the arrowhead shape.
    pub path: String,
    /// Fill color.
    pub fill: String,
    /// Optional stroke color.
    pub stroke: Option<String>,
    /// Optional stroke width.
    pub stroke_width: Option<f32>,
}

impl ArrowheadMarker {
    /// Create a standard filled arrowhead (triangle pointing right).
    #[must_use]
    pub fn standard(id: &str, fill: &str) -> Self {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .line_to(10.0, 5.0)
            .line_to(0.0, 10.0)
            .close()
            .build();

        Self {
            id: id.to_string(),
            marker_width: 10.0,
            marker_height: 10.0,
            ref_x: 10.0,
            ref_y: 5.0,
            orient: MarkerOrient::Auto,
            path,
            fill: fill.to_string(),
            stroke: None,
            stroke_width: None,
        }
    }

    /// Create a filled arrowhead (solid triangle).
    #[must_use]
    pub fn filled(id: &str, fill: &str) -> Self {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .line_to(12.0, 6.0)
            .line_to(0.0, 12.0)
            .line_to(3.0, 6.0)
            .close()
            .build();

        Self {
            id: id.to_string(),
            marker_width: 12.0,
            marker_height: 12.0,
            ref_x: 12.0,
            ref_y: 6.0,
            orient: MarkerOrient::Auto,
            path,
            fill: fill.to_string(),
            stroke: None,
            stroke_width: None,
        }
    }

    /// Create an open arrowhead (V shape).
    #[must_use]
    pub fn open(id: &str, stroke: &str) -> Self {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .line_to(10.0, 5.0)
            .line_to(0.0, 10.0)
            .build();

        Self {
            id: id.to_string(),
            marker_width: 10.0,
            marker_height: 10.0,
            ref_x: 10.0,
            ref_y: 5.0,
            orient: MarkerOrient::Auto,
            path,
            fill: String::from("none"),
            stroke: Some(stroke.to_string()),
            stroke_width: Some(1.5),
        }
    }

    /// Create a circle marker.
    #[must_use]
    pub fn circle_marker(id: &str, fill: &str) -> Self {
        // Use a path to approximate a circle
        let path = PathBuilder::new()
            .move_to(8.0, 4.0)
            .arc_to(4.0, 4.0, 0.0, true, true, 8.0, 4.0)
            .close()
            .build();

        Self {
            id: id.to_string(),
            marker_width: 8.0,
            marker_height: 8.0,
            ref_x: 4.0,
            ref_y: 4.0,
            orient: MarkerOrient::Auto,
            path,
            fill: fill.to_string(),
            stroke: None,
            stroke_width: None,
        }
    }

    /// Create a cross (X) marker.
    #[must_use]
    pub fn cross_marker(id: &str, stroke: &str) -> Self {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .line_to(8.0, 8.0)
            .move_to(8.0, 0.0)
            .line_to(0.0, 8.0)
            .build();

        Self {
            id: id.to_string(),
            marker_width: 8.0,
            marker_height: 8.0,
            ref_x: 4.0,
            ref_y: 4.0,
            orient: MarkerOrient::Auto,
            path,
            fill: String::from("none"),
            stroke: Some(stroke.to_string()),
            stroke_width: Some(1.5),
        }
    }

    /// Render the marker to an SVG element.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut marker = Element::marker()
            .id(&self.id)
            .attr_num("markerWidth", self.marker_width)
            .attr_num("markerHeight", self.marker_height)
            .attr_num("refX", self.ref_x)
            .attr_num("refY", self.ref_y)
            .attr("orient", &self.orient.as_str())
            .attr("markerUnits", "strokeWidth");

        let mut path_elem = Element::path().d(&self.path).fill(&self.fill);

        if let Some(ref stroke) = self.stroke {
            path_elem = path_elem.stroke(stroke);
        }
        if let Some(width) = self.stroke_width {
            path_elem = path_elem.stroke_width(width);
        }

        marker = marker.child(path_elem);
        marker
    }
}

/// A gradient stop.
#[derive(Debug, Clone)]
pub struct GradientStop {
    /// Offset (0.0 to 1.0 or 0% to 100%).
    pub offset: f32,
    /// Stop color.
    pub color: String,
    /// Opacity (0.0 to 1.0).
    pub opacity: Option<f32>,
}

impl GradientStop {
    /// Create a new gradient stop.
    #[must_use]
    pub fn new(offset: f32, color: &str) -> Self {
        Self {
            offset,
            color: color.to_string(),
            opacity: None,
        }
    }

    /// Create a new gradient stop with opacity.
    #[must_use]
    pub fn with_opacity(offset: f32, color: &str, opacity: f32) -> Self {
        Self {
            offset,
            color: color.to_string(),
            opacity: Some(opacity),
        }
    }

    /// Render to an SVG stop element.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut elem = Element::new(crate::element::ElementKind::Stop)
            .attr("offset", &format!("{}%", (self.offset * 100.0) as i32))
            .attr("stop-color", &self.color);

        if let Some(opacity) = self.opacity {
            elem = elem.attr_num("stop-opacity", opacity);
        }

        elem
    }
}

/// Gradient definition.
#[derive(Debug, Clone)]
pub struct Gradient {
    /// Unique identifier.
    pub id: String,
    /// Whether this is a linear or radial gradient.
    pub kind: GradientKind,
    /// Gradient stops.
    pub stops: Vec<GradientStop>,
}

/// Kind of gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind {
    /// Linear gradient (x1, y1 -> x2, y2).
    Linear { x1: f32, y1: f32, x2: f32, y2: f32 },
    /// Radial gradient (cx, cy with radius r).
    Radial { cx: f32, cy: f32, r: f32 },
}

impl Gradient {
    /// Create a new linear gradient (horizontal by default).
    #[must_use]
    pub fn linear(id: &str, stops: Vec<GradientStop>) -> Self {
        Self {
            id: id.to_string(),
            kind: GradientKind::Linear {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0,
            },
            stops,
        }
    }

    /// Create a new linear gradient with custom direction.
    #[must_use]
    pub fn linear_with_coords(
        id: &str,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stops: Vec<GradientStop>,
    ) -> Self {
        Self {
            id: id.to_string(),
            kind: GradientKind::Linear { x1, y1, x2, y2 },
            stops,
        }
    }

    /// Create a new radial gradient.
    #[must_use]
    pub fn radial(id: &str, cx: f32, cy: f32, r: f32, stops: Vec<GradientStop>) -> Self {
        Self {
            id: id.to_string(),
            kind: GradientKind::Radial { cx, cy, r },
            stops,
        }
    }

    /// Render to an SVG element.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut elem = match self.kind {
            GradientKind::Linear { x1, y1, x2, y2 } => {
                Element::new(crate::element::ElementKind::LinearGradient)
                    .id(&self.id)
                    .attr("x1", &format!("{}%", (x1 * 100.0) as i32))
                    .attr("y1", &format!("{}%", (y1 * 100.0) as i32))
                    .attr("x2", &format!("{}%", (x2 * 100.0) as i32))
                    .attr("y2", &format!("{}%", (y2 * 100.0) as i32))
            }
            GradientKind::Radial { cx, cy, r } => {
                Element::new(crate::element::ElementKind::RadialGradient)
                    .id(&self.id)
                    .attr("cx", &format!("{}%", (cx * 100.0) as i32))
                    .attr("cy", &format!("{}%", (cy * 100.0) as i32))
                    .attr("r", &format!("{}%", (r * 100.0) as i32))
            }
        };

        for stop in &self.stops {
            elem = elem.child(stop.to_element());
        }

        elem
    }
}

/// Filter definition.
#[derive(Debug, Clone)]
pub struct Filter {
    /// Unique identifier.
    pub id: String,
    /// Filter kind.
    pub kind: FilterKind,
}

/// Kind of filter effect.
#[derive(Debug, Clone)]
pub enum FilterKind {
    /// Drop shadow effect.
    DropShadow {
        dx: f32,
        dy: f32,
        std_dev: f32,
        opacity: f32,
    },
    /// Gaussian blur effect.
    GaussianBlur { std_dev: f32 },
}

impl Filter {
    /// Create a drop shadow filter.
    #[must_use]
    pub fn drop_shadow(id: &str, dx: f32, dy: f32, std_dev: f32, opacity: f32) -> Self {
        Self {
            id: id.to_string(),
            kind: FilterKind::DropShadow {
                dx,
                dy,
                std_dev,
                opacity,
            },
        }
    }

    /// Create a Gaussian blur filter.
    #[must_use]
    pub fn blur(id: &str, std_dev: f32) -> Self {
        Self {
            id: id.to_string(),
            kind: FilterKind::GaussianBlur { std_dev },
        }
    }

    /// Render to an SVG element.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut filter = Element::new(crate::element::ElementKind::Filter)
            .id(&self.id)
            .attr("x", "-50%")
            .attr("y", "-50%")
            .attr("width", "200%")
            .attr("height", "200%");

        match &self.kind {
            FilterKind::DropShadow {
                dx,
                dy,
                std_dev,
                opacity,
            } => {
                // Use feDropShadow for modern browsers
                let shadow = Element::new(crate::element::ElementKind::FeDropShadow)
                    .attr_num("dx", *dx)
                    .attr_num("dy", *dy)
                    .attr_num("stdDeviation", *std_dev)
                    .attr("flood-opacity", &format!("{:.2}", opacity));
                filter = filter.child(shadow);
            }
            FilterKind::GaussianBlur { std_dev } => {
                let blur = Element::new(crate::element::ElementKind::FeGaussianBlur)
                    .attr("in", "SourceGraphic")
                    .attr_num("stdDeviation", *std_dev);
                filter = filter.child(blur);
            }
        }

        filter
    }
}

/// Builder for the SVG defs section.
#[derive(Debug, Clone, Default)]
pub struct DefsBuilder {
    markers: Vec<ArrowheadMarker>,
    gradients: Vec<Gradient>,
    filters: Vec<Filter>,
    custom_elements: Vec<Element>,
}

impl DefsBuilder {
    /// Create a new defs builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a marker.
    #[must_use]
    pub fn marker(mut self, marker: ArrowheadMarker) -> Self {
        self.markers.push(marker);
        self
    }

    /// Add a gradient.
    #[must_use]
    pub fn gradient(mut self, gradient: Gradient) -> Self {
        self.gradients.push(gradient);
        self
    }

    /// Add a filter.
    #[must_use]
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Add a custom element to defs.
    #[must_use]
    pub fn custom(mut self, elem: Element) -> Self {
        self.custom_elements.push(elem);
        self
    }

    /// Check if the defs section is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
            && self.gradients.is_empty()
            && self.filters.is_empty()
            && self.custom_elements.is_empty()
    }

    /// Build to an Element.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut defs = Element::new(crate::element::ElementKind::Defs);

        for marker in &self.markers {
            defs = defs.child(marker.to_element());
        }

        for gradient in &self.gradients {
            defs = defs.child(gradient.to_element());
        }

        for filter in &self.filters {
            defs = defs.child(filter.to_element());
        }

        for elem in &self.custom_elements {
            defs = defs.child(elem.clone());
        }

        defs
    }

    /// Write the defs section to a string.
    pub fn write_to_string(&self, output: &mut String) {
        if self.is_empty() {
            return;
        }

        let elem = self.to_element();
        elem.write_to_string(output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_standard_arrowhead() {
        let marker = ArrowheadMarker::standard("arrow", "#333");
        let elem = marker.to_element();
        let svg = elem.render();
        assert!(svg.contains("<marker"));
        assert!(svg.contains("id=\"arrow\""));
        assert!(svg.contains("markerWidth"));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn creates_filled_arrowhead() {
        let marker = ArrowheadMarker::filled("arrow-filled", "#000");
        let elem = marker.to_element();
        let svg = elem.render();
        assert!(svg.contains("id=\"arrow-filled\""));
    }

    #[test]
    fn creates_open_arrowhead() {
        let marker = ArrowheadMarker::open("arrow-open", "#666");
        let elem = marker.to_element();
        let svg = elem.render();
        assert!(svg.contains("id=\"arrow-open\""));
        assert!(svg.contains("fill=\"none\""));
        assert!(svg.contains("stroke=\"#666\""));
    }

    #[test]
    fn creates_linear_gradient() {
        let gradient = Gradient::linear(
            "grad1",
            vec![
                GradientStop::new(0.0, "#fff"),
                GradientStop::new(1.0, "#000"),
            ],
        );
        let elem = gradient.to_element();
        let svg = elem.render();
        assert!(svg.contains("<linearGradient"));
        assert!(svg.contains("id=\"grad1\""));
        assert!(svg.contains("<stop"));
        assert!(svg.contains("stop-color=\"#fff\""));
        assert!(svg.contains("stop-color=\"#000\""));
    }

    #[test]
    fn creates_radial_gradient() {
        let gradient = Gradient::radial(
            "grad2",
            0.5,
            0.5,
            0.5,
            vec![
                GradientStop::new(0.0, "white"),
                GradientStop::new(1.0, "black"),
            ],
        );
        let elem = gradient.to_element();
        let svg = elem.render();
        assert!(svg.contains("<radialGradient"));
        assert!(svg.contains("id=\"grad2\""));
    }

    #[test]
    fn creates_drop_shadow_filter() {
        let filter = Filter::drop_shadow("shadow", 2.0, 2.0, 3.0, 0.3);
        let elem = filter.to_element();
        let svg = elem.render();
        assert!(svg.contains("<filter"));
        assert!(svg.contains("id=\"shadow\""));
        assert!(svg.contains("<feDropShadow"));
        assert!(svg.contains("stdDeviation=\"3\""));
    }

    #[test]
    fn creates_blur_filter() {
        let filter = Filter::blur("blur", 5.0);
        let elem = filter.to_element();
        let svg = elem.render();
        assert!(svg.contains("<filter"));
        assert!(svg.contains("id=\"blur\""));
        assert!(svg.contains("<feGaussianBlur"));
    }

    #[test]
    fn builds_defs_section() {
        let defs = DefsBuilder::new()
            .marker(ArrowheadMarker::standard("arrow", "#333"))
            .gradient(Gradient::linear(
                "grad",
                vec![
                    GradientStop::new(0.0, "#fff"),
                    GradientStop::new(1.0, "#000"),
                ],
            ))
            .filter(Filter::drop_shadow("shadow", 1.0, 1.0, 2.0, 0.2));

        let elem = defs.to_element();
        let svg = elem.render();
        assert!(svg.contains("<defs>"));
        assert!(svg.contains("<marker"));
        assert!(svg.contains("<linearGradient"));
        assert!(svg.contains("<filter"));
        assert!(svg.contains("</defs>"));
    }

    #[test]
    fn empty_defs_writes_nothing() {
        let defs = DefsBuilder::new();
        let mut output = String::new();
        defs.write_to_string(&mut output);
        assert!(output.is_empty());
    }
}
