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
- Existing fork branches and historical version tags are preserved. Imported
  tags identify upstream history, not newly built or independently validated
  releases. GitHub release assets have not been copied or republished.
- `prerequisite/917-sheet-instance-paths` and `fix/913-project-root-uuid` preserve
  the pending hierarchy prerequisite and locally tested UUID fix. Neither has
  been merged into this repository's `main`.

The original fork remains available at
[dylbertram/Konnect](https://github.com/dylbertram/Konnect). Its issues, PRs,
comments, and review state do not migrate with Git branches.

## Stabilization and release gates

GitHub Actions are disabled during import. Before enabling them, review workflow
permissions, scheduled issue-closing automation, upstream-specific references,
secrets, and all publishing destinations. Pushing imported tags must not publish
replacement releases.

Next, inventory the archived project's open issues and PRs. Preserve source links,
author attribution, and dependency order when carrying actionable work into this
repository. An upstream PR is evidence to review, not an automatically accepted
change. Do not wait for merges in the archived repository.

Review the inherited contributor/governance documents and repository links before
adopting them as this project's maintenance policy. Their original maintainer
roles, queue, infrastructure, and release destinations are historical context,
not a claim that those services operate for this repository.

The first independent release follows scoped stabilization, CI validation, and a
non-publishing package test. Choose its version and supported platforms before
tagging. No release is created as part of this migration.
