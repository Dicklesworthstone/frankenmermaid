//! Scratch isolated A/B micro-bench for estimate_dimensions (not committed).
//! OLD = two lines() passes (multiline_width + separate height count),
//! NEW = fused single-pass estimate_dimensions. Same estimate_width underneath.
use fm_core::FontMetrics;
use std::hint::black_box;

fn main() {
    let fm = FontMetrics::default();
    let lh = fm.line_height_px();
    // Representative label corpus: mostly single-line short labels (the common node label),
    // a few multi-line.
    let corpus: Vec<String> = {
        let mut v = Vec::new();
        for i in 0..400 {
            v.push(format!("Node {i}"));
            v.push(format!("service-{i}-handler"));
            if i % 7 == 0 {
                v.push(format!("Line one {i}\nLine two {i}\nthird"));
            }
        }
        v
    };
    let old = |t: &str| -> (f32, f32) {
        let w = fm.estimate_multiline_width(t);
        let n = t.lines().count().max(1) as f32;
        (w, n * lh)
    };
    let new = |t: &str| -> (f32, f32) { fm.estimate_dimensions(t) };

    // Correctness: NEW must equal OLD bit-for-bit on the whole corpus.
    for t in &corpus {
        assert_eq!(old(t), new(t), "mismatch on {t:?}");
    }

    let iters = 4000usize;
    let mut bo = u64::MAX;
    let mut bn = u64::MAX;
    for _ in 0..40 {
        // OLD
        let t0 = std::time::Instant::now();
        let mut acc = 0.0f32;
        for _ in 0..iters {
            for t in &corpus {
                let (w, h) = old(black_box(t));
                acc += w + h;
            }
        }
        black_box(acc);
        let to = t0.elapsed().as_nanos() as u64;
        // NEW
        let t1 = std::time::Instant::now();
        let mut acc2 = 0.0f32;
        for _ in 0..iters {
            for t in &corpus {
                let (w, h) = new(black_box(t));
                acc2 += w + h;
            }
        }
        black_box(acc2);
        let tn = t1.elapsed().as_nanos() as u64;
        bo = bo.min(to);
        bn = bn.min(tn);
    }
    let pct = (bn as f64 - bo as f64) / bo as f64 * 100.0;
    println!(
        "corpus={} OLD_min={}ns NEW_min={}ns delta={:+.2}%",
        corpus.len(),
        bo,
        bn,
        pct
    );
}
