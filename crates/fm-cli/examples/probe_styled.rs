//! Scratch: what does inline `:::` on a flowchart node currently produce? (not committed)
use fm_parser::parse;

fn dump(label: &str, src: &str) {
    let pf = parse(src);
    println!("--- {label} ---");
    println!("  warnings: {:?}", pf.warnings);
    for n in &pf.ir.nodes {
        println!("  node id={:?} classes={:?}", n.id, n.classes);
    }
}

fn main() {
    dump("single", "flowchart LR\nN0[Node 0]:::foo");
    dump("triple", "flowchart LR\nN0[Node 0]:::foo:::bar:::baz");
    dump("plain-id", "flowchart LR\nA:::hot");
    dump("directive", "flowchart LR\nN0[Node 0]\nclass N0 foo");
    dump("edge-then-class", "flowchart LR\nA-->B\nclass A foo");
}
