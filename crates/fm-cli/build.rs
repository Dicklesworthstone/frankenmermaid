use std::process::Command;

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

    let revision = git_output(&["rev-parse", "HEAD"])
        .filter(|revision| {
            revision.len() == 40
                && revision
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
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
