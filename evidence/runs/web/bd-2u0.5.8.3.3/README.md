# React `/web_react` E2E Replay

Use the shared headless Chromium runner to exercise the hosted React route with the same scenario corpus used by `/web`:

```bash
python3 scripts/run_static_web_e2e.py \
  --bead-id bd-2u0.5.8.3.3 \
  --repo-root . \
  --output-root evidence/runs/web/bd-2u0.5.8.3.3 \
  --route-prefix /web_react \
  --surface web_react \
  --host-kind react-web \
  --scenario-prefix react-web \
  --chromium /snap/bin/chromium
```

What it does:

- serves the repository root over a local HTTP server
- opens `/web_react` in headless Chromium with query-restored scenarios
- captures post-JavaScript DOM dumps as reviewable HTML artifacts
- emits schema-valid JSON logs with `surface=web_react` and `host_kind=react-web`
- reuses the same determinism summary flow as `/web`

Current scenarios:

- `react-web-compare-export`
- `react-web-diagnostics-recovery`
- `react-web-determinism-check`
- `react-web-presenter-tour`
