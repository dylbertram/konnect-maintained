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
The coordinator subsequently tested merged revision `9f5d69ad`, as recorded
below. This child did not execute those checks.

## Isolated label fixture rejection and correction

Coordinator worker `bx_fv2c2jjy`, command `cmd_ramcfajlbuhk`, tested exact
revision `9f5d69ad`. `stage1-full.log` records formatting, focused/batch/unit-aware
tests, Clippy, and all three native supervisor tests passing. The native source
control, typed round-trip, and `connect_pins` around/blocked before/after exports
also passed. The next invocation failed at `connect_pins/label/before`, before
any routed mutation of that case.

Evidence is retained in the parent's
`.remote-build-jobs/boat-route-verification/route-native-artifacts/`:

- `connect_pins/label/before/request.json` identifies `/usr/bin/kicad-cli`,
  `sch export netlist --format kicadsexpr --output
  /tmp/.tmpICX7L7/route.before.net /tmp/.tmpICX7L7/route.kicad_sch`, and the
  precise tool/case/phase.
- Its `stderr.log` says `Failed to load schematic`; `finished.json` records
  status 3, no timeout/interruption, and `group_reaped: true`.
- The captured `route.kicad_sch` SHA-256 is
  `e0f868710a6f0905f45fe97bdbb27c6a6309e26cde4e2a6e5e551c2047d67e64`.
  Lines 149-156 contain two local labels with `(at 114.3 105.41)` and
  `(at 120.65 107.95)`, lacking an angle.
- The accepted `source_fixture.kicad_sch` labels, for example lines 201-209,
  have three-coordinate forms such as `(at 149.86 160.02 0)`.

This identifies a concrete native grammar violation. KiCad 10.0.8's
[`SCH_IO_KICAD_SEXPR_PARSER::parseSchText`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.8/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr_parser.cpp#L4641-L4696)
handles local and hierarchical labels, then reads `parseXY()` followed
unconditionally by `parseDouble("text angle")` inside `at`. The captured closing
parenthesis appears where that numeric angle is required.

The source path to those exact bytes is also explicit: `Label::new` and
`Schematic::add_hierarchical_label` initialize `At::new`, whose rotation is
`None`; `At::to_sexp` emits no angle for `None`. The route fixtures called
`add_label` without the existing `set_rotation` step. This is not a missing
symbol UUID, library geometry, guard behavior or native process cleanup issue.

The minimal correction calls `set_rotation(0.0)` on every label created in this
test module. That existing typed API emits the required third coordinate and
normal text effects. The child hierarchical label had the same constructor
omission, so it receives the same correction before its native case runs.
That child defect is source-proven, not a claim of an observed sheet failure.
No production serializer defaults, route guard, routing assertions, captured
CAD files or repository fixture files are changed.

`route_safety_generated_labels_include_native_required_angles` checks the
serialized local and child hierarchical labels, with nonzero expected label
counts, exactly x/y/zero-angle coordinates, and effects from `set_rotation`.
Removing these setters is a proposed negative control for this regression.
No native assertions or terminal checks are relaxed. The coordinator still
must rerun the complete native oracle on the corrected commit and then run
the guard/filter mutation controls. None of those mutation controls has run,
and the child ran no builds, tests, jobs or native commands for this correction.

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

## Prepared mutation runner

`tests/support/route_guard_mutations.py` is a coordinator-only runner, prepared
without executing it. This follow-up changes only that runner and this document.
It does not change any Rust source, tests, supervisor policy or CAD fixture
while the coordinator's `d7f51fc` rerun is in flight.

The runner declares source pin
`d7f51fc034122b6c43c679bc1e27f666cb422854` and verifies SHA-256 fingerprints for
the wiring source, served test module, batch source, supervisor, native source
fixture and Cargo.lock. In a sanitized archive without Git metadata these are
critical-file checks, not proof of the entire archive's Git identity. The
coordinator must still record the exact source archive/revision. New runner/docs
commits are compatible only while those pinned files remain byte-identical.

Run only after the current native rerun finishes and the coordinator admits an
exclusive disposable source copy with measured capacity and an outer cgroup/job
deadline. Git checkouts/worktrees are rejected, even with the acknowledgement
flag. Do not point it at this child worktree, the integration parent, an active
verification copy or a live hardware project. It neither submits jobs nor
installs anything. Cargo uses `--locked --offline`; an explicitly installed
toolchain cargo binary and its sibling rustc/rustdoc bypass rustup shims.
The dependency/compiler cache must already be warm.

Example in the admitted sanitized worker source root:

```sh
set +e
python3 crates/konnect-core/tests/support/route_guard_mutations.py \
  --source "$PWD" \
  --output "$PWD/route-mutation-evidence" \
  --cargo /absolute/installed/toolchain/bin/cargo \
  --native-cli /usr/bin/kicad-cli \
  --admitted-disposable-source
status=$?
if test -d route-mutation-evidence; then
  tar -czf route-mutation-evidence.tar.gz route-mutation-evidence || exit 1
fi
exit "$status"
```

To split bounded work across coordinator-owned commands, use different fresh
evidence directories and the same clean pinned source:

```sh
python3 crates/konnect-core/tests/support/route_guard_mutations.py \
  --source "$PWD" --output "$PWD/route-native-controls" \
  --cargo /absolute/installed/toolchain/bin/cargo \
  --only guard,local-t --native-cli /usr/bin/kicad-cli \
  --admitted-disposable-source

python3 crates/konnect-core/tests/support/route_guard_mutations.py \
  --source "$PWD" --output "$PWD/route-terminal-controls" \
  --cargo /absolute/installed/toolchain/bin/cargo \
  --only alternate-bend,label,sheet,ambiguity \
  --admitted-disposable-source
```

An existing evidence directory or runner lock is a refusal, not permission to
delete another attempt. A missing dependency/toolchain or a 60-second command
budget that is too small is a blocked run, not an expected negative control.
`--seconds` accepts 1 through 60 and defaults to 60. Admit/prewarm compilation
separately if it cannot fit; do not silently fall back to installs or a longer
unbounded command.

The baseline runs focused served/fixture positives, the batch regression, and
unit-aware wiring positives. With `--native-cli`, it also requires the unchanged
complete native oracle to pass before any mutation. Each control changes only
`sch_wiring.rs` through one exact, unique source anchor:

- `guard` clears the computed foreign-join list, retaining type checking and
  normal parse/geometry work. Required named failures are alternate-bend,
  blocked-pin/label and straight-third-pin regressions.
- `local-t` removes only the local T-position filter. Required named failures
  are the untouched-tee and intentional-tap regressions.
- `alternate-bend` removes only the second bend. Its named alternate-route
  regression must fail.
- `label` omits label terminals without changing graph label seeding. The
  standalone-label refusal regression must fail.
- `sheet` still extracts sheet pins but does not add their terminals, avoiding
  an unused-import compile failure. The alternate-route regression must fail.
- `ambiguity` selects the first matching placed pin. The missing/ambiguous/
  unknown-geometry no-write regression must fail.

Every mutant must first pass `cargo test --lib --no-run`. Each negative then
runs its exact qualified test name and must exit 101 with exactly that named
Rust test in the failure list and panic evidence. Compile, startup, filtered-out
tests, timeout, supervisor failure and unexpected success do not count.
Baseline and restored positives must explicitly report their required named
tests as passed, preventing empty-filter successes.

For `guard` and `local-t`, `--native-cli` additionally runs the unchanged native
oracle as an expected named failure. No test adapter or success-gate relaxation
is needed: the current oracle exports and captures its native after-netlist
before checking memberships. The runner requires native export status 0 and
completed, reaped, non-timeout native audits, then reads native netlist
membership rather than inferring geometry:

- Guard disabled must change R1.2/R2.1 into exactly R1.2/R2.1/R3.1, with R3.1
  separate before the route.
- Local T filter disabled must add exactly R53.2 to R51.2/R52.1, with R53.2
  separate before the route.

The expected baseline partitions must also match. A mere response assertion,
load rejection, missing capture or unparseable native netlist is not accepted as
accidental-join evidence. The small standard-library reader handles the captured
native `kicadsexpr` structure; it is not a routing or connectivity engine.
Each native control writes `join-proof.json` alongside its unchanged raw
before/after captures. Native baseline and restored native positives get their
own output directories; restored native gates run after guard and local-T
controls, while focused/batch/unit-aware restored positives run after every
completed control.

Original source bytes and file mode are backed up before mutation. Every
control and the whole run restore them in `finally`, verify exact bytes/SHA and
mode, and retain a backup in the evidence directory. Atomic replacement uses a
new mtime intentionally so Cargo must rebuild restored source rather than reuse
a mutant binary. A foreign edit is a hard conflict: it is not overwritten and
`restoration_failed` is reported with the original backup retained. A replaced
lock is not removed. SIGINT/SIGTERM trigger cleanup/restoration; SIGKILL, worker
crash, filesystem failure and uninterruptible kernel waits cannot be repaired
by Python `finally`.

Each cargo subprocess reuses the existing Python supervisor with its independent
deadline, process-group kill, subreaper and completion audit. The runner bounds
its wait too and reaps its direct supervisor child. Any command deadline,
interruption or uncertain cleanup quarantines the attempt: source restoration
is attempted, no next command/control is started, and restored positives are
not claimed. Nested native tests create separate native sessions; killing the
cargo group can kill their supervisors before their native groups finish.
Therefore an aborted command **requires coordinator outer-cgroup cleanup**,
not worker reuse or a claim that all nested processes were reaped. This is why
the exclusive admitted worker and whole-job lifetime bound are prerequisites.

Evidence lives under the requested directory:
`summary.json`, `original-sch_wiring.rs`, per-command `command.json`,
`combined.log`, supervisor audits, and separate native `input-capture` trees.
`summary.json` distinguishes requested native coverage, named failures, compile
success, restored positives, byte restoration and cleanup quarantine. Preserve
the whole evidence directory even on failure. The complete example above
packages it without masking the runner's exit status.

Retrieve the archive with the coordinator's job results, subject to artifact
size limits. No runner, positive, negative or native-join result is claimed by
this preparation commit.
