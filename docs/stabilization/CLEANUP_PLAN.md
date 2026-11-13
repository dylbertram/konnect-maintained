# Cleanup execution

## Steps

- [x] Complete wave 1: small documentation fixes, stable read identities and board stale-write protection.
- [x] Review wave 1, verify failing-before/passing-after regressions, and integrate every accepted child into `cleanup/main`.
- [x] Complete the selected wave-2 export, route, private-field, text and symbol scopes; other packet remainders stay open.
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

### Remaining bounded safety work

The verified `3bqz/emvb` guard now inventories exact live replacement targets
by KIID in sync and refresh, rechecks apply, refuses whole mixed plans and fails
closed on unknown inventory. Noop and additions-only inventory boundaries are
unchanged. Native controls reproduced owned group/variant loss while the point
and board-level group membership survived.

Use that bounded refusal when assessing A7's wider updates; it is not universal
preservation. Broader A8 metadata, direct-placement presentation and stroke/no-op
work remain separate. Project rename is the active safety packet, with its
cross-platform qualification limits recorded before acceptance.

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
  At that checkpoint, fourteen Kata records were closed with typed evidence, including
  exports `pe7h/459k/xsd6` and private fields `27jb/we5v`.
  The `mvy5` closure's plugin count was corrected to 18 in a comment.
- Every accepted child above has been merged; its clean branch/worktree was
  removed. Failed and successful remote evidence is retained in the parent.
- Route candidate `54f3afe` passed focused suites but its generated native
  schematic was rejected in job `2fe4fbfa-2b08-4202-8286-22c3bab65de2`.
  Correction `ba53ecb` adds complete native-instance cloning, independent
  process supervision and captured inputs/outputs. Candidate `9f5d69ad`
  incorporates parent `ac4c4823`. Its captured label-before failure exposed
  omitted mandatory label angles. Fixture correction `d7f51fc` passed 12
  focused tests, three lifecycle tests and all 34 native netlist exports in
  `cmd_vog7xvmdi9st` on `bx_fv2c2jjy`. Every native audit reports successful
  status and group cleanup. All six controls passed on runner `27a226a` in
  `cmd_gkol9p8a3coc` and `cmd_y5dhfpe3w3ky`, with byte-exact restoration.
  Native guard/local-T controls proved the expected accidental joins from
  captured netlists; restored native checks passed. Merged at `7a01c94`;
  its clean fully merged branch/worktree was removed. Full combined-parent
  verification passed in `cmd_atizatpurxez`; `qe1n/mta5` are closed.
  Route worker deletion completed at `bdop_741706b307634b17bd4eea202e5ee364`.
- Text preservation `174e/pdps` is verified and closed. The user
  explicitly approved a documented public Rust `Text` source break while
  preserving `Text::new` and tool schemas. Native LF fixture capture in
  `cmd_wk7jitojaaxx` passed supervised KiCad 10.0.8 upgrade and netlist export,
  with no newline normalization. Native SHA-256 is
  `1bbd268c32a780f2314fb0a280405be92800086ff2ac1a5cf50b15f5ca266398`.
  Exact unchanged text blocks are verified, not whole-file identity.
  Candidate `f993014` passed source review, formatting, focused tests,
  workspace Clippy and supervised native acceptance in `cmd_f62jiv6ac6xm`.
  Failing-before and six controls passed in `cmd_1vxkborslixt`.
  Merged at `7677915`; the clean merged child was removed.
  Its first full parent gate exited 0, but the worker expired during a user
  choice before full logs/captures were retrieved. Those artifacts are not
  claimed recovered. Fresh verification of the same source
  `49a221f5bd411a9c4fc6555079f61a051134a30391c31485ddcde3871f7c4a33`
  passed in `cmd_vjcwbyscnsv3` and `cmd_oixybansfavq`, with full evidence retained
  under `.remote-build-jobs/boat-retained-parent-verification/`.
- Field foundation `04f5d81` merged at `d9b33fd`, with identical Git tree
  `d62775f731a20033722a368a30c7c213109494dc` and source archive
  `0e439fdc4856363d8091320e6f6fcba4f584bf8c1bb8d692cb5abfd1ef52994f`.
  All 12 controls passed on unchanged production, with nine restored positives.
  Fresh native KiCad 10.0.8 field/lifecycle acceptance passed in
  `cmd_t20f2y8csku2`; combined candidate checks passed in
  `cmd_bfx0jxxpnlh7` and `cmd_bgglg6xp2w6k`. Post-merge
  `cmd_qxtdapc6scme` passed workspace checks and native field/root cleanup.
  Logs, native comparisons and independent root-absence proofs are retained in
  `.remote-build-jobs/boat-field-lifecycle-verification/`.
  Earlier setup/GVFS-cleanup failures remain failed; no shared services were
  killed or unmounted. The clean merged child was removed. `annz` is closed;
  `z705`, A7, broader A8 metadata and stroke/no-op work remain open.
- Independent four-file audit found `origin/fix/789-library-fields` fully
  superseded. After native/parent acceptance, its remote SHA was rechecked at
  `63123625e1e4512cdca3cce1ac5ab4997db3ee4d` and deletion used an exact-SHA
  lease. The subsequent remote query confirmed absence.
- Symbol candidate `938e8d8` passed six model/three served tests, workspace
  Clippy, native acceptance and all 15 controls, including failing-before.
  Merged at `bbf475c`; the clean merged child was removed. Full post-merge
  `cmd_uzwksvy4rxqz` passed workspace/build/Clippy/docs/Python and every accepted
  native gate, including field lifecycle/root cleanup. Evidence is retained in
  `.remote-build-jobs/boat-symbol-verification/`.
  `qpq3/4nj5` are closed. The user-approved raw-syntax/bool policy preserves
  Rust source compatibility; no new public `Symbol` fields were added.
- A8 child `a8b79adc` merged at `ca8632d`. Seven focused tests, all eight
  assertion-pinned controls and native positive/loss/restored gates passed.
  The actual sync control lost U1's owned group and variant; its point and
  board-level membership survived. Same-document raw IPC, zero-mutation
  refusals, safe R1 readbacks, copied-binary hashes and all lifecycle/root
  audits passed independent review. Full child and combined-parent Rust gates
  passed 2,549 tests, Clippy, nine doctests, Python 18/9, counts and builds.
  Combined-parent native supervisor/export/private/route/text/symbol/field/A8
  gates passed, including independent root absence. Evidence and retained
  executables are in `.remote-build-jobs/boat-native-children-verification/`.
  `3bqz/emvb` are closed; the clean, fully merged child branch/worktree was
  removed. Twenty-three original records are now closed with evidence;
  broader A8 scopes remain open.
- Project-rename safety `svj3/xsz0` uses the approved snapshot-guarded,
  truthful partial/uncertain-outcome policy. Its implementation retains
  Windows, macOS and Linux through already-locked `rustix` 1.1.4 and
  `windows-sys` 0.61.2, without unsafe fallback. Independent source review and
  formatting passed at `a4cc8b0`; Linux runtime gates remain pending.
  Cross-target and native macOS/Windows/UNC gates are unverified. Linux checks
  do not qualify their runtime behavior.
- All four retained Dependabot branches were audited against the parent.
  None is incorporated or demonstrably superseded; preserve them. Findings
  are recorded on existing Kata `f0kv`.
- Private worker deletion completed at
  `bdop_937517fce2e74c88b8a8e135b51391ab`; parent worker deletion completed
  at `bdop_a67a53a9da904598aa535402d17f612c`, after evidence retrieval.
- The failed CLI test worker deletion completed at
  `bdop_6ce1b8d27fe0448fa6b0d189811b6aa7`; its local transfer copy was removed.
