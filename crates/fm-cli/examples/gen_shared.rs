pub fn gen_input(shape: &str, n: usize) -> String {
    let mut l: Vec<String> = Vec::new();
    match shape {
        "seq" => {
            let p = ((n as f64).sqrt() as usize).max(3);
            l.push("sequenceDiagram".into());
            for i in 0..p {
                l.push(format!("  participant P{i}"));
            }
            for i in 0..n {
                let a = i % p;
                let b = (i + 1) % p;
                l.push(format!("  P{a}->>P{b}: m{i}"));
            }
        }
        "gantt" => {
            l.push("gantt".into());
            l.push("  title T".into());
            l.push("  dateFormat YYYY-MM-DD".into());
            let s = ((n as f64).sqrt() as usize).max(2);
            for sec in 0..s {
                l.push(format!("  section S{sec}"));
                let per = n / s;
                for t in 0..per {
                    let idx = sec * per + t;
                    if t == 0 {
                        l.push(format!(
                            "  Task{idx} :t{idx}, 2024-01-01, {}d",
                            3 + (idx % 20)
                        ));
                    } else {
                        l.push(format!(
                            "  Task{idx} :t{idx}, after t{}, {}d",
                            idx - 1,
                            2 + (idx % 15)
                        ));
                    }
                }
            }
        }
        "sankey" => {
            l.push("sankey-beta".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("N{i},N{},{}", i + 1, 5 + (i % 20)));
            }
        }
        "state" => {
            l.push("stateDiagram-v2".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  S{i} --> S{}", i + 1));
            }
        }
        "pie" => {
            l.push("pie".into());
            for i in 0..n {
                l.push(format!("  \"Slice {i}\" : {}", 1 + (i % 50)));
            }
        }
        "git" => {
            let b = ((n as f64).sqrt() as usize).max(2);
            l.push("gitGraph".into());
            l.push("  commit".into());
            for i in 0..b {
                l.push(format!("  branch b{i}"));
                l.push(format!("  checkout b{i}"));
                let per = n / b;
                for _ in 0..per {
                    l.push("  commit".into());
                }
            }
        }
        "block" => {
            l.push("block-beta".into());
            l.push("columns 3".into());
            for i in 0..n {
                if i % 10 == 0 {
                    l.push("  space".into());
                } else {
                    l.push(format!("  B{i}[\"B{i}\"]"));
                }
            }
        }
        "class" => {
            l.push("classDiagram".into());
            for i in 0..n {
                l.push(format!("  class C{i} {{ +int f{i} }}"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  C{i} <|-- C{}", i + 1));
            }
        }
        "er" => {
            l.push("erDiagram".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  E{i} ||--o{{ E{} : has", i + 1));
            }
        }
        "mindmap" => {
            l.push("mindmap".into());
            l.push("  root".into());
            for i in 0..n {
                l.push(format!("    child{i}"));
            }
        }
        _ => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}"));
            }
        }
    }
    l.join("\n")
}

// Cargo auto-discovers every file in examples/ as an example binary.
#[allow(dead_code)]
fn main() {}
