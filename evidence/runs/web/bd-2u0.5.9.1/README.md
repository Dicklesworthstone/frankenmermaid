# Cloudflare Hosting Plan Evidence

This bundle records the checked-in route, cache, and asset strategy for `/web` and `/web_react`.

What it proves:

- `/web` and `/web_react` now share an explicit Cloudflare Pages cache matrix.
- Stable `/pkg/*` runtime assets are revalidating, not `immutable`, until revisioned asset paths exist.
- The cross-surface hosting-plan validator is deterministic across repeated runs.

Replay commands:

```bash
python3 scripts/showcase_harness.py validate-static-web \
  --entry web/index.html \
  --headers web/_headers \
  --contract evidence/contracts/showcase_static_entrypoint_contract.md \
  --log evidence/runs/web/bd-2u0.5.9.1/static-web-route-cache-contract/2026-04-06T21-51-28Z__evidence__log.json

python3 scripts/showcase_harness.py validate-react-web \
  --entry web_react/index.html \
  --headers web_react/_headers \
  --contract evidence/contracts/showcase_react_embedding_contract.md \
  --log evidence/runs/web/bd-2u0.5.9.1/react-web-route-cache-contract/2026-04-06T21-51-28Z__evidence__log.json

python3 scripts/showcase_harness.py validate-hosting-plan \
  --static-headers web/_headers \
  --react-headers web_react/_headers \
  --static-contract evidence/contracts/showcase_static_entrypoint_contract.md \
  --react-contract evidence/contracts/showcase_react_embedding_contract.md \
  --strategy-doc evidence/demo_strategy.md
```

Key artifacts:

- `static-web-route-cache-contract/2026-04-06T21-51-28Z__evidence__summary.json`
- `static-web-route-cache-contract/2026-04-06T21-51-28Z__evidence__log.json`
- `react-web-route-cache-contract/2026-04-06T21-51-28Z__evidence__summary.json`
- `react-web-route-cache-contract/2026-04-06T21-51-28Z__evidence__log.json`
- `cloudflare-hosting-plan/2026-04-06T21-51-28Z__evidence__summary.json`
- `cloudflare-hosting-plan/2026-04-06T21-51-28Z__determinism__summary.json`

This is the pre-`wrangler` acceptance bundle for `bd-2u0.5.9.1`. Actual preview/prod deployment execution and rollback evidence belong in `bd-2u0.5.9.2`.
