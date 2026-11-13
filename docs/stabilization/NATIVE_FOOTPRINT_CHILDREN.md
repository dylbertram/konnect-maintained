# Native footprint child refusal

Bounded A8 implementation for `konnect-maintained#3bqz` and `#emvb`.
Accepted child `a8b79adc15031ea8c1e33c4b5c7d77d7038dd7fe` merged into
`cleanup/main` at `ca8632d7c53ebee9cafb1625a5ed7d703d3cbd37`.
Child and combined-parent verification passed. `3bqz/emvb` are closed with
typed evidence; broader A8 scopes remain open.

## Contract

`update_pcb_from_schematic` and `update_footprints_from_library` inventory
actual existing whole-footprint replacement targets before reporting ready
or applying. They serialize the exact bound live document through
`SaveDocumentToString`, require a `kicad_pcb` root and match direct top-level
footprints by exact KIID. Only directly owned groups, points and variants count.
Board-level membership or variant declarations alone do not cause refusal.

An unsupported child produces the existing conflict response with
`footprint_children_not_carried`, reference, KIID, library ID, child kinds,
counts and variant names. One unsafe target clears the whole mixed plan,
including otherwise safe additions and updates. Refusal precedes commit,
mutation, save and revert RPCs.

Unreadable, malformed, wrong-root, missing-target, duplicate-target and
malformed-variant inventories fail closed. An `inventory_error` with empty
`kinds` means unknown, not safe. There is no saved-file fallback or reference
guess. Apply obtains fresh native inventory even when the protobuf revision
has not changed.

Noop and add-only plans add no inventory read. Sync retains its existing
pre-commit recovery snapshot, including add-only apply. Library-input policy
and existing revision guards are unchanged. The inventory does not lock out
later GUI edits or make requests atomic across the GUI.

This is bounded compatibility refusal, not universal metadata preservation.
Other metadata, other replacement tools, A7 propagation, direct-placement
styling and stroke/no-op convergence remain separate work. The accepted field
foundation is unchanged.

## Observed KiCad 10.0.8 behavior

The supervised Linux native sequence passed a positive refusal gate, an actual
unguarded sync control, then a restored positive gate. The control disables
only sync's inventory target selection in a separately retained binary. It
uses the maintained whole-footprint replacement path, not fabricated protobuf
or text-edited CAD.

| U1 directly owned child | Before control | After control |
| --- | ---: | ---: |
| Group | 1 | 0 |
| Point | 1 | 1 |
| Variant `NoTimer` | 1 | 0 |

The point survived completely. Board-level group membership also survived,
including R1 and U1. Do not adopt the archived blanket point-loss claim.

Independent raw IPC captures show no owned-group message or native group KIID
in U1's received payload, a `kiapi.board.types.ReferencePoint` Any, and raw
variant field 20. Its saved payload independently decodes to `NoTimer` with
DNP true before legacy FootprintInstance decoding. A tag count or old-schema
re-encoding is not the oracle. Production protobuf definitions are unchanged.

Both positive runs exercised dry run and apply for both tools. All four
refusals reported the exact U1 children. Recorded proxy traffic contains zero
commit/mutation/save/revert RPCs during refusal. Native strings, raw footprint
payloads and disk bytes remained unchanged. Disk equality is an executed
helper assertion; no separate after-refusal disk copy is retained.

R1 genuinely changed through library refresh and sync, with independent
GetItems/native captures and saved-board readback. U1's native footprint and
raw payload remained unchanged. The control recorded exactly one U1
UpdateItems, successful commit/save, and the requested value on native disk
readback before attributing group/variant loss.

All three supervisors completed without timeout. NativeRun audits found no
remaining owned descendants, mounts or cleanup errors. The coordinator
independently verified each temporary root absent after helper exit.

These results qualify this Linux/KiCad version and bounded paths, not every
platform, child kind, native race or future KiCad release.

## Fixtures and native setup

The imported board and README remain byte-identical to upstream commit
`8fdc5ba6f494b7349b632de093d623cb4d7288cf`, with AGPL provenance recorded in
`crates/konnect-core/tests/fixtures/footprint_children_kicad10.SOURCE.md`.
The README's native 10.0.6 generation claim is distinct from the independent
10.0.8 acceptance above.

The fixture is imported verbatim. Accepted-copy changes use native pcbnew or
public Konnect tools. Project-local library copies isolate library-input
policy from placed-instance refusal; placed U1's children are never removed
during setup.

KiCad 10.0.8 uses `SetLibDescription` / `GetLibDescription`. Footprint points
occupy `Points()`, not `GraphicalItems()`, and Python exposes opaque elements
that cannot be passed to `RemoveNative`. A supervised probe qualified
`Points().clear()` on these disposable library copies, followed by native
save/reload with zero points and an unchanged stock-library hash.
The helper requires SafeDIP to start with one point and SafeR with none, then
requires zero both immediately and on library reload. This is not a general
point-deletion implementation.

The immutable 10.0.8 variant schema, source commit, hash and field mapping are
recorded in
`crates/konnect-ipc/tests/fixtures/footprint_variant_schema_kicad10_0_8.SOURCE.md`.
Raw received payloads, native strings, served responses and RPC records remain
separate artifacts.

## Verification

At the accepted child:

- Seven A8 served/model tests passed, covering independent child kinds,
  dry run/apply, whole mixed-plan refusal, fresh apply inventory, unknown and
  duplicate inventory, actual safe updates, and noop/add-only read boundaries.
- Focused sync, refresh and field gates passed.
- All eight compile-first controls reached their designated assertion sites.
  Each restored exact source bytes and a seven-test positive gate.
- Full workspace checks passed: 2,549 tests, nine doctests, formatting,
  workspace Clippy, 18 plugin tests, nine script tests, documentation counts
  and revision-matched binary builds.
- The native sequence and independent evidence review passed.

Combined-parent verification at `ca8632d` repeated the workspace gate and all
accepted native supervisor, export, private-field, route, text, symbol, field
and A8 gates. Both standalone native helpers also passed independent post-exit
root-absence checks.

The isolated builder records the source revision, production Rust/Cargo
manifests, sole control mutation, build logs and copied binary hashes. Baseline
and restored binaries have the same hash. Both retained executables were
independently stream-rehashed against the build and runtime manifests.
The 180 Rust/dependency files verified at `f3578fc` remained unchanged through
the helper-only corrections.

Coordinator artifacts are retained locally under
`.remote-build-jobs/boat-native-children-verification/`, including failed setup
attempts, their clean lifecycle audits, the API probe, controls, complete native
captures, build manifests and the copied executables. These generated artifacts
are not committed source or hosted CI evidence.

## Reproduction and execution limits

Only the coordinator admits jobs in disposable sanitized source snapshots.
Follow `byskap-builds`, measure worker/host capacity and retain at least 500 MB
RAM. Keep source/cache access exclusive during mutation controls. No local
fallback, shared editor, live hardware or unbounded native process is allowed.

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

For the native control, supply the full revision from the verified archive
metadata. Sanitized snapshots contain no Git history. `EVIDENCE` must be
outside the source snapshot and fresh; `ROOT/target` must be an exclusively
admitted warm cache.

```sh
python3 crates/konnect-core/tests/native_footprint_children_loss_build.py \
  --disposable-checkout --source-revision "$SOURCE_REVISION" \
  --out "$EVIDENCE/loss-build"
```

The builder shares a fixed 600-second outer budget, restores exact source in
`finally`, and requires restored positives. A timeout or compilation failure
is never accepted as a control. Distinct copied binaries survive later cache
rebuilds.

Run `native_footprint_children_native.py` through the unchanged
`support/pcb_export_supervisor.py` with a 60-second child deadline and a
90-second coordinator outer deadline per invocation. Its adjacent JSON must
select `/usr/bin/python3` and a fresh supervisor audit directory. Every run
gets its own Xvfb, native root and evidence output.

Use `--disposable-worker`, `--source-revision`, `--out`, the copied
`loss-build/positive-konnect` as `--konnect`, and the revision-matched
`native_footprint_children_capture` as `--capture-helper`.
The sequence is:

1. Default refusal mode on a fresh copy.
2. A fresh `--mode sync-loss-control` run with `--control-konnect` pointing
   to the copied control and `--control-build-evidence` to its
   `native-loss-build.json`.
3. Default refusal mode again in `finally`, provided lifecycle cleanup is
   independently verified safe.

Require successful supervisor completion, a passed native report, empty final
owned descendants/mounts/errors, `temporary_directory_removed`, and a separate
post-exit root-absence check for every run. Preserve a failed root for inspection;
never manually delete it to make acceptance pass. Cleanup uncertainty stops
further native execution. A failed control remains failed even if restoration
passes.
