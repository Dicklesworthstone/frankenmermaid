<!-- GENERATED from mermaid_config_schema() (fm-core) — do not edit by hand.
Regenerate: fm-cli config-schema --reference <path> -->

# Mermaid initialization configuration reference

Strict contract for `%%{init: …}%%` payloads and API consumers (schema version 1.0.0). Unknown keys are rejected with an actionable diagnostic naming the offending key. String fields marked case-insensitive accept the listed spellings in any case.

### Root

| Key | Type | Notes |
|---|---|---|
| `flowchart` | object — see [`flowchart`](#flowchart) |  |
| `gantt` | object — see [`gantt`](#gantt) |  |
| `securityLevel` | string — one of `strict` \| `antiscript` \| `loose` (case-insensitive) | Case-insensitive sanitization level: strict, antiscript, or loose. |
| `sequence` | object — see [`sequence`](#sequence) |  |
| `startOnLoad` | boolean | Accepted for compatibility; currently has no runtime effect |
| `theme` | string |  |
| `themeVariables` | object (free-form) |  |

### flowchart

| Key | Type | Notes |
|---|---|---|
| `flowchart.curve` | string | Edge curve style, e.g. basis, linear, natural. |
| `flowchart.direction` | string — one of `lr` \| `rl` \| `tb` \| `td` \| `bt` (case-insensitive) | Case-insensitive layout direction: LR, RL, TB, TD, or BT. |
| `flowchart.nodeSpacing` | number (minimum 0) |  |
| `flowchart.rankDir` | string — one of `lr` \| `rl` \| `tb` \| `td` \| `bt` (case-insensitive) | Case-insensitive rank direction: LR, RL, TB, TD, or BT. Alias of direction. |
| `flowchart.rankSpacing` | number (minimum 0) |  |

### gantt

| Key | Type | Notes |
|---|---|---|
| `gantt.topAxis` | boolean |  |

### sequence

| Key | Type | Notes |
|---|---|---|
| `sequence.mirrorActors` | boolean |  |
| `sequence.showSequenceNumbers` | boolean |  |

## Directive forms

These one-line directive forms embed the configuration object above:

- `constraintsDirective` — One-line Mermaid directive: %%{constraints: <object>}%%. Pattern: `^%%\\{constraints:[^\n]+}%%$`
- `initDirective` — One-line Mermaid directive: %%{init: <config-object>}%%. The payload must validate against this root schema. Pattern: `^%%\\{init:[^\n]+}%%$`
