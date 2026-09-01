#!/usr/bin/env python3
"""Assert that a head-to-head benchmark ELF names the source revision it was built from.

⚠️ WHY THIS EXISTS AT ALL (bd-vdrx9). The ELF carries the revision it was compiled from so a reader
-- and ``scripts/headtohead/run.mjs`` -- can bind a measurement to a source revision. On an RCH
worker the build script cannot derive that revision: the worker builds the transferred source inside
a directory carrying its OWN ``.git``, left wherever it last was. A build of ``aaa334d9`` stamped
``43480807`` -- a real commit, 35 behind, well-formed enough to pass every shape check downstream.
``crates/fm-cli/build.rs`` no longer guesses, so the only correct value is the one the CALLER knows,
passed in ``FM_H2H_BUILD_GIT_REV``. Whether that variable actually survived the trip to the worker
is NOT observable from inside the build -- RCH forwards a variable only if ``[environment]
allowlist`` names it, and an allowlist that silently stops applying looks exactly like a successful
build. The ELF's own ``__binary__`` record is the only evidence, so this reads it back.

Usage: assert_build_stamp.py <headtohead-elf> <expected-40-hex-revision>
"""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys
import tempfile

PROBE_CORPUS = '[{"id":"stamp_probe","texts":["flowchart LR\\n  A-->B"],"reps":1,"warmup":1}]'


def binary_record(elf: str) -> dict:
    """Run the ELF over a one-diagram corpus and return its ``__binary__`` provenance line."""
    with tempfile.TemporaryDirectory() as work:
        corpus = pathlib.Path(work) / "corpus.json"
        corpus.write_text(PROBE_CORPUS, encoding="utf-8")
        completed = subprocess.run(
            [elf, str(corpus)], capture_output=True, text=True, check=False
        )
    if completed.returncode != 0:
        sys.exit(
            f"[assert_build_stamp] the ELF exited {completed.returncode} on a one-diagram corpus:\n"
            f"{completed.stderr.strip()}"
        )
    for line in completed.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            candidate = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(candidate, dict) and candidate.get("id") == "__binary__":
            return candidate
    sys.exit("[assert_build_stamp] the ELF emitted no __binary__ provenance record")


def main(argv: list[str]) -> None:
    if len(argv) != 3:
        sys.exit(f"usage: {argv[0]} <headtohead-elf> <expected-40-hex-revision>")
    elf, want = argv[1], argv[2]
    if not re.fullmatch(r"[0-9a-f]{40}", want):
        sys.exit(f"[assert_build_stamp] {want!r} is not a 40-hex Git revision")

    record = binary_record(elf)
    got = record.get("build_git_revision")
    source = record.get("build_git_revision_source")
    print(
        f"[assert_build_stamp] elf_sha256={record.get('elf_sha256')} "
        f"bytes={record.get('elf_bytes')}"
    )
    print(f"[assert_build_stamp] build_git_revision={got} (source={source})")

    if got == want:
        print(f"[assert_build_stamp] OK: the ELF names the source it was built from ({want})")
        return

    # The two failures are different diagnoses and must not be reported as one. A non-``env``
    # provenance means the caller's value never arrived; an ``env`` provenance that disagrees means
    # a different value was passed, which is a caller bug rather than a transport one.
    if source != "env":
        sys.exit(
            f"[assert_build_stamp] STAMP NOT PROPAGATED: expected {want}, ELF says {got!r} "
            f"derived from {source!r}. FM_H2H_BUILD_GIT_REV did not reach the build. Check that "
            "`rch config get environment.allowlist` still lists FM_H2H_BUILD_GIT_REV."
        )
    sys.exit(
        f"[assert_build_stamp] STAMP MISMATCH: expected {want}, ELF says {got!r} "
        f"(source={source!r})"
    )


if __name__ == "__main__":
    main(sys.argv)
