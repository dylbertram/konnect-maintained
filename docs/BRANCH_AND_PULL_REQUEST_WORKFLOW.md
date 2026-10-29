# Branch and pull request workflow

The maintained repository is `dylbertram/konnect-maintained`. Use its current
`main` as your development base, not an archived upstream branch or release tag.
Remote names vary; the example assumes `origin` points to the maintained repository.
For a fork, fetch maintained `main` from your corresponding upstream remote.

```sh
git fetch origin
git switch -c fix/example origin/main
# Edit, test, inspect the diff, and commit.
git push -u origin fix/example
```

Open one PR against maintained `main` for each independent change. Include the
problem, scope, dependencies, tests run, unavailable checks, and risk/rollback notes.

## Review requests and completion

@dylbertram is the sole maintainer. CODEOWNERS routes notifications; it does not
require another person's approval. There is no mandatory AI-tool pairing,
ten-check queue, inherited author authorization, or automatic merge loop.

Review the current diff and resolve substantive findings. For your own PR, record
the review assessment in a comment rather than trying to approve it yourself.
An agent may prepare a PR but may merge only with explicit maintainer authorization.

The initial CI job is `Core checks`. Once CI has been activated and its check name
verified, it is intended to be the single required CI gate. Until then, Actions
and branch-protection activation remain pending as described in
[GOVERNANCE.md](../GOVERNANCE.md#activation-and-branch-protection).
Report local checks honestly; an unavailable hosted run is not a green hosted run.

## Dependencies and changed branches

Keep independent work separate. If B depends on A, state that order and keep B
draft until A lands. Then update B onto maintained `main` and inspect its unique
diff. Do not treat tests on an obsolete combined tree as current evidence.

Resolve conflicts before merging. Any substantive head or base change needs
updated review and relevant tests. Rewriting a published branch requires care
and `--force-with-lease`, never plain `--force`.

## Merging

After review, the maintainer decides whether to merge. Use a merge commit to
retain individual commit attribution, especially for adopted upstream fixes.
Do not enable auto-merge or continue to other PRs without authorization.
Confirm the resulting commit and close only issues whose acceptance criteria
are supported by evidence. No labels automatically close issues or authorize merges.

## Releases

Releases are separate from ordinary merges. The inherited release workflow is
archived outside `.github/workflows`; tags do not authorize publication. A future
release procedure requires its own review before any distribution or activation.
