#!/usr/bin/env python3
"""Normalize a complete read-only GitHub snapshot for idempotent Kata import."""

import argparse
import json
from pathlib import Path
import re


def author(value):
    return (value or {}).get("login", "deleted-github-user")


def normalize(record, captured_at):
    issue = record["issue"]
    kind = "pr" if "pull" in record else "issue"
    labels = ["import:archived", f"kind:{kind}", "triage:unreviewed"]
    labels.extend("upstream:" + re.sub(r"[^a-z0-9]+", "-", label["name"].lower()).strip("-")
                  for label in issue["labels"])
    body = [
        f"Source: {issue['html_url']}",
        f"Original author: @{author(issue['user'])}",
        f"Original state at capture: {issue['state']}; captured {captured_at}.",
        "Imported from the archived mixelpixx/Konnect repository. Upstream labels, "
        "assignees, reviews, and claimed/ready states are historical evidence, not "
        "local assignments or merge approval. Local triage is pending.",
        "Original assignees: " + (", ".join("@" + a["login"] for a in issue["assignees"]) or "none"),
        "Original labels: " + json.dumps([label["name"] for label in issue["labels"]]),
        "## Original description\n\n" + (issue["body"] or "No description supplied."),
    ]
    if kind == "pr":
        pr = record["pull"]
        body.append("## Archived PR evidence\n\n" + json.dumps({
            "draft": pr["draft"],
            "head": {"repository": (pr["head"]["repo"] or {}).get("full_name"),
                     "ref": pr["head"]["ref"], "sha": pr["head"]["sha"]},
            "base": {"ref": pr["base"]["ref"], "sha": pr["base"]["sha"]},
            "changed_files": pr["changed_files"], "additions": pr["additions"],
            "deletions": pr["deletions"], "commits": pr["commits"],
            "maintainer_can_modify": pr["maintainer_can_modify"],
            "files": [f["filename"] for f in record["files"]],
            "commit_shas": [c["sha"] for c in record["commits"]],
        }, indent=2))
        body.append("Importing this PR does not accept its implementation or establish "
                    "licensing for every attached asset. Review provenance, dependencies, "
                    "tests, and the diff before reuse.")
    comments = []
    for comment in record["comments"]:
        comments.append({
            "author": author(comment["user"]),
            "body": f"Original comment: {comment['html_url']}\n\n{comment['body'] or ''}",
            "created_at": comment["created_at"], "external_id": comment["html_url"],
        })
    for review in record["reviews"]:
        comments.append({
            "author": author(review["user"]),
            "body": f"Archived PR review: {review['state']}\nSource: {review['html_url']}\nCommit: {review['commit_id']}\n\n{review['body'] or ''}",
            "created_at": review["submitted_at"], "external_id": review["html_url"],
        })
    for comment in record["review_comments"]:
        comments.append({
            "author": author(comment["user"]),
            "body": f"Archived inline review: {comment['html_url']}\nFile: {comment['path']}\nCommit: {comment['commit_id']}\n\n{comment['body'] or ''}",
            "created_at": comment["created_at"], "external_id": comment["html_url"],
        })
    comments.sort(key=lambda comment: (comment["created_at"], comment["external_id"]))
    return {
        "author": author(issue["user"]), "body": "\n\n".join(body),
        "comments": comments, "created_at": issue["created_at"],
        "updated_at": issue["updated_at"], "external_id": issue["html_url"],
        "labels": labels, "status": "open",
        "title": f"[upstream {kind} #{issue['number']}] {issue['title']}",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    snapshot = json.loads(args.snapshot.read_text())
    records = snapshot["items"]
    if len(records) != snapshot["open_items_expected"]:
        raise RuntimeError("Incomplete snapshot")
    items = [normalize(record, snapshot["captured_at"]) for record in records]
    if len({item["external_id"] for item in items}) != len(items):
        raise RuntimeError("Duplicate source identities")
    args.output.mkdir(parents=True, exist_ok=True)
    # Keep batches small by both item count and total JSON size.
    batches, current = [], []
    for item in items:
        if current and (len(current) >= 10 or len(json.dumps(current + [item])) > 200000):
            batches.append(current)
            current = []
        current.append(item)
    if current:
        batches.append(current)
    for index, batch in enumerate(batches):
        (args.output / f"batch-{index}.json").write_text(json.dumps({
            "project": "konnect-maintained", "source": "github", "items": batch,
        }))
    manifest = {
        "items": len(items), "batches": len(batches),
        "comments": sum(len(item["comments"]) for item in items),
        "captured_at": snapshot["captured_at"],
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2))
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
