# Migrating mermaid-js `initialize()` configuration to frankenmermaid

frankenmermaid accepts mermaid-js-style configuration objects in `%%{init: …}%%` directives and in
its CLI/WASM config surfaces. The supported key set is the **strict** contract in
[`docs/generated/CONFIG_REFERENCE.md`](generated/CONFIG_REFERENCE.md) (generated from
`mermaid_config_schema()` in fm-core, schema version 1.0.0); TypeScript consumers get the same
contract as `docs/generated/mermaid-config.d.ts`.

## What maps directly

| mermaid-js `initialize` key | frankenmermaid | Notes |
|---|---|---|
| `theme` | `theme` | Free string in both; theme names resolve through the same theme machinery. |
| `themeVariables` | `themeVariables` | Pass-through map of `string \| number \| boolean`. |
| `securityLevel` | `securityLevel` | `strict`, `antiscript`, `loose` — case-insensitive in frankenmermaid. |
| `flowchart.rankDir` | `flowchart.rankDir` | `lr`, `rl`, `tb`, `td`, `bt` — case-insensitive. |
| `flowchart.curve` | `flowchart.curve` | Edge curve style (e.g. `basis`, `linear`, `natural`). |
| `startOnLoad` | `startOnLoad` | Accepted for compatibility; currently has no runtime effect. |

## What maps with a rename or a different shape

| mermaid-js key | frankenmermaid | Difference |
|---|---|---|
| `flowchart.dir` / diagram-level direction | `flowchart.direction` | The canonical spelling here is `direction`; `rankDir` is accepted as an alias. |
| `sequence.mirrorActors` | `sequence.mirrorActors` | Same semantics (top and bottom actor rows). |
| `sequence.showSequenceNumbers` | `sequence.showSequenceNumbers` | Equivalent of mermaid-js `sequence.showSequenceNumbers` / `autonumber`. |
| `gantt.topAxis` | `gantt.topAxis` | Same semantics (date axis above the bars). |
| `flowchart.nodeSpacing` / `flowchart.rankSpacing` | same | Minimum gaps in layout units (minimum 0), not pixels. |

## What frankenmermaid rejects (and why)

The config contract is `additionalProperties: false` at every level: an unknown key — root,
`flowchart.*`, `sequence.*`, or `gantt.*` — is a validation error, not a silent ignore. The
diagnostic names the exact dotted key and the value:

```text
themeVariables.foo: {"bar": true}  ... is not supported by config schema 1.0.0
flowchart.direction: diagonal      ... must be one of lr, rl, tb, td, bt (case-insensitive)
```

This is deliberate: mermaid-js silently ignores misspelled keys, which turns a typo
(`showSequenceNumbers` vs `showSequenceNumber`) into a diagram that renders without the feature
the author asked for. Here the same typo stops the parse with the key and a remediation hint.

Keys from mermaid-js that frankenmermaid does not implement yet (and therefore reject):
`fontFamily`, `logLevel`, `altFontFamily`, `flowchart.diagramPadding`, `flowchart.htmlLabels`,
`sequence.actorFontFamily`, `gantt.fontSize`, `class.*`, `er.*`, `journey.*`, `gitGraph.*`,
`themeCSS`, and the rest of the mermaid-js surface that has no frankenmermaid implementation.
File an issue or open a PR for keys you need; adding one is a schema entry plus a projection rule.

## Directive forms

- `%%{init: <config-object>}%%` — one-line directive whose payload validates against the same
  schema. The payload must stay on one line (the parser's documented v1 limitation).
- `%%{constraints: <object>}%%` — frankenmermaid's constraint directives; free-form object.

## Programmatic surfaces

- **Rust**: `fm_core::validate_mermaid_config_value(&serde_json::Value)` — strict validation with
  per-field errors; `fm_core::parse_mermaid_js_config_value` — the compatibility projection into
  `MermaidInitConfig`.
- **CLI**: `fm-cli validate-config <file|inline|->` (same validator), `fm-cli config-schema`
  (the schema itself; `--typescript`/`--reference` regenerate the committed docs).
- **WASM**: `configSchema()` exports the identical schema; validation is the same code path via
  the parse entry points.
