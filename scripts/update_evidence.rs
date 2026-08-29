use fm_core::{
    capability_readme_surface_markdown,
    capability_readme_supported_diagram_types_markdown, feature_parity_parser_families_markdown,
};
use fm_layout::{full_capability_matrix, layout_algorithms_markdown};
use std::fs;
use std::io::{Error, ErrorKind, Result};

fn replace_block(readme: &mut String, start: &str, end: &str, block: &str) -> Result<()> {
    let start_index = readme
        .find(start)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("missing marker: {start}")))?;
    let content_start = start_index + start.len();
    let end_offset = readme[content_start..]
        .find(end)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("missing marker: {end}")))?;
    let end_index = content_start + end_offset;
    readme.replace_range(content_start..end_index, block);
    Ok(())
}

fn replace_block_in(path: &str, marker: &str, block: &str) -> Result<()> {
    let start_marker = format!("<!-- BEGIN GENERATED: {marker} -->");
    let end_marker = format!("<!-- END GENERATED: {marker} -->");
    let mut text = fs::read_to_string(path)?;
    replace_block(&mut text, &start_marker, &end_marker, block)?;
    fs::write(path, text)?;
    Ok(())
}

fn main() -> Result<()> {
    // The committed artifact is the MERGED matrix (fm-core claims plus fm-layout's
    // algorithm claims) — the same payload the CLI `capabilities` command and the WASM
    // `capabilityMatrix()` print. The byte-pinning tests live beside the sources:
    // `full_capability_matrix_json_matches_checked_in_artifact` (fm-layout).
    let matrix_json = serde_json::to_string_pretty(&full_capability_matrix())
        .map_err(|err| Error::new(ErrorKind::Other, format!("serialize matrix: {err}")))?;
    fs::write("evidence/capability_matrix.json", matrix_json)?;

    let mut readme = fs::read_to_string("README.md")?;

    // Update diagram types block (required).
    let supported_block = capability_readme_supported_diagram_types_markdown();
    let supported_start = "<!-- BEGIN GENERATED: supported-diagram-types -->\n";
    let supported_end = "\n<!-- END GENERATED: supported-diagram-types -->";
    replace_block(&mut readme, supported_start, supported_end, &supported_block)?;

    // Update surface block (required).
    let surface_block = capability_readme_surface_markdown();
    let surface_start = "<!-- BEGIN GENERATED: runtime-capability-metadata -->\n";
    let surface_end = "\n<!-- END GENERATED: runtime-capability-metadata -->";
    replace_block(&mut readme, surface_start, surface_end, &surface_block)?;

    // Update layout algorithms block (required).
    let layout_block = layout_algorithms_markdown();
    let layout_start = "<!-- BEGIN GENERATED: layout-algorithms -->\n";
    let layout_end = "\n<!-- END GENERATED: layout-algorithms -->";
    if readme.contains(layout_start) {
        replace_block(&mut readme, layout_start, layout_end, &layout_block)?;
    } else {
        println!("Could not find layout-algorithms, adding it");
        readme.push_str("\n");
        readme.push_str(layout_start);
        readme.push_str(&layout_block);
        readme.push_str(layout_end);
        readme.push_str("\n");
    }

    fs::write("README.md", readme)?;

    // FEATURE_PARITY tables: runtime/parity per family, and the layout algorithms.
    replace_block_in(
        "docs/planning/FEATURE_PARITY.md",
        "feature-parity-families",
        &feature_parity_parser_families_markdown(),
    )?;
    replace_block_in(
        "docs/planning/FEATURE_PARITY.md",
        "feature-parity-layouts",
        &layout_algorithms_markdown(),
    )?;

    Ok(())
}
