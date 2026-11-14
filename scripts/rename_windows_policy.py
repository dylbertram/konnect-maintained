"""Disposable GitHub-hosted Windows trial of the 64-bit VS optional telemetry policy.

--evidence is a dedicated absolute directory under RUNNER_TEMP, outside checkout.
Review before use. This single policy is not a claim that all telemetry is off.
https://learn.microsoft.com/en-us/visualstudio/ide/visual-studio-experience-improvement-program?view=visualstudio
"""

import argparse
import json
import os
from pathlib import Path
import re
import stat
import sys
import uuid


KEY = r"Software\Policies\Microsoft\VisualStudio\SQM"
CHAIN = ["\\".join(KEY.split("\\")[:i]) for i in range(1, 6)]
NAME = "OptIn"
TARGET = {"exists": True, "type": 4, "value": 0}
ABSENT = {"exists": False, "type": None, "value": None}
STATE = "policy-state.json"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def failure(error):
    detail = str(error) if type(error) is RuntimeError else "Unexpected error"
    return {"error": type(error).__name__, "detail": detail[:240],
            "winerror": getattr(error, "winerror", None)}


def plain_path(path):
    """Reject symlinks and Windows junctions, including existing ancestors."""
    for part in (path, *path.parents):
        try:
            info = part.lstat()
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(info.st_mode) and not (
            getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
        ), "Reparse points are not owned evidence paths")


def context(evidence):
    require(sys.platform == "win32", "Windows is required")
    required = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted",
                "RUNNER_OS": "Windows"}
    for name, value in required.items():
        require(os.environ.get(name) == value, "Unsupported runner context")
    identity = {name: os.environ.get(name, "") for name in (
        "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB",
        "VALIDATION_WORKFLOW_SHA", "TESTED_SOURCE_SHA")}
    for name, value in identity.items():
        pattern = (r"[0-9a-fA-F]{40}" if name.endswith("_SHA") else
                   r"[1-9][0-9]{0,19}" if name != "GITHUB_JOB" else r"[A-Za-z_][A-Za-z0-9_-]{0,99}")
        require(re.fullmatch(pattern, value), "Missing or invalid job identity")
    temp = Path(os.environ.get("RUNNER_TEMP", ""))
    require(temp.is_absolute() and temp.is_dir() and evidence.is_absolute(),
            "Absolute evidence and RUNNER_TEMP paths are required")
    plain_path(temp)
    plain_path(evidence)
    temp, evidence = temp.resolve(), evidence.resolve()
    roots = [Path(__file__).resolve().parents[1]]
    if os.environ.get("GITHUB_WORKSPACE"):
        roots.append(Path(os.environ["GITHUB_WORKSPACE"]).resolve())
    require(len(str(evidence)) <= 1024 and evidence != temp and temp in evidence.parents and
            all(evidence != root and root not in evidence.parents for root in roots),
            "Evidence must be under RUNNER_TEMP and outside checkout")
    identity["evidence"] = str(evidence)
    return evidence, identity


def load(path):
    plain_path(path)
    require(path.is_file() and path.stat().st_size <= 32768, "Invalid evidence file")
    with path.open(encoding="utf-8") as stream:
        return json.load(stream)


def save(directory, name, data, exclusive=False):
    """Flush a complete file, then publish atomically, never replacing state."""
    path = directory / name
    plain_path(path)
    if not exclusive and path.exists():
        require(load(path).get("identity") == data["identity"], "Report ownership mismatch")
    temporary = directory / ("." + name + "." + uuid.uuid4().hex)
    try:
        with temporary.open("x", encoding="utf-8") as stream:
            json.dump(data, stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        # Windows rename fails if the destination already exists.
        (os.rename if exclusive else os.replace)(temporary, path)
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def exists(reg, key):
    try:
        with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, key, 0,
                         reg.KEY_READ | reg.KEY_WOW64_64KEY):
            return True
    except FileNotFoundError:
        return False


def value(reg):
    try:
        with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, KEY, 0,
                         reg.KEY_QUERY_VALUE | reg.KEY_WOW64_64KEY) as handle:
            data, kind = reg.QueryValueEx(handle, NAME)
            return {"exists": True, "type": kind, "value": data}
    except FileNotFoundError:
        return dict(ABSENT)


def validate(snapshot, identity):
    require(set(snapshot) == {"version", "identity", "policy", "intent", "original", "missing"}
            and snapshot["version"] == 1 and snapshot["identity"] == identity
            and snapshot["policy"] == ["HKLM", KEY, NAME, "64"]
            and snapshot["intent"] == "set_REG_DWORD_0", "Snapshot context mismatch")
    old, missing = snapshot["original"], snapshot["missing"]
    require(isinstance(old, dict) and set(old) == set(ABSENT) and
            ((old["exists"] is False and old["type"] is None and old["value"] is None)
             or (old["exists"] is True and type(old["type"]) is int
             and old["type"] == 4 and type(old["value"]) is int
             and 0 <= old["value"] <= 0xFFFFFFFF)), "Invalid typed snapshot")
    require(isinstance(missing, list) and
            any(missing == CHAIN[i:] for i in range(len(CHAIN) + 1)) and
            not (old["exists"] and missing), "Invalid missing-key snapshot")


def restore(reg, snapshot):
    old = snapshot["original"]
    current = value(reg)
    require(current == TARGET or current == old, "Concurrent policy change; refusing overwrite")
    if current != old:
        with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, KEY, 0,
                         reg.KEY_SET_VALUE | reg.KEY_WOW64_64KEY) as handle:
            if old["exists"]:
                reg.SetValueEx(handle, NAME, 0, reg.REG_DWORD, old["value"])
            else:
                reg.DeleteValue(handle, NAME)
    require(value(reg) == old, "Original value was not restored")
    for key in reversed(snapshot["missing"]):
        try:
            with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, key, 0,
                             reg.KEY_READ | reg.KEY_WOW64_64KEY) as handle:
                children, values, _ = reg.QueryInfoKey(handle)
                require(children == 0 and values == 0,
                        "Cleanup uncertain: newly created key has remaining content")
            reg.DeleteKeyEx(reg.HKEY_LOCAL_MACHINE, key, reg.KEY_WOW64_64KEY, 0)
        except FileNotFoundError:
            pass
    require(value(reg) == old and not any(exists(reg, key) for key in snapshot["missing"]),
            "Restoration verification failed")


def restore_report(reg, directory, identity, snapshot=None):
    report = {"identity": identity, "status": "restored"}
    try:
        if snapshot is None:
            snapshot = load(directory / STATE)
        validate(snapshot, identity)
        restore(reg, snapshot)
    except BaseException as error:
        report.update(status="failed", cleanup_uncertain=True, **failure(error))
        save(directory, "policy-restore.json", report)
        raise
    save(directory, "policy-restore.json", report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--action", required=True, choices=("apply", "restore"))
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    directory, identity = context(args.evidence)
    import winreg as reg

    if args.action == "restore":
        directory.mkdir(parents=True, exist_ok=True)
        restore_report(reg, directory, identity)
    else:
        # A fresh directory prevents accidentally adopting another trial's evidence.
        directory.mkdir(parents=True, exist_ok=False)
        old = value(reg)
        missing = [key for key in CHAIN if not exists(reg, key)]
        snapshot = {"version": 1, "identity": identity,
                    "policy": ["HKLM", KEY, NAME, "64"], "intent": "set_REG_DWORD_0",
                    "original": old, "missing": missing}
        validate(snapshot, identity)  # Reject non-DWORD values before any registry write.
        save(directory, STATE, snapshot, exclusive=True)
        try:
            require(value(reg) == old, "Policy changed after snapshot")
            for key in missing:
                require(not exists(reg, key), "Policy ancestor appeared after snapshot")
                with reg.CreateKeyEx(reg.HKEY_LOCAL_MACHINE, key, 0,
                                     reg.KEY_WRITE | reg.KEY_WOW64_64KEY):
                    pass
            require(value(reg) == old, "Policy changed during key creation")
            with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, KEY, 0,
                             reg.KEY_SET_VALUE | reg.KEY_WOW64_64KEY) as handle:
                reg.SetValueEx(handle, NAME, 0, reg.REG_DWORD, 0)
            require(value(reg) == TARGET, "Policy readback failed")
            save(directory, "policy-apply.json", {"identity": identity, "status": "applied"})
        except BaseException as error:
            report = {"identity": identity, "status": "failed", **failure(error)}
            try:
                restore_report(reg, directory, identity, snapshot)
                report["rollback"] = "restored"
            except BaseException as rollback_error:
                report.update(rollback="failed", rollback_error=failure(rollback_error))
            save(directory, "policy-apply.json", report)
            raise
    print(json.dumps({"status": args.action + "_complete", "evidence": str(directory)}))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(json.dumps({"status": "failed", **failure(error)}), file=sys.stderr)
        sys.exit(1)
