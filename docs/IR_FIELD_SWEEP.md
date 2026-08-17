# IR dead-field sweep — method note

Relocated out of `docs/NEGATIVE_EVIDENCE.md`: that file is a ledger of MEASURED verdicts and its
preflight contract requires an A/A null or a counted mechanism plus an executing ELF sha for every
`###` row. This note carries no ratio, no arm and no ELF, so the guard was right to refuse it. The
content is unchanged; only its home is.

### Method note: the dead-IR-field sweep is spent — 182 of 183 fields are consumed (2026-08-17)

**Not a verdict row: no ratio, no arm, no ELF — this is a method note.** Do not re-run this sweep. It produced two real defects and is now spent; the residue is one
filed bead. Recorded so the next agent spends a quiet window on something else.

**Result.** Every `pub` field of every `Ir*` struct in `fm-core` (183 fields across 40 structs),
checked against every identifier appearing in `fm-layout`, `fm-render-svg`, `fm-render-term` and
`fm-render-canvas` sources. Three candidates survived:

| candidate | disposition |
|---|---|
| `IrPort.side_hint` | **REAL — bd-zce4.** architecture-beta drops per-edge sides; the parser creates no ports at all, so the field is dead by construction. |
| `IrGanttMeta.date_format` | Not a defect. Consumed at PARSE time (dates normalise to `Absolute`), which is why no renderer names it. Verified: `05-03-2024` reads as 5 March under `DD-MM-YYYY` and 3 May under `MM-DD-YYYY`. |
| `IrEdge.extras` | False positive. ER notation and class cardinality both reach the document through labels; the feature-parity gate's own `er_*` and `class_cardinality` cases confirm the output changes. |

**What the method caught before it was written down:** bd-jgco (gitGraph branch names) and bd-jerh
(ER attribute comments) — both parsed, stored, and drawn by nothing.

**⚠️ THE SWEEP WAS BROKEN THE FIRST TIME, AND ITS OUTPUT LOOKED LIKE A JACKPOT.** The initial widened
run reported **all 183 fields as dead**, including `IrNode.label` and `IrPieSlice.value`. Cause: in a
bracket expression, `[.\w]` matches the literal characters `.`, `\` and `w` — not a word class — so
the consumer-token set was garbage. Nothing about the output said "broken"; it said "183 defects".

**A uniform verdict across every case is the signature of a broken instrument, not a discovery.**
This is the same failure shape as grepping a validate report for `error` and matching its own header.
The corrected sweep therefore carries a SANITY CONTROL: it asserts that fields which must obviously
be consumed (`IrNode.label`, `IrNode.shape`, `IrPieSlice.value`, `IrLabel.text`) are seen as
consumed, and refuses to report anything at all if they are not. Any future re-scope of this sweep
must keep that control, or its silence is worthless.

**Scope note that mattered.** An earlier run of this sweep matched only `pub struct Ir\w*Meta`, and
reported the class clean. That result was true for the structs it covered and said nothing about the
rest — bd-jerh was sitting in `IrEntityAttribute`, outside the pattern. A sweep's scope belongs in
its conclusion.
