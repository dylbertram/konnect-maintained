# Project rename safety

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
