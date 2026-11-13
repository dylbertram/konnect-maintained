# Cleanup execution

## Steps

- [x] Complete wave 1: small documentation fixes, stable read identities and board stale-write protection.
- [x] Review wave 1, verify failing-before/passing-after regressions, and integrate every accepted child into `cleanup/main`.
- [ ] Complete wave 2: export correctness and bounded schematic connectivity/preservation fixes.
- [ ] Correct the footprint-field foundation, then complete only preservation-safe propagation.
- [ ] Reconcile remaining fix packets against current source and available native/platform evidence.
- [ ] Run final integration checks, reconcile Kata evidence, and remove completed child branches/worktrees.

## Branch and ownership contract

`cleanup/main` is the integration parent. Git cannot simultaneously store a
branch named `cleanup` and branches named `cleanup/...`.

Children use `cleanup/fix/<scope>` or `cleanup/docs/<scope>` and start from the
current parent. Keep at most three active implementation children. Each has
one bounded scope and its own worktree. Do not modify the parent's files from
a child, overwrite another agent's work, or adopt the active hierarchy repair.

The coordinator reviews and merges each accepted child into `cleanup/main`.
After merge and a clean-worktree/ancestry check, remove that child's worktree
and delete its fully merged branch. Do not delete unmerged work to make the
branch list look tidy. Do not push temporary branches unless specifically needed.
The integration parent remains until its final disposition is authorized.

## Wave 1

1. Documentation: existing Kata `mvy5/7enh` and `t2ms/g10n`.
   Verify the served batch-delete description and current Muse discovery guidance.
2. Read identities: `t7yx/9ane` and `h1aj/qdmh`.
   Return correct mirror state and label UUIDs through served tools. Preserve
   existing response compatibility and cover the target identities.
3. Guarded board writes: `7bgg/djvc`.
   Reproduce stale overwrites deterministically, protect all affected fallback
   writers, preserve open-board authority and report persistence truthfully.

Agents first read current Kata scope and maintained code. If an item is already
fixed or cannot be verified, report that on the existing record rather than
inventing a patch or opening a successor ticket.

## Subsequent order

The complete inventory and packet boundaries are in `FIX_BATCHES.md`.

- Wave 2 starts with A13 exports, A3 silent-net-join protection and A5 native
  serialization preservation, choosing disjoint callers and reviewing overlap
  before dispatch.
- Junction changes follow the shared connectivity decision. They must not add
  joins at untouched intersections.
- In the footprint path, establish A8 preservation/refusal, fix A6's known raw
  angle defect, then implement A7 additions and safe existing-footprint updates.
  Do not accept the historical cumulative PRs as-is.
- Reconcile merged hierarchy work before residual A11 implementation.
- Reproduction-only, platform-dependent and deferred-capability work remains
  gated on its actual environment, scope and evidence.

## Verification and closure

The coordinator owns remote execution admission. Agents must not compete for
Boat workers or run tests locally. Load `byskap-builds`; measure host/target
headroom, keep the 500 MB reserve, and use one coordinator-owned job at a time
unless capacity is explicitly reconciled. Other sessions' jobs are not ours.

The user permits more than two Boat instances. Authenticated `boat limits --all`
confirmed 100 concurrent sandboxes and start limits of 200/day, 60/hour and
12/minute. The two-job cap belongs to the existing BySkap executor. Additional
coordinator-owned CLI workers may be used with the same sanitized-source,
no-secret-environment, resource-budget, bounded-lifetime and cleanup protections.
Do not touch another session's workers or copy the executor's private Boat key.
Recheck live limits before provisioning and preserve daily/hourly/minute start
reserves; the 200/day limit is not a concurrency limit or permission for overages.

Use native KiCad-written fixtures where behavior depends on serialization.
Require deterministic failing-before/passing-after evidence, served dispatch,
committed readback, no-write refusals and appropriate negative controls.
After integrating children, rerun relevant checks on the combined parent tree.

Record exact commits, commands, remote job IDs and limitations. Close existing
Kata reports only when their complete acceptance is met. Partial increments stay
open with an accurate remainder. No live design mutations, deployment, Actions
activation, branch protection, release or publication are authorized by this plan.

## Integration record

- Prerequisite PR #11 was already merged into remote `main` at `af8d4c4`.
  Imported it into `cleanup/main` at `c3d357f` before accepting any cleanup child.
  Retained editor shutdown and reconciled the combined catalogue counts.
- Documentation child `6f77284` passed independent review and Boat job
  `88cc69f6-ccd5-4814-b38f-d1c037ed2707`: four positive regressions passed,
  selector/Muse negative controls failed as expected, and disabled eager loading
  was caught. Merged into the parent at `16bb336`; its clean fully merged branch
  and worktree were removed. Evidence was preserved under the parent.
- Board guards passed job `4f0c78f6-1b93-4180-abcb-799f88b00157`, including
  missing-guard negative control, writer tests and Clippy. Merged at `4bb3d66`.
- Read identities passed job `3f040888-6cca-467b-9949-91b797f71b16`, including
  failing-before and mutation controls. Merged at `b1c0b3c`.
- Reusable no-secret Boat image `konnect-rust-1-96` is ready at snapshot
  `2654d199-a5cb-4aa6-b7f4-6204d76085d9`. Rust, Cargo and rustdoc report 1.96.0;
  an actual rustdoc smoke test passed. The source-free image builder sandbox was
  deleted, with completed operation `bdop_f23069362a4d491e816546b21b4dacd8`.
  Existing executor images are not claimed changed by saving this snapshot.
- Combined gate `275343da-a26a-4628-8743-88ae2c4d271d` passed on `b1c0b3c`:
  workspace tests, Clippy, doctests and Python suites. Nine existing Kata records
  are closed with typed evidence. Historical PR-review record `7enh` remains open
  for a superseded disposition; its optional runtime hint was not adopted, and
  Kata rejected the superseded-close evidence arguments. The original report
  `mvy5` is complete.
- Exports `32bd120` passed independent review, native KiCad/Poppler artifacts,
  lifecycle checks and dispatch/default-layer controls in job
  `9caeddf2-5a53-4380-9211-b467ae8be17b`. Merged at `410675b`.
  Export-inclusive parent `ecc6e42` passed the full combined gate on
  `bx_5k54nmcv`, stages 1 and 2.
- Private fields `cab73ea` passed four model tests, eight served tests,
  native KiCad 10.0.8 resave/readback and five rejected mutation controls
  with restored positives on `bx_e7n7qrw3`. Merged at `ac4c4823`.
  Public exhaustive Rust `Property` literals/destructuring need migration;
  constructors and tool schemas retain compatibility. See `API_MIGRATIONS.md`.
- Private-inclusive parent `ac4c4823` passed formatting, workspace tests,
  Clippy with warnings denied, doctests, 18 plugin tests, nine script tests,
  native supervisor/export acceptance and native private resave/readback.
  Boat command `cmd_i6nxiw2fjkpi` exited 0 on `bx_5k54nmcv`; source archive
  SHA-256 is `dd31946dcf39a0f869f270327ed203d7d2054feef2055707d280d6f3d8c7cc93`.
  Full logs and native artifacts are retained under
  `.remote-build-jobs/boat-parent-verification/`.
  Fourteen Kata records are now closed with typed evidence, including
  exports `pe7h/459k/xsd6` and private fields `27jb/we5v`.
  The `mvy5` closure's plugin count was corrected to 18 in a comment.
- Every accepted child above has been merged; its clean branch/worktree was
  removed. Failed and successful remote evidence is retained in the parent.
- Route candidate `54f3afe` passed focused suites but its generated native
  schematic was rejected in job `2fe4fbfa-2b08-4202-8286-22c3bab65de2`.
  Correction `ba53ecb` adds complete native-instance cloning, independent
  process supervision and captured inputs/outputs. Candidate `9f5d69ad`
  incorporates parent `ac4c4823`; verification is running on `bx_fv2c2jjy`,
  command `cmd_ramcfajlbuhk`. Native acceptance and controls remain pending.
- Active implementation children are routes and the library-field foundation.
  Foundation `95e9d23` is not accepted. Review requires absent/default-zero
  thickness normalization and a genuine native upright-operation oracle.
  Preserve its uncommitted corrections and `origin/fix/789-library-fields`
  at `63123625` until the replacement is verified and merged.
- All four retained Dependabot branches were audited against the parent.
  None is incorporated or demonstrably superseded; preserve them. Findings
  are recorded on existing Kata `f0kv`.
- Private worker deletion completed at
  `bdop_937517fce2e74c88b8a8e135b51391ab`; parent worker deletion completed
  at `bdop_a67a53a9da904598aa535402d17f612c`, after evidence retrieval.
- The failed CLI test worker deletion completed at
  `bdop_6ce1b8d27fe0448fa6b0d189811b6aa7`; its local transfer copy was removed.
