"""Coordinator-only mutation controls in an admitted disposable Linux checkout.

No Git, network, native KiCad or installs. Every variant compiles first, then
must fail one exact served test at its designated outcome assertion. Finally
restore exact source bytes/modes and rerun all positives. Source qualification
on Linux does not establish Windows/macOS runtime acceptance.
"""

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[3]
SOURCE = Path("crates/konnect-core/src/tools/project.rs")
PINS = ROOT / "crates/konnect-core/tests/project_rename_source_pins.json"
MODULE = "tools::project::rename_revision_tests::"
SAVE_TEST = MODULE + "original_full_snapshots_reject_root_child_project_and_prl_interposed_saves"
POSITIVES = {
    MODULE + name for name in [
        "clean_rename_changes_only_independently_enumerated_reference_bytes",
        "original_full_snapshots_reject_root_child_project_and_prl_interposed_saves",
        "pre_move_save_is_a_clean_refusal_without_filename_changes",
        "old_project_and_design_owners_refuse_before_any_move",
        "exact_live_board_refuses_and_unrelated_live_board_is_untouched",
        "pending_journals_are_refused_without_recovery_or_moves",
        "late_new_or_old_design_lock_reports_partial_without_rewriting_the_locked_sheet",
        "late_project_lock_reports_partial_with_the_verified_project_rewrite",
        "nonregular_child_source_refuses_before_any_project_move",
        "concurrent_new_target_and_recreated_old_source_survive_without_rollback",
        "first_move_collision_is_clean_and_preserves_the_new_target",
        "postwrite_mismatch_and_io_are_uncertain_and_never_clean_refusals",
        "backup_collision_reports_partial_and_never_claims_a_backup_move",
        "late_directory_collision_keeps_completed_backup_and_project_moves",
        "late_move_io_failure_reports_uncertain_with_known_applied_work",
        "synthetic_board_project_token_uses_the_guard_but_is_not_native_evidence",
        "successful_directory_move_reports_final_current_paths",
        "final_directory_readback_failure_is_uncertain_at_the_relocated_path",
        "dry_run_is_read_only_even_with_locks_and_pending_journals",
        "native_exclusive_moves_preserve_existing_file_and_directory_destinations",
        "exclusive_move_preserves_even_a_dangling_destination_symlink",
    ]
}
CONTROLS = {
    "failing-before-stale-write": SAVE_TEST,
    "guard-removed": SAVE_TEST,
    "expected-refreshed": SAVE_TEST,
    "partial-hidden": SAVE_TEST,
    "uncertain-became-refusal":
        MODULE + "postwrite_mismatch_and_io_are_uncertain_and_never_clean_refusals",
    "old-owner-checks-disabled": MODULE + "old_project_and_design_owners_refuse_before_any_move",
    "exclusive-move-replaced":
        MODULE + "concurrent_new_target_and_recreated_old_source_survive_without_rollback",
    "plain-refusal-envelope-disabled":
        MODULE + "exact_live_board_refuses_and_unrelated_live_board_is_untouched",
    "late-project-lock-wording-disabled":
        MODULE + "late_project_lock_reports_partial_with_the_verified_project_rewrite",
}
ASSERTION_MARKERS = {
    "plain-refusal-envelope-disabled": "designated envelope assertion",
    "late-project-lock-wording-disabled": "designated wording assertion",
}
MAX_LOG = 16 * 1024 * 1024


def once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"control anchor must occur exactly once: {old!r}")
    return source.replace(old, new, 1)


def marked(source, name, replacement):
    expression = rf"(?ms)^([ \t]*)// {name}_BEGIN\n.*?^\1// {name}_END\n"
    matches = list(re.finditer(expression, source))
    if len(matches) != 1:
        raise RuntimeError(f"expected one marked region: {name}")
    return re.sub(expression, lambda match: replacement, source, count=1)


def variant(original, name):
    source = original.decode()
    if name == "failing-before-stale-write":
        # Replay the old unconditional stale-write behavior at the same
        # consumed hook. This is not the entire historical project.rs.
        source = marked(source, "RENAME_GUARDED_TRANSACTION",
            "        let result = if doc.hits > 0 {\n"
            "            attempted = true;\n"
            "            konnect_sexp::write_atomic(path, &doc.after).map(|()| true)\n"
            "        } else { Ok(true) };\n")
    elif name == "guard-removed":
        source = marked(source, "RENAME_ORIGINAL_SNAPSHOT", "            let _ = current;\n")
    elif name == "expected-refreshed":
        source = once(source,
            "let result = konnect_sexp::transact_atomic(path, |current| {",
            "let refreshed = konnect_sexp::read_consistent(path)?;\n"
            "        let result = konnect_sexp::transact_atomic(path, |current| {")
        source = once(source, "if current != doc.before {", "if current != refreshed {")
    elif name == "partial-hidden":
        source = once(source, "let status = if progress.changed() {",
            "let status = if progress.changed() && progress.files.is_empty() {")
    elif name == "uncertain-became-refusal":
        start = source.index("fn rename_uncertain(")
        end = source.index("\nfn rename_lock_error(", start)
        source = source[:start] + (
            "fn rename_uncertain(path: &Path, reason: impl std::fmt::Display,\n"
            "    progress: &RenameProgress, documents: &[RenameDocument]) -> CallToolResult {\n"
            "    rename_outcome(rename_conflict(path, reason), progress, documents, path,\n"
            "        crate::outcome::OutcomeStatus::Failed)\n"
            "}\n"
        ) + source[end:]
    elif name == "old-owner-checks-disabled":
        start = source.index("async fn rename_authority(")
        end = source.index("\nfn pending_rename_journal(", start)
        source = source[:start] + (
            "async fn rename_authority(ctx: &ToolContext, project: &Path,\n"
            "    documents: &[RenameDocument]) -> anyhow::Result<Option<CallToolResult>> {\n"
            "    let _ = (ctx, project, documents);\n"
            "    Ok(None)\n"
            "}\n"
        ) + source[end:]
    elif name == "exclusive-move-replaced":
        source = once(source,
            "renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(Into::into)",
            "std::fs::rename(from, to)")
        source, count = re.subn(r"(?m)^    use rustix::fs::\{[^\n]+\};\n", "", source)
        if count != 1:
            raise RuntimeError("expected one now-unused rustix import")
    elif name == "plain-refusal-envelope-disabled":
        source = marked(source, "RENAME_REFUSAL_NORMALIZATION",
            "        let mut body = match serde_json::from_str::<serde_json::Value>(text) {\n"
            "            Ok(body) => body,\n"
            "            Err(_) => return CallToolResult {\n"
            "                content: vec![crate::mcp::protocol::ToolContent::Text { text: text.clone() }],\n"
            "                is_error: was_error,\n"
            "            },\n"
            "        };\n")
    elif name == "late-project-lock-wording-disabled":
        source = marked(source, "RENAME_PROJECT_LOCK_WORDING", "")
    else:
        raise RuntimeError(name)
    changed = source.encode()
    if changed == original:
        raise RuntimeError(f"control made no change: {name}")
    return changed


def stop_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=10)
    deadline = time.monotonic() + 10
    descendants = []
    while True:
        try:
            pid, status = os.waitpid(-process.pid, os.WNOHANG)
        except ChildProcessError:
            return descendants
        if pid:
            descendants.append({"pid": pid, "status": os.waitstatus_to_exitcode(status)})
        elif time.monotonic() >= deadline:
            raise RuntimeError("owned Cargo group did not finish reaping")
        else:
            time.sleep(0.05)


def run(command, label, output, deadline, per_command, expected_test=None, failure=False,
        assertion_marker="designated outcome assertion"):
    budget = min(per_command, deadline - time.monotonic())
    if budget <= 0:
        raise RuntimeError("phase budget exhausted")
    log = output / (label + ".log")
    audit = {"command": command, "expected_test": expected_test,
             "expected_failure": failure,
             "expected_assertion_marker": assertion_marker if failure else None,
             "group_reaped": False}
    process = None
    cleaned = False
    try:
        with log.open("xb") as stream:
            process = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.DEVNULL,
                stdout=stream, stderr=subprocess.STDOUT, start_new_session=True,
                env={**os.environ, "CARGO_TERM_COLOR": "never"})
            audit["leader"] = process.pid
            expires = time.monotonic() + budget
            while process.poll() is None:
                if log.stat().st_size > MAX_LOG:
                    raise RuntimeError("log limit exceeded")
                if time.monotonic() >= expires:
                    audit["timed_out"] = True
                    raise RuntimeError("command runtime limit exceeded")
                time.sleep(0.1)
        audit["descendants"] = stop_group(process)
        cleaned = True
        audit.update(status=process.returncode, group_reaped=True)
        if log.stat().st_size > MAX_LOG:
            raise RuntimeError("log limit exceeded")
        text = log.read_text(errors="replace")
        print(text, end="", flush=True)
        if re.search(r"^error(?:\[E\d+\]|:)", text, re.M):
            # "error: test failed" is Cargo's normal assertion-failure footer.
            errors = re.findall(r"^error[^\n]*", text, re.M)
            if not failure or any(not line.startswith("error: test failed,") for line in errors):
                raise RuntimeError(f"{label}: compilation/link/build error is not evidence")
        results = re.findall(r"^test ([\w:]+) \.\.\. (ok|FAILED)$", text, re.M)
        if expected_test is not None:
            wanted = "FAILED" if failure else "ok"
            summary = "FAILED. 0 passed; 1 failed; 0 ignored;" if failure else "ok. 1 passed; 0 failed; 0 ignored;"
            if (results != [(expected_test, wanted)]
                or len(re.findall(r"^running 1 test$", text, re.M)) != 1
                or f"test result: {summary}" not in text
                or process.returncode != (101 if failure else 0)):
                raise RuntimeError(f"{label}: exact named assertion outcome was not observed")
            if failure and not re.search(
                r"assertion [^\n]*failed: " + re.escape(assertion_marker) + ":", text
            ):
                raise RuntimeError(f"{label}: unrelated panic is not a caught control")
        elif "--no-run" in command:
            if process.returncode != 0 or "Executable unittests" not in text:
                raise RuntimeError(f"{label}: compile-first gate did not succeed")
        elif (process.returncode != 0 or len(results) != len(POSITIVES)
              or {name for name, _ in results} != POSITIVES
              or any(state != "ok" for _, state in results)
              or f"test result: ok. {len(POSITIVES)} passed; 0 failed; 0 ignored;" not in text):
            raise RuntimeError(f"{label}: missing/failed positive tests")
        audit["accepted"] = True
    finally:
        if process is not None and not cleaned:
            audit["descendants"] = stop_group(process)
            audit.update(status=process.returncode, group_reaped=True)
        (output / (label + ".audit.json")).write_text(json.dumps(audit, indent=2))


def interrupted(signum, _frame):
    raise KeyboardInterrupt(f"signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("control", choices=[*CONTROLS, "all"])
    parser.add_argument("--coordinator-admitted", action="store_true", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--command-timeout", type=int, default=600)
    parser.add_argument("--work-budget", type=int, default=3600)
    parser.add_argument("--recovery-budget", type=int, default=900)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("control runner requires an admitted disposable Linux worker")
    if not 1 <= args.command_timeout <= 600 or not 1 <= args.work_budget <= 3600 \
            or not 1 <= args.recovery_budget <= 900:
        parser.error("command/work/recovery budgets exceed 600/3600/900 seconds")
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "PR_SET_CHILD_SUBREAPER failed")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    pins = json.loads(PINS.read_text())
    originals = {}
    modes = {}
    if str(SOURCE) not in pins or len(pins) < 10:
        raise RuntimeError("critical source pins are incomplete")
    for name, digest in pins.items():
        path = Path(name)
        source = (ROOT / path).read_bytes()
        if hashlib.sha256(source).hexdigest() != digest:
            raise RuntimeError(f"source pin mismatch: {path}")
        originals[path] = source
        modes[path] = stat.S_IMODE((ROOT / path).stat().st_mode)

    def verify():
        for path, source in originals.items():
            if (ROOT / path).read_bytes() != source \
                    or stat.S_IMODE((ROOT / path).stat().st_mode) != modes[path]:
                raise RuntimeError(f"reviewed source bytes/mode changed: {path}")

    names = list(CONTROLS) if args.control == "all" else [args.control]
    changes = {name: variant(originals[SOURCE], name) for name in names}
    owned = [originals[SOURCE], *changes.values()]

    def restore():
        if (ROOT / SOURCE).read_bytes() not in owned:
            raise RuntimeError("refusing to overwrite an unowned project.rs revision")
        if (ROOT / SOURCE).read_bytes() != originals[SOURCE]:
            (ROOT / SOURCE).write_bytes(originals[SOURCE])
        os.chmod(ROOT / SOURCE, modes[SOURCE])
        verify()

    output = Path(args.output_dir).resolve()
    output.mkdir(parents=True, exist_ok=False)
    cargo = ["cargo", "test", "--offline", "--locked", "-p", "konnect-core", "--lib"]
    deadline = time.monotonic() + args.work_budget
    try:
        run(cargo + ["--no-run"], "positive-compile", output, deadline, args.command_timeout)
        run(cargo + [MODULE, "--", "--test-threads=1"], "positive-before",
            output, deadline, args.command_timeout)
        verify()
        for name in names:
            verify()
            try:
                (ROOT / SOURCE).write_bytes(changes[name])
                run(cargo + ["--no-run"], name + "-compile",
                    output, deadline, args.command_timeout)
                run(cargo + [CONTROLS[name], "--", "--exact", "--test-threads=1"],
                    name, output, deadline, args.command_timeout,
                    expected_test=CONTROLS[name], failure=True,
                    assertion_marker=ASSERTION_MARKERS.get(name, "designated outcome assertion"))
                if (ROOT / SOURCE).read_bytes() != changes[name]:
                    raise RuntimeError("control source changed during execution")
            finally:
                restore()
    finally:
        restore()
        recovery = time.monotonic() + args.recovery_budget
        run(cargo + ["--no-run"], "restored-compile", output, recovery, args.command_timeout)
        run(cargo + [MODULE, "--", "--test-threads=1"], "positive-restored",
            output, recovery, args.command_timeout)
        verify()
    print("All requested controls caught; exact bytes/modes restored; positives passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
