# Cloudflare Deploy Smoke Evidence

This bundle records the staged-bundle deployment smoke gate for `/web` and `/web_react`.

What it proves:

- `scripts/cloudflare_pages_ops.py smoke-check` stages the Pages bundle, verifies route integrity, and replays both hosted demo surfaces from the staged artifact instead of the repo root.
- The staged `/web` and `/web_react` bundles both pass the required compare-export and diagnostics-recovery scenarios for the `desktop-default` profile.
- Cross-surface parity remains intact after Pages packaging, and both staged hosts preserve normalized determinism across repeated runs.

Replay commands:

```bash
python3 -m unittest tests.test_static_web_e2e
python3 -m unittest tests.test_cloudflare_pages_ops
python3 -m unittest tests.test_showcase_harness

python3 scripts/cloudflare_pages_ops.py smoke-check \
  --repo-root . \
  --stage-dir dist/cloudflare-pages/bd-2u0.5.9.3-stage \
  --output-root evidence/runs/web/bd-2u0.5.9.3/deploy-smoke \
  --chromium /snap/bin/chromium \
  --scenario-id static-web-compare-export \
  --scenario-id static-web-diagnostics-recovery \
  --profile-id desktop-default \
  --repeat 2 \
  --report-out evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/deploy-smoke-summary.json

python3 scripts/showcase_harness.py validate-e2e-summary \
  --summary evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static/2026-04-06T23-12-47Z__determinism__summary.json \
  --repo-root . \
  --require-replay-bundle

python3 scripts/showcase_harness.py validate-e2e-summary \
  --summary evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/react/2026-04-06T23-12-54Z__determinism__summary.json \
  --repo-root . \
  --require-replay-bundle

python3 scripts/showcase_harness.py compare-host-parity \
  --static-root evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static \
  --react-root evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/react \
  --report-out evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/deploy-smoke-parity-recheck.json
```

Key artifacts:

- `deploy-smoke/deploy-smoke-summary.json`
- `deploy-smoke/deploy-smoke-parity.json`
- `deploy-smoke/deploy-smoke-parity-recheck.json`
- `deploy-smoke/2026-04-06T23-17-14Z__evidence__log.json`
- `deploy-smoke/static/2026-04-06T23-12-47Z__determinism__summary.json`
- `deploy-smoke/react/2026-04-06T23-12-54Z__determinism__summary.json`
- `deploy-smoke/static-replay/replay_manifest.json`
- `deploy-smoke/react-replay/replay_manifest.json`

This is the acceptance bundle for `bd-2u0.5.9.3`. CI now runs the same staged-bundle smoke gate so release evidence fails closed if Cloudflare Pages packaging breaks route integrity, replay behavior, or cross-surface parity.
