# Maintaining Konnect

This repository, [dylbertram/konnect-maintained](https://github.com/dylbertram/konnect-maintained),
is maintained by [@dylbertram](https://github.com/dylbertram). Upstream authorship
remains in Git history and copyright notices. Upstream maintainers have no
standing review or merge authority here.

## Review and merge

- Use one focused PR per independent change, based on current maintained `main`.
- State the problem, tests run, known gaps, and any dependencies. Review the
  actual diff and resolve substantive findings before merging.
- The maintainer makes the merge decision. No second-person approval,
  code-owner approval, mandatory pair of AI tools, or merge queue is required.
  Agents must obtain explicit merge authorization; creating a PR is not permission
  to merge it or subsequent PRs.
- Use merge commits to retain adopted upstream commits and their authorship.
  Auto-merge is not part of this workflow.
- If the head or base changes, assess the new combined result. Never reuse old
  passing results as proof that changed code passed.
- Issues are closed deliberately, not by a label timer. Kata tracks the
  maintained backlog; archived GitHub labels and approvals are historical context.

See [the short branch workflow](docs/BRANCH_AND_PULL_REQUEST_WORKFLOW.md).

## CI

The initial automatic gate is `Core checks` in `.github/workflows/ci.yml`:
one Linux job running format, Clippy, workspace unit/integration tests,
doctests, Python plugin tests, script tests when present, and dependency-license
checks. It uses a read-only token, pinned external actions, a 45-minute limit,
and cancellation of superseded runs. No ordinary CI artifacts are uploaded.

This deliberately does not certify Windows, macOS, the separate viewer,
or distribution packages. Changes affecting those paths need targeted evidence.
The existing Windows/KiCad 10.0.5 E2E workflow is manual only,
has a 60-minute limit, and retains failure artifacts for seven days. It neither
runs weekly nor responds to PR labels. Its version pin is not a new support claim.

### Activation and branch protection

Ordinary CI and manual KiCad E2E remain individually disabled until their
separate activation review. The manual rename-platform validation described
below has explicit execution authorization. That exception does not authorize
ordinary CI, release automation, branch-protection changes or a rename merge.

After review and explicit authorization:

1. Land the workflow cleanup before enabling Actions. Do not activate the inherited
   workflows from old branches. Confirm no other workflow is enabled for execution.
2. Enable only the reviewed CI initially and manually run it on maintained `main`.
   Observe its result and actual check name. Do not retry a failure until it looks
   green; investigate and record the cause.
3. Require PRs and that successful CI check on `main`, block force-push/deletion,
   and require resolved conversations. Require zero approving reviews and no
   code-owner approval. Do not require the retired ten-check matrix.
4. Keep auto-merge off. Enable manual E2E only when it is needed.

Hosted execution and required-check enforcement are pending until those steps
are completed and verified. Local tests alone do not establish that hosted CI works.

### Manual rename-platform validation

`.github/workflows/rename-platforms.yml` has three explicit native jobs:
Windows x64, Intel macOS and Apple Silicon macOS. No matrix, automatic trigger,
package assembly, release or deployment is permitted. Each dispatch selects a
full reviewed source SHA. Checkout disables persisted credentials. The job
records both the workflow SHA and tested SHA, uses pinned tool/actions versions,
checks available capacity before setup, bounds cargo concurrency and process
lifetime, monitors the 500 MB memory reserve, and retains evidence for seven days.
One shared 50-minute deadline starts before tool setup. Core work reserves five
minutes for UNC, and UNC cannot create a share without at least three minutes
remaining. The 60-minute job limit leaves additional cleanup/upload time.
Apple Silicon uses one build/test worker. Preflight admits a minimum 2 GiB tree
budget plus the unchanged 500 MB reserve and 256 MiB headroom; capacity is
measured again after setup. Capacity refusals retain their measurements.
The platform selector defaults to all three jobs. Explicit Windows-only
investigation skips both Mac jobs and records that narrower scope; it cannot
be cited as a new all-platform pass. Per-platform source evidence stays separate.
Windows requests Visual Studio telemetry opt-out through job-local environment
variables. The observed `vctip.exe` survivor is not allowlisted or deleted;
the empty-owned-job condition still decides whether a command passes.
After the environment-only trial failed, the user separately authorized a
disposable GitHub runner registry-policy trial. Only the 64-bit-view
`HKLM\Software\Policies\Microsoft\VisualStudio\SQM\OptIn` DWORD is set to zero.
Its original state is saved before mutation and restored in an always-run step
before artifact upload, including removal of only newly created empty keys.
A small owned native compiler probe must finish with an empty job before any
full build. Failure stops the gates. No toolchain files are changed. This is
an optional-telemetry policy request, not a claim that all telemetry is disabled.

The first gates build the full core, execute the rename filesystem regressions,
and check the workspace. Windows additionally uses a temporary localhost SMB
share for an explicit UNC test and removes only its owned share in `finally`.
Missing infrastructure or failed cleanup fails that gate; neither is a pass.
These checks do not certify KiCad GUI editing, arbitrary network shares,
schematic/PCB parity or same-session editor recovery.

Before repository-wide Actions is enabled, audit the remote workflow list and
keep all other repository-defined workflows disabled. The existing separately
managed Dependabot service/configuration is explicitly preserved. GitHub can
run that service independently of repository Actions disablement; it is not a
release workflow and this validation does not change dependency-update policy.

GitHub requires the dispatch workflow on the default branch. A workflow/policy-only
bootstrap there and publication of the cleanup-derived candidate validation branch
are authorized. Landing the cleanup parent or rename implementation on `main` is
not. A green run qualifies only its recorded source, runner and tests. Investigate
failures before any further dispatch; do not repeat runs until one is green.

## Proportionate scope review

Fix the demonstrated problem with the smallest coherent change. Do not attach
unrelated refactors or speculative compatibility work to a narrow fix.

### Risk-proportionate validation

- Derive reported results from observed state, not merely the request.
- Test regressions, refusals, and preservation of unrelated design data. Prefer
  fixtures from real KiCad output for file-format changes.
- Record unavailable checks as unavailable, never passed. Missing secondary
  platform evidence is not automatically a blocker for a focused reversible fix.
  Missing evidence of core behavior, data safety, or security is a blocker.
- List dependent changes in order. Review and land prerequisites first; keep
  dependent work draft until its base and evidence are current.

## Releases

Automated publication is disabled. The inherited release workflow has been
removed, not parked as inactive configuration. Pushing a tag must not publish
from the active workflow set.

Before distribution, agree a separate release procedure covering the exact tested
commit, version, package identity and source/license information, affected-platform
acceptance, and signing/provenance needs. No package identity, copyright, license,
or contributor-license terms are changed by this maintenance-policy cleanup.

PR #7 records the October 27 audit. It remains historical evidence, not a mandate
to recreate upstream's infrastructure or proof that its follow-up work is finished.
