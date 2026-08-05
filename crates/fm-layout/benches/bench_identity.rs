//! Shared binary-identity reporting for the fm-layout benches.
//!
//! `docs/PERF_LEDGER.md` requires every KEEP row to carry
//! `**Executing ELF SHA-256 (self-reported by process):**`, and AGENTS.md is explicit that
//! computing a hash beside the run is not sufficient — the executing process must identify its own
//! ELF. A shell step that hashes a path proves nothing about which binary actually ran, which is
//! precisely the provenance hole that lets a harness compare a build against itself.
//!
//! This lives in its own file, included by each bench with `#[path]`, so the two benches share ONE
//! implementation. Benches are separate binaries with no common crate module, and a copy-pasted
//! helper is the shape that silently drifts between forks.

use sha2::{Digest, Sha256};

/// SHA-256 of this executable plus its byte length, read from inside the measured process.
///
/// Returns `"unavailable"` rather than panicking or guessing if the executable cannot be read:
/// a bench that cannot prove its identity must say so, because a missing digest is a refusal to
/// certify, while a wrong one is a false certification.
pub fn self_identity() -> String {
    let Ok(path) = std::env::current_exe() else {
        return "unavailable".to_string();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return "unavailable".to_string();
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();

    // Neither `write!(..).expect(..)` nor a hex lookup table: the first leaves an unreachable
    // panic site in the one routine whose job is to certify what ran, and the second reads as
    // unchecked indexing. This runs 32 times, once per process, so the per-byte format is free.
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        sha256.push_str(&format!("{byte:02x}"));
    }
    format!("{sha256} ({} bytes)", bytes.len())
}

/// Print the executing ELF identity in the exact `bench_elf_sha256=<digest>` form the ledger
/// tooling and the perf lanes already grep for.
///
/// Idempotent, and called from every benchmark group rather than from `main`: criterion owns
/// `main`, and a filtered run (`--bench 'single_node_label_edit/incremental/1000'`) executes only
/// the groups that match. Reporting from one group would leave exactly the targeted single-row
/// runs — the ones a ledger entry is written from — with no identity line at all.
pub fn report_self_identity() {
    static REPORTED: std::sync::Once = std::sync::Once::new();
    REPORTED.call_once(|| {
        println!("bench_elf_sha256={}", self_identity());
    });
}
