# React `/web_react` Replay

Use the shared showcase harness to validate the current `/web_react` host adapter:

```bash
python3 scripts/showcase_harness.py validate-react-web \
  --entry web_react/index.html \
  --headers web_react/_headers \
  --contract evidence/contracts/showcase_react_embedding_contract.md
```

What it validates:

- the route bootstraps the standalone showcase artifact rather than forking host semantics
- the injected host markers switch to `react-web`
- the route defines a stable `showcase-react-root`
- deep-link/cache rules exist for `/web_react` and `/web_react/*`
- the checked-in React embedding contract still matches the route surface

Current evidence artifact:

- `react-web-entrypoint`
