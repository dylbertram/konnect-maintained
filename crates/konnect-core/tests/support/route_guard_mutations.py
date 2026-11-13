#!/usr/bin/env python3
"""Coordinator-only route controls. Never run in a Git checkout or live project.

Requires an admitted, exclusive, disposable Linux source copy and warm compiler
cache. This runner does not install tools, submit jobs, or change Rust tests.
Each cargo command uses the existing Python process-group supervisor. Native
tests retain their own supervisor and their unchanged success assertions.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import tempfile


PIN = "d7f51fc034122b6c43c679bc1e27f666cb422854"
WIRE = "crates/konnect-core/src/tools/sch_wiring.rs"
TEST = "crates/konnect-core/src/tools/sch_route_safety_tests.rs"
SUPERVISOR = "crates/konnect-core/tests/support/pcb_export_supervisor.py"
HASHES = {
    WIRE: "45d63b98af7b63a283f134edf76bd55d9a559f9ba6c67924b740b8b667c020ee",
    TEST: "da804a633c3ed8e37a52ff4b633d8d4c781cc5d2354a2191ce8ca4a8c09f0de1",
    "crates/konnect-core/src/tools/sch_batch.rs":
        "538da62f4085901c2b936888c4642a444ae379c19d8ccad983f68497bf9b49c9",
    SUPERVISOR: "95108143509ca77a8b3e112ebe6e9386daf33e21712c67afa3213bd57fa0a665",
    "crates/konnect-core/tests/fixtures/rotate_junctions_kicad10.kicad_sch":
        "b92e78543a8ed869fe3a6712ad0a11ec730c17db732f2923f33f81fef10ed46c",
    "Cargo.lock": "60d54cdb64f5e686f0475abe0cab72c655fca7cb6756b6cb03f66404daebef99",
}
PREFIX = "tools::sch_route_safety_tests::"
ALTERNATE = PREFIX + "route_safety_served_alternate_bend_avoids_third_pin_and_sheet_pin"
BLOCKED = PREFIX + "route_safety_served_blocked_pin_and_standalone_label_write_nothing"
TEE = PREFIX + "route_safety_served_clear_route_and_remote_tee_are_preserved"
TAP = PREFIX + "route_safety_served_midspan_tap_keeps_unrelated_tee_undotted"
AMBIGUITY = PREFIX + "route_safety_served_missing_ambiguous_and_unknown_pin_geometry_fail_closed"
NATIVE = PREFIX + "route_safety_native_before_after_netlist_oracle"
CONTROLS = {
    "guard": (
        "        let joined = guard.foreign_joins(&parse_sexp(&routed)?);\n",
        "        let mut joined = guard.foreign_joins(&parse_sexp(&routed)?);\n"
        "        joined.clear();\n",
        (ALTERNATE, BLOCKED, PREFIX + "route_safety_served_straight_route_refuses_a_third_pin"),
    ),
    "local-t": (
        "        .filter(|&(jx, jy)| konnect_sexp::geometry::point_on_segment(jx, jy, x1, y1, x2, y2, 0.01))\n",
        "",
        (TEE, TAP),
    ),
    "alternate-bend": (
        '            ("vertical-first", vec![(x1, y1), (x1, y2), (x2, y2)]),\n',
        "",
        (ALTERNATE,),
    ),
    "label": (
        "        for label in &labels {\n"
        '            push((label.x, label.y), format!("label {}", label.net));\n'
        "        }\n",
        "",
        (BLOCKED,),
    ),
    "sheet": (
        "        for at in extract_sheet_pins(before) {\n"
        '            push(at, "sheet pin".into());\n'
        "        }\n",
        "        let _ = extract_sheet_pins(before);\n",
        (ALTERNATE,),
    ),
    "ambiguity": (
        '        _ => Err(format!("ambiguous route target {reference} pin {number}")),\n',
        "        _ => Ok(pin_endpoint(&matches[0].0, matches[0].1)),\n",
        (AMBIGUITY,),
    ),
}


class Failure(Exception):
    pass


class Quarantined(Failure):
    """Do not run another command; coordinator must reconcile worker cleanup."""


class Interrupted(BaseException):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def save_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def require(condition, message):
    if not condition:
        raise Failure(message)


def net_groups(path):
    """Read native kicadsexpr net membership only, never infer connectivity."""
    require(path.stat().st_size <= 8 * 1024 * 1024, "native netlist exceeds reader limit")
    source = path.read_text()
    tokens = re.findall(r'\s+|;[^\n]*|[()]|"(?:\\.|[^"\\])*"|[^\s()"]+', source)
    require("".join(tokens) == source, "unsupported native netlist token")
    stack, roots = [], []
    for token in tokens:
        if token.isspace() or token.startswith(";"):
            continue
        if token == "(":
            require(len(stack) < 64, "native netlist nesting exceeds reader limit")
            stack.append([])
        elif token == ")":
            require(bool(stack), "unbalanced native netlist")
            node = stack.pop()
            (stack[-1] if stack else roots).append(node)
        else:
            require(bool(stack), "native atom outside document")
            stack[-1].append(json.loads(token) if token.startswith('"') else token)
    require(not stack and len(roots) == 1, "incomplete native netlist")

    def children(node, tag):
        return [part for part in node if isinstance(part, list) and part and part[0] == tag]

    nets = children(roots[0], "nets")
    require(len(nets) == 1, "native netlist must contain one nets block")
    groups = []
    for net in children(nets[0], "net"):
        group = set()
        for node in children(net, "node"):
            ref, pin = children(node, "ref"), children(node, "pin")
            require(len(ref) == len(pin) == 1, "native node has ambiguous ref/pin")
            require(len(ref[0]) == len(pin[0]) == 2, "native ref/pin must be scalar")
            group.add(ref[0][1] + "." + pin[0][1])
        groups.append(group)
    return groups


def group_at(path, pin):
    found = [group for group in net_groups(path) if pin in group]
    require(len(found) == 1, f"native pin {pin} missing or duplicated in {path}")
    return found[0]


class Runner:
    def __init__(self, args):
        self.args = args
        self.root = Path(args.source).resolve()
        self.output = Path(args.output).resolve()
        self.cargo = Path(args.cargo).resolve()
        require(sys.platform == "linux" and sys.version_info >= (3, 9), "Linux/Python 3.9+ required")
        require(args.admitted_disposable_source, "explicit coordinator admission acknowledgement required")
        require(not (self.root / ".git").exists(), "refusing to mutate a Git checkout/worktree")
        require(self.cargo.is_file() and self.cargo.name == "cargo", "use actual installed cargo, not a rustup shim")
        for program in ("rustc", "rustdoc"):
            require((self.cargo.parent / program).is_file(), f"missing installed sibling {program}")
        if args.native_cli:
            args.native_cli = str(Path(args.native_cli).resolve())
            require(Path(args.native_cli).is_file(), "native CLI must be an explicit installed executable")
        self.output.mkdir(parents=True, exist_ok=False)
        self.path = self.root / WIRE
        require(self.path.resolve().is_relative_to(self.root) and not self.path.is_symlink(),
                "wire source must be a regular path inside the disposable copy")
        self.original = self.path.read_bytes()
        require(digest(self.original) == HASHES[WIRE], "wire source does not match pinned d7f51fc bytes")
        self.current = self.original
        self.owned_states = {self.original}
        self.mode = stat.S_IMODE(self.path.stat().st_mode)
        self.report = {
            "declared_source_pin": PIN,
            "critical_file_hashes": HASHES,
            "source": str(self.root),
            "controls": [],
            "commands": [],
            "native_requested": bool(args.native_cli),
            "status": "pending",
        }
        self.check_sources()
        # Preflight every selected edit before launching a single compiler.
        for name in args.only:
            old, _, _ = CONTROLS[name]
            require(self.original.count(old.encode()) == 1, f"{name}: anchor must occur exactly once")
        (self.output / "original-sch_wiring.rs").write_bytes(self.original)
        self.write_report()

    def write_report(self):
        save_json(self.output / "summary.json", self.report)

    def check_sources(self):
        for relative, expected in HASHES.items():
            path = self.root / relative
            require(path.resolve().is_relative_to(self.root), f"source escapes disposable root: {relative}")
            require(not path.is_symlink(), f"refusing a symlinked critical source: {relative}")
            if relative == WIRE:
                require(stat.S_IMODE(path.stat().st_mode) == self.mode, "wire source mode changed")
            wanted = digest(self.current) if relative == WIRE else expected
            require(digest(path.read_bytes()) == wanted, f"source pin/conflicting writer: {relative}")

    def replace_wire(self, content):
        self.check_sources()
        # Journal both sides before rename so interruption between rename and
        # Python assignment still has an exact, known recovery state.
        self.owned_states.add(content)
        self.report["restoration"] = {"verified": False, "current_sha256": digest(content)}
        # Atomic bytes/mode replacement, with a NEW mtime so Cargo must rebuild
        # restored source instead of reusing a mutant binary.
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=self.path.parent, prefix=".route-control-", delete=False) as file:
                temporary = Path(file.name)
                file.write(content)
                file.flush()
                os.fsync(file.fileno())
                os.fchmod(file.fileno(), self.mode)
            temporary.replace(self.path)
            self.current = content
            require(self.path.read_bytes() == content, "wire replacement readback mismatch")
        finally:
            if temporary is not None and temporary.exists():
                temporary.unlink()

    def restore(self):
        # Never discard another writer's work, even in a disposable copy.
        live = self.path.read_bytes()
        require(live in self.owned_states, "foreign edit: original backup retained; not overwriting")
        self.current = live
        if self.current != self.original:
            self.replace_wire(self.original)
        self.check_sources()
        require(self.path.read_bytes() == self.original, "source restoration is not byte-exact")
        require(stat.S_IMODE(self.path.stat().st_mode) == self.mode, "source mode changed")
        self.report["restoration"] = {"verified": True, "sha256": digest(self.original)}
        self.write_report()

    def command(self, label, cargo_args, native=False):
        self.check_sources()
        directory = self.output / label
        directory.mkdir(parents=True, exist_ok=False)
        wrapper = directory / "supervisor.py"
        wrapper.write_bytes((self.root / SUPERVISOR).read_bytes())
        save_json(wrapper.with_suffix(".json"), {
            "program": str(self.cargo), "seconds": self.args.seconds,
            "audit": str(directory / "audit"),
        })
        argv = [sys.executable, str(wrapper), "--color", "never", *cargo_args]
        env = os.environ.copy()
        env["RUSTC"] = str(self.cargo.parent / "rustc")
        env["RUSTDOC"] = str(self.cargo.parent / "rustdoc")
        # Do not share native temporary roots between commands or controls.
        temporary = directory / "tmp"
        temporary.mkdir()
        env["TMPDIR"] = str(temporary)
        env.pop("KONNECT_ROUTE_NATIVE_ARTIFACT_DIR", None)
        env.pop("KONNECT_ROUTE_KICAD_CLI", None)
        if native:
            env["KONNECT_ROUTE_KICAD_CLI"] = self.args.native_cli
            env["KONNECT_ROUTE_NATIVE_ARTIFACT_DIR"] = str(directory / "input-capture")
        item = {"label": label, "argv": argv, "seconds": self.args.seconds, "status": "pending"}
        self.report["commands"].append(item)
        save_json(directory / "command.json", item)
        self.write_report()
        try:
            with (directory / "combined.log").open("wb") as log:
                process = subprocess.Popen(argv, cwd=self.root, env=env, stdin=subprocess.DEVNULL,
                                           stdout=log, stderr=subprocess.STDOUT)
                try:
                    # The SCRIPT owns the independent child deadline/group
                    # cleanup. This extra wait only detects broken supervision.
                    code = process.wait(timeout=self.args.seconds + 10)
                except BaseException:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
                    raise
            finishes = list((directory / "audit").glob("*/finished.json"))
            require(len(finishes) == 1, f"{label}: no unique supervisor completion")
            audit = json.loads(finishes[0].read_text())
            item.update({"exit_code": code, "supervisor": audit})
            if audit.get("timed_out") or audit.get("interrupted") or not audit.get("group_reaped"):
                raise Quarantined(f"{label}: deadline/interruption/uncertain cleanup, not an expected test failure")
            item["status"] = "completed"
            return code, (directory / "combined.log").read_text(errors="replace"), directory
        except BaseException as error:
            item.update({"status": "aborted", "error": repr(error)})
            # Cargo's group can contain native supervisors whose native groups
            # are separate sessions. Never continue after an aborted command;
            # the coordinator's outer cgroup must reconcile the whole worker.
            self.report["worker_cleanup_required"] = True
            raise
        finally:
            save_json(directory / "command.json", item)
            self.write_report()

    @staticmethod
    def test_args(selector, exact=False, ignored=False):
        args = ["test", "-p", "konnect-core", "--locked", "--offline", "--lib", selector, "--"]
        if exact:
            args.append("--exact")
        if ignored:
            args.append("--ignored")
        return args + ["--color", "never", "--test-threads=1", "--nocapture"]

    def positive(self, label, native=False):
        selectors = [
            ("focused", "route_safety_", ALTERNATE),
            ("batch", "batch_connect_pins",
             "tools::sch_batch::batch_place_and_connect_tests::batch_connect_pins_dedupes_junction_and_collects_errors"),
            ("units", "unit_aware_wiring_tests",
             "tools::sch_wiring::unit_aware_wiring_tests::connect_pins_uses_the_instance_unit"),
        ]
        if native:
            selectors.append(("native", NATIVE, NATIVE))
        for suffix, selector, required in selectors:
            code, text, _ = self.command(label + "/" + suffix,
                                         self.test_args(selector, exact=suffix == "native",
                                                        ignored=suffix == "native"),
                                         native=suffix == "native")
            require(code == 0 and "test result: ok." in text, f"{label}/{suffix}: positive failed")
            require(re.search(r"(?m)^test " + re.escape(required) + r" \.\.\. ok$", text),
                    f"{label}/{suffix}: required named positive did not run")

    def expected_failure(self, label, test, native=False):
        code, text, directory = self.command(label, self.test_args(test, exact=True, ignored=native),
                                              native=native)
        failed = set(re.findall(r"(?m)^    (tools::[A-Za-z0-9_:]+)\s*$", text))
        require(code == 101 and "test result: FAILED." in text and failed == {test},
                f"{label}: need exactly the named Rust failure, not compile/startup/timeout failure")
        require("panicked at" in text, f"{label}: missing Rust assertion/panic evidence")
        return directory

    def native_join(self, name, directory):
        # These captures precede the unchanged native test's assertions. Require
        # an actual exported accidental join, not merely a response mismatch.
        case, pin, expected, addition = (
            ("around", "R1.2", {"R1.2", "R2.1"}, "R3.1") if name == "guard"
            else ("tee", "R51.2", {"R51.2", "R52.1"}, "R53.2")
        )
        relative = Path("input-capture") / "connect_pins" / case
        baseline = self.output / "baseline" / "native" / relative
        mutant = directory / relative
        for phase in ("before", "after"):
            result = json.loads((mutant / phase / "result.json").read_text())
            audit = json.loads((mutant / phase / "finished.json").read_text())
            require(result.get("success") and result.get("exit_code") == 0, "native export itself failed")
            require(audit.get("group_reaped") and not audit.get("timed_out")
                    and not audit.get("interrupted"), "native export cleanup not proven")
        normal = group_at(baseline / "after" / "result.net", pin)
        broken = group_at(mutant / "after" / "result.net", pin)
        separate = group_at(mutant / "before" / "result.net", addition)
        require(normal == expected, "native baseline partition does not match fixture contract")
        require(separate == {addition}, "foreign pin was not separate before mutation")
        require(broken == expected | {addition}, "no exact expected accidental native join")
        save_json(directory / "join-proof.json", {
            "case": case, "pin": pin, "baseline_after": sorted(normal),
            "mutant_before_foreign": sorted(separate), "mutant_after": sorted(broken),
            "native_netlist": str(mutant / "after" / "result.net"),
        })

    def run(self):
        self.positive("baseline", native=bool(self.args.native_cli))
        for name in self.args.only:
            old, new, tests = CONTROLS[name]
            mutant = self.original.replace(old.encode(), new.encode(), 1)
            item = {"name": name, "status": "pending", "mutant_sha256": digest(mutant),
                    "old": old, "new": new, "expected_failures": list(tests),
                    "native_join": "not_requested" if name in ("guard", "local-t") else "not_applicable"}
            self.report["controls"].append(item)
            self.write_report()
            error = None
            try:
                self.replace_wire(mutant)
                code, _, _ = self.command(name + "/compile",
                    ["test", "-p", "konnect-core", "--locked", "--offline", "--lib", "--no-run"])
                require(code == 0, f"{name}: mutant must compile before negative evidence counts")
                for index, test in enumerate(tests):
                    self.expected_failure(name + f"/failure-{index + 1}", test)
                if self.args.native_cli and name in ("guard", "local-t"):
                    directory = self.expected_failure(name + "/native", NATIVE, native=True)
                    self.native_join(name, directory)
                    item["native_join"] = "verified"
            except Exception as failure:
                error = failure
                item.update({"status": "failed", "error": repr(failure)})
                self.write_report()
            finally:
                self.restore()
            if self.report.get("worker_cleanup_required"):
                item["restored_positives"] = "not_run_quarantined"
                raise error or Quarantined("worker cleanup required")
            # Also attempt restored positives when a completed control failed
            # its expectations. An error is never converted into a passing run.
            self.positive(name + "/restored",
                          native=bool(self.args.native_cli) and name in ("guard", "local-t"))
            item["restored_positives"] = "passed"
            if error is not None:
                item.update({"status": "failed", "error": repr(error)})
                raise error
            item["status"] = "passed"
            self.write_report()
        self.report["status"] = "passed"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", required=True, help="new evidence directory, no prior-run reuse")
    parser.add_argument("--cargo", required=True, help="absolute installed toolchain/bin/cargo, not rustup")
    parser.add_argument("--native-cli", help="enable native baseline/restored gates and two join controls")
    parser.add_argument("--only", default=",".join(CONTROLS))
    parser.add_argument("--seconds", type=int, choices=range(1, 61), default=60)
    parser.add_argument("--admitted-disposable-source", action="store_true")
    args = parser.parse_args()
    args.only = args.only.split(",")
    require(args.only and len(set(args.only)) == len(args.only)
            and all(name in CONTROLS for name in args.only), "invalid/repeated control selection")

    def interrupt(signum, _frame):
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        raise Interrupted(f"signal {signum}")

    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)
    runner = Runner(args)
    # Exclusive runner lock in the disposable source. Never remove a lock owned
    # by another attempt and never inspect or kill another session's processes.
    lock = runner.root / ".route-guard-mutation.lock"
    descriptor = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    code = 0
    try:
        os.write(descriptor, str(os.getpid()).encode())
        runner.run()
    except BaseException as error:
        runner.report.update({"status": "failed", "error": repr(error)})
        print(f"route controls failed: {error}", file=sys.stderr)
        code = 1
    finally:
        try:
            runner.restore()
        except BaseException as error:
            runner.report.update({"status": "restoration_failed", "restore_error": repr(error),
                                  "restoration": {"verified": False}})
            code = 1
        runner.write_report()
        own_lock = lock.exists() and lock.stat().st_ino == os.fstat(descriptor).st_ino
        os.close(descriptor)
        if own_lock:
            lock.unlink()
        else:
            runner.report["lock_error"] = "runner lock was removed or replaced; not unlinking foreign state"
            runner.write_report()
            code = 1
    return code


if __name__ == "__main__":
    sys.exit(main())
