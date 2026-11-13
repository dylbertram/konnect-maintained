#!/usr/bin/env python3
"""Coordinator-only compile-first native loss control in a disposable archive.

Never run in a shared source tree. Changes only the sync inventory targets marker,
restores exact bytes in finally, then requires the restored positive Rust gate.
No Git required. Uses one exclusively admitted warm ROOT/target sequentially.
Does not launch KiCad. Fixed 600-second budget, no timeout retries.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import traceback

import native_footprint_children_controls as controls

ROOT = controls.ROOT
SOURCE = controls.TOOLS / "pcb_sync.rs"
OLD = b"&targets, // A8 sync inventory targets"
NEW = b"&[], // A8 sync inventory targets"
TARGET = ROOT / "target"
BUDGET_SECONDS = 600
CLEANUP_RESERVE_SECONDS = 10


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rust_sources():
    """Hash every Rust source, including build.rs/tests, outside target/.git."""
    def unreadable(error):
        raise error

    manifest = {}
    for directory, children, files in os.walk(ROOT, onerror=unreadable):
        children[:] = sorted(name for name in children
                             if name != ".git" and Path(directory) / name != TARGET)
        for name in sorted(files):
            if name.endswith(".rs"):
                path = Path(directory) / name
                manifest[str(path.relative_to(ROOT))] = sha(path)
    return manifest


def command(out, label, arguments, deadline, report):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("fixed build budget exhausted; no retry or further Cargo command")
    report["commands"].append({"argv": arguments, "log": f"{label}.log",
                               "timeout_seconds": remaining})
    with (out / f"{label}.log").open("w") as log:
        return subprocess.run(arguments, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                              timeout=remaining,
                              env={**os.environ, "CARGO_TERM_COLOR": "never",
                                   "CARGO_TARGET_DIR": str(TARGET)})


def positive(out, label, deadline, report):
    # Same compile-first/nonzero-test criteria and suite as require_positive.
    command(out, f"{label}-compile", [*controls.BASE, "--no-run"],
            deadline, report).check_returncode()
    result = command(out, f"{label}-tests",
                     [*controls.BASE, "a8_", "--", "--test-threads=1"], deadline, report)
    output = (out / f"{label}-tests.log").read_text()
    if result.returncode or not re.search(r"test result: ok\. [1-9]\d* passed;", output):
        raise RuntimeError(f"{label}: positive gate failed or ran zero tests")


def build(out, label, deadline, report):
    arguments = ["cargo", "build", "-p", "konnect", "--locked"]
    # Copies live outside the shared cache. A later restored build cannot
    # overwrite either the baseline or the control copy.
    command(out, f"{label}-build", arguments, deadline, report).check_returncode()
    destination = out / ("positive-konnect" if label == "restored" else f"{label}-konnect")
    shutil.copy2(TARGET / "debug/konnect", destination)
    report[f"{label}_binary_path"] = str(destination)
    return sha(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disposable-checkout", action="store_true", required=True)
    parser.add_argument("--source-revision", required=True,
                        help="coordinator-supplied full 40-hex source commit, not inferred from archive")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.source_revision):
        parser.error("--source-revision must be the coordinator's full 40-hex commit SHA")
    deadline = time.monotonic() + BUDGET_SECONDS - CLEANUP_RESERVE_SECONDS
    out = args.out.resolve()
    assert not out.is_relative_to(ROOT), "build evidence must be outside the source snapshot"
    assert (TARGET / "debug/deps").is_dir(), "requires the admitted warm ROOT/target cache"
    out.mkdir(parents=True, exist_ok=False)
    original = SOURCE.read_bytes()
    assert original.count(OLD) == 1, "need exactly one sync inventory target marker"
    mutated = original.replace(OLD, NEW, 1)
    report = {
        "status": "incomplete",
        "source_revision": args.source_revision.lower(),
        "revision_provenance": "explicit coordinator archive revision; no Git inferred or initialized",
        "rust_sources_before": rust_sources(),
        "source_path": str(SOURCE.relative_to(ROOT)),
        "original_source_sha256": hashlib.sha256(original).hexdigest(),
        "control_source_sha256": hashlib.sha256(mutated).hexdigest(),
        "mutation": {"old": OLD.decode(), "new": NEW.decode(), "occurrences": 1},
        "build_command": ["cargo", "build", "-p", "konnect", "--locked"],
        "cargo_target_dir": str(TARGET), "exclusive_target_admission_required": True,
        "budget_seconds": BUDGET_SECONDS, "cleanup_reserve_seconds": CLEANUP_RESERVE_SECONDS,
        "commands": [],
        "exact_restoration": False, "restored_positive_gate": False,
        "scope": "build provenance only; native loss not yet executed",
    }
    # Keep even a failed build's original and intended control bytes as evidence.
    (out / "pcb_sync.original.rs").write_bytes(original)
    (out / "pcb_sync.control.rs").write_bytes(mutated)
    try:
        positive(out, "baseline", deadline, report)
        report["baseline_positive_gate"] = True
        report["baseline_binary_sha256"] = build(out, "baseline", deadline, report)
        assert rust_sources() == report["rust_sources_before"], "baseline source drift"
        try:
            SOURCE.write_bytes(mutated)
            assert SOURCE.read_bytes() == mutated
            expected = {**report["rust_sources_before"],
                        str(SOURCE.relative_to(ROOT)): report["control_source_sha256"]}
            report["rust_sources_control"] = rust_sources()
            assert report["rust_sources_control"] == expected, "unexpected control source drift"
            report["control_binary_sha256"] = build(out, "control", deadline, report)
            assert SOURCE.read_bytes() == mutated, "control source changed during build"
            assert rust_sources() == expected, "control build source drift"
        finally:
            SOURCE.write_bytes(original)
            assert SOURCE.read_bytes() == original, "exact source restoration failed"
            report["exact_restoration"] = True
            report["rust_sources_restored"] = rust_sources()
            assert report["rust_sources_restored"] == report["rust_sources_before"], (
                "Rust source drift after exact target restoration; never overwrite other files")
            # Also required after a failed control build. A compile failure is
            # never evidence. If the budget expired, restore bytes but fail,
            # without starting another command or extending the deadline.
            positive(out, "restored", deadline, report)
            report["restored_binary_sha256"] = build(out, "restored", deadline, report)
            report["positive_binary_hashes_match"] = (
                report["baseline_binary_sha256"] == report["restored_binary_sha256"])
            assert report["positive_binary_hashes_match"], (
                "identical-source positive binary hashes differ; reject pending investigation")
            report["restored_positive_gate"] = True
        assert SOURCE.read_bytes() == original
        report["status"] = "passed"
    except BaseException as error:
        report.update(status="failed", error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        report["rust_sources_after"] = rust_sources()
        if report["rust_sources_after"] != report["rust_sources_before"]:
            report.update(status="failed", source_drift=True)
        (out / "native-loss-build.json").write_text(json.dumps(report, indent=2))
        if report.get("source_drift"):
            raise RuntimeError("Rust source manifest drift; retain evidence, do not execute native gate")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
