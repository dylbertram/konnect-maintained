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
and thickness in nanometres. It does not compare only the outgoing message.
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
- six separately corrupted readbacks: custom omission, layer, visibility,
  upright, angle and thickness;
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
raw angle, ignoring thickness and losing apply staging. Its expected assertions
have not been executed here.

## Native capture

Existing checked-in 0603 and socket footprints provide native-authored parser
bases. In-code altered strings are controlled synthetic variants, **not**
additional native captures. Existing board files contain raw non-upright 180°
properties but are not a paired library-to-IPC angle oracle.

`crates/konnect-core/tests/library_field_native.py` supplies the missing paired
capture. It requires Linux, Python with KiCad 10 `pcbnew`, a compiled Konnect
binary, installed libraries/templates, PCB Editor and Xvfb. Missing prerequisites
fail rather than skip. No packages are installed by the script.

The helper loads the existing native socket through `pcbnew.FootprintLoad`,
authors three library variants through the native model and `FootprintSave`,
then registers their project-local library through the public Konnect tool.
Its visible custom field also checks for duplicate ordinary-text drawings.
It creates a new synthetic project through public tools. The live sync runs
through stdio MCP with real `kicad-cli` and the helper's own headless PCB Editor.
`save_project` saves the native result.

Native controls independently place the same libraries through pcbnew's model.
Six refresh controls include both reported angle cases, both sides and upright
controls. A changed custom value forces refresh without claiming no-op
convergence. Native-saved results are reloaded and compared field by field,
including raw angle, thickness, side, visibility, upright and position.
Sync must also return no field-readback diagnostics from the actual IPC query.
There is no mock IPC or fabricated native oracle.

Run it under the **existing export supervisor**, which owns a 60-second runtime
cutoff, process-group termination and Linux subreaping. Native helper children
stay in that group. The helper also closes its own MCP child and disposable
editor in `finally`. An external cancellation must not bypass the supervisor.
Use a fresh output path each run, and retain failure artifacts.

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
xvfb-run -a /usr/bin/python3 target/a6-supervisor/run.py \
  crates/konnect-core/tests/library_field_native.py \
  --disposable-worker --konnect "$PWD/target/debug/konnect" \
  --source-revision "$SOURCE_REVISION" --out "$PWD/target/a6-native"
tar -czf target/a6-native-evidence.tar.gz target/a6-native target/a6-supervisor/audit
```

Request artifact `target/a6-native-evidence.tar.gz`. Key contents:

- `native-fields.json`, including version, supplied revision and pass/incomplete;
- `served-responses.json`, `konnect.log` and both editor logs;
- `sync-native-saved.kicad_pcb` and `sync-native-control.kicad_pcb`;
- `refresh-native-before.kicad_pcb`, `refresh-native-saved.kicad_pcb` and
  `refresh-native-control.kicad_pcb`;
- the three native `project/A6.pretty/*.kicad_mod` files;
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
