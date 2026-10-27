#!/usr/bin/env python3
"""Generate an explicitly preliminary catalog and Kata triage metadata."""

import argparse
from collections import Counter
import gzip
import hashlib
import json
from pathlib import Path
import re


SMALL = {
    834: "Two Java launch sites omit the headless JVM option; inspect command ordering and cover both sites.",
    870: "Blank configured IPC address blocks fallback; inspect configuration precedence and test without live IPC.",
    922: "Client-specific tool discovery workaround is documentation-only; verify against current configuration schema.",
    950: "Single-tool zero-length stub differs from batch semantics; reproduce no-wire and refusal behavior through dispatch.",
    954: "Description should name the separate uuids/references arguments; preserve argument semantics.",
}


def triage(record):
    issue = record["issue"]
    number = issue["number"]
    labels = {label["name"] for label in issue["labels"]}
    title = issue["title"].lower()
    if number in SMALL:
        return "small-fix-candidate", SMALL[number]
    if number in {907, 949}:
        return "review-findings", "Archived review identifies incomplete ambiguity/refusal/readback coverage; do not treat a small diff as a safe fix."
    if number in {913, 917}:
        return "dependency-review", "Already preserved as local draft PRs #1 and #2; combined-tree local tests exist, but fresh prerequisite review is pending."
    if "pull" in record:
        if record["pull"]["base"]["ref"] != "main" or re.search(r"stacked on\s+#\d+", issue["body"] or "", re.I):
            return "dependency-review", "PR declares a non-main base or stacked prerequisite; inspect unique commits and rebuild the dependency chain."
        if any(review["state"] == "CHANGES_REQUESTED" for review in record["reviews"]):
            return "review-findings", "Archived review includes requested changes; determine whether the current PR head resolves them."
        return "pr-review", "Existing implementation candidate; inspect provenance, dependencies, current-head diff, and independent test evidence before adoption."
    if number == 947:
        return "historical-context", "Upstream commercial migration/contributor sign-off is historical context, not a stabilization fix."
    if "tracker" in labels:
        return "tracker", "Coordinate child issues; an umbrella tracker is not a standalone implementation task."
    if "wontfix" in labels or "deprecated" in labels:
        return "historical-decision-review", "Preserve upstream disposition without inheriting its scheduled closure or automatically rejecting the request."
    if "status:needs-information" in labels:
        return "needs-reproduction", "Upstream requested more information; establish a reproducible case on the maintained baseline."
    if "documentation" in labels or title.startswith("docs:"):
        return "documentation-review", "Check inherited documentation against the maintained repository, supported clients, and runtime behavior."
    if "enhancement" in labels or any(word in title for word in ("feature request", "proposal:", "no tool ", "no way to ", "feat(")):
        return "design-backlog", "Capability request, not automatically in the first stabilization release; define a bounded design before implementation."
    if any(word in title for word in ("overwrit", "drops", "drop ", "stale", "short", "revision check", "corrupt", "wrong net", "reverts")):
        return "safety-review", "Title reports possible state loss, identity disagreement, or connectivity risk. This is a risk screen, not a verified diagnosis."
    return "needs-reproduction", "Confirm the reported behavior against the maintained baseline and identify the smallest regression test."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("receipts", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    raw = args.snapshot.read_bytes()
    snapshot = json.loads(raw)
    manifest = json.loads((args.receipts / "manifest.json").read_text())
    receipts = [json.loads((args.receipts / f"result-{i}.json").read_text())["result"]
                for i in range(manifest["batches"])]
    if any(receipt["errors"] for receipt in receipts):
        raise RuntimeError("Kata import reported item errors")
    identities = {item["external_id"]: item["issue_short_id"]
                  for receipt in receipts for item in receipt["items"]}
    readback = json.loads((args.receipts / "result-100.json").read_text())["issues"]
    by_ref = {issue["ref"]: issue for issue in readback}
    if len(identities) != len(snapshot["items"]) or set(identities.values()) != set(by_ref):
        raise RuntimeError("Import receipts and Kata readback do not match the GitHub snapshot")
    catalog = []
    for record in snapshot["items"]:
        issue = record["issue"]
        stage, reason = triage(record)
        references = sorted({int(n) for n in re.findall(r"(?<![\w/])#(\d+)", (issue["body"] or ""))
                             if int(n) != issue["number"]})
        ref = identities[issue["html_url"]]
        kata = by_ref[ref]
        catalog.append({
            "number": issue["number"], "kind": "pr" if "pull" in record else "issue",
            "title": issue["title"], "source_url": issue["html_url"],
            "author": (issue["user"] or {}).get("login"),
            "upstream_labels": [label["name"] for label in issue["labels"]],
            "kata_ref": kata["qualified_ref"], "kata_url": kata.get("web_url"),
            "stage": stage, "reason": reason, "engineering_review_complete": False,
            "referenced_upstream_numbers": references,
            "pr_head_sha": record.get("pull", {}).get("head", {}).get("sha"),
            "pr_changed_files": record.get("pull", {}).get("changed_files"),
            "comments_imported": len(record["comments"]) + len(record["reviews"]) + len(record["review_comments"]),
        })
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / "upstream-open-2026-10-27.json.gz"
    archive.write_bytes(gzip.compress(raw, mtime=0))
    result = {"source": snapshot["repository_url"], "captured_at": snapshot["captured_at"],
              "raw_snapshot_sha256": hashlib.sha256(raw).hexdigest(),
              "issues": snapshot["issues"], "pull_requests": snapshot["pull_requests"],
              "comments_imported": manifest["comments"],
              "triage_is_preliminary": True, "stage_counts": dict(Counter(item["stage"] for item in catalog)),
              "items": catalog}
    (args.output / "catalog.json").write_text(json.dumps(result, indent=2) + "\n")
    updates = [{"ref": item["kata_ref"], "metadata": {
        "upstream_repository": snapshot["repository"], "upstream_number": item["number"],
        "upstream_url": item["source_url"], "triage_stage": item["stage"],
        "triage_basis": item["reason"], "engineering_review_complete": False,
        "snapshot_captured_at": snapshot["captured_at"],
    }} for item in catalog]
    # Reuse the bounded local import transport for tool calls, not direct DB writes.
    for i in range(0, len(updates), 10):
        (args.receipts / f"batch-{100 + i // 10}.json").write_text(json.dumps(updates[i:i + 10]))
    rows = ["# Archived backlog catalog", "",
            f"Captured {snapshot['captured_at']} from {snapshot['repository_url']}.", "",
            f"{snapshot['issues']} open issues, {snapshot['pull_requests']} open PRs; {manifest['comments']} discussion/review entries imported into Kata.", "",
            "Every item has a preliminary queue and rationale. This is not a claim that every bug has been reproduced or every PR reviewed. Upstream assignments and approvals are not inherited.", "",
            "See `catalog.json` for per-item rationale, references, author, original labels, Kata identity, and PR head SHA. The compressed JSON snapshot preserves full bodies, discussions, PR reviews, files/diffs, and commit metadata.", "",
            "## Preliminary queues", "", "| Queue | Items |", "|---|---:|"]
    rows.extend(f"| {stage} | {count} |" for stage, count in sorted(result["stage_counts"].items()))
    rows.extend(["", "## Complete list", "", "| Upstream | Kind | Title | Kata | Preliminary queue |", "|---|---|---|---|---|"])
    for item in catalog:
        kata = f"[{item['kata_ref']}]({item['kata_url']})" if item["kata_url"] else item["kata_ref"]
        title = item["title"].replace("|", "\\|").replace("\n", " ")
        rows.append(f"| [#{item['number']}]({item['source_url']}) | {item['kind']} | {title} | {kata} | {item['stage']} |")
    (args.output / "CATALOG.md").write_text("\n".join(rows) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "items"}, indent=2))


if __name__ == "__main__":
    main()
