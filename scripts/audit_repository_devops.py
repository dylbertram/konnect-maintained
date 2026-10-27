#!/usr/bin/env python3
"""Read-only GitHub/config inventory. Retains secret/variable names, never values.

This collects evidence for human review; it is not a security certification.
No workflow runs, settings changes, or publishing operations are performed.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess


def query(path, projection=None):
    command = ["gh", "api", path]
    if projection:
        command.extend(["--jq", projection])
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode:
        return {"status": "unavailable", "error": result.stderr.strip()}
    return json.loads(result.stdout)


def action_reference(use):
    repository, ref = use.rsplit("@", 1)
    pinned = bool(re.fullmatch(r"[0-9a-f]{40}", ref))
    paths = [f"repos/{repository}/git/commits/{ref}"] if pinned else [
        f"repos/{repository}/git/ref/tags/{ref}",
        f"repos/{repository}/git/ref/heads/{ref}",
    ]
    for path in paths:
        value = query(path)
        if value.get("status") != "unavailable":
            obj = value.get("object", value)
            return {"use": use, "pinned": pinned, "resolved": True,
                    "object_sha": obj.get("sha"), "object_type": obj.get("type", "commit")}
    return {"use": use, "pinned": pinned, "resolved": False,
            "error": "Reference not resolved; distinguish missing refs from API/access failures."}


def inventory(repository, root):
    base = f"repos/{repository}"
    settings = {
        "repository": query(base, "{full_name,archived,default_branch,allow_auto_merge,delete_branch_on_merge,allow_merge_commit,allow_squash_merge,allow_rebase_merge,permissions,security_and_analysis}"),
        "actions": query(f"{base}/actions/permissions"),
        "workflow_token": query(f"{base}/actions/permissions/workflow"),
        "rulesets": query(f"{base}/rulesets?includes_parents=true"),
        "main_protection": query(f"{base}/branches/main/protection"),
        "environments": query(f"{base}/environments", "{total_count,environments:[.environments[]|{name,protection_rules,deployment_branch_policy}]}"),
        "secret_names": query(f"{base}/actions/secrets", "{total_count,names:[.secrets[].name]}"),
        "variable_names": query(f"{base}/actions/variables", "{total_count,names:[.variables[].name]}"),
        "webhooks": query(f"{base}/hooks", "[.[]|{name,active,events}]"),
        "codeowners_errors": query(f"{base}/codeowners/errors", "{errors:[.errors[]|{line,column,kind,message}]}"),
        "actions_runs": query(f"{base}/actions/runs", "{total_count}"),
        "release_count": query(f"{base}/releases", "length"),
    }
    uses, files = [], []
    for file in sorted((root / ".github/workflows").glob("*.yml")):
        source = file.read_bytes()
        relative = str(file.relative_to(root))
        files.append({"path": relative, "sha256": hashlib.sha256(source).hexdigest()})
        for number, line in enumerate(source.decode().splitlines(), 1):
            match = re.match(r"\s*(?:-\s*)?uses:\s*([^\s#]+)", line)
            if match:
                uses.append({"file": relative, "line": number, "use": match.group(1)})
    external = sorted({item["use"] for item in uses if not item["use"].startswith("./")})
    with ThreadPoolExecutor(max_workers=4) as pool:
        refs = list(pool.map(action_reference, external))
    commit = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
    return {
        "captured_at": datetime.now(timezone.utc).isoformat(), "repository": repository,
        "checkout_commit": commit, "settings": settings, "workflow_files": files,
        "action_uses": uses, "action_refs": refs,
        "external_action_occurrences": sum(not item["use"].startswith("./") for item in uses),
        "unique_external_refs": len(refs), "mutable_refs": sum(not item["pinned"] for item in refs),
        "unresolved_refs": [item["use"] for item in refs if not item["resolved"]],
        "coverage": "Repository settings and checked-in workflow/config inspection only. No hosted execution, deployment, signing, or secret-value access.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="dylbertram/konnect-maintained")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo):
        parser.error("--repo must be owner/name")
    result = inventory(args.repo, args.root.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ["repository", "captured_at", "unique_external_refs", "mutable_refs", "unresolved_refs"]}, indent=2))


if __name__ == "__main__":
    main()
