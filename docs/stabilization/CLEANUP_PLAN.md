# Cleanup execution

## Steps

- [ ] Complete wave 1: small documentation fixes, stable read identities and board stale-write protection.
- [ ] Review wave 1, verify failing-before/passing-after regressions, and integrate every accepted child into `cleanup/main`.
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
  Retained the independently integrated editor-shutdown feature and reconciled
  documentation counts. Combined-tree verification remains pending.
- Documentation child `6f77284` passed independent review and Boat job
  `88cc69f6-ccd5-4814-b38f-d1c037ed2707`: four positive regressions passed,
  selector/Muse negative controls failed as expected, and disabled eager loading
  was caught. Merged into the parent at `16bb336`; its clean fully merged branch
  and worktree were removed. Evidence was preserved under the parent.
- Full combined-parent verification is running on `16bb336`.
- Board guards `cc88c99` and read identities `489a462` await independent review
  and coordinator verification before integration.
- The next bounded export child starts from the updated parent, including PR #11.
