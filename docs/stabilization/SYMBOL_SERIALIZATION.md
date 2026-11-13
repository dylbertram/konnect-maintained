# Symbol serialization preservation

Bounded A5 work for
[konnect-maintained#qpq3](https://kata.bertramdesigns.com/kata?issue=01M5YYNKFJF38CJ5QJRB0NQPQ3#direct=1)
and candidate
[konnect-maintained#4nj5](https://kata.bertramdesigns.com/kata?issue=01M5YYNKFWW6DX5T1AMZQC4NJ5#direct=1),
reconstructed on `76779157d73a14f46669320e14205b0f67f0a21d`.
The archived PR694 at `57087b2798624017aea15da5778995561f13cf8a` is evidence,
not an adopted patch or current verification.

## Approved boundary

The user selected "Preserve syntax unless explicitly edited." This packet adds
no public Symbol fields or changes to existing types. It changes production
`crates/konnect-schematic-editor/src/schematic/symbol.rs` only. Dedicated model,
served, native acceptance and control tests plus documentation are separate.
Text, Sheet, labels, Property, wire production, the generic writer and point
wrapping are not modified.

The old parser kept only flag presence and wrote a bare autoplacement clause.
Explicit no therefore became true/bare, and yes lost its spelling. Existing raw
body-style and position-file clauses survived, but were appended after properties.
Any whole-file typed-model write, including served add_wire, reached this.

## Bool and raw syntax contract

Original autoplacement clauses now remain in the existing `raw_sub_nodes`.
The effective view of the first clause is:

- Absence: false.
- Exactly one unquoted no argument: false.
- Bare or yes: true.
- Every other raw form: true, the old presence-based fallback.

For example, quoted `"no"`, an unknown scalar, a nested future clause and
multiple arguments are opaque, not newly accepted native syntax. They survive
unrelated writes without a guessed semantic conversion. All duplicate clauses
survive unchanged while the bool equals the first clause's effective value.

At serialization, a bool different from that raw-derived value replaces the
entire autoplacement group with one canonical yes or no. False writes explicit
no, never implicit removal. New true writes yes; new default false invents
nothing. A same-value assignment is not observable as edit intent.

This is not edit-history tracking. A bool changed and changed back to its raw
baseline preserves the raw syntax. One bool cannot distinguish every syntax
intent, such as deleting explicit no versus preserving it, or normalizing an
opaque true value to yes. Such deliberate syntax edits must update/remove the
existing raw nodes and keep the bool coherent. No stronger API promise is made.
Reading explicit no as false is a behavioral correction. Raw-node consumers
also now observe the retained autoplacement node. Neither change is a Rust
struct-literal source break. The model regression contains an exhaustive
historical public Symbol literal, and the same tests must compile with the
complete original implementation during the failing-before control.

## Bounded emission

Raw body_style nodes follow unit; raw in_pos_files nodes follow on_board;
autoplacement follows dnp before UUID. Values are not parsed into typed optional
fields, so invalid values, nesting, quoting distinctions and multiplicity are
not silently lost. Each group retains its internal order.

Known groups are removed from tail replay, not deleted from raw storage.
Other unknown scalars and lists retain their contents and mutual order at the
tail. This is not a general sibling-index-preservation mechanism, and it does
not preserve arbitrary original positions among modelled nodes. The S-expression
writer may reformat synthetic nested data; those cases have structural raw-node
oracles rather than claims of arbitrary byte-faithful serialization.

The canonical order is also visible in KiCad 10.0.8 source at commit
`e432beac2668303194ba2040274f9e5efe773fc0`,
`eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr.cpp:772-786`.
Its symbol parser uses parseMaybeAbsentBool(true) for autoplacement at
`sch_io_kicad_sexpr_parser.cpp:3253-3255`.

## Native fixture provenance

Reuse the unchanged existing fixture:

`crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.kicad_sch`

SHA-256:
`1bbd268c32a780f2314fb0a280405be92800086ff2ac1a5cf50b15f5ca266398`.
Its adjacent README and provenance JSON record actual Linux KiCad 10.0.8
sch upgrade output and native netlist acceptance, captured by the coordinator.
The source was the repository's KiCad multichannel demo fixture. The committed
LF output is not newline-normalized from the older CRLF input.

There are exactly 25 direct placed symbols, all with body_style and in_pos_files.
24 carry explicit yes. The remaining symbol
`77bb0645-1955-4d0c-9936-5f6568f7bc8e` is mirrored y and has no flag. The complete
25-UUID set is pinned in `tests/support/symbol_serialization.rs`, not learned
from production output. UUID uniqueness and direct-child counts are mandatory.
Library definitions must never be included as placed symbols.

The archived review's junction_sheet_pin suggestion is insufficient for the flag:
its autoplacement clause is on a Sheet, not a Symbol. No native no, bare or invalid
Symbol capture is claimed here. Those cases are controlled in-memory synthetic
derivatives of the native fixture and are never passed to native KiCad.
The existing fixture and its provenance are not rewritten by this packet.

## Independent oracles

`crates/konnect-schematic-editor/tests/symbol_serialization.rs` covers:

- native_25_symbol_blocks_round_trip_exactly
- flag_syntax_survives_unrelated_writes
- invalid_raw_attributes_and_duplicate_clauses_survive_once
- unknown_scalar_and_list_children_keep_content_and_mutual_order
- intentional_bool_edits_override_raw_syntax_with_yes_or_no
- constructors_and_existing_exhaustive_literals_remain_source_compatible

The shared source oracle uses konnect-sexp's direct-child byte-range scanner,
matches UUIDs and compares original source slices including all internal bytes.
It never derives expected blocks from Symbol::to_sexp or normalizes whitespace,
numbers, quoting or line endings. All 25 native blocks must match, not a selected
passing subset. Synthetic raw-node tests compare structure, multiplicity, value
and canonical group slots, plus the exact other 24 untouched blocks. Unknown
scalar/list contents and mutual order are checked separately.

`crates/konnect-core/tests/symbol_serialization.rs` serves actual tools/call:

- served_nonnoop_add_wire_preserves_all_25_native_symbol_blocks
- served_nonnoop_add_wire_preserves_flag_syntax
- served_nonnoop_add_wire_preserves_raw_attributes_and_unknown_children

The served helper proves the requested wire did not already exist, the saved
bytes changed, wire count increased by one, and exactly one new matching wire
with a new UUID was committed. Successful tool dispatch alone cannot satisfy it.

The ignored native test is
native_resave_and_served_edit_preserve_symbols_and_native_pin_partitions.
It requires explicit absolute CLI and artifact paths on an admitted Linux worker.
It reuses the existing tested `pcb_export_supervisor.py` unchanged. The supervisor
and config/audits are outside TempDir, survive a dropped Rust future, enforce
10-second version and 45-second per-native-operation deadlines, and kill/reap
the owned process group. Its audit must report exit zero, no timeout/interruption
and group_reaped=true. New Rust glue remains unexecuted in this child.

Only the native valid fixture enters this test. The test captures actual native
resave bytes before the served edit, exports a native before netlist, performs
the non-noop edit and compares all 25 symbol blocks, exports the after netlist,
then resaves natively again and compares the same symbol blocks. Nonempty pin
partitions, including singleton partitions, come exclusively from native netlist
exports, not the internal wire graph. This is a fixture-scoped oracle, not a
general hierarchy or whole-project connectivity proof.

Retained artifacts include:

- symbol_native_before.kicad_sch and symbol_native_before.provenance.json
- symbol_served_after.kicad_sch
- symbol_native_after.kicad_sch and symbol_native_after.provenance.json
- symbol_before.net and symbol_after.net
- each native phase's exact command, stdout/stderr and supervisor finished audit

Provenance records native version, fixture/captured/output hashes, the 25 UUIDs,
deadlines, supervision and partition count. No expected artifact is substituted
for actual native output.

## Compile-preserving controls

`crates/konnect-core/tests/support/symbol_before.rs.txt` is the complete original
symbol.rs copied byte for byte before editing, not a reconstructed approximation.
Its SHA-256 is
`3e8d312d2cfdc556d2d90919ad57eea51c84624ada5ea70d81c898526e8996e2`.
No public-field dependency was added to the regression suite.

The coordinator-only runner is
`crates/konnect-core/tests/symbol_serialization_controls.py`.
It pins production, original baseline, test/helper, fixture/provenance and native
supervisor sources. It refuses drift rather than learning new hashes at runtime.
Only symbol.rs is mutated; restoration accepts only exact bytes owned by this
runner. All nine positive tests run before controls and again after exact restore.

Named controls and their designated assertion tests:

- failing-before: model-native and served-native, complete original source.
- flag-bare: model-native and served-native.
- no-is-presence: model-flags, including effective-bool assertion for explicit no.
- flag-capture-drop: model-flags and served-flags.
- body-tail, positions-tail and flag-tail: model-native and served-native,
  independently.
- raw-tail-duplication: model-native and served-native.
- invalid-raw-drop: model-raw and served-raw.
- unknown-drop and unknown-reorder: model-unknown and served-raw, independently.
- invented-absence: model-native, model-new and served-native.
- raw-always-wins and false-removes-clause: model-intentional, independently.
- invented-raw-defaults: model-new and model-raw.

Each designated test must actually run exactly once and fail an assertion.
Compilation/link/build failures, zero tests, unrelated test failures, timeouts
and signals do not count. The runner is a Linux subreaper and kills/reaps its
owned command groups. Per-command logs are bounded to 4 MiB and audited.
Work and restoration-positive budgets are independent and bounded.
Run on an exclusive disposable source snapshot only, never alongside another
source writer. Prefer one control per admitted command; all requires a separate
adequate budget. The runner performs no Git, network or native operations.

## Coordinator commands and status

No formatter/toolchain, build, test, control, native execution or Boat command
has been run by this implementation child. Source and Git whitespace review
are not runtime evidence. qpq3/4nj5 remain open pending coordinator verification.

### Independent review and format-only correction

Production policy and the original 15 source pins were independently reviewed
on `348417d`. One runner blocker was found: the constructor's custom-message
assert! did not produce the standard assertion prefix required by the runner.
It now uses assert_eq!, retaining the strict runner failure classifier rather
than accepting arbitrary panics.

The coordinator's format-only preflight `cmd_1kxjym78lmhw` exited 1. Its snapshot
included the uncommitted constructor correction. The complete formatting diff is
retained in the parent at
`.remote-build-jobs/boat-symbol-verification/format-full.log`.
Only those formatting changes were applied by patch, with no formatter or
toolchain execution in this child. The shared helper appeared twice in the
reported diff through its two integration-test include paths; it was changed
once. Production symbol.rs, the original complete baseline, the reused native
fixture and its provenance remain unchanged.

Post-correction review SHA-256 values, also refreshed in the control runner:

- `crates/konnect-schematic-editor/tests/symbol_serialization.rs`:
  `fb6c781788dc041fa970da07c826c9a965ca5e3352a24b0f08a392332c714d1f`
- `crates/konnect-schematic-editor/tests/support/symbol_serialization.rs`:
  `0e602cbc6cd1871b6a23a6f1c7d3179555f25641e1dde996c6033142c0633d4c`
- `crates/konnect-core/tests/symbol_serialization.rs`:
  `72a222c3db3ee37cbc2854c9e71b2f11a002b8fd074ee104507a3f163b56a858`

Format rejection is not a failed regression test or proof of compilation.
The coordinator owns verification and parent integration.

### Exact-head stage-one result and platform refusal

The coordinator reported stage one `cmd_2zshusfwghnc` on exact head
`0729c4ce7048e90f51ee7b3285d2dd23a38b49e4`: formatting passed, all six model
tests and three served tests passed, then Clippy with -D warnings rejected the
native helper's assert!(cfg!(target_os = "linux"), ...) as assertions_on_constants.
The retained result is in the parent at
`.remote-build-jobs/boat-symbol-verification/stage1-result.json`.
Native acceptance and controls did not execute because Clippy blocked them.
Those positive results apply to 0729c4c, not to this subsequent correction.

The native helper now refuses a non-Linux target with if !cfg!(...) followed by
panic!, matching the existing parent helper pattern without allowing the lint.
The core test source hash above and its runner pin were refreshed. Production,
boolean policy and native preservation/connectivity assertions are unchanged.
No formatter, tests, Clippy, native command or job ran in this child for the
correction. The coordinator had already integrated 0729c4c; no additional parent
merge was performed here. Exact-head Clippy rerun, native acceptance and all
controls remain coordinator-owned and pending.

After explicit capacity admission and dependency/cache preparation:

```sh
cargo test --offline --locked -p konnect-schematic-editor --test symbol_serialization -- --test-threads=1
cargo test --offline --locked -p konnect-core --test symbol_serialization -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --offline --workspace --locked --all-targets -- -D warnings
python3 crates/konnect-core/tests/symbol_serialization_controls.py failing-before --coordinator-admitted --output-dir /absolute/disposable/symbol-controls-before
```

Repeat the runner for each named control with a fresh output directory.
Native acceptance, on the explicitly admitted worker only:

```sh
KONNECT_TEST_KICAD_CLI=/absolute/path/to/kicad-cli \
KONNECT_TEST_NATIVE_ARTIFACT_DIR=/absolute/disposable/symbol-native-artifacts \
cargo test --offline --locked -p konnect-core --test symbol_serialization \
  native_resave_and_served_edit_preserve_symbols_and_native_pin_partitions \
  -- --ignored --exact --nocapture --test-threads=1
```

These are proposed commands, not execution evidence or permission to bypass
capacity checks. Keep a 500 MB memory reserve. If formatting or review changes a
pinned source, review and refresh its explicit hash before controls. Never bypass
the source-drift check. Coordinator owns verification, merge, Kata reconciliation
and removal of only the clean fully merged child. Do not close the broader A5
serialization work or claim whole-file, older-format, or point-wrapping fixes.
