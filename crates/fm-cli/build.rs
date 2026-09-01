use std::path::Path;
use std::process::Command;

/// Paths whose content decides what Cargo actually compiles into this crate's binaries.
///
/// A tracked modification under any of these means the checked-out revision does NOT describe the
/// source on disk, so `HEAD` is the wrong answer for "what was compiled". Everything else in the
/// repository -- docs, benchmark artifacts, the beads tracker -- can move freely without changing a
/// single emitted byte, and gating on those would make the stamp unavailable for reasons that have
/// nothing to do with the binary.
const SOURCE_PATHS: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "crates",
    ".cargo",
    "rust-toolchain.toml",
];

fn is_git_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
}

/// The revision of the checkout Cargo is building in, but ONLY when that checkout provably
/// describes the source being compiled.
///
/// ⚠️ THE FOREIGN-CHECKOUT TRAP THIS EXISTS TO CLOSE (bd-vdrx9). A remote build host receives the
/// SOURCE and builds it inside a directory that carries its own `.git`, left at whatever revision it
/// last held. `git rev-parse HEAD` there answers for the host's checkout, not for the transferred
/// source, and it answers with a well-formed 40-hex commit that really exists -- a build of
/// `aaa334d9` stamped `43480807`, 35 commits behind, and every shape check downstream passed it.
/// A missing stamp is caught; a plausible wrong one is believed, so this refuses to guess.
///
/// The transferred source differs from the host checkout's `HEAD` in tracked files, which is exactly
/// what this predicate sees. It is the same predicate that keeps a local build with uncommitted
/// source edits from claiming `HEAD`, because that claim is wrong for the same reason.
fn revision_of_verified_checkout() -> Option<String> {
    let toplevel = git_output(&["rev-parse", "--show-toplevel"])?;
    if !Path::new(&toplevel).is_dir() {
        return None;
    }

    // `--no-optional-locks` keeps this read-only: two agents share this checkout and a build script
    // must never contend for the index lock just to answer a question about it.
    let mut args = vec![
        "--no-optional-locks",
        "-C",
        toplevel.as_str(),
        "status",
        "--porcelain",
        "--untracked-files=no",
        "--",
    ];
    args.extend_from_slice(&SOURCE_PATHS);
    if !git_output(&args)?.is_empty() {
        return None;
    }

    git_output(&["rev-parse", "HEAD"]).filter(|revision| is_git_revision(revision))
}

fn main() {
    println!("cargo:rerun-if-env-changed=FM_H2H_BUILD_GIT_REV");

    // The caller is the only party that can know the revision of source it hands to a remote build
    // host, so an explicit `FM_H2H_BUILD_GIT_REV` outranks anything derived here. `.rch/config.toml`
    // names it in rch's environment allowlist so it survives the trip to the worker; without that
    // entry the variable is dropped and the build script silently falls through to the branch below.
    // The H2H driver verifies the resulting value against both its build-base argument and HEAD.
    let (revision, source) = match std::env::var("FM_H2H_BUILD_GIT_REV")
        .ok()
        .filter(|revision| is_git_revision(revision))
    {
        Some(revision) => (revision, "env"),
        None => match revision_of_verified_checkout() {
            Some(revision) => (revision, "git"),
            None => ("unavailable".to_owned(), "unavailable"),
        },
    };
    println!("cargo:rustc-env=FM_H2H_COMPILED_GIT_REV={revision}");
    println!("cargo:rustc-env=FM_H2H_COMPILED_GIT_REV_SOURCE={source}");

    if let Some(head_path) = git_output(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head_path}");
    }
    if let Some(symbolic_head) = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        && let Some(ref_path) = git_output(&["rev-parse", "--git-path", &symbolic_head])
    {
        println!("cargo:rerun-if-changed={ref_path}");
    }
}
