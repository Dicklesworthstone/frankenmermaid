"""bd-akv2 follow-on: skip allocator/static teardown at process exit.

Applies to crates/fm-cli/src/main.rs. Reapply with `python3 <this file>` and gate with
`cargo test -j 1 -p frankenmermaid-cli`.
"""

p = '/data/projects/frankenmermaid/crates/fm-cli/src/main.rs'
s = open(p).read()

old = '''fn main() -> Result<()> {
    let cli = Cli::parse();
'''

new = '''/// Skip allocator and static teardown once the work is done.
///
/// The worst certified vs-incumbent ratio (bd-kpgs, 90.86x) measures CLI process lifecycle -- its
/// counted mechanism records that every arm "started zero render workers" -- and its profile puts
/// **allocator teardown at 4.37%** and C++ static initialization at 3.55%. Returning from `main`
/// runs the whole atexit chain: static destructors, the C++ runtime's, and mimalloc's teardown of
/// every segment the pipeline allocated. None of it is observable; the process is about to stop
/// existing.
///
/// ⚠️ THIS IS ONLY SAFE AT THE END OF `main`, WHICH IS WHY IT IS A SEPARATE FUNCTION TAKING THE
/// FINISHED OUTCOME. `std::process::exit` runs no destructors, so calling it from inside a command
/// would skip guards that ARE observable. Audited, every `Drop` the CLI can reach:
///
///   - `InteractiveTerminalGuard` (main.rs) -- `disable_raw_mode` + `LeaveAlternateScreen`. Skipping
///     it strands the user's terminal in the alternate screen with a hidden cursor. It is a local in
///     `cmd_interactive`, so it has already dropped by the time `run` returns.
///   - `BatchRenderCacheLease` (main.rs) -- writes its manifest back into the in-process session.
///     In-memory only; the cache file is written explicitly elsewhere.
///   - `ActiveIncrementalStateGuard` / `ActiveIncrementalSessionGuard` (fm-layout) -- restore
///     process-local thread state.
///   - `CwdGuard` (main.rs) -- test-only.
///
/// Every one of them is scoped inside `run`, so all have run before this is called. What is skipped
/// is exactly the teardown nobody observes.
///
/// ⚠️ THE FLUSH IS LOAD-BEARING, NOT DEFENSIVE. `process::exit` does NOT flush Rust's buffered
/// `io::stdout`, and this CLI writes JSON to stdout that the head-to-head harness parses. Without
/// the explicit flush this lever silently truncates output -- a correctness defect traded for a
/// few percent, which is the worst possible bargain. The integration test asserts a payload large
/// enough to still be sitting in the buffer arrives whole.
///
/// Flush errors are IGNORED, deliberately, because that is what the Rust runtime already does on
/// the normal return path. A perf lever must not quietly change what happens on a broken pipe.
fn finish(outcome: Result<()>) -> ! {
    use std::io::Write as _;

    let code = match outcome {
        Ok(()) => 0,
        Err(err) => {
            // Matches the `Termination` impl for `Result<(), E: Debug>` that `main` used before,
            // so error text and exit status are unchanged.
            eprintln!("Error: {err:?}");
            1
        }
    };

    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    std::process::exit(code);
}

fn main() -> ! {
    finish(run())
}

fn run() -> Result<()> {
    let cli = Cli::parse();
'''

assert s.count(old) == 1, "main() signature not found in the expected form"
s = s.replace(old, new)
open(p, 'w').write(s)
print('lever applied: main -> run + finish(), teardown skipped after guards drop')
