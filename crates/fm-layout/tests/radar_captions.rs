//! Radar caption clearance through the public layout API.

use fm_core::{FontMetrics, FontMetricsConfig, FontPreset};
use fm_layout::{PathCmd, RadarChartLayout, RadarPathRole, RadarTextRole};

#[test]
fn wrapped_axis_captions_stay_outside_the_wheel_without_changing_data() {
    let mut north_leader = false;
    let mut south_leader = false;
    for axis_count in [4, 8, 12, 16] {
        let labels: Vec<_> = (0..axis_count)
            .map(|index| {
                let paragraph = format!(
                    "Coordinate delivery across all participating services and regions with clear recovery ownership {index}"
                );
                match index % 3 {
                    0 => format!("{paragraph}\nConfirm availability during peak traffic and review every downstream dependency"),
                    1 => paragraph,
                    _ => format!("Readiness {index}\n{paragraph}\nDocument recovery decisions and communicate the complete rollout plan"),
                }
            })
            .collect();
        let axes = (0..axis_count)
            .map(|index| format!("axis{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = (0..axis_count)
            .map(|index| (index + 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let parsed = fm_parser::parse(&format!(
            "radar-beta\n  title Operational readiness\n  axis {axes}\n  curve current{{{values}}}\n"
        ));
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        let mut ir = parsed.ir;
        for (axis, label) in ir.radar_meta.as_mut().unwrap().axes.iter_mut().zip(&labels) {
            axis.label = Some(label.clone());
        }
        let layout = fm_layout::layout_diagram(&ir);
        let radar = layout.extensions.radar.as_ref().unwrap();
        let meta = ir.radar_meta.as_ref().unwrap();
        let original = RadarChartLayout::new(meta, radar, None, &FontMetrics::default_metrics());
        for font_size in [15.0, 24.0] {
            let metrics = FontMetrics::new(FontMetricsConfig {
                font_size,
                preset: FontPreset::Monospace,
                ..Default::default()
            });
            let chart = RadarChartLayout::new(meta, radar, ir.declared_title_if_drawn(), &metrics);
            let captions: Vec<_> = chart
                .text
                .iter()
                .filter(|block| matches!(block.role, RadarTextRole::Axis(_)))
                .collect();
            assert_eq!(captions.len(), axis_count);
            for (index, caption) in captions.iter().enumerate() {
                let RadarTextRole::Axis(axis) = caption.role else {
                    unreachable!();
                };
                assert_eq!(caption.lines.concat(), labels[axis].replace('\n', ""));
                assert!(caption.lines.len() > 1);
                let bounds = caption.bounds;
                let nearest_x = radar.center.x.clamp(bounds.x, bounds.x + bounds.width);
                let nearest_y = radar.center.y.clamp(bounds.y, bounds.y + bounds.height);
                let distance = (nearest_x - radar.center.x).hypot(nearest_y - radar.center.y);
                assert!(
                    distance >= radar.outer_radius + 14.9,
                    "{axis_count} axes at {font_size}px: caption {axis} covers plotted data"
                );
                for previous in &captions[..index] {
                    let other = previous.bounds;
                    assert!(
                        bounds.x + bounds.width <= other.x
                            || other.x + other.width <= bounds.x
                            || bounds.y + bounds.height <= other.y
                            || other.y + other.height <= bounds.y,
                        "{axis_count} axes at {font_size}px: caption {axis} overlaps another caption"
                    );
                }
            }
            for entry in &chart.paths {
                if entry.role != RadarPathRole::Leader {
                    continue;
                }
                let [
                    PathCmd::MoveTo {
                        x: from_x,
                        y: from_y,
                    },
                    PathCmd::LineTo { x: to_x, y: to_y },
                ] = entry.path.commands.as_slice()
                else {
                    panic!("a moved caption must retain a direct leader to its spoke")
                };
                assert!(radar.axes.iter().any(|axis| {
                    (axis.tip.x - from_x).abs() < 0.01 && (axis.tip.y - from_y).abs() < 0.01
                }));
                assert!(captions.iter().any(|caption| {
                    let bounds = caption.bounds;
                    *to_x >= bounds.x - 0.01
                        && *to_x <= bounds.x + bounds.width + 0.01
                        && *to_y >= bounds.y - 0.01
                        && *to_y <= bounds.y + bounds.height + 0.01
                }));
                north_leader |= *from_y < radar.center.y - 1.0 && to_y < from_y;
                south_leader |= *from_y > radar.center.y + 1.0 && to_y > from_y;
            }
            let quantitative = |role| {
                matches!(
                    role,
                    RadarPathRole::Grid | RadarPathRole::Axis | RadarPathRole::Series(_)
                )
            };
            assert_eq!(
                chart
                    .paths
                    .iter()
                    .filter(|entry| quantitative(entry.role))
                    .collect::<Vec<_>>(),
                original
                    .paths
                    .iter()
                    .filter(|entry| quantitative(entry.role))
                    .collect::<Vec<_>>(),
                "font changes must not move the grid, spokes or plotted series"
            );
        }
    }
    assert!(
        north_leader,
        "crowded northern captions need outward leaders"
    );
    assert!(
        south_leader,
        "crowded southern captions need outward leaders"
    );
}
