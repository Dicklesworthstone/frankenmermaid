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

### Addendum (2026-08-18): a STRICTER matcher is not a better one — it just swaps the error

I re-ran this idea under the build freeze with a member-access matcher (`.field` / `field(`) instead
of the bare-token match above, on the theory that bare tokens over-count. It found 43 candidates
where this note found 3, and the extra ones were WRONG.

**The false positive that nearly became a bead: `IrSequenceMeta.autonumber`.** My matcher scored it
dead in every renderer, and `autonumber` is a real mermaid feature, so it looked like a live defect
worth implementing. It is already implemented: `fm-render-term` reads it as
`meta.autonumber_value(edge_path.edge_index)` and `fm-render-svg` carries six references, with a
terminal test pinning the configured start and increment. A field consumed through an ACCESSOR is
invisible to `.field` matching — `\b` does not sit between `autonumber` and `autonumber_value`, so
the stricter pattern rejects the very call that proves the field alive.

So the two matchers fail in opposite directions and neither is safe alone:

| matcher | failure mode |
|---|---|
| bare token (this note's original) | over-counts: a comment, a local, or a test name reads as consumption |
| member access (`.field`) | under-counts: accessor-mediated reads look dead |

**What IS reliable for proving absence: a bare-token count of ZERO.** That is how
`IrNodeInteraction.tooltip` was confirmed dead the same day (bd-bk7h) — zero occurrences of the
string `tooltip` anywhere in fm-render-svg or fm-render-canvas sources, so there was no accessor to
miss. Zero bare tokens is proof; a zero member-access count is only a lead.

**⚠️ MY OWN CONTROL PASSED WHILE THE INSTRUMENT WAS BROKEN, AGAIN, and differently.** My first
version's brace matcher ran past every struct's end, crediting `IrEdgeExtras` with `budget_broker`,
`cpu_pressure_permille` and ~200 other fields that are not its. The usage control I had written
(`label`, `shape`, `arrow` must read as consumed) passed anyway, because it only varied "is a used
field seen as used" and never varied STRUCT ATTRIBUTION. I added a structural control — assert
`IrEdgeExtras` holds exactly its seven known fields — and only then did the boundary bug surface.
A control proves the dimension it varies and no other; that is the same lesson this note already
records about the `\w` bug, arriving by a different route.

**The disposition of this note is unchanged: the sweep is spent.** Re-running it with a different
regex does not make it un-spent, and the one lead the new matcher produced was already-shipped
behaviour.

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
