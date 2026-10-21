//! Shared semantic appearance for mindmaps, independent of the rendering backend.

use crate::{DiagramType, MermaidDiagramIr};

/// Branch accents for the default theme. Other themes supply their own eight accents.
pub const DEFAULT_BRANCH_ACCENTS: [&str; 8] = [
    "#4f46e5", "#0284c7", "#0d9488", "#7c3aed", "#d97706", "#db2777", "#059669", "#e11d48",
];

/// Parser-assigned hierarchy markers, rather than arbitrary node identity colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRole {
    pub branch: usize,
    pub is_root: bool,
    pub borderless: bool,
}

/// Read the same semantic classes used by the SVG stylesheet.
#[must_use]
pub fn node_role(ir: &MermaidDiagramIr, node_index: usize) -> Option<NodeRole> {
    if ir.diagram_type != DiagramType::Mindmap {
        return None;
    }
    let node = ir.nodes.get(node_index)?;
    let branch = node.classes.iter().find_map(|class| {
        class
            .strip_prefix("mindmap-branch-")?
            .parse::<usize>()
            .ok()
            .filter(|&index| index < DEFAULT_BRANCH_ACCENTS.len())
    })?;
    Some(NodeRole {
        branch,
        is_root: node.classes.iter().any(|class| class == "mindmap-root"),
        borderless: node
            .classes
            .iter()
            .any(|class| class == "mindmap-no-border"),
    })
}

/// Tint a theme's node surface with its branch accent; the root gets stronger emphasis.
///
/// Integer sRGB mixing gives SVG and Canvas exactly the same paint, without relying on a
/// browser's support for `color-mix()`. Non-hex custom paints keep the author's base surface.
#[must_use]
pub fn branch_fill(base: &str, accent: &str, is_root: bool) -> String {
    let Some((base, accent)) = hex_rgb(base).zip(hex_rgb(accent)) else {
        return base.to_owned();
    };
    let weight: u16 = if is_root { 30 } else { 15 };
    let mixed: [u16; 3] = std::array::from_fn(|index| {
        (u16::from(base[index]) * (100 - weight) + u16::from(accent[index]) * weight + 50) / 100
    });
    format!("#{:02x}{:02x}{:02x}", mixed[0], mixed[1], mixed[2])
}

fn hex_rgb(paint: &str) -> Option<[u8; 3]> {
    let hex = paint.trim().strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => Some([
            u8::from_str_radix(&hex[0..1], 16).ok()? * 17,
            u8::from_str_radix(&hex[1..2], 16).ok()? * 17,
            u8::from_str_radix(&hex[2..3], 16).ok()? * 17,
        ]),
        6 => Some([
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_surfaces_keep_their_theme_and_root_emphasis() {
        assert_eq!(branch_fill("#ffffff", "#000000", false), "#d9d9d9");
        assert_eq!(branch_fill("#ffffff", "#000000", true), "#b3b3b3");
        assert_eq!(branch_fill("#000", "#fff", false), "#262626");
        assert_eq!(branch_fill("#000", "#fff", true), "#4d4d4d");
        assert_eq!(branch_fill("navy", "#ffffff", true), "navy");
        assert_eq!(branch_fill("#f8fafc", "var(--accent)", false), "#f8fafc");
        assert_eq!(branch_fill("#f8fafc", "#é12", false), "#f8fafc");
    }
}
