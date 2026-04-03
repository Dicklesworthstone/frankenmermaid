# bd-2u0.5.8.4 parity evidence

This directory captures the cross-surface parity harness for `/web` and `/web_react`.

## What was run

```bash
python3 scripts/run_static_web_e2e.py \
  --bead-id bd-2u0.5.8.4 \
  --repo-root . \
  --output-root evidence/runs/web/bd-2u0.5.8.4/static \
  --route-prefix /web \
  --surface web \
  --host-kind static-web \
  --scenario-prefix static-web \
  --chromium /snap/bin/chromium \
  --repeat 2

python3 scripts/run_static_web_e2e.py \
  --bead-id bd-2u0.5.8.4 \
  --repo-root . \
  --output-root evidence/runs/web/bd-2u0.5.8.4/react \
  --route-prefix /web_react \
  --surface web_react \
  --host-kind react-web \
  --scenario-prefix react-web \
  --chromium /snap/bin/chromium \
  --repeat 2

python3 scripts/showcase_harness.py compare-host-parity \
  --static-root evidence/runs/web/bd-2u0.5.8.4/static \
  --react-root evidence/runs/web/bd-2u0.5.8.4/react \
  --report-out evidence/runs/web/bd-2u0.5.8.4/2026-04-01T17-49-30Z__parity__report.json
```

## Verdict

- `ok: true`
- compared 12 scenario/profile pairs
- no missing scenario coverage on either host
- no strict parity mismatches

## Acceptable deltas

The parity harness treats the following as expected host-level differences:

- `surface`
- `host_kind`
- `output_artifact_hash`
- `pass_fail_reason`
- `parse_ms`, `layout_ms`, `render_ms` within the configured tolerance
- `input_hash`, because route prefixes differ
- `config_hash`, `revision`, and `trace_id`

Strict parity is still required for renderer/theme selection, diagnostic counts, degradation tier, runtime mode, fallback state, and determinism status.
