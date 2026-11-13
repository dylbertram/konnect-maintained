# Fix-first work batches

Snapshot: 2026-11-13. Kata project `konnect-maintained` had 261 open records:
248 imported upstream records and 13 maintained-project records. Every record
appears once in the packet lists below. Paired issue/PR records count separately;
261 records do not mean 261 independent defects.

Coverage was checked against a second complete live inventory: all 261 references
matched, with no missing, extra or duplicate assignments.

This is a work plan, not implementation acceptance. Existing claims stay with
their owners. No new Kata tickets, assignments, status changes or closures were
made. Do not open follow-on tickets for findings that cannot be verified.

The imported catalog and `REVIEW_NOTES.md` record historical screening, not
current readiness. This pass inventoried the open tickets, inspected critical
report bodies and review discussions, and spot-checked maintained source/history
at `1afff6e`. It did not run builds, tests or native KiCad operations, inspect
live GitHub settings, or establish that every historical report still reproduces.

## Execution rules

- Before implementation, compare the report with current maintained `main`.
  A merged fix or obsolete code path may call for a no-change audit, not another
  patch. Archived approvals, assignments and green checks are historical.
- Require a minimal supported fixture/environment, an expected result and a
  failing regression or deterministic failure-injection test. A code-reading
  suspicion is not an established live defect.
- Close with the exact tested commit/PR, served-tool coverage where applicable,
  relevant native readback, and preservation/no-write evidence. A candidate PR
  landing does not close acceptance criteria it did not implement.
- Keep unavailable hosts, missing inputs, unsupported versions and uncertain
  persistence explicit. Linux mocks do not verify Mac or Windows behavior.
- Use existing reports and PR-review records. Record unverified remainder there;
  do not create speculative successors, duplicate epics or replacement trackers.
- Load `byskap-builds` before installs, builds or tests. Use the approved
  execution environment and retain the required 500 MB capacity reserve.
- This plan authorizes no live CAD repair, deployment, merge, Actions activation,
  branch-protection change, credential provisioning, tagging or publication.

## Recommended order

1. Let the existing hierarchy-root repair owner continue A1. Keep diagnostic,
   data repair and causal investigation closures distinct.
2. Take independent small wins: the two documentation pairs in A21, mirror/UUID
   read responses in A20, and the export contracts in A13.
3. Prioritize silent corruption and data loss: A2 guarded writes, A3 net joins,
   A5 serialization preservation and A10 library mutations. A4 follows the
   shared junction decisions without weakening A3.
4. In the overlapping PCB code, establish A8 preservation/refusal, correct A6's
   field-angle blocker, then complete A7 additions and safe existing-footprint
   propagation. These remain separate review units.
5. Reconcile already merged hierarchy work before implementing A11. Continue
   independent layer, coordinate, routing, rules, SVG and analysis packets when
   their verification environment is available.
6. Keep R1 at reproduction. D1-D5 are capability choices, not filler for the fix
   queue. O1/O3 need owner decisions; O2 is one dependency family at a time.
7. Reconcile T1/H1 from completed evidence or explicit disposition. Do not close
   umbrellas because one child lands.

## Fix-first packets

These 23 work families cover 155 records. They are candidates for verification,
not 23 approved implementations. Each split below is a smaller review unit;
in particular, a 16-record family is not one large PR.

### A1. Finish the active hierarchy-root work

Tickets: `qd68 q9sy 8vb8`

The repair session is active. Do not duplicate or take over its claimed work.
[The repair ticket](https://kata.bertramdesigns.com/kata?issue=01M7AMR0K419N34B4T8YVKQD68#direct=1)
needs bounded, revision-guarded root-prefix repair, native hierarchy/netlist
validation, identity-preserving readback and rollback/refusal tests. Its live
hardware repair has separate recorded authorization.

`q9sy` already records restored sanitized diagnostics and unchanged CAD hashes.
Reconcile its original diagnostic symptom separately from the blocked transfer.
`8vb8` needs causal provenance or explicit limits on what can be established,
plus a synthetic writer regression. A successful repair does not explain the
origin. The mismatch predates the new transfer implementation.

### A2. Prevent stale overwrites and report persistence honestly

Tickets: `2xzq dyxj 7bgg djvc svj3 xsz0 k1x1 81c7`

Split into board fallback protection `7bgg/djvc`, project rename `svj3/xsz0`,
project-file protection `dyxj`, footprint outcomes `k1x1`, transaction diagnostics
`81c7`, and a separately diagnosed same-file queue `2xzq`.

Inject an intervening save deterministically and prove newer bytes survive.
Distinguish pre-commit refusal from post-commit uncertain persistence through
served calls and committed readback. Verify rename rollback or truthful partial
state. Queuing must cover the full read/validate/commit interval, canonical
identity, cancellation, bounds and multi-file deadlock avoidance while retaining
external-edit checks and independent-file concurrency. Never blindly retry an
uncertain mutation. Reuse A15's overlapping project-writer changes once.

### A3. Stop schematic routes from silently joining other nets

Tickets: `qe1n mta5 3b3b 0y95 2h48`

Review `mta5`'s route guard/local junction filter before adapting it to stubs.
Keep ambiguous-reference selection independently tested.

Native before/after netlists must show no accidental joins from L-routes,
stubs, remote junction-less T intersections or duplicate references. Unsafe
routes must refuse without writing. Cover single/batch tools, served dispatch,
alternative routes and negative controls with the guard disabled.

### A4. Make junction creation and pruning agree with KiCad

Tickets: `5q1b pxm3 s0xk tb94`

Use A3's local junction handling. Review the distinct-exit predicate, then adopt
it for passthrough stubs and moved/deleted pins. Test wire-end T shapes,
pin/wire corners, two-exit cases and untouched remote intersections.

Check both drawing and native netlists. Correct dots must not introduce shorts;
delete/move pruning must not retain or remove dots by an inconsistent rule.

### A5. Preserve schematic fields and untouched serialization

Tickets: `27jb we5v 174e pdps qpq3 4nj5 hj4c`

Split private-field parsing, text-child preservation, canonical symbol
serialization and remaining older-format churn. A harmless served edit must
preserve untouched private names/values, unknown text children,
`fields_autoplaced` values and attribute positions on KiCad-written fixtures.

The `pdps/4nj5` reviews still require native fixtures, current-base evidence and
an honest public-struct compatibility decision. Do not claim byte-stable
old-format support from a newer-format test or expand these fixes into an
unbounded serializer rewrite.

### A6. Correct library-field fidelity and refresh no-op behavior

Tickets: `annz z705 x698 asfr fnn9`

First fix `annz`'s not-upright angle blocker before accepting
`fix/789-library-fields`.
[The existing review record](https://kata.bertramdesigns.com/kata?issue=01M5YYNTEY13WE314TX038ANNZ#direct=1)
contains the unresolved finding. Test a raw 180-degree not-upright field and a zero-degree
field on a footprint rotated 180 degrees, plus front/back and upright controls.
Current source contains unconditional readable-angle normalization; runtime
reachability and the candidate branch still need regression evidence.

Then review direct-placement field fidelity and `fnn9`'s stroke/no-op changes
as separate increments. Compare saved layer, thickness, visibility, raw angle
and keep-upright state against native output. A second unchanged refresh must
plan `noop` without masking meaningful changes. Account for all remaining
`z705` criteria before closing the broader report.

### A7. Carry symbol fields without destructive footprint updates

Tickets: `w7y5 5g20`

Follow the explicit split decision on `w7y5`, not the broader historical PR
description. Additions follow corrected A6 library fields. Existing-footprint
propagation waits for A8 preservation/refusal evidence.

Check LCSC, MPN, Datasheet, Description and custom-field precedence/presentation,
dry-run changes, committed readback and native schematic-parity DRC. An
additions-only increment is partial progress, not closure of changed-field
propagation. Keep the remainder on the existing issue.

### A8. Protect PCB sync identity, exclusions and native children

Tickets: `agmr vspd ms3p 3bqz emvb th5d 0753 qe1f c8mm 1d0x xa45`

First verify `3bqz/emvb` preservation/refusal for whole-footprint replacement.
Dry run and apply must refuse before dropping groups, points or variants. This
gates A7's wider update path.

Other small packets cover BOM versus board exclusion, library placement flags,
pad fabrication properties, exact-root identity normalization, zone/net routing
accounting and missing-layer validation. Exclude-from-BOM must not mean
exclude-from-board; zones count on their own net only. Preserve fabrication
properties or refuse unsupported data explicitly. Verify native/saved readback
and no-write refusals. Do not normalize foreign roots indiscriminately.

### A9. Fix board layer, side and text semantics

Tickets: `17j7 7kam r70k n9nb c5ea j748 w25a`

Split layer validation/serialization, inner-layer mirroring and file/IPC text
parity. Reject unknown layers before mutation, write aliases supported KiCad can
load, mirror the full copper stack, and compare thickness/style on native output.

`w25a` is a version-specific upstream units-block limitation. Reproduce it with
a native/manual control and establish a warning, refusal or supported remedy.
Do not promise a Konnect fix for KiCad behavior without evidence.

### A10. Make library mutation and deletion safe

Tickets: `0fbq 85bz adhb b0pj 9f7g ppm1 w8bp 6w9c`

Review `b0pj`'s UTF-8 BOM/private-field findings before widening its field editor.
Apply creation/deletion guards on the chosen structural reader. Review `85bz`'s
remaining duplicate-name, structured-error and persistence-outcome gaps.

Deletion must select exactly one top-level symbol, refuse ambiguity and verify
absence. Creation must refuse an existing name. Cover native tab/LF output,
CRLF, concurrent saves, post-write uncertainty, BOM input and private fields.
Metadata edits must preserve supported attributes such as
`allow_soldermask_bridges` and report intentional replacement. Coordinate with
A2 rather than duplicating its outcome fix. Neither old PR is accepted unchanged.

### A11. Close residual hierarchy and instance-identity gaps

Tickets: `f895 by2z 3dcn pyq5 7rgq 5f5w 6kff fzx8`

First compare each report with merged nested-hierarchy/root-identity work.
Reproduce only the remaining multi-root ownership, foreign-project records,
reattachment/repointing, page allocation, annotation or unit-selection behavior.

Use native projects with reused/nested sheets, multiple roots and multi-unit
components. Preserve stable identities and refuse ambiguity. No stale paths
after repoint/re-add. `7rgq` still needs whole-hierarchy annotation, not another
per-file fix. `5f5w` needs a compact unit-selection reproduction. A1's bounded
root-prefix repair is not a blanket foreign-instance cleanup.

### A12. Use structural edits and stable coordinates

Tickets: `f40k 1shq v9fc mzm2 tbmg ktsj afj0 w61h m0n9 3fvj 1x6d 7qp9 d2gx 7z65 fnyt 1s1s`

Split string-safe label/component edits, routing-copy parsing, serialized
coordinate rounding, response rounding and field-rotation semantics.

Names/values containing `(at` must not be rewritten as geometry. Labels retain
associated fields; routing-copy accepts KiCad-saved structures. Verify
millimetre/nanometre conversion, negative values and computed round trips without
shifting geometry. Field rotations need tests on rotated symbols and exact
unit targets. Do not substitute cosmetic output truncation for geometry fixes.

### A13. Repair export contracts and manufacturing completeness

Tickets: `pe7h 459k xsd6 c2y7 h22t`

Four independent changes: PCB/schematic netlist CLI dispatch, omitted-layer
defaults, schematic SVG output naming and literal hyphenated BOM designators.

Check actual supported CLI commands, exit status and artifact content. Omitted
layers must produce the documented view; output names must be honored.
`SMB-19-A1` must survive into a complete package, with missing artifacts reported
honestly. Command-string tests alone are insufficient. No ordering or real-BOM
qualification is authorized.

### A14. Make Specctra geometry and native fallback trustworthy

Tickets: `bg7k h596 pwsw rakh zyxn 32k8 sye1 fwxg stvq`

Split per-layer padstack refusal, single pad rotation, roundrect support, native
DSN adoption and native/Rust gating order. Compare geometry with KiCad-produced
DSN, including courtyard polygons and half-micrometre coordinates.

Never represent every copper layer by its front pad shape. Native `require`
must not silently fall back. `stvq` needs an explicitly supported nightly scope
and environment; do not turn these fixes into unverified KiCad 11/multilayer
support. Coordinate with D4 without waiting for a broad routing redesign.

### A15. Write design rules that KiCad actually loads

Tickets: `03fj 9cyj 7fk1 ntkh fpep b4xx 0m5y g8ak`

Split range/readback correctness, copper-edge/silkscreen minima, annular/hole
minima and differential-pair settings. `ntkh` precedes stacked `fpep`.
Reuse A2's overlapping revision-checked project writer once.

Reject values KiCad would reset rather than echoing them as accepted. Load and
re-save the project, verify effective settings/native DRC, preserve unrelated
JSON and refuse intervening edits. Review only each stacked PR's unique change.

### A16. Preserve SVG transforms and fill topology

Tickets: `twc8 arzt`

Resolve physical-unit/viewBox and nested transforms before checking compound
holes at their final coordinates. Output must have the expected dimensions and
orientation. Holes stay holes or unsupported topology is refused without writing.
Cover units, nested transforms, fill rules and no-write refusal. Never silently
flatten a hole into copper.

### A17. Make analysis match real geometry and pin types

Tickets: `vr0z wzb4 2vnw s4rg dga6 cah2 1krd 25ze dq8c`

Split power-pin typing, valid power-symbol contact, text overlap, footprint-owned
outline/edge-mounted connector handling, fanout limits and custom-pad
approximations. Each needs a measurable scoped contract before implementation.

Decoupling must use electrical pin types and exclude no-connects. Overlap checks
must avoid both false power-pin collisions and missed label/field collisions.
Document geometry approximations. `dq8c` needs native before/after copper-net
evidence: clean DRC cannot prove placement did not silently re-net copper.

### A18. Normalize IPC configuration and path diagnostics

Tickets: `4545 qape m8xv 8wy9 4cdq r7vj`

Review blank/whitespace configuration separately from endpoint selection.
`qape`'s whitespace normalization is not proof that `4545`'s connection failure
is fixed. Reproduce or explicitly narrow the original symptom.

Test resolved requested-document identities, Windows raw/verbatim path display
and truthful absent/unsupported states. Validate real Windows identity behavior
where required. Do not close unrelated editors or infer ownership from a path.
The initial stabilization review explicitly rejected this as an easy paired
closure.

### A19. Report real UI startup and avoid focus theft

Tickets: `8brf d6gn mek1 71s1`

Startup reporting and Mac focus behavior are independent packets. A KiCad
process that exits during bounded startup must not be reported as launched.
Mac start/stop actions must leave the intended PCB editor focused on a real
verified Mac setup. Headless Linux evidence does not close the Mac report.

### A20. Make tool responses and target selection usable

Tickets: `t7yx 9ane gvy3 f9am dpwp a94t h1aj qdmh 8q9d dtgq bnht nm85 7s41`

Start with mirror reporting and stable UUIDs for labels/net objects. Then handle
unknown-net semantics, actual net pins, power-reference prefixes, schema branch
diagnostics, argument/path consistency and bounded mock-server release.

Exercise served catalogs/calls. Prefer additive aliases and documented
migrations to unexplained argument renames. `7s41` needs prompt release-time
evidence, not eventual endpoint disappearance. These read-only/schema packets
need not wait for writer refactors.

### A21. Take small documentation wins

Tickets: `mvy5 7enh t2ms g10n hj15 n0ty ev8k 87d4`

First take `mvy5/7enh` and `t2ms/g10n`. Verify served `batch_delete`
descriptions distinguish UUIDs from reference arguments, and Muse
`eager_toolsets` guidance matches discovery behavior.

Keep fixture-normalization and file-ownership guidance separate. Scope the
broad quickstart request before rewriting it. `87d4` needs a CLI contract/test
for custom skill roots and clients, not just prose. Do not promise nonexistent
CI coverage or mark the broader items done with the two tiny documentation fixes.

### A22. Stop misleading catalogue search and cost answers

Tickets: `0mgw ggdj 6y7v n8x7`

Split whole-value search, unavailable-data semantics and cost accounting.
Recorded-data fixtures should prove `20pF` does not match `220pF`, missing
stock/price is not a real zero, and costs use supported prices/fees or disclose
their assumptions and missing inputs.

Do not call fixture data fresh stock/pricing or recommend nonexistent BOM tools.
No distributor research or ordering is part of this grouping.

### A23. Repair templates without promising unwritten connections

Tickets: `qhm6 hcfk 6889`

Review the existing symbol/pin correction separately from `apply_template`'s
placement-versus-wiring contract. Templates must use actual supported library
symbols/pins and suit the stated MCU/regulator. Verify connectivity with native
netlists. Placement alone must not be reported as completed wiring or electrical
qualification.

## Reproduction-only queue

### R1. Require the actual environment before promising a fix

Tickets: `bwch x94t b691 5m88 mdvz hmzs rns5 aezd 9vpp tng9`

Group the client catalog reports `b691/5m88/mdvz` by exact client/version.
Keep schematic write hangs, router interaction, `AS_UNHANDLED` preflight,
intermittent rotation readback, Windows endpoint collision, Mac signing and
nightly discovery separate.

Require sanitized minimal inputs, exact versions, a bounded failing call,
expected behavior and a comparison/control. For `rns5`, repeat serial native
runs and diagnose stale rotation ordering; do not weaken geometry assertions.
For `aezd`, handshake failure on a bound endpoint is not proof of a malicious
peer. If a host or reproducer is unavailable, record the blocker on the existing
ticket. Do not open successors or label these implementation-ready.

## Deferred capabilities

These 71 records remain visible, but do not outrank verified corruption fixes.
Selecting a capability requires its own bounded contract and available evidence.

### D1. Schematic capabilities

Tickets: `hp9r 2hsg vmxn jnrk 62zh x71k cgem 4cza 2yex jnze rghk vzmz kb4k mx91 34kv 2r69 3mm1 t2e9 7m5m`

Per-unit mirroring, labels/fields, custom power symbols, named-pin/rail placement,
rigid moves, batch field placement, hierarchy-label batches, library refresh,
annotations, pin-grid guidance and exclusion controls stay separate. Named-pin
placement precedes its stacked rail-group PR. Require exact target identity,
native fixtures, ambiguity refusal and saved readback before adoption.

### D2. Library and import capabilities

Tickets: `hmsy 7355 3vw9 wzpg 5z8m 33p5 saqk zfms s2cm gsmr gs1t 6msw 2z30 692m`

Review oval-drill/pad-fabrication candidates separately from authoring controls,
database libraries, unregister operations and research integrations.
IntLib import is a preservation feature, not a quick converter script. It needs
a supported backend, source fingerprints, complete symbol/pad/region mappings,
loss reporting, rollback/refusal and native readback. Preserve the PHYTEC
source's separate copper/mask/paste regions and report its known discrepancies.
No real supplier import, replacement or hardware qualification is authorized.

### D3. Opt-in footprint text and broader refresh

Tickets: `6ff7 v344 srvp 8fcs`

A6 comes first; `v344` is stacked on `annz`. Choose reset sizes/styles,
reset positions and manual placed-field controls explicitly. Defaults preserve
placed presentation. Verify text, raw angles and keep-upright front/back.
One option cannot close `8fcs`'s broader metadata-refresh coverage.

### D4. PCB editing, setup and routing expansion

Tickets: `75k6 6amt 2jr4 vkkh d272 96q2 2p6m anj5 5j2a eksr d3tb 6mjh 8rs9 sdzf 2s18 a0jm n20e 4hvh x6ny b8fw smnr zfx7 4jfp`

Keep zone deletion, schematic-less editing, bulk routing, locks, rules, outlines,
stackup/layer expansion, keepouts, live page settings and routed-net policy
bounded. Board title-block `b8fw` precedes stacked schematic title-block `zfx7`.

Require exact document/revision binding and native preservation/readback.
`4jfp` is not permission to bypass A8's safety guard. Multilayer/Freerouting
expansion needs an explicit profile and native round trip. `x6ny` remains a
board-source consumer under `t9sx`, not a separate broad rewrite.

### D5. Optional execution, discovery and response bounds

Tickets: `dq6v 6rtf nw2m 5ttz 9tfp ye8f wx5r q7nw znaa nb15 87ww`

Separate discovery/status, response-size/truncation, endpoint reopening,
autostart, catalogue enrichment and vendored-protocol provenance contracts.
`znaa` stays deferred until an ngspice isolation/input policy is accepted:
netlists can execute commands and access files. Availability detection alone
does not justify exposing an unsafe subprocess tool. Do not create speculative
execution/security tickets.

## Operational work

### O1. Finish approved CI and governance work

Tickets: `amh7 yvt7 zktt pcvp 244j teav`

PR #8 is recorded merged. Do not redo its read-only CI preparation. Reconfirm
live settings, then obtain explicit approval before enabling Actions or
enforcing actual check names. Hosted exact-commit checks and live protection
readback remain necessary for `zktt/yvt7`.

Keep package identity, release provenance and supported-platform evidence
separate. `amh7` remains open while its children remain open. Do not restore
deleted release automation, publish, or treat quarantining it as a completed
distribution pipeline.

### O2. Review dependency branches one family at a time

Tickets: `f0kv cnvt`

Separate gix, grouped core dependencies, the excluded viewer crate and active
action-pin updates. Coordinate sha2/hex encoding rather than duplicating it.
Choose intended versions and review lockfiles. Workspace tests do not build
the excluded viewer. Reconstruct only active workflow pins, never deleted
release workflows. Retire a branch only after a landed replacement or explicit
superseding decision.

### O3. Defer distribution channels to identity/release decisions

Tickets: `xnny pccx`

Settle `244j` and release policy before choosing PCM or Homebrew destinations.
Require an owned destination, tested install/upgrade/uninstall behavior and
source/attribution/signing compliance. No publication authorization is implied.

## Tracker and historical accounting

### T1. Reconcile umbrellas after concrete work

Tickets: `11h5 0s4c qsfx q6bs g7x5 s583 25md t9sx 784t vm08 avx1`

These 11 records coordinate acceptance; they are not 11 additional
implementation tasks. Preserve `11h5`'s original fixed 47-item cohort. This new
inventory does not replace or enlarge it. One child, one platform run or one
module does not complete an umbrella. `qsfx` is an architectural register,
not permission for one giant refactor. Do not create duplicate epics.

### H1. Review historical decisions without resurrecting dead work

Tickets: `148k jtmk pa3x h0g3`

`148k` is contributor/migration context. `jtmk` concerns the old cdylib server,
not the now-closed editor-shutdown feature `nvrs`. `pa3x/h0g3` retain archived
wontfix decisions requiring local review, not automatic implementation.

Use maintained-code evidence and an explicit owner disposition for any
no-change, wontfix or superseded closure. Do not close records solely because
their upstream labels look historical.

## Completion accounting

Record the current reproducer, exact tested revision, actual patch scope,
native/platform limits and remaining acceptance on the existing ticket.
If only an increment lands, keep the terminal requirement open.

No closure forecast or time estimate is promised. Reproduction may establish an
existing merged fix, an evidence-backed no-change audit or an unavailable
prerequisite instead of a new patch. Those are valid outcomes only when the
evidence and limits are explicit.
