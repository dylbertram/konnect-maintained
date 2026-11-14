# Project rename safety

## Hosted platform qualification

The rename implementation remains frozen at `9f6afc2`, with production Rust
and Cargo bytes from `efb72c6`. The user chose to hold integration for native
platform evidence, then authorized a test-only GitHub Actions workflow,
publication of a candidate validation branch, and a workflow/policy-only
default-branch bootstrap. Rename is not authorized for merge by that decision.

`rename-platforms.yml` has explicit Windows x64, Intel macOS and Apple Silicon
jobs. It checks out a full reviewed source SHA, records that SHA separately
from the workflow SHA, builds the core, runs the focused native filesystem
regressions and checks the workspace. Windows adds an explicitly invoked
localhost UNC test using an owned temporary SMB share. The native KiCad GUI
helper below remains Linux-only and is not claimed as Windows/macOS evidence.

Each runner checks CPU, memory and disk before setup. The validation helper
bounds cargo concurrency and process lifetime, monitors resource reserves,
and records source hashes, tool versions, exact command outcomes and cleanup.
Missing capacity, unavailable infrastructure, zero-test runs, changed protected
source, deadlines or uncertain cleanup fail the relevant gate.

Evidence artifacts are retained for seven days, including failed runs. A
passing result qualifies only its recorded commit, architecture, filesystem
and executed tests. It does not establish arbitrary network-share behavior,
native PCB project-token rewriting, schematic/PCB parity or same-session
editor recovery.

Before activation, the coordinator verified that remote ordinary CI and E2E
were individually disabled. The existing separately managed Dependabot service
and its configuration remain unchanged with explicit user approval. No release
or deployment workflow is added. Source preparation and authorization alone
do not establish that hosted validation has passed.

Bounded A2 work for `konnect-maintained#svj3` and `#xsz0`, based on
`96b2126a9acc39a9a042609f6b66cf6c1850c834`. The selected contract is guarded
writes with reported partial outcomes, not automatic rollback.

## Contract

The existing `project`, `new_name`, `rename_directory = false` and
`dry_run = false` schema and reference transformer are unchanged. Apply reads
all project-stem files and sibling schematic files with `read_consistent` before
moving any. Nonregular source entries, including symlinks, refuse before moves.
Directory siblings remain the target-selection rule; this change does not
introduce recursive hierarchy discovery or prove that every sibling belongs to
this project.

Before moving files, apply refuses active transaction-journal names without
recovering or interpreting their contents. Explicitly abandoned journals are
not active. It checks the old-name project lock, design locks and exact live
board through existing authority helpers. Unrelated open boards are not renamed
or edited. Design locks and project locks are checked again during apply.
The shared design-lock checker is exposed publicly without changing its logic.

Each rewrite compares the full original snapshot inside `transact_atomic`,
including unrelated same-length changes. Expected bytes are never refreshed
to make a stale request pass. The shared writer retains its own lock checks,
atomic replacement, fsync and readback behavior. Apply independently verifies
saved bytes and checks every final document path after optional directory moves.
Zero-hit documents participate in snapshot checks but are not rewritten.

No filename rollback is attempted. In particular, a recreated old source is
never overwritten in an attempt to undo an earlier move. Late conflicts may
leave new filenames containing old project references. That is reported partial
work requiring inspection, not a successful or automatically recoverable rename.
This operation creates no transaction journal.

## Response accounting

Successful apply preserves the legacy success fields and adds:

- `outcome.status`, one whole-request unit, and retry guidance.
- `applied.moved_files`, recording each completed file move.
- `applied.verified_rewrites`, recording only rewrites whose committed bytes
  were independently verified.
- `applied.current_paths`, derived from completed moves.
- `applied.backup_move` and `directory_move`, populated only after native move
  success.
- `applied.unverified_move_attempt`, identifying both sides when a failed
  native call has an uncertain result.

Paths describe completed moves, not a guarantee against subsequent external
changes. When an uncertain move could relocate a document,
`current_path` is null and `possible_paths` lists both candidates.
Earlier verified rewrites remain historical observations if a later external
save changes them.

A before-mutation refusal is `failed`; a later known refusal is `partial`.
Unexpected move errors, attempted-write failures, and failed postwrite/final
readback are `mutation_outcome_uncertain` with status `uncertain`. No whole
request retry is marked safe after partial or uncertain work. Known prewrite
KiCad-lock refusals remain refusals, not invented attempted writes.

Plain-text helper refusals are normalized to JSON before attaching accounting.
Their error flag and message survive; the existing plain-error `handler_error`
classification becomes explicit. Existing structured error fields are retained.
Project-lock wording is corrected inside the rename wrapper so a late refusal
does not claim that an already verified project rewrite never happened. Shared
helper behavior for other tools is unchanged.

A backup destination collision now stops apply with reported partial work.
It does not report an unperformed backup move. The optional containing-directory
move follows the same rules. Dry-run retains the previous preview fields,
including zero-hit entries and proposed names; it performs no authority calls,
move probes or writes. Preview is not apply authorization.

## Platform adapter and limits

The repository advertises Windows, macOS and Linux in `README.md:314–316`.
All three retain native implementations:

- Linux and macOS targets use `rustix = "=1.1.4"` with `fs` and
  `renameat_with(..., RenameFlags::NOREPLACE)`.
- Windows uses `windows-sys = "=0.61.2"` with `Win32_Storage_FileSystem`.
  `MoveFileExW` receives zero flags: no `REPLACE_EXISTING`, `COPY_ALLOWED`
  or delayed move. UTF-16 buffers reject embedded NUL, retain native path
  spelling including valid verbatim/UNC paths, and outlive the FFI call.
  `last_os_error` is captured immediately on failure.

Both versions already existed in the baseline lockfile. Only target-specific
direct dependencies and the core package's two lockfile dependency entries
change. There are no package/version updates, copying fallbacks, check-then-rename
fallbacks or link/unlink moves. Other, unadvertised targets have no emulation.

Source qualification:

- <https://docs.rs/rustix/1.1.4/src/rustix/fs/at.rs.html#292-314>
- <https://docs.rs/crate/windows-sys/0.61.2/source/src/Windows/Win32/Storage/FileSystem/mod.rs>
- <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw>

These are not macOS or Windows runtime evidence. A filesystem may reject a
particular native operation. Its failure can occur during backup/directory work
after earlier project moves and rewrites; no preflight claims to predict all
such failures. Unexpected errors are conservatively uncertain.
Unix parent directories are synced after completed moves. The Windows adapter
does not claim directory-fsync crash durability from a successful MoveFileExW
return. Neither adapter promises whole-operation crash atomicity.

Document locks coordinate participating writes, not arbitrary KiCad/external
saves. Snapshot comparisons and lock observations do not make the final
comparison-to-replacement interval atomic against noncooperating writers.
Old and new filename locks are distinct; they are not held as a multi-file
read/validate/move/commit queue. `2xzq` remains separate. Other project writers
`dyxj` and general transaction diagnostics `81c7` are unchanged.

## Fixtures and evidence boundaries

The fixtures are reused without edits or new asset imports:

- `multichannel_mixer.kicad_sch`, its child and `.kicad_pro` are documented as
  verbatim KiCad 10.0.5 multichannel demo files in
  `crates/konnect-core/tests/fixtures/annotate_duplicates.README.md`.
  Independent byte oracles count 29 root and 36 child project keys and compare
  exact transformed bytes, including original CRLF and unrelated content.
- `specctra_two_resistors.kicad_pcb` and its adjacent README establish native
  KiCad 10.0.5 serialization for board-move/byte-preservation checks.
- PRL JSON, changed-paper/version variants, injected locks/journals and the
  board project-token example are explicitly synthetic.

No existing native PCB fixture here contains a project token. Synthetic branch
coverage is not native board-reference-rewrite evidence. The multichannel
provenance note does not provide a precise upstream revision and explicit asset
license; that limitation remains. No additional assets were imported.

Tests use current-thread, one-shot hooks at synchronous boundaries inside real
served `McpHandler::handle_message` `tools/call` execution. Every armed hook must
be observed consumed. Cases cover clean exact-byte changes; root/child/pro/PRL
saves; clean and partial conflicts; old and late locks; exact versus unrelated
live-board mocks; pending journals; recreated old sources and concurrent new
targets; unavailable source paths; backup/directory outcomes; postwrite mismatch
and I/O; final paths; dry-run; Unicode and exclusive destination behavior.
These mocks are not real KiCad runtime evidence.

## Coordinator gates

The child ran no builds, tests, formatter, installs or native commands. All
commands below are proposed gates, not reported successes. Run only after
coordinator capacity admission.

First format remotely and review the returned diff. Formatting changes the
critical source hashes; update only this packet's pins after reviewing those
exact new bytes. Do not silently refresh unrelated accepted runners' lockfile
pins. Their old `Cargo.lock` pins require a separate coordinator decision.

```text
cargo fmt --all -- --check
cargo test -p konnect-core --locked --lib rename_revision_tests -- --test-threads=1
cargo test -p konnect-core --locked --lib rename_hierarchy_tests
cargo test -p konnect-core --locked --lib rename_rewrite_tests
cargo clippy --workspace --locked --all-targets -- -D warnings
python3 crates/konnect-core/tests/project_rename_controls.py all --coordinator-admitted --output-dir /tmp/opencode/project-rename-controls
cargo test --workspace --locked --lib --tests
cargo test --workspace --locked --doc
```

The control runner is Linux-only, offline, and owns its Cargo process groups,
runtime limits and JSON audits. Each variant must compile first and fail one
exact served test at its designated outcome, envelope or wording assertion, not
at compilation, zero tests, timeouts or an unrelated panic. All exact positive names are checked
before mutation and again after finally restoring pinned bytes and modes.
Only `project.rs` is mutated.

Controls are `failing-before-stale-write`, `guard-removed`,
`expected-refreshed`, `partial-hidden`, `uncertain-became-refusal`,
`old-owner-checks-disabled`, `exclusive-move-replaced`,
`plain-refusal-envelope-disabled` and `late-project-lock-wording-disabled`.
The failing-before case replays the original unconditional stale replacement
at the consumed hook. It is explicitly not a whole-file baseline checkout.
No control has been run by the child.

Cross-target compile gates, on admitted workers with the required toolchains,
SDKs, NNG C compiler and protobuf tools already provisioned:

```text
cargo check -p konnect-core --locked --lib --target x86_64-pc-windows-msvc
cargo check -p konnect-core --locked --lib --target x86_64-apple-darwin
cargo check -p konnect-core --locked --lib --target aarch64-apple-darwin
```

Run the focused served tests on native Windows and macOS as well. Windows adds
embedded-NUL rejection and verbatim Unicode/long-path checks; Unix adds a
dangling-symlink destination check. A real UNC share, case-only rename behavior,
filesystem-specific failures and platform error mapping require native fixtures
and are pending. Missing SDKs or workers must be reported as unverified gates,
not grounds to remove advertised platform implementations.

Native KiCad acceptance remains coordinator-owned: disposable project copies,
public Konnect rename/exports or native KiCad, retained before/after bytes,
identity/netlist checks, and supervised deadlines/audits using the unchanged
existing export supervisor. Do not open shared hardware, hand-patch CAD, infer
native acceptance from synthetic variants or use unbounded native processes.

## Source-only Linux native rename gate

`crates/konnect-core/tests/project_rename_native.py` is prepared for reviewer
inspection and coordinator admission. It has **not been executed by the child**.
It requires provisioned Linux, Python 3.9+, KiCad CLI/pcbnew 10.0.8, Xvfb, the
existing KiCad configuration templates, and an already-built Konnect binary.
Missing prerequisites fail; nothing is installed or skipped.

The helper imports the unchanged `library_field_native.NativeRun`, `Api`,
`open_editor` and `stop_editor` implementations. It also reuses the unchanged
A8 `isolate` and optional `OwnedProxy`. Private HOME/XDG/runtime/DBus settings
are established before importing pcbnew or opening the owned editor. It never
uses the global Konnect service or a shared hardware project.

The source manifest verifies the existing 15 pins without changing them and
records the helper/lifecycle/supervisor source hashes before and after the run.
The revision is supplied externally; no Git is needed in the uploaded archive.
The report records executable hashes and actual native versions. A matching
binary-to-source build provenance record remains the coordinator's responsibility.

The gate uses two fresh, clearly reported fixture copies:

1. The refusal copy opens the old-name board in its own real PCB Editor.
   Public `open_project` must confirm that exact board. Public `rename_project`
   must return an error with a failed outcome, zero applied work, unchanged
   current paths, and byte-identical project files/tree entries during the
   call. The actual reason/message are retained. The unchanged `Api.call`
   deliberately asserts on MCP errors; a small adapter accepts only that exact
   assertion with its complete retained `isError` response, not transport
   failures or unrelated exceptions.
2. After stopping/reaping that owned editor and closing its transport, the
   positive copy starts from untouched fixture bytes. This avoids deleting
   authentic locks or incorporating editor-generated PRL/project state into the
   byte oracle. It does **not** prove recovery of a previously observed-live
   project's saved-file authority in the same server session.

The refusal must match a current production ownership contract, not merely an
arbitrary failed/unchanged result. `recognized_route` records `project-lock`,
`design-lock` or `exact-live-board`; the nested refusal evidence retains the
exact guard path, reason and message contract. Project/board lock machine
reasons require their matching owned old-name path and source-defined message.
Schematic `conflict` has no `error.reason` in the current schema, so only its
exact lock message and one owned old-name schematic path qualify. The plain
exact-live-board refusal requires the exact message in both `message` and
`handler_error.reason`, bound to this case's preceding exact `open_project`
result and old-name project outcome target. Missing/unclassified reasons,
generic conflicts and foreign-case paths fail. No error path is invented for
the pathless live-board helper.

No authentic lock is removed to force the exact IPC branch. A project/design
lock may refuse first. That is valid native ownership evidence, not exact IPC
branch coverage. The report distinguishes it from the existing served-mock
coverage. When `--capture-helper` names the already-built
`native_footprint_children_capture` example, its unchanged proxy mode forwards
real bytes to the same owned editor and retains raw request/response files.
The refusal interval must contain zero mutation RPCs and no unclassified
non-read commands; an empty interval caused by an earlier lock is reported
honestly. Omitting this optional argument records RPC observation as unavailable,
not zero. Exact IPC-branch coverage requires the recognized exact-live-board
route and its real `GetOpenDocuments` RPC, never just a message substring or a
lock refusal. No fake RPC is generated.

The positive case uses public MCP dry run and apply with
`rename_directory: true`. Dry run must preserve all tree entries and bytes and
preview the independently known counts. Apply must report the actual three
filename moves, three verified rewrites and final directory/current paths.
Independent byte comparisons allow exactly 29 root and 36 child project-key
changes, the existing project's one documented `.kicad_pro` filename
substitution, and **no PCB byte changes**. Neither production transformer nor
CAD text authoring supplies the expected outputs.

Actual `kicad-cli sch export netlist --format kicadxml` runs on retained read
copies before and after rename, not on the actual four-file positive project.
Native execution showed that CLI export can add the root's `.kicad_prl`
preferences without changing any original data file. The helper therefore
retains separate pre/post export manifests and permits no addition except that
one exact root-stem filename. Every original file and directory entry must
remain identical; deletion, modification, a new directory, another filename or
a changed pre-existing PRL fails. An added PRL must be a nonsymlink regular
file with one link, at most 64 KiB, and a valid JSON object. Its native bytes
remain in the read copy and are also copied unchanged into an explicitly named
artifact with size/hash/mode/JSON metadata. No PRL is fabricated, removed or
normalized. A CLI run with no additions still requires all original entries
unchanged. This is not a claim that the whole CLI input copy remains read-only.

Exported XML is retained and compared by
component references/UUIDs/library/value/footprint/sheet UUIDs and named
pin-net membership, excluding only export ordering, net codes, generator dates
and design source filenames. pcbnew loads the saved before/after PCB and checks
footprints, pad net names, UUIDs and placement. It never saves/upgrades the CAD.

The schematic demo and two-resistor PCB are **independent fixtures**. This is
not schematic/PCB parity or sync evidence. Native PCB project-token rewriting
remains **UNESTABLISHED**; the existing PCB has no such token. No additional
asset is imported, fabricated or patched.

### Coordinator command, not executed

Run only after reviewing this source, capacity admission, and confirming that
the executable hashes belong to the intended immutable production source.
Use fresh paths outside the source archive. The following configures a copy of
the **unchanged** existing supervisor; it does not modify that source helper.
An already-built proxy helper is optional; remove only its argument when proxy
observation is not requested. Do not build/install prerequisites in this gate.

```sh
SRC=/home/user/rename-source
RUN=/home/user/project-rename-native-run
OUT=/home/user/project-rename-native-evidence
KONNECT=/absolute/path/to/already-built/konnect
CAPTURE=/absolute/path/to/already-built/native_footprint_children_capture
: "${SOURCE_REVISION:?Set the externally supplied full 40-hex source-archive commit}"

mkdir "$RUN"
cp "$SRC/crates/konnect-core/tests/support/pcb_export_supervisor.py" "$RUN/supervisor.py"
cmp "$SRC/crates/konnect-core/tests/support/pcb_export_supervisor.py" "$RUN/supervisor.py"
cat >"$RUN/supervisor.json" <<JSON
{"program":"/usr/bin/python3","seconds":60,"audit":"$RUN/supervisor-audit"}
JSON

timeout --signal=TERM --kill-after=5s 75s xvfb-run -a \
  /usr/bin/python3 "$RUN/supervisor.py" \
  "$SRC/crates/konnect-core/tests/project_rename_native.py" \
  --disposable-worker --source-revision "$SOURCE_REVISION" \
  --konnect "$KONNECT" --capture-helper "$CAPTURE" --out "$OUT"
```

The child runtime budget is 60 seconds. CLI calls have 12-second leader
timeouts; reused MCP calls have 12-second deadlines and editor readiness has
its unchanged 15-second deadline. The outer wrapper sends TERM at 75 seconds
and KILL five seconds later, bounding supervisor and Xvfb shutdown as well.
These are limits, not a promise that prerequisites
or acceptance will finish within them.

Retain `native-project-rename.json`, `native-lifecycle.json`, served response
JSON/logs, before/after/final CAD copies and their hash manifests, native XML and canonical JSON,
each `netlist-*.readcopy-*.manifest.json`, `netlist-*.native-cli-preferences.json`
and any `netlist-*.native-cli-generated.kicad_prl`,
saved-PCB identity JSON, CLI audits/stdout/stderr, optional proxy raw files and
the supervisor's `started.json`/`finished.json`. A pass requires unchanged
source/fixture hashes, successful lifecycle audits and verified absence of the
owned control root. The supervisor must report status 0, `timed_out: false`
and `group_reaped: true`; a helper report alone cannot establish acceptance.
Existing lifecycle rules remain unchanged: no signalling
discovered/shared services, no unmounts, and no manual deletion of a failed
control root. Missing artifacts, timeout, cleanup errors or unverifiable root
absence are failures, not acceptance evidence.

This Linux source-only gate does not establish native macOS/Windows behavior,
cross-SDK availability, UNC behavior, filesystem-general atomicity, external-save
race exclusion, native PCB project-token rewrites or full transaction semantics.
