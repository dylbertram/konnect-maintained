#!/usr/bin/env python3
"""Coordinator-only compile-first native loss control in a disposable checkout.

Never run in a shared checkout. Changes only the sync inventory targets marker,
restores exact bytes in finally, then requires the restored positive Rust gate.
Does not launch KiCad. Native execution is separately supervised and admitted.
"""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import traceback

import native_footprint_children_controls as controls

ROOT = controls.ROOT
SOURCE = controls.TOOLS / "pcb_sync.rs"
OLD = b"&targets, // A8 sync inventory targets"
NEW = b"&[], // A8 sync inventory targets"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def positive(out, label):
    # Reuse the existing compile-first/nonzero-test acceptance unchanged.
    with (out / f"{label}-tests.log").open("w") as log:
        with contextlib.redirect_stdout(log):
            controls.require_positive()


def build(out, label, target):
    command = ["cargo", "build", "-p", "konnect", "--locked"]
    with (out / f"{label}-build.log").open("w") as log:
        subprocess.run(command, cwd=ROOT, check=True, stdout=log, stderr=subprocess.STDOUT,
                       env={**os.environ, "CARGO_TERM_COLOR": "never",
                            "CARGO_TARGET_DIR": str(target)})
    destination = out / ("control-konnect" if label == "control" else "positive-konnect")
    shutil.copy2(target / "debug/konnect", destination)
    return sha(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disposable-checkout", action="store_true", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    assert not out.is_relative_to(ROOT), "build evidence must be outside the source snapshot"
    assert not subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT).strip(), (
        "requires a clean disposable checkout")
    out.mkdir(parents=True, exist_ok=False)
    original = SOURCE.read_bytes()
    assert original.count(OLD) == 1, "need exactly one sync inventory target marker"
    mutated = original.replace(OLD, NEW, 1)
    report = {
        "status": "incomplete",
        "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_path": str(SOURCE.relative_to(ROOT)),
        "original_source_sha256": hashlib.sha256(original).hexdigest(),
        "control_source_sha256": hashlib.sha256(mutated).hexdigest(),
        "mutation": {"old": OLD.decode(), "new": NEW.decode(), "occurrences": 1},
        "build_command": ["cargo", "build", "-p", "konnect", "--locked"],
        "exact_restoration": False, "restored_positive_gate": False,
        "scope": "build provenance only; native loss not yet executed",
    }
    # Keep even a failed build's original and intended control bytes as evidence.
    (out / "pcb_sync.original.rs").write_bytes(original)
    (out / "pcb_sync.control.rs").write_bytes(mutated)
    try:
        positive(out, "baseline")
        report["baseline_positive_gate"] = True
        report["baseline_binary_sha256"] = build(out, "baseline", out / "positive-target")
        try:
            SOURCE.write_bytes(mutated)
            assert SOURCE.read_bytes() == mutated
            # Separate target and copied binary prevent a restored rebuild from
            # silently replacing the binary used for the native control.
            report["control_binary_sha256"] = build(out, "control", out / "control-target")
            assert SOURCE.read_bytes() == mutated, "control source changed during build"
        finally:
            SOURCE.write_bytes(original)
            assert SOURCE.read_bytes() == original, "exact source restoration failed"
            report["exact_restoration"] = True
            # Also required after a failed control build. A compile failure is
            # never accepted as either native reproduction or positive evidence.
            positive(out, "restored")
            report["restored_binary_sha256"] = build(out, "restored", out / "positive-target")
            report["restored_positive_gate"] = True
        assert SOURCE.read_bytes() == original
        report["status"] = "passed"
    except BaseException as error:
        report.update(status="failed", error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        (out / "native-loss-build.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
