# Routed connection acceptance

Scope is `connect_pins`, `batch_connect_pins`, and
`add_schematic_connection`. The explicit-wire and stub writers are untouched.
Existing Kata `qe1n/mta5` track this change. `3b3b/0y95` remain adjacent work.
Do not close these records before parent integration and verification.

## Candidate review

Reviewed the archived #901 description and fetched its public diff at
<https://github.com/mixelpixx/Konnect/pull/901.diff>. Its commit was not present
in this checkout. The implementation follows the candidate's bounded
before/after root comparison and two existing bends, rather than adopting its
whole patch or introducing another router. Target resolution additionally
refuses multiple matching placed pins and empty resolved pin sets. Existing
stub resolution is unchanged.

The guard protects pins, labels, power symbols, and sheet pins. It compares
root identity, not displayed net names. Endpoint nets and explicitly tapped
wire nets may join. Bare unnamed wires carrying no terminals are not separately
protected. Hierarchy is checked conservatively through parent-side sheet pins;
the production guard does not claim a whole-project native connectivity proof.

The batch keeps its partial-success contract. `connected_count` counts only
saved pairs. Added `connected` and `skipped` arrays carry zero-based input
indexes. `skipped` gives `plan_blocked` reasons for refused pairs. Existing
`errors` strings remain. Only skipped indexes need retrying. An all-refused
batch is an error and writes nothing. All three callers retain the existing
revision-checked atomic write and its saved-byte readback.

## Fixtures and provenance

`sch_route_safety_tests.rs` generates temporary designs through
`konnect_schematic_editor` model APIs. It reuses the embedded stock Device:R
definition and property formatting from the existing native
`fixtures/junction_reconcile.kicad_sch`. It does not patch or add CAD source
files. Generated sheets identify themselves as `konnect-route-tests`, not
Eeschema. They have not been native-resaved or accepted by KiCad in this child.
The ignored native acceptance is intended to establish that acceptance.

Cases cover the issue's third-pin reproduction, two blocked bends, standalone
labels without a masking pin terminal, a sheet pin at the first corner with a
real child resistor, an untouched undotted remote T, an intentional midspan
tap, missing/duplicate targets, and unresolved/empty pin geometry. Positive
controls include a clear route, an already-same-net pin and an unjoined wire
crossing. Every routing regression calls served `tools/call`.

## Coordinator-owned execution

No installs, builds, tests, typechecks, native exports, or Boat submissions were
run by this child. Run these only after coordinator admission under the build
skill. First format and compile, then run the focused and broader suites:

```sh
cargo fmt --all -- --check
cargo test -p konnect-core --locked route_safety_
cargo test -p konnect-core --locked batch_connect_pins
cargo test -p konnect-core --locked unit_aware_wiring_tests
cargo clippy -p konnect-core --locked --all-targets -- -D warnings
```

In an admitted KiCad 10 CLI environment:

```sh
KONNECT_ROUTE_KICAD_CLI=/absolute/path/to/kicad-cli \
  cargo test -p konnect-core --locked route_safety_native_before_after_netlist_oracle \
  -- --ignored --nocapture
```

This test exports before and after through
`kicad-cli sch export netlist --format kicadsexpr`. It parses the native result,
not the internal graph. All three served routed tools must keep R3 off the
R1.2/R2.1 net, leave the blocked/label designs byte-identical and net-identical,
and leave the child pin and remote T net partitions unchanged. The intentional
tap must add only R54.2 to R51.2/R52.1, not R53.2. Export or parse failure is a
failure, never a skip. A generated fixture rejected by KiCad is a validation
blocker, not native evidence.

## Negative controls

Apply each mutation independently in the coordinator's disposable sanitized
test source, never in a live design or accepted integration branch. Restore
between controls, then rerun the focused tests and native acceptance above.
These controls are proposed, not executed evidence.

1. Disable guard acceptance in `sch_wiring::route_between` by replacing
   `if joined.is_empty() {` with `if true {`.
   The alternate-bend, blocked pin/label, and straight-route regressions must
   fail. The native oracle must expose the third pin joined to the route.
2. Disable the local T filter in `insert_wire_with_junctions` by replacing its
   `point_on_segment` filter with `.filter(|_| true)`.
   The remote-tee and midspan-tap tests must fail. Native R53.2 must join
   R51.2/R52.1 in the broken output rather than remain separate.
3. Remove the vertical-first candidate. Safe alternate-bend regressions must
   fail while blocked requests still write nothing.
4. Remove label terminal collection from `RouteGuard::new`. The standalone
   label case must fail because labels, not a local third pin, block its bends.
5. Remove sheet-pin terminal collection. The sheet case must fail.
6. Replace the multiple-match target branch with first-match selection, or
   bypass missing/empty geometry refusal. Exact-target no-write tests must fail.

Record exact commands, mutations, job IDs, failures and native CLI version on
the existing Kata records after coordinator execution. Then rerun relevant
checks on the merged parent. No success is claimed here for proposed checks.
