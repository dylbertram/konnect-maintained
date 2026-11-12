# Independent maintenance

This repository continues the AGPL-licensed code from the archived
[mixelpixx/Konnect](https://github.com/mixelpixx/Konnect) project. Original credit
belongs to @mixelpixx and the contributors recorded in Git history. Independent
maintenance is managed by @dylbertram.

It is not the commercial Konnect-EDA project and does not import code from that
product. The existing license and copyright notices remain in effect. This
migration does not relicense inherited code to MIT.

## Imported history

- `archive/upstream-main` preserves the archived upstream's final `main` at
  `c7e87cd8a7d440bfd6912025bc4370c7ebc00a00`.
- `archive/fork-main` preserves the previous fork's `main` at
  `9488e5f09717cb508cb25779dc16586d1f06f758`.
- `main` starts from the archived upstream snapshot, with independent-maintenance
  documentation added. No application code changed during the migration.
- Historical version tags and the two archive branches are preserved. Completed
  working branches are retired after their changes are reconciled. Imported
  tags identify upstream history, not newly built or independently validated
  releases. GitHub release assets have not been copied or republished.
- The sheet-instance lifecycle prerequisite and project-root UUID fix landed in
  maintained [PR #1](https://github.com/dylbertram/konnect-maintained/pull/1) and
  [PR #2](https://github.com/dylbertram/konnect-maintained/pull/2). Their temporary
  maintained branches have been retired.

The original fork remains available at
[dylbertram/Konnect](https://github.com/dylbertram/Konnect). Its issues, PRs,
comments, and review state do not migrate with Git branches.

## Stabilization and release gates

GitHub Actions remain disabled. The maintained CI is read-only, E2E is manual,
and inherited release and scheduled issue-closing workflows have been removed.
Hosted checks and branch protection still need deliberate activation and
verification. Pushing imported tags must not publish replacement releases.

The archived backlog is inventoried in [the dated catalog](docs/stabilization/CATALOG.md).
Current issue status lives in Kata, not the import snapshot. Preserve source links,
author attribution, and dependency order when adopting work. An upstream PR is
evidence to review, not an automatically accepted change. Do not wait for merges
in the archived repository.

The maintained contributor and governance policies landed in
[PR #8](https://github.com/dylbertram/konnect-maintained/pull/8). Original upstream
roles, infrastructure, and release destinations remain historical context, not
a claim that those services operate for this repository.

The first independent release follows scoped stabilization, CI validation, and a
non-publishing package test. Choose its version and supported platforms before
tagging. No release is created as part of this migration.
