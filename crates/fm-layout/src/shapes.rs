use crate::{LayoutRect, PathCmd};
use fm_core::NodeShape;
use std::f32::consts::PI;

#[must_use]
pub fn node_path(bounds: LayoutRect, shape: NodeShape) -> Vec<PathCmd> {
    match shape {
        NodeShape::Rect => rounded_rect_path(bounds, 5.0),
        NodeShape::Rounded => rounded_rect_path(bounds, 10.0),
        NodeShape::Stadium => stadium_path(bounds),
        NodeShape::Diamond => diamond_path(bounds),
        NodeShape::Hexagon => hexagon_path(bounds),
        NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => {
            polygon_ellipse_path(bounds, 24)
        }
        NodeShape::Cylinder => cylinder_path(bounds),
        NodeShape::Trapezoid => trapezoid_path(bounds),
        NodeShape::HorizontalBar => horizontal_bar_path(bounds),
        NodeShape::InvTrapezoid => inv_trapezoid_path(bounds),
        NodeShape::Parallelogram => parallelogram_path(bounds),
        NodeShape::InvParallelogram => inv_parallelogram_path(bounds),
        NodeShape::Asymmetric => asymmetric_path(bounds),
        NodeShape::Note => note_path(bounds),
        NodeShape::Triangle => triangle_path(bounds),
        NodeShape::Pentagon => polygon_path(bounds, 5, -std::f32::consts::FRAC_PI_2),
        NodeShape::Star => star_path(bounds, 5),
        NodeShape::Cloud => cloud_path(bounds),
        NodeShape::Tag => tag_path(bounds),
        NodeShape::Subroutine => {
            // For composite shapes, we use the primary boundary path.
            // Inner lines are added by specialized render logic if needed,
            // but for simple path representation we return the outer box.
            rounded_rect_path(bounds, 0.0)
        }
        NodeShape::CrossedCircle => polygon_ellipse_path(bounds, 24),
    }
}

#[must_use]
pub fn stadium_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let r = bounds.width.min(bounds.height) / 2.0;
    rounded_rect_path(bounds, r)
}

#[must_use]
pub fn hexagon_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let cy = y + h / 2.0;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::LineTo { x, y: cy },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn cylinder_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let ry = (h * 0.1).max(2.0);
    let rx = w / 2.0;

    vec![
        PathCmd::MoveTo { x, y: y + ry },
        PathCmd::QuadTo {
            cx: x + rx,
            cy: y - ry,
            x: x + w,
            y: y + ry,
        },
        PathCmd::LineTo {
            x: x + w,
            y: y + h - ry,
        },
        PathCmd::QuadTo {
            cx: x + rx,
            cy: y + h + ry,
            x,
            y: y + h - ry,
        },
        PathCmd::LineTo { x, y: y + ry },
        PathCmd::Close,
        PathCmd::MoveTo { x, y: y + ry },
        PathCmd::QuadTo {
            cx: x + rx,
            cy: y + (ry * 3.0),
            x: x + w,
            y: y + ry,
        },
    ]
}

#[must_use]
pub fn trapezoid_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn inv_trapezoid_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn parallelogram_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn inv_parallelogram_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn asymmetric_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let flag = w * 0.15;
    let cy = y + h / 2.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w - flag, y },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - flag,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn note_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let fold = 10.0_f32.min(w * 0.4);
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w - fold, y },
        PathCmd::LineTo {
            x: x + w,
            y: y + fold,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn triangle_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let cx = x + w / 2.0;
    vec![
        PathCmd::MoveTo { x: cx, y },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn horizontal_bar_path(bounds: LayoutRect) -> Vec<PathCmd> {
    rounded_rect_path(bounds, (bounds.height / 2.0).min(4.0))
}

#[must_use]
pub fn polygon_path(bounds: LayoutRect, sides: usize, angle_offset: f32) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let r = bounds.width.min(bounds.height) / 2.0;
    let mut cmds = Vec::with_capacity(sides + 1);
    for i in 0..sides {
        let angle = angle_offset + (i as f32) * 2.0 * PI / (sides as f32);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x: px, y: py });
        } else {
            cmds.push(PathCmd::LineTo { x: px, y: py });
        }
    }
    cmds.push(PathCmd::Close);
    cmds
}

#[must_use]
pub fn star_path(bounds: LayoutRect, points: usize) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let outer_r = bounds.width.min(bounds.height) / 2.0;
    let inner_r = outer_r * 0.4;
    let angle_offset = -std::f32::consts::FRAC_PI_2;
    let total_points = points * 2;
    let mut cmds = Vec::with_capacity(total_points + 1);
    for i in 0..total_points {
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let angle = angle_offset + (i as f32) * PI / (points as f32);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x: px, y: py });
        } else {
            cmds.push(PathCmd::LineTo { x: px, y: py });
        }
    }
    cmds.push(PathCmd::Close);
    cmds
}

#[must_use]
pub fn cloud_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    #[allow(clippy::many_single_char_names)]
    let r = h / 3.0;
    // Simplified cloud path
    vec![
        PathCmd::MoveTo {
            x: x + r,
            y: h.mul_add(0.6, y),
        },
        PathCmd::LineTo {
            x: x + r * 2.0,
            y: h.mul_add(0.3, y),
        },
        PathCmd::LineTo {
            x: w.mul_add(0.5, x),
            y: y + r * 0.5,
        },
        PathCmd::LineTo {
            x: x + w - r * 2.0,
            y: h.mul_add(0.3, y),
        },
        PathCmd::LineTo {
            x: x + w - r,
            y: h.mul_add(0.6, y),
        },
        PathCmd::LineTo {
            x: x + w - r,
            y: h.mul_add(0.8, y),
        },
        PathCmd::LineTo {
            x: x + r,
            y: h.mul_add(0.8, y),
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn tag_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let point = w * 0.2;
    let cy = y + h / 2.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo {
            x: x + w - point,
            y,
        },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - point,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn rounded_rect_path(bounds: LayoutRect, radius: f32) -> Vec<PathCmd> {
    let mut commands = Vec::with_capacity(10);
    let r = radius.min(bounds.width / 2.0).min(bounds.height / 2.0);
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;

    commands.push(PathCmd::MoveTo { x: x + r, y });
    commands.push(PathCmd::LineTo { x: x + w - r, y });
    commands.push(PathCmd::QuadTo {
        cx: x + w,
        cy: y,
        x: x + w,
        y: y + r,
    });
    commands.push(PathCmd::LineTo {
        x: x + w,
        y: y + h - r,
    });
    commands.push(PathCmd::QuadTo {
        cx: x + w,
        cy: y + h,
        x: x + w - r,
        y: y + h,
    });
    commands.push(PathCmd::LineTo { x: x + r, y: y + h });
    commands.push(PathCmd::QuadTo {
        cx: x,
        cy: y + h,
        x,
        y: y + h - r,
    });
    commands.push(PathCmd::LineTo { x, y: y + r });
    commands.push(PathCmd::QuadTo {
        cx: x,
        cy: y,
        x: x + r,
        y,
    });
    commands.push(PathCmd::Close);

    commands
}

#[must_use]
pub fn diamond_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    vec![
        PathCmd::MoveTo { x: cx, y: bounds.y },
        PathCmd::LineTo {
            x: bounds.x + bounds.width,
            y: cy,
        },
        PathCmd::LineTo {
            x: cx,
            y: bounds.y + bounds.height,
        },
        PathCmd::LineTo { x: bounds.x, y: cy },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn polygon_ellipse_path(bounds: LayoutRect, segments: usize) -> Vec<PathCmd> {
    let segment_count = segments.max(8);
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let rx = bounds.width / 2.0;
    let ry = bounds.height / 2.0;

    let mut commands = Vec::with_capacity(segment_count + 2);
    for index in 0..segment_count {
        let theta = (index as f32 / segment_count as f32) * 2.0 * PI;
        let x = cx + (rx * theta.cos());
        let y = cy + (ry * theta.sin());
        if index == 0 {
            commands.push(PathCmd::MoveTo { x, y });
        } else {
            commands.push(PathCmd::LineTo { x, y });
        }
    }
    commands.push(PathCmd::Close);
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cylinder_path_contains_curved_caps() {
        let path = cylinder_path(LayoutRect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 40.0,
        });

        assert!(matches!(path.first(), Some(PathCmd::MoveTo { .. })));
        assert!(path.iter().any(|cmd| matches!(cmd, PathCmd::QuadTo { .. })));
        assert_eq!(
            path.iter()
                .filter(|cmd| matches!(cmd, PathCmd::QuadTo { .. }))
                .count(),
            3
        );
        assert!(path.iter().any(|cmd| matches!(cmd, PathCmd::Close)));
    }

    #[test]
    fn crossed_circle_uses_circular_primary_boundary() {
        let path = node_path(
            LayoutRect {
                x: 10.0,
                y: 20.0,
                width: 60.0,
                height: 60.0,
            },
            NodeShape::CrossedCircle,
        );

        assert!(matches!(path.first(), Some(PathCmd::MoveTo { .. })));
        assert_eq!(
            path.iter()
                .filter(|cmd| matches!(cmd, PathCmd::LineTo { .. }))
                .count(),
            23
        );
        assert!(path.iter().any(|cmd| matches!(cmd, PathCmd::Close)));
    }
}
