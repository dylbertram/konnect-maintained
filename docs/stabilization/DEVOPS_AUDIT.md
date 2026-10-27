# DevOps audit, 27 October 2026

Repository: [dylbertram/konnect-maintained](https://github.com/dylbertram/konnect-maintained).
Audited baseline: `9a63092`, with the inherited workflow files unchanged.
Machine-readable observations are in `devops-observations-2026-10-27.json`.

**Disposition: keep Actions disabled.** The inherited automation is not yet
ready to operate as this repository's CI/release policy. No settings, workflows,
secrets, permissions, tags, or releases were changed during this audit.

## Verified repository state

| Control | Observation |
|---|---|
| Ownership | @dylbertram has admin/maintain/push access; independent public repository. |
| Actions | Disabled; zero runs. |
| Workflow token | Default read access; workflows cannot approve PR reviews. |
| Repository credentials | No repository Actions secrets or variables configured. Only names/counts were collected; no secret values were accessed. |
| Environments | None, including no protected release environment. |
| Branch/ruleset enforcement | No rulesets; GitHub reports `main` is not protected. |
| CODEOWNERS | GitHub reports nine unknown-owner errors for inherited @mixelpixx/@neusse entries. Do not grant access merely to make historical entries valid. |
| Automatic merging | Disabled. Merge, squash, and rebase methods are all allowed; no maintained-repo choice has been enforced yet. |
| Webhooks/releases | No configured webhooks; no independent releases. |
| Secret scanning | Secret scanning and push protection enabled. |

The empty inventories above were successful API reads. The `main` protection
endpoint's explicit HTTP 404, "Branch not protected", is evidence of absence,
not a missing-permission result. Future inventory API failures must remain
`unavailable`; they must not be converted to empty/healthy results.

## Findings and remediation order

### P1: isolate CI from inherited mutations before enabling Actions

Evidence:

- `.github/workflows/wontfix-review-window.yml:3-12, 103-108` runs daily and on
  issue labels/reopens, obtains `issues: write`, and closes `wontfix` issues after
  30 days. It targets the current repository via `context.repo`, not the archive.
- `.github/workflows/e2e-kicad.yml:15-21` schedules Windows E2E weekly.
- `.github/workflows/ci.yml` has no explicit workflow-level permissions. It is
  currently constrained by the verified repository read-default policy, but
  should declare its own least privilege rather than depend on that default.
- Twelve unique external action refs occur 41 times. All refs resolved on GitHub;
  nine unique refs are mutable tags/branches, not commit pins. This is a
  reproducibility/supply-chain hardening gap, not evidence of compromise.
- The only workflow secret expression found is the built-in `GITHUB_TOKEN` in
  the release job. No PAT, cloud deployment credential, `pull_request_target`,
  `workflow_run` execution chain, or self-hosted runner is declared in these four
  workflow files.

Required before enabling: make scheduled/mutating jobs explicitly opt-in or
remove them through an intentional policy change; declare read-only CI tokens;
review and pin external actions to actual commit SHAs. Annotated tag object
SHAs in the inventory must be peeled to commits before use as pins.

Start with a bounded, non-publishing CI run. Agree runner/platform scope,
concurrency, and artifact retention before restoring the full matrix. Declared
CI timeout ceilings total 240 runner-slot minutes across ten matrix-expanded
jobs; the release graph totals 495 across nine jobs. These are upper bounds from
workflow timeouts, not measured runtimes or billing estimates. Existing CI
concurrency cancellation is useful and should be retained.

### P1: establish owned review and branch protection

Evidence: the GitHub API reports no rulesets and an unprotected `main`; the
CODEOWNERS API reports nine invalid historical-owner entries. `.github/CODEOWNERS`
and `GOVERNANCE.md` still name the upstream maintainers and their merge policy.

Required: replace review routing with actual maintained-repo owners while
retaining author credit elsewhere; define supported merge methods and required
checks. Enable meaningful protection only after the new CI has produced the
exact check names. Do not invent approval requirements that a sole maintainer
cannot satisfy or grant upstream maintainers access without authorization.

### P1: gate publication on validated maintained commits

Evidence:

- `.github/workflows/release.yml:3-15, 256-276` accepts every pushed `v*` tag.
  Publication needs the real-KiCad, binary-build, and PCM jobs, but does not
  require ordinary CI's unit/integration, Clippy, format, license, or Nix jobs.
- No release environment or environment approval is configured. No workflow
  check establishes that the tag is an approved maintained-`main` commit or
  matches the workspace version. `v*` is not a semantic-version validator.
- `workflow_dispatch` accepts an arbitrary synthetic version string. Packaging
  scripts use that version in paths, and Windows interpolates it into a script.
  Validate the version before packaging; this is maintainer-input hardening,
  not a demonstrated public-PR privilege escalation.
- No release concurrency policy, published checksum manifest, artifact
  attestation step, or explicit signing/notarization step is declared.

Required: retain the existing non-publishing dry run and mandatory real-KiCad
gate, then add maintained-repo approval, exact-commit CI/version checks, bounded
concurrency, checksums, and an explicit signing/provenance decision for the chosen
platforms. Changes must update the executable workflow contracts in
`crates/konnect/tests/release_workflow_contract.rs`, not silently weaken them.

The publisher currently uses `softprops/action-gh-release`'s current-repository
default. It would publish to **dylbertram/konnect-maintained**, not to the archive.
Enabling Actions alone does not republish old tags, but a later matching tag push
would enter this pipeline. No tag was pushed during this audit.

### P1: give maintained packages a deliberate identity and source route

Evidence:

- `packaging/metadata.json:6-16` and `plugin/plugin.json:2` retain
  `com.github.mixelpixx.konnect` and the upstream homepage/author metadata.
- `packaging/build-pcm.sh:176` and `packaging/build-pcm.ps1:107` print download
  URLs under the archived repository. Those URLs are submission guidance, not
  the actual GitHub publishing target.
- The standalone archives contain the binary alone; PCM staging copies plugin
  files/icons/binaries but does not include the root license text or maintained
  source instructions. The validator checks PCM structure/platforms, not a
  complete license/source-distribution policy.

Required before distribution: choose coexistence versus an explicit upgrade
path for the original plugin identity, point package/support/download routes to
the maintained repository, and bundle appropriate license/attribution/source
information. Preserve the original authors and AGPL-3.0-only license. Do not
claim the new packages are upstream releases or that the audit is a legal
compliance opinion. Check installed paths and user settings as part of any
identity migration; do not overwrite the working installation silently.

No automatic application updater was found in the inspected server installation
code. These observed stale routes are packaging/support issues, not evidence of
an automatic download of the commercial product.

### P2: align validation with supported versions and the new tooling

Evidence:

- `.github/workflows/e2e-kicad.yml:49-53` pins KiCad **10.0.5** on Windows. Recent
  identity/oracle reports concern 10.0.6/10.0.7, and the local environment is
  macOS/KiCad 10.0.7. Windows CLI E2E is not macOS GUI/IPC acceptance.
- Existing CI tests Rust workspace targets, the separate viewer, Python plugin,
  Nix package, licenses, formatting, and PCM structure. It does not yet run the
  new backlog/audit script tests from the pending catalog work.
- The Rust toolchain and Cargo locks are pinned. PCM schema validation uses a
  vendored schema, which is a useful reproducibility control. The Python
  `jsonschema` installation is unversioned in CI/release jobs.
- The inherited dependency-license comment assumes dual commercial licensing;
  this repository remains AGPL. Keep the license gate, but review its policy and
  wording deliberately rather than loosening it as part of a dependency bump.

Required: define the initial supported version/platform matrix; deliberately
update the KiCad pin and add targeted macOS evidence. Include the new Python
tests when adopting the catalog/audit tooling; make Python build dependencies
reproducible. Review the separate macOS signing report, upstream #131, before
claiming an independently distributable Mac package is ready.

## IPC blocker result

The narrow fix from archived upstream PR #911 is preserved in local
[PR #6](https://github.com/dylbertram/konnect-maintained/pull/6), with its original
author and cherry-pick attribution. Deterministic controls catch both disabling
the ENOTCONN mapping and overmatching other errnos. With the fix restored, all
four local workspace gates pass, and a fixed 100-trial macOS Unix-socket check
passes 100/100. No test was weakened or retried until green.

The headless-launch draft was also evaluated with this fix in an isolated
combined tree. All four local workspace gates pass on that combined tree too.
Exact source commits, tree identity, gate exits, and control results are recorded
in `ipc-711-validation.json`. The earlier failed #5 run remains in the record;
it was not rewritten as a passing run.

These are validated **candidates**, not changes to `main`. Supported-platform
hosted CI and review remain pending. Neither draft branch was rewritten or merged
to produce this combined-tree evidence.

## Reproduce the inventory

```sh
python3 scripts/audit_repository_devops.py \
  --repo dylbertram/konnect-maintained \
  --output devops-observations.json
python3 -m unittest discover -s scripts/tests -p 'test_devops_audit.py' -v
```

The script makes read-only GitHub API calls, validates action-ref existence,
and retains settings and credential names/counts. It cannot prove that a hosted
workflow builds, that a signing identity works, or that a release is acceptable.
No hosted runs, workflow activation, setting changes, deployments, or release
publication were performed as part of this audit.

## Tracked remediation

The audit is recorded in
[Kata #amh7](https://kata.bertramdesigns.com/kata?issue=01M5Z93BHES1N4QYABR7A0AMH7#direct=1).
These tasks are unimplemented follow-up work, not completed controls.

| Priority | Task | Kata |
|---|---|---|
| P1 | Opt-in, read-only, pinned and bounded CI | [#zktt](https://kata.bertramdesigns.com/kata?issue=01M5Z9S8H50F3FG7GBJ45HZKTT#direct=1) |
| P1 | Owned review routing and enforced merge checks | [#yvt7](https://kata.bertramdesigns.com/kata?issue=01M5Z9S8MNQE8SJBXSY7VNYVT7#direct=1) |
| P1 | Approved-commit/version/publication gates | [#pcvp](https://kata.bertramdesigns.com/kata?issue=01M5Z9S8R31VWATE2SRDDWPCVP#direct=1) |
| P1 | Independent package identity and source routes | [#244j](https://kata.bertramdesigns.com/kata?issue=01M5Z9S8VQFZXRNZ7ZN34J244J#direct=1) |
| P2 | Supported-version and tooling validation coverage | [#teav](https://kata.bertramdesigns.com/kata?issue=01M5Z9S8ZS8T46PZKG4GQ6TEAV#direct=1) |
