# Schematic text preservation

Bounded A5 implementation on `cleanup/fix/text-preservation`, based on parent
`3b2ec85`. Existing Kata `konnect-maintained#174e` and
`konnect-maintained#pdps` remain open for coordinator verification and integration.
No Kata write, build, test, formatter, dependency install, toolchain job, Boat
submission, native execution or live CAD operation was performed by this child.

## Source finding and scope

At the base, `misc.rs` reconstructs Text using only its content, position,
effects and UUID. Every other argument disappears. Served `add_wire` in
`crates/konnect-core/src/tools/sch_wiring.rs` loads this model and calls
`overwrite`, so an unrelated real edit exposes the loss.

Read both existing Kata records and the archived candidate
`12d2eba7455b2dfda7b8d130e1bcdf2a242f4289`. Historical green runs are not
evidence for this maintained base. The candidate's list-only filter also
discards unknown scalar arguments. This implementation skips exactly the
leading content argument and retains every other unmodelled argument.

Verified native source semantics at KiCad 10.0.6,
`caf7377e9cb6fa1535ec3596dcb8c99bf44a996e`:

- [`saveText`, lines 1423-1478](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr.cpp#L1423-L1478)
  writes exclusion before `at`, followed by effects and UUID.
- [Text parser, lines 4670-4672](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr_parser.cpp#L4670-L4672)
  reads a boolean, not attribute presence.
- [`SCH_TEXT`, line 64](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/sch_text.cpp#L64)
  defaults exclusion to false.
- [SPICE directive collection, lines 319-334](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/netlist_exporters/netlist_exporter_spice.cpp#L319-L334)
  skips excluded text. Losing `yes` can change simulation input; the archived
  cosmetic assessment describes only its observed `no` case.

Only `misc.rs` changes production behaviour. Symbol attributes from
`qpq3/4nj5`, point wrapping from `hj4c`, root ordering, line ending preservation,
other miscellaneous object types and malformed/duplicate known attributes
are outside this packet.

## Approved compatibility decision

The user explicitly selected "Fix and document the source break".
Text is publicly re-exported, and adding the two fields breaks downstream
exhaustive literals and destructuring. The tracked Rust-source audit found
only the type's own two construction literals and no exhaustive destructures
of this schematic Text type. That does not establish downstream compatibility.

`Text::new` and existing fields keep their signatures/types. Defaults are
`exclude_from_sim: None` and `raw_sub_nodes: vec![]`. The migration instructions
are in `docs/API_MIGRATIONS.md`. No hidden marker inside effects, global side
table or private-field compatibility claim is used.

The flag preserves `yes`, `no` and absence independently. Unknown lists and
scalars retain their values, nesting, scalar/list distinction and mutual order.
They are replayed after UUID, not at original interleaved positions. This is
not a claim that arbitrary future native layouts round-trip byte-identically.

## Native fixture

The coordinator supplied a real Linux KiCad 10.0.8 re-save of the existing
multichannel demo root and its provenance. The two supplied files are unchanged.
See `crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.README.md`.
Native SHA-256 is
`1bbd268c32a780f2314fb0a280405be92800086ff2ac1a5cf50b15f5ca266398`.

Native capture and pre-edit netlist acceptance were reported complete by the
coordinator. They do not prove this implementation works. Both native texts
carry `no`. The yes/absence/unknown variants are explicitly synthetic. Unknown
syntax is not a native-validity test input.

## Regression design

`crates/konnect-schematic-editor/tests/text_round_trip.rs` covers:

- Exact unchanged blocks for both native UUIDs after model save and reload.
- Yes/no/absence values and canonical placement, plus standalone Text round-trip.
- Unknown scalar/list payloads and mutual order without duplicated content.
- Constructor absence and content emitted once.

The source oracle is in `tests/support/text_preservation.rs`. It finds direct
root children using `konnect-sexp`, checks the parser's text count, requires the
two unique expected UUIDs and compares complete balanced source slices. No
whitespace, quoting or line ending normalization is used. Native fixture LF is
a precondition. Whole-file byte identity is intentionally not asserted.

`crates/konnect-core/tests/text_preservation.rs` uses public
`McpHandler::handle_message` and a real `tools/call` to `add_wire`. It requires
successful response data, changed saved bytes, exactly one additional wire,
the requested snapped endpoints and a new native UUID. The preservation test
therefore cannot pass by silently doing no work. The synthetic served matrix
also checks flags and unknown children.

Neither integration target accesses the added Text fields. They remain
compilable with the exact original Text implementation, enabling a real
failing-before control rather than treating a source-compatibility error as
a regression failure.

## Coordinator-only native helper

The ignored native case requires `KONNECT_TEST_KICAD_CLI` as an absolute path
and an absolute, empty `KONNECT_TEST_NATIVE_ARTIFACT_DIR`. It uses the existing
`tests/support/pcb_export_supervisor.py`, not a new timeout wrapper.
The Linux subreaper owns and kills/reaps each native command's process group
even if the Rust future disappears. Rust uses asynchronous output with
`kill_on_drop(false)`, leaving that cleanup owner alive.
Supervisor/config/audit files live in the retained artifact directory, not
the temporary schematic directory that a cancelled Rust future could remove.
Native stdout/stderr are retained before checking success.

Version discovery has its own 10-second deadline. Upgrade, before-netlist
and after-netlist each have independent 45-second deadlines. Every invocation
must have exactly one finished audit with successful native status,
`group_reaped: true`, no timeout and no interruption.

The helper copies actual native bytes before the served edit into artifacts
with SHA-256 provenance and the version/upgrade/before-netlist audits. It then
requires exact text block preservation after served `add_wire`, exports another
native netlist and compares nonempty pin connectivity partitions. It does not
claim whole-netlist byte identity or validate text simulation semantics through
an ordinary connectivity netlist. The native case covers `no` only.

## Failing-before and mutation controls

`crates/konnect-core/tests/text_preservation_controls.py` is for a disposable,
capacity-admitted Linux worker only. It never runs Git, the native CLI or the
network. Dependencies must already be available for its offline Cargo calls.
It pins production, tests, helper, native fixture/provenance, supervisor and
baseline bytes. Only the Text section of `misc.rs` is replaced.

`tests/support/text_before.rs.txt` is the exact original Text section at
`3b2ec85`, including the following boundary marker, not a weakened synthetic
imitation. Its bytes were checked against `git show` without running tests.
The `failing-before` control
puts that section back and requires both native model and served regressions
to fail as actual named assertions, with compilation still successful.
It also checks that the resulting complete `misc.rs` matches original SHA-256
`6eabe5fb06f5aaa5c6d5359b51dc55f894e2a587dc2daf988fdfb07874a8d1a8`.

Isolated controls then remove flag emission, move the flag after `at`, replace
boolean parsing with presence, drop unknown children, duplicate content and
invent an explicit flag for absence. Each starts and ends with all six exact
positive integration tests. Failed compilation, linking, build scripts,
zero-test runs, timeouts and arbitrary nonzero exits never count as caught
mutations. A failed control still enters exact restoration and reserved positive
recovery gates.

The runner is a Linux subreaper, kills/reaps its owned command group and writes
per-command logs/audits. It refuses unowned source drift rather than overwriting
it. Work and recovery budgets are independent and bounded. Do not run alongside
another source writer, in a live checkout, or without admission. Prefer one
control per admitted command if the complete matrix would exceed its budget.

## Execution status and proposed gates

The coordinator approved source review of `18bd317`. Its remote stage-one
command `cmd_ulo0emrptt2j` stopped at `cargo fmt --all -- --check` before tests.
The complete formatting diff is retained in the parent worktree at
`.remote-build-jobs/boat-text-verification/stage1-full.log`.
Only those reported formatting changes were applied here through patches;
no local formatter or toolchain was executed.

The three affected source pins in the control runner were refreshed. Reviewed
post-correction SHA-256 values:

- `crates/konnect-core/tests/text_preservation.rs`:
  `799012f14977b62e4ac8a490c43f713e488b791473b2a18ef8b1f7834e767ec6`
- `crates/konnect-schematic-editor/tests/support/text_preservation.rs`:
  `57ae35105cacaa772d828986bce33f7946aab0877b7733ef1b7eff33aa95d065`
- `crates/konnect-schematic-editor/tests/text_round_trip.rs`:
  `a8e9e744c4accb3d2c3dd2b6526a32dbe196c2cb465b4e64d86f222101ebdce2`

Production `misc.rs`, the baseline section, native fixture and its provenance
are unchanged. Formatting-only rejection is not a failed regression test or
evidence of successful compilation. The coordinator still owns the rerun.

No positive, failing-before, mutation or ignored native test has run in this
child. No formatting or Clippy result is claimed. Coordinator commands after
capacity admission and dependency preparation:

```sh
cargo test -p konnect-schematic-editor --locked --test text_round_trip
cargo test -p konnect-core --locked --test text_preservation
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
python3 crates/konnect-core/tests/text_preservation_controls.py failing-before --coordinator-admitted --output-dir text-controls-before
```

Run each remaining named control in its own fresh output directory, or admit
`all` only with sufficient time. For native acceptance:

```sh
KONNECT_TEST_KICAD_CLI=/absolute/path/to/kicad-cli \
KONNECT_TEST_NATIVE_ARTIFACT_DIR=/absolute/disposable/artifact-directory \
cargo test -p konnect-core --locked --test text_preservation \
  native_resave_then_served_add_wire_keeps_text_and_connectivity \
  -- --ignored --exact --nocapture --test-threads=1
```

These commands are a proposal, not execution evidence. Parent integration,
exact-head verification, Kata reconciliation and accepted-child cleanup remain
coordinator-owned. Do not remove the child worktree before integration.
If coordinator formatting changes a pinned source file, review and refresh the
runner's hashes before executing controls. Never bypass its source-drift check.
