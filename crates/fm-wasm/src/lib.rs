#![forbid(unsafe_code)]

use fm_layout::{LayoutAlgorithm, layout};
use fm_parser::parse;
use fm_render_svg::render_svg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmRenderOutput {
    pub svg: String,
    pub detected_type: String,
}

#[must_use]
pub fn render(input: &str) -> WasmRenderOutput {
    let parsed = parse(input);
    let _stats = layout(&parsed.ir, LayoutAlgorithm::Auto);

    WasmRenderOutput {
        svg: render_svg(&parsed.ir),
        detected_type: format!("{:?}", parsed.ir.diagram_type),
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn render_returns_svg_and_type() {
        let output = render("flowchart LR\nA-->B");
        assert!(output.svg.starts_with("<svg"));
        assert_eq!(output.detected_type, "Flowchart");
    }
}
