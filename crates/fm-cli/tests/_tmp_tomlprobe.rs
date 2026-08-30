#[test]
fn probe() {
    let git = fm_parser::parse("gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit").ir;
    if let Some(m) = git.git_graph_meta.as_ref() {
        println!("TOML_GIT {:?}", toml::to_string(m).map(|s| s.replace('\n', " | ")));
    } else {
        println!("TOML_GIT no git_graph meta; keys: n/a");
    }
    let md = fm_parser::parse("flowchart LR\n  A[\"`**b**`\"] --> B").ir;
    println!("TOML_MARKUP_LEN {}", md.label_markup.len());
    println!("TOML_MARKUP {:?}", toml::to_string(&md.label_markup).map(|s| s.replace('\n', " | ")).map_err(|e| e.to_string()));
}
