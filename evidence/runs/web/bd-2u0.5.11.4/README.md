# React `/web_react` release-grade replay suite

This bundle extends the earlier React smoke run into a retained replay package with exact rerun commands.

## What was run

```bash
python3 scripts/run_static_web_e2e.py \
  --bead-id bd-2u0.5.11.4 \
  --repo-root . \
  --output-root evidence/runs/web/bd-2u0.5.11.4/react \
  --route-prefix /web_react \
  --surface web_react \
  --host-kind react-web \
  --scenario-prefix react-web \
  --chromium /snap/bin/chromium \
  --repeat 5 \
  --replay-bundle-dir evidence/runs/web/bd-2u0.5.11.4/replay
```

Focused replay validation also passed:

```bash
python3 scripts/run_static_web_e2e.py \
  --bead-id bd-2u0.5.11.4 \
  --repo-root . \
  --output-root evidence/runs/web/bd-2u0.5.11.4/replay-check \
  --route-prefix /web_react \
  --surface web_react \
  --host-kind react-web \
  --scenario-prefix react-web \
  --chromium /snap/bin/chromium \
  --repeat 1 \
  --scenario-id static-web-determinism-check \
  --profile-id desktop-default
```

## Artifacts

- replay summary: `react/2026-04-01T17-54-52Z__determinism__summary.json`
- replay manifest: `replay/replay_manifest.json`
- replay helper script: `replay/replay_suite.sh`
- per-run HTML dumps and logs under `react/react-web-*/<profile>/`

## Verdict

- 4 scenarios x 3 profiles x 5 repeats completed
- `stable_normalized_log: true` for every scenario/profile group
- determinism-check produced a stable output hash across all five repeats for every profile
- compare/export, diagnostics recovery, and presenter tour remain semantically stable but byte-level DOM hashes still drift across repeats

That split is now explicit in the retained evidence and replay bundle, which is the main closure for this bead.
