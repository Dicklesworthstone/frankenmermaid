use fm_core::{
    capability_matrix_json_pretty, capability_readme_surface_markdown,
    capability_readme_supported_diagram_types_markdown,
};
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

fn main() -> Result<()> {
    let matrix_json = capability_matrix_json_pretty()
        .map_err(|err| Error::new(ErrorKind::Other, format!("serialize matrix: {err}")))?;
    fs::write("evidence/capability_matrix.json", matrix_json)?;

    let mut readme = fs::read_to_string("README.md")?;

    // Update diagram types block (required).
    let supported_block = capability_readme_supported_diagram_types_markdown();
    let supported_start = "<!-- BEGIN GENERATED: supported-diagram-types -->\n";
    let supported_end = "\n<!-- END GENERATED: supported-diagram-types -->";
    replace_block(&mut readme, supported_start, supported_end, &supported_block)?;

    // Update surface block (optional; add if missing).
    let surface_block = capability_readme_surface_markdown();
    let surface_start = "<!-- BEGIN GENERATED: runtime-capability-metadata -->\n";
    let surface_end = "\n<!-- END GENERATED: runtime-capability-metadata -->";
    if readme.contains(surface_start) {
        replace_block(&mut readme, surface_start, surface_end, &surface_block)?;
    } else {
        println!("Could not find runtime-capability-metadata, adding it");
        readme.push_str("\n");
        readme.push_str(surface_start);
        readme.push_str(&surface_block);
        readme.push_str(surface_end);
        readme.push_str("\n");
    }

    fs::write("README.md", readme)?;
    Ok(())
}
