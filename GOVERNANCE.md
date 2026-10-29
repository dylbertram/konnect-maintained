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

Actions remain disabled while this setup PR is prepared. This document describes
the intended policy, not an already-enforced GitHub setting.

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
