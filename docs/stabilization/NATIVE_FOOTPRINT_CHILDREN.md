# A8 native footprint child refusal

Bounded implementation for existing Kata `konnect-maintained#3bqz` and
`konnect-maintained#emvb`, based on `43c7497`. The field foundation merged at
`d9b33fd` remains intact. No issue writes, jobs, installs, builds, tests, Boat
calls or native executions were performed by this child. Restored acceptance
remains pending; coordinator-only attempts are recorded below.

## Replacement boundary

Schematic sync already replaces an existing whole footprint for differences
in Reference, Value, symbol path, DNP or pad nets. Its typed snapshot is not
a complete native child inventory. A point in opaque `Any` data can survive
some sync updates. That does not establish support for every native child.

Library refresh validates the input library's children. It did not establish
whether the placed instance owns unsupported native children before replacing
it. The vendored FootprintInstance schema has no variant field 20.
Coordinator `02324668` captures now establish group omission, point Any
serialization and variant field 20 observations on native KiCad 10.0.8.
The refresh blocker below still prevents full native acceptance.

Independent review of `fbb1d5d` blocked native execution. Its A8 helper called
the field module's `save_native` without initializing that module's pcbnew
global, and its raw field 20 evidence counted tags without decoding a variant
name. The follow-up uses its own isolated local pcbnew import for SaveBoard
and a pinned-schema, test-only raw variant decoder. These were source-only
corrections at that review; the later coordinator `02324668` capture results
and refresh blocker are recorded below. The accepted field helper and
NativeRun are unchanged.

Coordinator format-only preflight `cmd_lyucyx067lyx` exited 1 on the source
snapshot including the in-progress native fixes. The full rustfmt diff was
retrieved to the parent worktree under
`.remote-build-jobs/boat-native-children-verification/format-full.log`.
This child applied the displayed Rust changes by patch, without invoking a
formatter. A restored passing formatting gate is still pending. The reviewed
production readiness, whole-plan refusal and noop/add-only boundaries are
unchanged; their follow-up Rust edits are formatting only.
Read-only text comparison matched all 34 displayed formatting hunks across
five Rust files. Reversing those hunks reconstructs `fbb1d5d` exactly for the
four core files; the capture example retains its separately documented native
evidence fixes. This comparison is not a formatter, compiler or test result.

Both tools now run the same read-only inventory for actual existing
replacement targets only. It serializes the exact bound live document with
`save_document_to_string_in`, parses a `kicad_pcb` root and matches only direct
top-level footprints by exact KIID. It counts direct owned `group`, `point`
and `variant` children and retains variant names. It does not count board
groups, group membership or board-level variant declarations as ownership.
Unknown inventory, including live read failure, malformed input, wrong root,
missing target, duplicate target KIID or malformed variant name, fails closed.
There is no saved-file fallback or reference guess.

Dry run checks before reporting ready. Every apply checks a fresh native
snapshot before any BeginCommit, CreateItems, UpdateItems, DeleteItems, save
or revert, even when its protobuf revision has not changed. One unsafe target
clears the whole mixed plan, including otherwise safe additions and updates.
The existing revision guards are unchanged. There is no atomic GUI lock across
requests, or between inventory and a later GUI edit.

`footprint_children_not_carried` uses each tool's existing conflict response.
It names the reference, KIID, library ID, unsupported kinds, counts and variant
names. Inventory errors use `inventory_error`, with unknown counts and empty
`kinds`, never an assertion of safety. Refresh changes also expose their exact
KIID so callers can identify the planned replacement without reference guesses.

Noop or add-only plans do not add an inventory read. Sync's existing
recovering commit still serializes the live board before a write, including
add-only apply. That recovery snapshot is not this child inventory.

This change refuses unsupported preservation. It does not expand
propagation, rebuild groups, preserve every unknown protobuf field, assert
blanket point loss, change library-input policy or cover every other A8
metadata kind. A7 and schematic serialization are out of scope.

## Fixture and mock regression evidence

The coordinator supplied the original board and README after provenance
review. Their bytes and README remain unchanged. Source URLs, source commit,
AGPL-3.0-only license and hashes are recorded separately in
`crates/konnect-core/tests/fixtures/footprint_children_kicad10.SOURCE.md`.
The README claims GUI creation and pcbnew 10.0.6 saving. The header says 10.0.
Neither is an independent native 10.0.8 oracle.

`native_footprint_children_tests.rs` drives the public served `tools/call`
handlers with a stateful protobuf IPC mock. The synthetic protobuf instances
use the checked-in capacitor library for repeatability, not as the imported
board's original electrical definition. Exact fixture KIIDs drive matching.
Single-kind reductions and malformed responses are explicitly synthetic,
in-memory strings. The board on disk is always imported verbatim.

The pending tests require:

- each of group, point and variant independently refusing dry run and apply;
- zero mutation/commit/save/revert RPCs, unchanged raw mock items, native
  state and disk bytes on every refusal;
- safe R1 update plus C1 addition plus unsafe U1 refusing the whole sync;
  refresh also refuses a mixed safe/unsafe selection;
- ready dry run, native children inserted before apply with identical
  protobuf, and a refusal before any mutation;
- unreadable, malformed, wrong-root, missing-target and duplicate-target
  inventory, including both safe/unsafe duplicate orders;
- actual changed safe R1, independent GetItems readback and unchanged U1,
  despite their board-level membership;
- noop dry run/apply and add-only requests without new inventory reads.

`native_footprint_children_controls.py` requires a positive baseline, compiles
each mutation before execution, and accepts only named regression assertion
failures with nonzero test counts. It never accepts compile failure or a
zero-test run. It independently disables group, point and variant refusal,
removes inventory from each tool, skips apply rechecks in each tool and
overlooks the unsafe target of a mixed plan. Every control restores exact
source bytes in `finally`, then requires a restored positive gate.
The formatting follow-up also pins each expected failure to its actual
reviewed helper assertion site, resolved from a unique source marker rather
than a stale numeric line. Kind-disabled, inventory-disabled and mixed-target
controls must fail the conflict-status assertion. Apply-recheck controls must
fail the missing compatibility diagnostic assertion in the each-kind and
mixed tests, and the conflict-status assertion in the fresh-apply test.
Named test failure alone, an RPC/mock panic or an unrelated assertion cannot
satisfy these pins. These site expectations have been reviewed statically;
actual control failure sites still require the coordinator's execution logs.

## Coordinator-admitted Rust and control commands

Run only in an admitted disposable source snapshot. Follow `byskap-builds`,
check worker and host capacity, and keep at least 500 MB RAM in reserve.
No local fallback is authorized by this packet.

```sh
cargo fmt --all -- --check
cargo test -p konnect-core --locked --lib --no-run
cargo test -p konnect-core --locked --lib a8_ -- --test-threads=1
python3 crates/konnect-core/tests/native_footprint_children_controls.py all
cargo test -p konnect-core --locked --lib pcb_sync
cargo test -p konnect-core --locked --lib pcb_footprint_update
cargo test -p konnect-core --locked --lib a6_ -- --test-threads=1
cargo build -p konnect --locked
cargo build -p konnect-ipc --locked --example native_footprint_children_capture
cargo clippy --workspace --locked --all-targets -- -D warnings
```

The coordinator also owns full restored workspace, plugin and script gates.
These commands and test source are not passing results. The child checked
fixture hashes and `git diff --check` only. Formatting and compilation have
not been executed here.

## Safe native gate and raw IPC evidence

`native_footprint_children_native.py` is source for a fresh disposable Linux
worker with matching 10.0.8 CLI, editor IPC and pcbnew. Missing prerequisites
fail, never skip or trigger installation. It reuses the verified, unchanged
`library_field_native.NativeRun` lifecycle. Desktop isolation is set in the
helper and child environments before importing pcbnew or starting an editor.
It registers only its own leaders, closes the owned editor and MCP/proxy
leaders before root removal, reaps and audits descendants, records mounts
and cleanup errors, and never signals shared services or unmounts anything.
Failure retains an unclean root for coordinator inspection.
The reused NativeRun, Api, stop_editor and open_editor functions use imports
initialized by their module. A8 does not call that module's save_native,
inspect, ordinary_texts or native_footprint functions, which depend on its
main-only pcbnew global. A8 saves through its own post-isolation local import.

Coordinator native attempt `cmd_ek51spw9yyuw` against the original `f3578fc`
helper failed before launching the owned editor because KiCad 10.0.8
`FOOTPRINT` has no `SetDescription`. This packet uses the exact
`SetLibDescription` setter and matching `GetLibDescription` readback, without
API-name fallbacks. The coordinator confirmed the direct methods in the
worker's installed
`/usr/lib/python3/dist-packages/pcbnew.py`, `GetLibDescription` at lines
23265-23267 and `SetLibDescription` at lines 23269-23271.
The coordinator owns inspection of the attempt's retained lifecycle evidence.
The failed attempt is not a passing native gate, and the amended helper has
not been executed here.

The coordinator's native `02324668` run passed the raw captures and produced
the expected A8 sync dry-run/apply diagnostics, but refresh preflight stopped
at unrelated `unsupported_library_footprint` point policy. Its library-control
report had `removed_native_library_points=[]`: `FOOTPRINT.GraphicalItems()`
does not enumerate the separate native point container. The coordinator
qualified `FOOTPRINT.Points() -> PCB_POINTS` in installed 10.0.8 SWIG source
around lines 23188-23193 and `RemoveNative(BOARD_ITEM, REMOVE_MODE=NORMAL)`
around lines 23129-23131.

The helper now uses only `list(fp.Points())` and `fp.RemoveNative(point)` on
the project-local library copy. It requires exactly one original SafeDIP point,
zero SafeR points and an empty `Points()` container after removal in each copy.
There is no fallback, CAD text edit or relaxation of library-input policy.
Placed U1 children, imported fixture bytes and the raw/refusal checks are
unchanged. Coordinator-retained evidence is
`.remote-build-jobs/boat-native-children-verification/a8-final-native-evidence.tar.gz`;
the coordinator reports lifecycle and independent root-absence checks passed.
Those actual three-kind serialization observations and sync diagnostics are
not full native acceptance. This narrow helper correction has not been run.

The helper imports the fixture verbatim. A disposable accepted copy changes
only through native pcbnew or public Konnect. A project-local native library
copy removes the stock DIP's library-owned point through KiCad's model,
without removing any placed U1 child. Native FPIDs select those library copies;
native empty symbol paths allow the normal existing reference-linking path
to the public scratch schematic. This prevents unrelated library-input policy
or schematic identity conflicts from substituting for the intended guard.
If a native API or precondition is unavailable, fix or report that blocker;
do not patch CAD text or manufacture expected captures.

The Rust example `native_footprint_children_capture` has two test-only modes:

- `capture SOCKET BOARD OUT` binds the exact board, serializes that document,
  reads GetItems and persists every raw `Any.value` before old-schema
  FootprintInstance decoding. It records raw top-level wire tags, child type
  URLs, native group KIID presence in raw bytes and field 20 occurrences.
  Each field 20 payload is saved and decoded as FootprintVariant before
  old-schema decoding of its containing FootprintInstance. Its actual name,
  DNP/exclusion flags and field overrides are recorded, not inferred from
  native text or a sent replacement.
- `proxy LISTEN UPSTREAM OUT` forwards exact request/response bytes to the
  same owned editor. It records each request before forwarding. In default
  refusal mode, guard calls use this proxy, so zero BeginCommit/mutation/save/
  revert evidence is a recorded observation, not inferred from tool output.
  The loss control uses the same proxy but must record actual mutations.

Raw protobuf files, full native before/after text and served responses remain
separate artifacts. The helper checks native U1 ownership of exactly one
group, one point and variant `NoTimer`, with R1 owning none. It separately
requires no group Any/native group KIID bytes, a point Any and raw field 20.
It never treats an old-schema decode as the original wire payload.
Field 20 must independently decode to exactly `NoTimer`, with DNP true.
A tag count alone is not evidence of that name. The read model and the
`kiapi.board.types.ReferencePoint` Any name come from KiCad's 10.0.8 schema
at immutable commit `e432beac2668303194ba2040274f9e5efe773fc0`.
The full upstream schema SHA-256 is
`fa7b175e769215541e8adb61200be2dafe473f3ec9f18d54801bada9be866587`.
Source URL, tag provenance and field mapping are in
`crates/konnect-ipc/tests/fixtures/footprint_variant_schema_kicad10_0_8.SOURCE.md`.
Production protobuf definitions are not upgraded. No received payload is
fabricated or re-encoded to supply an expected result.

Both tools are called through public served dry run/apply guard paths against
the mixed native board. Refusal must leave the full native string, every raw
footprint payload and disk unchanged, with zero recorded mutation RPCs.
Safe R1 refresh and sync then actually apply and read back natively while
U1 remains present with its children. The native gate does not exercise every
synthetic malformed/race case; those are separate served mock gates above.

The default `--mode refusal` is unchanged. Passing it proves bounded refusal
and safe R1 application, not the original native loss report. Reviewer
`ses_e297379b7ffeXjac7g85C2OgQu` requires actual native loss reproduction before
`3bqz` can close. Neither the helper source nor a passing mock control supplies
that evidence. No new native loss execution or result is claimed here.

The optional `--mode sync-loss-control` is coordinator-only and requires both
`--control-konnect` and `--control-build-evidence`. It must run in a separate
disposable worker invocation, never against a shared editor. It uses the same
NativeRun, desktop isolation, imported fixture bytes, native-prepared board,
library preparation, independent capture binary and raw forwarding proxy.
It checks the same exact U1 native ownership and raw field 20 `NoTimer`/DNP
evidence before replacement, including an actual ReferencePoint Any.

Setup uses the restored positive binary. Only after setup does the served
connection switch to the supplied control binary. Public schematic deletion
leaves only U1 as a sync target; the native-prepared board is not edited to
manufacture loss. The actual maintained `update_pcb_from_schematic` handler
must report ready, then apply exactly one U1 update with its returned revision.
It sends the current whole-footprint update through native IPC, then saves
through public `save_project`. Independent GetItems/native capture follows.
pcbnew reloads the saved disk board and must find the same U1 KIID/reference
with the requested value changed to `a8 desired value`.

The control persists served plans/counts, raw request/response files,
`control-rpcs.json`, per-command RPC counts, full native before/after captures
and disk readback. Before attributing loss it requires one applied update,
one recorded UpdateItems, a completed BeginCommit/EndCommit pair, a public
SaveDocument, changed U1 raw bytes and the changed saved value. Recovery,
creation, deletion, a refusal, noop or compile failure cannot satisfy it.
Only the positive refusal interval makes a zero-mutation claim.

Each of group, point and variant gets its actual native before/after count
and loss flag. Raw point Any counts and decoded field 20 payloads remain
separate observations, as do variant names. The gate requires observed native
group or variant loss. Point loss alone does not pass, and point survival is
not treated as failure. If neither group nor variant is lost after a proven
replacement, the control fails rather than claiming reproduction.

### Compile-first isolated loss binary

Source review of the new Python helper and runner must precede execution.
`native_footprint_children_loss_build.py` runs only in an admitted disposable
source archive. Sanitized archives have no Git history or `.git`; neither
the runner nor the native commands need Git. The coordinator must supply the
actual archived source commit as required `--source-revision`, validated as
a full 40-hex SHA. This revision is coordinator provenance, not something
inferred or verified from Git in the archive. Never initialize Git or invent
a revision to satisfy the argument. The runner inventories SHA-256 for
`crates/**/src/**/*.rs`, crate build scripts and workspace `Cargo*` files.
It also covers `xtask/src` Rust, its build script and Cargo files.
Every directory named `target`, `__pycache__` or `.git` is excluded before
traversal, including nested generated directories. Standalone tests and
examples are outside this production-source inventory. The report records
the manifest scope and before/control/restored/after source manifests. It
rejects Rust or Cargo drift beyond the one intended transient control edit.
Its sole transient Rust edit replaces exactly
`&targets, // A8 sync inventory targets` with
`&[], // A8 sync inventory targets`. It does not change the refresh guard,
replacement code, protobuf definitions, tests, assertions, library or fixture.
No production Rust edit is part of this packet.

The runner first requires the same compile-first, nonzero-test positive Rust
gate and builds a positive binary. All Cargo commands run sequentially using
the existing warm `ROOT/target`, never separate cold target directories. The
coordinator must admit exclusive use of that cache and source tree. No other
build, test, formatter or mutation may run concurrently there. The runner
requires `ROOT/target/debug/deps` to exist and forces `CARGO_TARGET_DIR` to
that same `ROOT/target` for every Cargo command.

It copies the baseline binary outside target as `baseline-konnect`, then
builds and copies `control-konnect` before restoration. In `finally`, including
after a failed control build, it restores exact original source bytes,
rejects other Rust/Cargo source drift and requires the restored positive Rust gate.
It rebuilds/copies `positive-konnect` without overwriting either earlier copy.
Paths and hashes for all three binaries, source manifests, original/control
source bytes, revision and command logs remain in the outside-source evidence
root. Identical-source baseline/restored binary hashes must match exactly.
A mismatch fails admission pending investigation; nondeterminism is not
silently accepted. The native helper checks current/restored source manifests,
the exact marker mutation hash, matching positive hashes and the supplied
positive/control binary hashes before setup. This is build provenance, not
a native reproduction result.

The admitted build job has a fixed 600-second budget. The runner shares one
590-second command deadline across baseline, control and restored commands,
reserving 10 seconds for exact restoration and evidence writes. There are no
automatic retries, cold-cache fallbacks or timeout increases. If the budget
expires, exact source restoration still runs, but a missing restored positive
gate makes the packet fail. Stop the disposable worker and retain evidence;
do not start another Cargo command or retry to manufacture acceptance.

Run remotely under the coordinator's admitted build deadline and capacity
limits. These commands are documentation only; this child ran none of them.
`EVIDENCE` must resolve outside the disposable source snapshot.
`SOURCE_REVISION` must be supplied by the coordinator's archive admission.
Keep the runner's outer job limit at 600 seconds. Build the capture helper
sequentially in its separately admitted bounded command, never concurrently.

```sh
python3 crates/konnect-core/tests/native_footprint_children_loss_build.py \
  --disposable-checkout --source-revision "$SOURCE_REVISION" \
  --out "$EVIDENCE/loss-build"
CARGO_TARGET_DIR="$PWD/target" cargo build -p konnect-ipc --locked \
  --example native_footprint_children_capture
```

### Native launch commands

Use the existing supervisor unchanged with a 60-second child limit, plus a
coordinator outer command deadline. These are commands to prepare and run
remotely, not authority for this child to execute them. `GATE` and `EVIDENCE`
must be fresh directories outside the source archive; keep evidence after exit.

```sh
mkdir -p "$GATE" "$EVIDENCE/supervisor-audit"
cp crates/konnect-core/tests/support/pcb_export_supervisor.py "$GATE/a8-supervisor.py"
python3 - "$GATE/a8-supervisor.json" "$EVIDENCE/supervisor-audit" <<'PY'
import json, sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({
    "program": "/usr/bin/python3", "seconds": 60, "audit": sys.argv[2]
}))
PY
xvfb-run -a python3 "$GATE/a8-supervisor.py" \
  "$PWD/crates/konnect-core/tests/native_footprint_children_native.py" \
  --disposable-worker --konnect "$PWD/target/debug/konnect" \
  --capture-helper "$PWD/target/debug/examples/native_footprint_children_capture" \
  --source-revision "$SOURCE_REVISION" --out "$EVIDENCE/native"
```

After the supervised command exits, independently verify the recorded root is
absent and the lifecycle has no final owned descendants, mounts or errors.
Require successful supervisor completion and retain its audit and all captures.
Never delete a failed root by hand to make lifecycle acceptance pass.

```sh
python3 - "$EVIDENCE/native/native-lifecycle.json" <<'PY'
import json, sys
from pathlib import Path
audit = json.loads(Path(sys.argv[1]).read_text())
assert not audit["cleanup_errors"], audit
assert not audit["mounts_under_control_root"], audit
assert not audit["snapshots"][-1]["owned_descendants"], audit
assert audit["temporary_directory_removed"], audit
assert not Path(audit["control_root"]).exists(), audit
PY
```

### Supervised native loss sequence

After source audit and the isolated build above, run the following from the
same clean disposable snapshot. This uses the unchanged 60-second supervisor
and a 90-second outer deadline per invocation. Every invocation gets its own
Xvfb, native root and evidence directory. No shared Konnect endpoint is used.
It runs the restored positive native gate before the loss control, and again
in `finally`. Each invocation must independently pass the lifecycle/root audit.
If cleanup fails, stop the worker and retain evidence; do not continue native
execution with uncertain ownership. The restored Rust gate has already run
inside the build runner regardless of native outcomes. Pass the coordinator's
explicit archived source revision here too; do not query or create Git history.

```sh
python3 - "$PWD" "$GATE" "$EVIDENCE" "$SOURCE_REVISION" <<'PY'
import json, re, shutil, subprocess, sys
from pathlib import Path

root, gate, evidence = map(lambda p: Path(p).resolve(), sys.argv[1:4])
revision = sys.argv[4].lower()
assert re.fullmatch(r"[0-9a-f]{40}", revision), "coordinator must supply full archive source SHA"
gate.mkdir(parents=True, exist_ok=True)
build = evidence / "loss-build"
helper = root / "crates/konnect-core/tests/native_footprint_children_native.py"
supervisor = root / "crates/konnect-core/tests/support/pcb_export_supervisor.py"
raw = root / "target/debug/examples/native_footprint_children_capture"
cleanup_ok = True

def run(name, *extra):
    global cleanup_ok
    script = gate / f"{name}.py"
    audit = evidence / f"{name}-supervisor"
    audit.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(supervisor, script)
    script.with_suffix(".json").write_text(json.dumps({
        "program": "/usr/bin/python3", "seconds": 60, "audit": str(audit)}))
    out = evidence / name
    cleanup_ok = False
    result = subprocess.run([
        "xvfb-run", "-a", "/usr/bin/python3", str(script), str(helper),
        "--disposable-worker", "--konnect", str(build / "positive-konnect"),
        "--capture-helper", str(raw), "--source-revision", revision,
        "--out", str(out), *extra], cwd=root, timeout=90)
    lifecycle = json.loads((out / "native-lifecycle.json").read_text())
    assert not lifecycle["cleanup_errors"], lifecycle
    assert not lifecycle["mounts_under_control_root"], lifecycle
    assert not lifecycle["snapshots"][-1]["owned_descendants"], lifecycle
    assert lifecycle["temporary_directory_removed"], lifecycle
    assert not Path(lifecycle["control_root"]).exists(), lifecycle
    cleanup_ok = True
    result.check_returncode()
    report = json.loads((out / "native-footprint-children.json").read_text())
    assert report["status"] == "passed", report
    if extra:
        assert report["unsafe_loss_control"]["status"] == "reproduced", report

run("loss-positive-before")
try:
    run("loss-control", "--mode", "sync-loss-control",
        "--control-konnect", str(build / "control-konnect"),
        "--control-build-evidence", str(build / "native-loss-build.json"))
finally:
    if cleanup_ok:
        run("loss-positive-restored")
    else:
        raise RuntimeError("native cleanup uncertain; retain worker for inspection, no further native run")
PY
```

Retain all three native reports, supervisor audits and the build manifest.
A failed control remains a failed sequence even when restored positives pass.
Do not close `3bqz` on the safe gates alone or infer blanket point loss from
group/variant loss. All these native/control results remain pending.

Pending acceptance: formatting, compile, served regressions, every isolated
control and restored positive, full workspace checks, native captures,
positive refusal zero-RPC proof, actual native loss reproduction and per-kind
observations, native readback, lifecycle audit and independent root absence.
The coordinator owns admission, final merge and clean merged-child removal.
