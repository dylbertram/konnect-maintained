# Read identity regressions

This packet covers Kata `konnect-maintained#t7yx/#9ane` and
`konnect-maintained#h1aj/#qdmh` on parent `abd2b5659de8f803dca9f76e13475dea52c90d99`.
Both missing-field reports still apply to that source. No archived implementation
was imported.

## Fixtures and assertions

All five tests use `McpHandler::handle_message` with JSON-RPC `tools/call`.
There are no direct handler calls, CLI subprocesses or IPC connections.

- `mirror_fields_kicad10.kicad_sch` is the existing KiCad 10.0.6 resave.
  Its adjacent README records provenance. Literal raw mirror tokens cover
  plain, X, Y, and rotated-X placements. Literal pin positions come from the
  saved library offsets and sheet-axis reflections, not Konnect's transform.
  Both served pin tools check top-level and unit flags, anchor coordinates,
  all pins and the existing flattened pin list.
- `ecc83_multiunit.kicad_sch` is the existing Eeschema demo save used by the
  maintained multi-unit tests. The read test first uses it unchanged, then
  uses the typed editor to mirror U1 unit 1 about X, unit 2 about Y, and leave
  unit 3 plain in a temporary copy. Both served readers must report different
  unit flags and the lowest-unit anchor. The mixed-mirror variant is
  test-generated, not a new KiCad-authored save or independent ERC oracle.
- `two_name_nets.kicad_sch` is the existing KiCad 10.0.5 resave. Its adjacent
  README records provenance. All 15 literal UUID/name/type/position rows were
  transcribed from the saved top-level label forms, including repeated names
  at different positions. Two reads must return the same UUIDs and unchanged
  legacy fields. A served list-to-`batch_delete` test selects one of the two
  SDA instances, SYS and MIX_H. It checks committed readback and the entire
  saved S-expression tree minus only those three labels. The other SDA,
  every other label, wire, symbol, junction, no-connect, library and metadata
  form must remain.

Read calls require byte-identical input afterward. Same-basename decoy files
exercise exact file targeting and remain unchanged. The batch pin call also
includes the missing reference `U`, which must not match `U1` or other prefixes.
Only the deletion regression invokes an existing write tool, against temporary
synthetic copies, never a live design.

## Coordinator-owned remote execution

No installs, builds, typechecks or tests were run by the child, and no Boat job
was submitted. The child ran only source inspection, a focused `rustfmt --check`
and `git diff --check`. The following are commands for a disposable admitted remote
snapshot, from its repository root. They do not authorize local execution.
The coordinator must apply the capacity checks and 500 MB reserve from
`byskap-builds` and provision the repository's pinned Rust/native dependencies.

Focused passing-after command:

```sh
CARGO_BUILD_JOBS=1 cargo test -p konnect-core --locked --test read_identities -- --test-threads=1
```

Deterministic failing-before response logic:

```sh
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py before
```

The script first requires a passing suite. `before` removes only the added
response fields from the two named handlers, restoring their parent response
construction while keeping the new tests and descriptions. It requires all
five named tests to fail through actual served dispatch, restores the exact
source bytes in `finally`, and requires a final passing suite. A compile error
or unexpected test failure set fails the control rather than counting as proof.

Independent negative controls, one at a time in disposable remote snapshots:

```sh
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py unit-mirror-false
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py anchor-mirror-false
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py unit-mirror-anchor
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py unit-mirror-swapped
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py anchor-mirror-swapped
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py net-uuid-empty
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py global-uuid-empty
CARGO_BUILD_JOBS=1 python3 crates/konnect-core/tests/read_identity_controls.py hierarchical-uuid-empty
```

Each false/swapped mirror control must fail the three pin tests. Copying the
anchor's flags into every unit must fail the multi-unit test. Each label-kind
control must fail both label tests. These are expected outcomes, not results
observed by the child. `all` runs every control sequentially if the coordinator's
time and resource budget allow it. The runner itself is also unexecuted here.

After parent integration, rerun the focused suite, existing schematic regressions,
formatting and the parent gates there. Generated tool catalogs, migration notes,
count artifacts and cleanup-plan updates belong to the coordinator. No ticket
should close on this unexecuted child evidence.
