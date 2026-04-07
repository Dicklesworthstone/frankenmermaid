# Cloudflare Deploy Ops Evidence

This bundle records the checked-in Pages/Wrangler deployment automation for `/web` and `/web_react`.

What it proves:

- `wrangler.jsonc` now defines a Pages deployment surface with explicit preview and production environment metadata.
- `scripts/cloudflare_pages_ops.py` stages a deterministic Pages bundle, prints preview/prod deploy commands, and emits a rollback drill payload that stays honest about Cloudflare's dashboard-mediated rollback flow.
- `scripts/showcase_harness.py validate-cloudflare-deploy-ops` gates the runbook against the checked-in contracts and dry-run command surface.

Replay commands:

```bash
python3 -m unittest tests.test_cloudflare_pages_ops
python3 -m unittest tests.test_showcase_harness

python3 scripts/showcase_harness.py validate-cloudflare-deploy-ops \
  --wrangler-config wrangler.jsonc \
  --ops-script scripts/cloudflare_pages_ops.py \
  --static-contract evidence/contracts/showcase_static_entrypoint_contract.md \
  --react-contract evidence/contracts/showcase_react_embedding_contract.md \
  --strategy-doc evidence/demo_strategy.md

python3 scripts/cloudflare_pages_ops.py preview-deploy \
  --repo-root . \
  --output-dir dist/cloudflare-pages/bd-2u0.5.9.2-preview \
  --project-name frankenmermaid \
  --branch preview-web \
  --commit-hash "$(git rev-parse HEAD)" \
  --commit-message "bd-2u0.5.9.2 preview drill" \
  --dry-run

python3 scripts/cloudflare_pages_ops.py production-deploy \
  --repo-root . \
  --output-dir dist/cloudflare-pages/bd-2u0.5.9.2-production \
  --project-name frankenmermaid \
  --commit-hash "$(git rev-parse HEAD)" \
  --commit-message "bd-2u0.5.9.2 production drill" \
  --dry-run

python3 scripts/cloudflare_pages_ops.py rollback-drill \
  --account-id demo-account \
  --project-name frankenmermaid \
  --deployment-id production-deployment-id \
  --reason "bd-2u0.5.9.2 rollback drill" \
  --dry-run
```

Key artifacts:

- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__stage-bundle.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__create-project.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__preview-deploy.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__production-deploy.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__rollback-drill.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__validator.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__evidence__log.json`
- `cloudflare-deploy-ops/2026-04-06T22-07-20Z__determinism__summary.json`

The rollback drill intentionally stops at a validated preflight payload. Cloudflare's current Pages docs describe rollback execution in the dashboard, not as a dedicated Wrangler subcommand, so the runbook keeps execution honesty explicit instead of pretending there is a fully automated CLI rollback path when the docs do not support that claim.
