use fm_parser::parse;
mod g { include!("gen_shared.rs"); }
fn main() {
    for n in [200usize, 800] {
        let pf = parse(&g::gen_input("gantt", n));
        eprintln!("gantt n={n}: nodes={} edges={}", pf.ir.nodes.len(), pf.ir.edges.len());
    }
}
