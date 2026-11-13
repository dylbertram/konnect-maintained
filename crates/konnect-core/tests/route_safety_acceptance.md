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
`konnect_schematic_editor` model APIs. It now reuses the embedded stock Device:R
definition and complete placed-symbol records from
`fixtures/rotate_junctions_kicad10.kicad_sch`. Its companion README records
Konnect construction, KiCad 10.0.6 force-resave, and native netlist results.
The parsed library record stays unchanged. Cloned
symbols retain native body style, positional flags and pin declarations,
receive fresh symbol/pin UUIDs, and replace only their saved instance mappings
through the model APIs. The modern generator-version header is explicit.
It does not patch or add CAD source files. Generated sheets identify themselves
as `konnect-route-tests`, not Eeschema. They have not been native-resaved or
accepted by KiCad in this child.
The ignored native acceptance is intended to establish that acceptance.

Cases cover the issue's third-pin reproduction, two blocked bends, standalone
labels without a masking pin terminal, a sheet pin at the first corner with a
real child resistor, an untouched undotted remote T, an intentional midspan
tap, missing/duplicate targets, and unresolved/empty pin geometry. Positive
controls include a clear route, an already-same-net pin and an unjoined wire
crossing. Every routing regression calls served `tools/call`.

## Coordinator-owned execution

No builds, tests, typechecks, native exports, or Boat submissions were run by
this child. During correction, the `rustfmt` rustup shim unexpectedly began
downloading the pinned 1.96.0 toolchain. That invocation was terminated after
10 seconds; partial download/install state was not removed or audited. Formatting
then used the already-installed stable formatter by its absolute binary path,
without invoking rustup again. Toolchain provisioning was not intended or
authorized. Run validation only after coordinator admission under the build
skill. First format and compile, then run the focused and broader suites:

```sh
cargo fmt --all -- --check
cargo test -p konnect-core --locked route_safety_
cargo test -p konnect-core --locked batch_connect_pins
cargo test -p konnect-core --locked unit_aware_wiring_tests
cargo clippy -p konnect-core --locked --all-targets -- -D warnings
```

Before native acceptance, run lifecycle coverage in an admitted Linux/Python
3.9+ environment. No KiCad is needed for this command:

```sh
cargo test -p konnect-core --locked --lib native_supervisor -- --ignored --nocapture
```

This includes the parent's two supervisor tests and
`route_safety_native_supervisor_deadline_and_cancellation`. The latter runs
the route helper on Tokio's current-thread runtime, proves it can observe a
running leader, then exercises both natural deadline expiry and cancellation
of the awaiting Rust future. It requires leader and descendant SIGKILL/reaping,
timeout status 124 on expiry, and supervisor exit/reaping. Finite five-second
stubs provide a secondary safety bound for broken controls.

In an admitted Linux/Python 3.9+/KiCad 10 CLI environment, use a new artifact
directory. This example preserves the test exit status while packaging evidence
even when the native gate fails:

```sh
set +e
KONNECT_ROUTE_KICAD_CLI=/absolute/path/to/kicad-cli \
KONNECT_ROUTE_NATIVE_ARTIFACT_DIR="$PWD/route-native-artifacts" \
  cargo test -p konnect-core --locked --lib route_safety_native_before_after_netlist_oracle \
  -- --ignored --nocapture
status=$?
if test -d route-native-artifacts; then
  tar -czf route-native-artifacts.tar.gz route-native-artifacts || exit 1
fi
exit "$status"
```

Retrieve `route-native-artifacts.tar.gz` as a coordinator job artifact.
The environment variable is optional; without it, diagnostics still report the
tool, case, phase, input, executable and argv. With capture enabled, the
directory must not already exist, preventing mixed evidence from prior runs.
Before each export, the test saves exact input parent/child bytes,
`request.json`, `supervisor.py`, `supervisor.json` and empty log files under
`<tool>/<case>/<before|after>/`. After the supervised command returns, but
before any success assertion, it saves `stdout.log`, `stderr.log`,
`result.json`, any `result.net`, and available `started.json`/`finished.json`
audit records there. Spawn errors produce `spawn-error.log`. The archive root
also contains `source_fixture.kicad_sch` and `source_fixture.README.md`.
Each route case also retains `served-call.json` with its arguments and response.
All captured CAD inputs are synthetic test derivatives or the unchanged source
fixture, never a live design.

The native gate first exports the unchanged repository source under
`source_control/native_source/before`, then a typed-model round-trip under
`source_control/typed_roundtrip/before`. This separates source/CLI
compatibility, serializer acceptance, generated inputs, and routed outputs.
Passing a control is not acceptance of the later derivatives.

The route cases export before and after through
`kicad-cli sch export netlist --format kicadsexpr`. It parses the native result,
not the internal graph. All three served routed tools must keep R3 off the
R1.2/R2.1 net, leave the blocked/label designs byte-identical and net-identical,
and leave the child pin and remote T net partitions unchanged. The intentional
tap must add only R54.2 to R51.2/R52.1, not R53.2. Export or parse failure is a
failure, never a skip. A generated fixture rejected by KiCad is a validation
blocker, not native evidence.

## Process supervision and evidence limits

Parent merge `8760db6` brought in `cleanup/main` at `410675b`, including the
existing `tests/support/pcb_export_supervisor.py`. The route tests reuse that
script without changing or copying its policy into another supervisor.
Each native export has its own 60-second Python deadline. The script starts a
new process group, becomes a Linux subreaper, SIGKILLs the group on timeout or
completion, collects pipes, reaps its leader and adopted descendants, and
records audit JSON before forwarding output. The Rust adapter awaits
`tokio::process::Command` with `kill_on_drop(false)`, so it neither blocks the
runtime nor kills the supervisor when the waiting future is dropped.
Observation timeouts in lifecycle tests are not the termination mechanism.

These guarantees cover trusted tools remaining in their group. Deliberate
process-group escape, SIGKILL of the supervisor, worker crash, and uninterruptible
kernel waits still require the coordinator's outer cgroup/job bound. The
supervisor and audit temporary directory must remain available until cleanup.
Artifact capture records returned command output, not a live streaming log;
an outer whole-job kill can still prevent final log/archive collection.

## Recorded failure and source diagnosis

Coordinator job `2fe4fbfa-2b08-4202-8286-22c3bab65de2` tested `54f3afe` with
KiCad 10.0.8. Its supplied logs show 10 focused tests, one batch test, and 25
unit-aware tests passed. Native acceptance failed after 1.65 seconds at the
export status assertion with `Failed to load schematic`. The logs did not
identify the tool/case/phase or preserve the generated input. They therefore
do not isolate a specific parser token or even the failed before/after phase.
No mutation controls ran after that failure. The old outer 600-second job kill
was not an independent per-call cleanup mechanism and does not establish final
native acceptance.

Source review found that the old fixture constructor used `Symbol::new`,
copied only native properties, omitted native pin declarations/body-style/raw
flags, and removed `generator_version`. The correction preserves a complete
documented native instance and its library through typed cloning, refreshes
UUIDs and instance paths, and keeps an explicit generator version. The prior
undocumented `junction_reconcile` source was replaced with the documented
rotation fixture. These are construction corrections, not proof that any
particular omission caused the failure. The structural regression and native
source/round-trip controls are intended to distinguish those possibilities.
This corrected revision has not been compiled, lifecycle-tested or exported
by this child. Generated native validity remains blocked on the coordinator's
new admitted run.

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
