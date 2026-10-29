#!/usr/bin/env python3
"""Read an archived GitHub backlog through gh; never modifies GitHub state."""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess


def github(path):
    result = subprocess.run(
        ["gh", "api", path], capture_output=True, text=True, check=True
    )
    return json.loads(result.stdout)


def pages(path):
    records = []
    for page in range(1, 10000):
        separator = "&" if "?" in path else "?"
        batch = github(f"{path}{separator}per_page=100&page={page}")
        records.extend(batch)
        if len(batch) < 100:
            return records
    raise RuntimeError(f"Pagination did not finish: {path}")


def collect(item, repo):
    number = item["number"]
    result = {"issue": item, "comments": [], "reviews": [], "review_comments": []}
    if item["comments"]:
        result["comments"] = pages(f"repos/{repo}/issues/{number}/comments")
    if "pull_request" in item:
        result["pull"] = github(f"repos/{repo}/pulls/{number}")
        result["reviews"] = pages(f"repos/{repo}/pulls/{number}/reviews")
        result["review_comments"] = pages(f"repos/{repo}/pulls/{number}/comments")
        result["files"] = pages(f"repos/{repo}/pulls/{number}/files")
        result["commits"] = pages(f"repos/{repo}/pulls/{number}/commits")
    if len(result["comments"]) != item["comments"]:
        raise RuntimeError(f"Comment coverage changed for #{number}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="mixelpixx/Konnect")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--workers", type=int, default=4, choices=range(1, 5))
    args = parser.parse_args()
    repo = github(f"repos/{args.repo}")
    if not repo["archived"]:
        raise RuntimeError("Expected a read-only archived repository")
    items = pages(f"repos/{args.repo}/issues?state=open&sort=created&direction=asc")
    expected = repo["open_issues_count"]
    if len(items) != expected:
        raise RuntimeError(f"Expected {expected} open issues/PRs, found {len(items)}")
    records = []
    # A failed request aborts the snapshot; no partial snapshot is called complete.
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        jobs = [pool.submit(collect, item, args.repo) for item in items]
        for index, job in enumerate(as_completed(jobs), 1):
            records.append(job.result())
            if index % 25 == 0 or index == len(items):
                print(f"Collected {index}/{len(items)}", flush=True)
    records.sort(key=lambda record: record["issue"]["number"])
    snapshot = {
        "repository": args.repo,
        "repository_url": repo["html_url"],
        "archived": repo["archived"],
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "open_items_expected": expected,
        "issues": sum("pull" not in record for record in records),
        "pull_requests": sum("pull" in record for record in records),
        "coverage": "all open issues and PRs, issue comments, PR reviews, inline comments, changed files, and commits",
        "items": records,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    staged = args.output.with_suffix(args.output.suffix + ".partial")
    staged.write_text(json.dumps(snapshot, indent=2) + "\n")
    staged.replace(args.output)
    print(f"Saved {len(records)} items to {args.output}", flush=True)


if __name__ == "__main__":
    main()
