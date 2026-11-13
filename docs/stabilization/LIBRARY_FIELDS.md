# A6a library-field foundation

This is an implementation and proposed verification gate, not test results.
Base: `cleanup/main` at `410675b`. Existing Kata `annz` owns this bounded work;
`z705` remains the broader, partially addressed report. The coordinator owns
remote admission, native capture, parent integration and subsequent cleanup.
No build, test, typecheck or job was run by the child. Local checks were scoped
Rust formatting and Git whitespace/source review only. After the server restart,
the final formatter invocation unexpectedly triggered rustup's automatic
repair/install of Rust 1.96.0. This was not an admitted build/test setup; no
further local toolchain commands were run. The coordinator should account for
that host-side effect rather than treating this as installation-free work.

## Corrections to the unapproved first commit

Independent review rejected `95e9d23` for two issues. Runtime gates must use
the corrected child, not that first commit:

- Readback compared message presence for angle/thickness. In KiCad 10.0.5,
  [`EDA_TEXT::Deserialize`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/common/eda_text.cpp#L208-L249)
  reads `angle().value_degrees()` and `stroke_width().value_nm()`, including their
  protobuf zero defaults. [`Serialize`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/common/eda_text.cpp#L174-L205)
  always materializes those messages. `FieldShape` now compares their values,
  with absent equal to zero. It does not ignore a nonzero thickness difference.
  A served fallback case rewrites outgoing absent widths to explicit zero
  messages, asserts the serialized response differs from the sent message and
  requires no spurious readback diagnostic. A separate omitted-angle shape
  check and nonzero-to-missing/nonzero-to-different-width losses remain covered.
- The native helper used `SetOrientationDegrees`, which is not a whole-footprint
  editor rotation. In
  [`FOOTPRINT::Rotate`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/footprint.cpp#L2900-L2920),
  KiCad sets orientation and then calls `KeepUpright` on its fields.
  [`SetOrientation`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/footprint.cpp#L3101-L3127)
  rotates children without that final step.
  [`PCB_TEXT::KeepUpright`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/pcb_text.cpp#L374-L392)
  returns immediately for non-upright text. The helper now invokes the real
  `FOOTPRINT::Rotate` operation, with no postprocessing of expected field angles.

These are source corrections and proposed controls, not claims of execution.
No local toolchain commands, builds, tests or jobs were run for this correction.

Subsequent review of parent-refreshed `c4a1d90` found that the native
`FallbackLocked` precondition did not survive saving. KiCad's
[field writer](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp#L1243-L1255)
emits properties separately from ordinary text; the latter's locked-clause
serialization cannot establish a PCB_FIELD lock in the saved library.
The helper now uses `FallbackDuplicate`, renaming the existing custom
AssemblyVendor field to KiLib_Generator through native `PCB_FIELD::SetName`.
The field writer iterates every field and writes its canonical name without
deduplicating. Immediately after native `FootprintSave`, a read-only assertion
requires exactly two saved `(property "KiLib_Generator" …)` entries and records
`fallback-native-precondition.json`. If the installed KiCad does not persist
that case, the gate fails before sync rather than editing the CAD artifact.

Coordinator provisioning initially requested nonexistent `python3-kicad`;
the existing setup subsequently confirmed pcbnew 10.0.8. The exact `c4a1d90`
remote run then stopped on two Rust formatting differences before tests.
Both reported differences are corrected by patch here. No local formatter,
test or job was run for this follow-up; all runtime acceptance remains pending.

Coordinator verification of `d612c964` subsequently passed formatting, focused
`a6_`, `library_footprint`, `pcb_sync` and `pcb_footprint_update` tests, workspace
Clippy and the debug build. Native command `cmd_yexsr7rgt9op` exited 1 before
production because `/usr/share/kicad/template/design-block-lib-table` was absent.
Installing `kicad-templates` 10.0.8 in `cmd_xjbjz4lf7k1m` did not supply it.
That is a configuration-layout assumption in the helper, not field acceptance
evidence or proof that KiCad's design-block capability is unavailable. The
minimal helper correction below has not been executed by the child.

Coordinator reports all 12 isolated `d612c964` mutation controls passed their
expected failure checks, with all nine positives passing again after exact
source restoration. Production Rust remains unchanged in the helper follow-ups.
Native `c9f3409` command `cmd_nzt49exyhq5q` got past configuration and failed
before production because convenience `pcbnew.FootprintSave` could not infer a
plugin from the newly empty `A6.pretty`. The helper now explicitly constructs
`PCB_IO_KICAD_SEXPR`, invokes `CreateLibrary` on the not-yet-existing path and
uses that plugin instance's `FootprintSave`. No seed CAD, plugin-name heuristic
or unverified plugin-manager enum is used.

Native v3 `cmd_brzxgphxmsfb` reached every sync/refresh comparison and populated
the actual/expected fields, then failed with `EBUSY` removing disposable
`home/.gvfs`. This is an overall failed run due to cleanup, not a field mismatch
and not accepted native evidence. Preserve its failed root and retrieved
`native-v3-full.log` / `native-v3/target/a6-native-v3` artifacts; do not reuse
or manually delete that root.

Coordinator read-only reconciliation `cmd_dnij1fbajqjs` found no remaining
ka6/helper/editor/Konnect processes. The observed gvfsd/gvfsd-fuse processes and
`/run/user/1000/gvfs` mount predated the tests and belonged to shared desktop
services. No orphan or ownership claim is inferred from those observations.
This child does not kill, unmount or otherwise repair those shared services.
The new helper must run with a fresh root and prove its own cleanup.

## Source review and replacement boundary

The archived #820 record has three discussion comments, zero formal reviews and
zero inline review comments. Its final comment identifies the unresolved raw
angle fold at exact candidate head `6312362`.

Preserved `origin/fix/789-library-fields` is
`63123625e1e4512cdca3cce1ac5ab4997db3ee4d`. Its only commits beyond `9488e5f`
are `e0a862b2db6b66fea63d2c47852a475050e8c212` and `6312362`. The three
affected Rust files at child base are byte-identical to `9488e5f`. No historical
branch merge, checkout or wholesale tree replacement was used.

The replacement reconstructs that unique four-file foundation:

- shared property reader in `library_footprint.rs`, accepting `(unlocked …)`
  as keep-upright rather than lock;
- library presentation and mandatory/custom fields on sync **additions**;
- property-free drawing when fields travel, the old drawing/template fallback
  with non-blocking `library_fields_not_carried` otherwise;
- committed field readback and the served apply-path staging assertion;
- migration guidance adapted to the maintained tree.

It also fixes the candidate's outstanding angle defect, compares raw angle and
thickness in live readback, and exercises actual fallback apply. Value-only
mandatory properties trigger the diagnostic fallback for additions rather than
silently losing a value with no complete typed presentation. Refresh retains
its earlier value-only handling.

This is an intentional corrected replacement, not byte-equivalence to #820.
The old archived test bodies and migration text are not all copied verbatim.
Their contracts are covered by the tests below and remain subject to execution.
Historical CI, stock-library counts and native observations are not results
for this child.

Only after this replacement is reviewed, verified on the combined parent and
merged may the coordinator classify the old candidate as superseded and retire
its branch, subject to checking that ref has not gained new content. There is no
remaining intended unique production feature from its two commits after that
accepted replacement. Keep its acceptance record and provenance even if its ref
is retired. Neither the old ref nor any unrelated branch is deleted here.

## Contracts and reachable paths

Sync: `prepare_footprint_source` reads properties; `build_added_footprint`
applies library data before schematic Reference/Value text, DNP, paths and nets.
Additions remain F.Cu at rotation 0°. A local 180° non-upright field is therefore
reachable through the served tool. A malformed hidden property still produces
a named diagnostic and does not refuse an otherwise supported addition.

Refresh: `plan_updates` → `parse_library_footprint` → `build_updated_instance`
→ `merge_custom_properties` → shared `transform_library_property`. Only custom
fields take this transform; existing Reference/Value presentation is preserved.
The transform retains its back-side position, angle, mirror and layer changes.
Readable-angle folding applies only when keep-upright is enabled. Otherwise
the transformed raw angle is reduced by full turns only.

Sync readback queries the live document after commit. It compares every sent
field by name, text, layer, visibility, keep-upright, raw angle modulo a full turn
and thickness in nanometres. Absent angle/width messages equal their protobuf
zero values; a nonzero value does not equal absence. It does not compare only
the outgoing message.
A mismatch is `board_readback_differs` after applied work, never a rollback
claim. Extra mandatory fields KiCad creates are ignored. This does not claim
complete font/style/position readback. Refresh still has no production
post-commit readback; the native gate separately checks its saved fields.

No existing-footprint sync update expansion, direct-placement field adoption,
stroke/no-op normalizer, A7 propagation/reset, protobuf or schematic changes
are included. A8 `3bqz/emvb` whole-footprint loss guards are not fixed here.
Review their overlap in `pcb_sync.rs` before integration, and do not run this
gate against a board owning groups, points, variants or other unsupported data.

## Coordinator-admitted remote gates

Follow `byskap-builds`, measure worker and shared-host capacity, and retain the
500 MB reserve. These are commands for a disposable, authorized remote source
snapshot only. The child does not submit them.

```sh
cargo fmt --all -- --check
cargo test -p konnect-core --locked --lib a6_ -- --test-threads=1
cargo test -p konnect-core --locked --lib library_footprint
cargo test -p konnect-core --locked --lib pcb_sync
cargo test -p konnect-core --locked --lib pcb_footprint_update
python3 crates/konnect-core/tests/library_field_controls.py all
cargo clippy --workspace --locked --all-targets -- -D warnings
```

The focused assertions cover:

- raw local 180° at footprint 0°, local 0° at footprint 180°, front/back,
  negative/full-turn angles, upright controls and preserved side transforms;
- native-base Reference/Value layers, thickness, visibility and custom fields;
- absent/no/yes unlocked clauses, invalid and repeated clauses, locked refusal;
- served sync dry run/apply, raw 180°, schematic text and hidden custom fields;
- a non-echo canonicalized-default readback that must succeed;
- independently corrupted readbacks: custom omission, layer, visibility,
  upright, angle, changed nonzero thickness and missing nonzero thickness;
- unsupported **hidden** property, repeated-name and partial-mandatory-field
  apply with diagnostic, not just plan readiness;
- visible-field no-double-drawing and fallback drawing;
- served refresh raw-angle dry run/apply, preserved Reference/Value, and
  unsupported-property refusal before any commit/update or saved-board change.

The Python runner uses the existing read-identity runner's pattern: passing
baseline, one compile-preserving production mutation at a time, named failing
regressions, finally-restored exact bytes, then a restored positive gate.
Compile failure is not accepted as evidence. Controls cover unconditional
folding, removing upright folding, always-off keep-upright, locked fields,
skipped field adoption, double drawing, missing fallback diagnostic, ignoring
raw angle, ignoring thickness, mistaking absent default messages for changes
and losing apply staging. Its expected assertions have not been executed here.

## Native capture

Existing checked-in 0603 and socket footprints provide native-authored parser
bases. In-code altered strings are controlled synthetic variants, **not**
additional native captures. Existing board files contain raw non-upright 180°
properties but are not a paired library-to-IPC angle oracle.

`crates/konnect-core/tests/library_field_native.py` supplies the missing paired
capture. It requires Linux, Python with KiCad 10 `pcbnew`, a compiled Konnect
binary, installed libraries/templates, PCB Editor and Xvfb. Missing prerequisites
fail rather than skip. No packages are installed by the script.

The symbol and footprint template tables remain required. The design-block
template is optional: A6a does not use any design-block library. KiCad's native
[first-run configuration model](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/include/dialogs/panel_global_lib_table_config.h#L30-L38)
explicitly offers `EMPTY` alongside `DEFAULT` and `CUSTOM`.
Its [library-table grammar](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/include/libraries/library_table_grammar.h#L119-L125)
accepts an optional version and zero library rows, and its checked-in
[KiCad 10 configuration fixture](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/qa/data/config/10.0/design-block-lib-table)
is exactly:

```text
(design_block_lib_table
  (version 7)
)
```

The helper copies the installed design-block template if present. Otherwise it
writes that valid empty configuration only under its disposable
`HOME/.config/kicad/10.0`, selected through `KICAD_CONFIG_HOME`, and records
`design_block_table_setup` in `native-fields.json`. It does not modify the
worker's real user configuration, install another package, fabricate CAD or
alter any native expected data. Native editor readiness is still checked
through IPC; configuration does not substitute for a functioning editor.
No extra installation is required to resolve this missing optional template.

The helper loads the existing native socket through `pcbnew.FootprintLoad`,
authors four library variants through the native model and `FootprintSave`,
then registers their project-local library through the public Konnect tool.
For writing it explicitly selects `pcbnew.PCB_IO_KICAD_SEXPR()`, whose
[native constructor and methods](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.h#L388-L406)
accept `CreateLibrary(path)` and `FootprintSave(path, footprint)` with optional
properties omitted. Native
[`CreateLibrary`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp#L3512-L3526)
refuses an existing directory, so the helper no longer pre-creates `A6.pretty`.
Native
[`FootprintSave`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.5/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp#L3360-L3367)
sets library-format control flags itself. The helper asserts directory/file
creation and the saved duplicate-property precondition as before, and records
`footprint_library_writer` in `native-fields.json`. No hand-authored footprint
is needed to make plugin inference succeed.
Its visible custom field also checks for duplicate ordinary-text drawings.
It creates a new synthetic project through public tools. The live sync runs
through stdio MCP with real `kicad-cli` and the helper's own headless PCB Editor.
`save_project` saves the native result. `FallbackDuplicate` is authored by
renaming the existing visible custom AssemblyVendor field to KiLib_Generator
and saving it alongside the original hidden KiLib_Generator field. The helper
asserts that both names were actually persisted before testing the fallback.
It must produce exactly the duplicate-name fallback diagnostic for R903, with
no additional `board_readback_differs` from the template's omitted widths.
Its saved fields are captured separately; template fallback is not claimed
as library fidelity.

The native control operation is explicit: load a front-side library footprint
at orientation zero, add it to a native board, translate it to its anchor,
optionally call native `Flip(anchor, False)`, retaining the existing helper's
flip operation, then call
`Rotate(anchor, EDA_ANGLE(target - current_orientation, DEGREES_T))`.
This represents a whole-footprint placement/rotation action, not raw setter
semantics and not KiCad's complete Update Footprints from Library command.

The six refresh cases are Raw180/front/0°, Raw0/front/180°, Raw0/back/0°,
Raw180/back/180°, Upright0/front/180° and Upright0/back/180°.
The back upright case targets 180° too, instead of flip-only back/0°. The
rotation delta uses KiCad's orientation after the flip. Rotate by zero returns
immediately, and the helper does not claim that arbitrary raw setters or
flip-only raw storage match the retained readable-angle policy. It does not
add a corrective field edit or rotate-and-undo sequence to its expected artifact.

Before any production field comparison, `native-operation-control.kicad_pcb`
and its JSON capture distinguish the operations on unmodified native results.
R920 uses whole-footprint Rotate 180° on Upright0; R921 uses the raw orientation
setter to 180° on the same library. R920's raw field angles must be 0° and R921's
180°. R922 rotates Raw0 to 180° and R923 leaves Raw180 at footprint 0°; both
must retain raw 180° and keep-upright false. These native-only assertions are
independent of production transforms. Replacing the helper's Rotate with the
old orientation setter must fail the R920 contract assertion, not be repaired
by folding the captured expected fields.

A changed custom value forces refresh without claiming no-op
convergence. Native-saved results are reloaded and compared field by field,
including raw angle, thickness, side, visibility, upright and position.
Sync must also return no field-readback diagnostics from the actual IPC query.
There is no mock IPC or fabricated native oracle.

### Desktop isolation and lifecycle

Before importing `pcbnew` or starting a child, the helper selects these settings
in its own process and child environments, never in the worker's global settings:

- `GIO_USE_VFS=local`, selecting GLib's local VFS rather than desktop GVFS.
  See [`g_vfs_get_default`](https://github.com/GNOME/glib/blob/2.80.0/gio/gvfs.c#L343-L366).
- `GSETTINGS_BACKEND=memory`, so settings are not stored/shared through dconf.
  See [GLib's backend selection](https://github.com/GNOME/glib/blob/2.80.0/gio/gsettingsbackend.c#L978-L1025).
- `GTK_USE_PORTAL=0`, disabling opt-in portal use in the non-Flatpak worker.
  See [GTK 3's portal selection](https://github.com/GNOME/gtk/blob/3.24.41/gtk/gtkprivate.c#L270-L292).
- `DBUS_SESSION_BUS_ADDRESS=unix:path=<fresh-owned-runtime>/no-session-bus`,
  an explicitly unavailable socket, and no inherited D-Bus starter variables.
  [GLib uses an explicit session address without its discovery/autolaunch fallback](https://github.com/GNOME/glib/blob/2.80.0/gio/gdbusaddress.c#L1345-L1355).
  This test does not need a session bus and must not request desktop activation.
- Private HOME/config/data/cache/runtime paths, a mode-0700 runtime directory,
  and `NO_AT_BRIDGE=1`. These controls are not a security sandbox.

`NativeRun` registers each owned editor immediately after launch, adopts orphan
descendants with Linux `PR_SET_CHILD_SUBREAPER` in the helper itself, and closes
all registered editor leaders and the MCP leader **before** attempting temporary
directory deletion. EOF/terminate/kill/reap operations address only those
explicitly spawned leaders. Post-kill waits are bounded; the helper never signals
a discovered service or performs an unmount.

`native-lifecycle.json` records process metadata, not arguments or environment:
preexisting desktop services; owned descendant PID/PPID/group/session/start-time
snapshots; nonblocking reaped adopted-child statuses; possible group/session
escapes; newly observed desktop-service candidates; mounts under the owned root;
and removal/cleanup errors. New unrelated desktop activity is not automatically
attributed to the test. After leader closure, a three-second settle/reap/audit
requires two consecutive snapshots without owned descendants, including adopted
children outside the original group/session. Observed PID/start-time identities
remain tracked across reparenting. New desktop-service candidates fail the audit
for coordinator reconciliation, without asserting test ownership or signalling
them. Mount auditing reads `/proc/self/mountinfo` only for the
fresh root. Any live descendant, audit error or owned-root mount fails and
retains the root without signalling services or unmounting anything.

`shutil.rmtree` is attempted only after those checks. A removal error, including
`EBUSY`, remains a cleanup failure, never ignored. Original comparison/native
exceptions and their traceback are preserved even if cleanup also fails; cleanup
errors and tracebacks are recorded separately. Evidence-writing errors also
produce a failed exit without masking an already active primary exception.

Overall `status: "passed"` is set only after field comparisons **and** successful
owned-child closure, descendant/mount audit and root removal. A comparison-only
success is separately `field_comparisons: "passed"` and is not acceptance.
Require `lifecycle.temporary_directory_removed: true`, empty cleanup errors and
no remaining owned descendants in the final audit, plus the supervisor's final
audit. `group_reaped` alone is insufficient.

Run under the **existing export supervisor**, which independently owns a
60-second runtime cutoff, process-group termination and Linux subreaping. On
cancellation it still stops/reaps group members. Deliberate session escapes,
SIGKILL or worker failure are outside a process-group-only cleanup guarantee;
the helper's normal-exit audit detects surviving owned descendants but does not
claim to repair them. A failed/missing lifecycle audit requires coordinator
reconciliation or worker disposal, not manual service cleanup. Use a fresh output
path and root for every retry; preserve failure artifacts.

After the coordinator admits the worker:

```sh
cargo build -p konnect --locked
mkdir -p target/a6-supervisor
cp crates/konnect-core/tests/support/pcb_export_supervisor.py target/a6-supervisor/run.py
python3 - <<'PY'
import json
from pathlib import Path
p = Path("target/a6-supervisor/run.json")
p.write_text(json.dumps({
    "program": "/usr/bin/python3", "seconds": 60,
    "audit": str(Path("target/a6-supervisor/audit").resolve()),
}))
PY
```

Set `SOURCE_REVISION` to the exact uploaded child or combined-parent SHA.
The source snapshot may have no `.git`, so the helper records the explicit
revision rather than inventing one:

```sh
native_status=0
xvfb-run -a /usr/bin/python3 target/a6-supervisor/run.py \
  crates/konnect-core/tests/library_field_native.py \
  --disposable-worker --konnect "$PWD/target/debug/konnect" \
  --source-revision "$SOURCE_REVISION" --out "$PWD/target/a6-native" || native_status=$?
cleanup_status=0
/usr/bin/python3 - <<'PY' || cleanup_status=$?
import json
from pathlib import Path
out = Path("target/a6-native")
report = json.loads((out / "native-fields.json").read_text())
audit = json.loads((out / "native-lifecycle.json").read_text())
assert report["status"] == report["field_comparisons"] == "passed", report
assert audit["temporary_directory_removed"] is True, audit
assert audit["cleanup_errors"] == [], audit
assert audit["mounts_under_control_root"] == [], audit
assert audit["snapshots"][-1]["owned_descendants"] == [], audit
assert all(not snapshot["new_desktop_service_candidates"]
           for snapshot in audit["snapshots"]), audit
# Check again after the helper has exited, not only inside its cleanup method.
assert not Path(audit["control_root"]).exists(), audit
PY
archive_status=0
tar -czf target/a6-native-evidence.tar.gz target/a6-native target/a6-supervisor/audit || archive_status=$?
if [ "$native_status" -ne 0 ]; then
  exit "$native_status"
fi
if [ "$cleanup_status" -ne 0 ]; then
  exit "$cleanup_status"
fi
exit "$archive_status"
```

This retains failure artifacts but never lets a successful archive command
hide either the native command's original failure or the post-exit cleanup check.

Request artifact `target/a6-native-evidence.tar.gz`. Key contents:

- `native-fields.json`, including version, supplied revision, operation and
  passed/failed/incomplete status;
- `native-lifecycle.json`, including descendant/session/mount audit and proven
  removal or explicit retention of the fresh control root;
- `native-operation-control.kicad_pcb` and `native-operation-control.json`;
- `fallback-native-fields.json`;
- `fallback-native-precondition.json`, proving the native-saved duplicate names;
- `served-responses.json`, `konnect.log` and both editor logs;
- `sync-native-saved.kicad_pcb` and `sync-native-control.kicad_pcb`;
- `refresh-native-before.kicad_pcb`, `refresh-native-saved.kicad_pcb` and
  `refresh-native-control.kicad_pcb`;
- the four native `project/A6.pretty/*.kicad_mod` files;
- supervisor started/finished audit records.

No native capture is committed or claimed yet. Review artifact provenance and
exact field comparisons before adding a compact reusable native fixture.
If native upright-angle representation disagrees with the retained fold,
record that evidence and stop for scope review rather than importing #972's
larger angle/no-op changes. Preserve the failing artifacts.

## Remaining acceptance and handoff

Keep `annz` open through review, native capture, negative controls, combined-tree
verification and parent merge. `z705` additionally requires current saved SMD/THT
attributes/models/description/tags, DRC mismatch accounting and default STEP
parts evidence. Pad-fabrication losses and coordinate-rounding mismatches are
separate unresolved acceptance concerns, not silently fixed by field adoption.

`x698` direct-placement presentation, `asfr/fnn9` stroke/native no-op behavior,
A7 symbol-field propagation, reset presentation and A8 preservation remain
separate. Do not close them or pull in their cumulative candidates.

The coordinator must merge every accepted child into `cleanup/main`, verify
ancestry and a clean child worktree, then remove only that fully merged child
branch/worktree. No push, deployment, settings changes, live hardware edits or
old-ref deletion is authorized by this child.
