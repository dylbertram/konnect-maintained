# First stabilization audit

The source snapshot covers every open item in the archived repository on
2026-10-27: 201 issues and 63 PRs. Kata contains all 264 items and 296 discussion
entries. The import was read back and deduplication checked. Open historical
items remain open; neither an upstream approval nor an imported implementation
is a local acceptance decision.

## Implemented candidates

| Report | Local draft | Evidence | Still required |
|---|---|---|---|
| Upstream #950, zero-length net stubs | [PR #4](https://github.com/dylbertram/konnect-maintained/pull/4), `b4a60f0` | Native-fixture direct and served tests reproduce the old wire/refusal failures, then pass with the fix. Local formatting, workspace Clippy, unit/integration tests, and doctests pass. | Review, hosted validation, relevant live-KiCad observation. |
| Upstream #834, headless Freerouting launches | [PR #5](https://github.com/dylbertram/konnect-maintained/pull/5), `5786c7e` | Both launch-command regressions fail before the option is added and pass afterward. Formatting, Clippy, and doctests pass. | Workspace test gate is not green because of baseline #711. Real routing/Dock comparison, review, and hosted validation remain pending. |

Both implementations start from maintained `main`; neither depends on the
hierarchy/UUID drafts. Neither has been merged, released, or installed. No real
design files were edited. Existing automation remains disabled.

## Findings that changed the shortlist

- **#870 is not established as fixed.** Current source already falls back for
  an empty IPC address and recognizes `ipc_socket_path`. PR #872 primarily adds
  whitespace normalization. The original endpoint/config-selection failure needs
  reproduction; it was removed from the immediate batch.
- **#907 / PR #949 is not an easy acceptance.** The archived review reports a
  duplicate-name deletion that returns success while a matching symbol remains,
  and incomplete refusal/readback evidence. Review those findings before reuse.
- **#711 is a reproduced baseline test blocker.** The IPC source/test tree is
  identical on maintained `main` and the #834 branch, with tree SHA
  `b3009fce8293c419ff53f5194fbc31768b3caab2`. The #834 workspace run failed at
  `unix::a_listener_that_closes_without_negotiating_reports_a_failed_handshake`.
  A fixed ten-trial diagnostic of the unchanged test produced eight passes and
  two failures. ENOTCONN was reported as `TransportError` instead of
  `HandshakeFailed`. This was not retried to relabel the workspace gate green.
  Archived PR #911 is a candidate to review in the next bounded batch.

## Next bounded work

1. Review the #711 diagnostic and PR #911; establish a deterministic regression
   before adopting any IPC classification change.
2. Audit DevOps and release destinations before enabling Actions. Do not reuse
   inherited scheduled issue-closing policy without an explicit decision.
3. Review the two local fix drafts and the existing hierarchy/UUID dependency
   chain. Keep untested larger features out of the first stabilization release.
4. After these gates, scope the next small-fix batch. Documentation reports #922
   and #954 remain candidates, not completed work.

The catalog queues are a preliminary risk/effort screen. `engineering_review_complete`
remains false for imported items until a full acceptance review is recorded.
