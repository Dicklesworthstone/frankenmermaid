use std::process::Command;

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

fn main() {
    println!("cargo:rerun-if-env-changed=FM_H2H_BUILD_GIT_REV");

    // RCH clean overlays intentionally omit .git metadata. Its strict remote build command passes
    // the immutable source base inside the remote shell, while local Cargo builds can derive it.
    // The H2H driver subsequently verifies this value against both its build-base argument and HEAD.
    let revision = std::env::var("FM_H2H_BUILD_GIT_REV")
        .ok()
        .filter(|revision| is_git_revision(revision))
        .or_else(|| git_output(&["rev-parse", "HEAD"]).filter(|revision| is_git_revision(revision)))
        .unwrap_or_else(|| "unavailable".to_owned());
    println!("cargo:rustc-env=FM_H2H_COMPILED_GIT_REV={revision}");

    if let Some(head_path) = git_output(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head_path}");
    }
    if let Some(symbolic_head) = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        && let Some(ref_path) = git_output(&["rev-parse", "--git-path", &symbolic_head])
    {
        println!("cargo:rerun-if-changed={ref_path}");
    }
}
