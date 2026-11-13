# Native KiCad tests on Boat

Run these commands inside the remote Ubuntu 24.04 worker, not the OpenChamber
host. The setup script uses passwordless sudo and the official stable KiCad 10
PPA. It installs the CLI, symbol/footprint libraries, protobuf compiler and
headers, and Xvfb tools. It excludes the large 3D model package.

```bash
bash scripts/setup-kicad-cli-ubuntu.sh
```

Setup is idempotent and checks the installed CLI and Python bindings. A new
worker can run the same script; a warm worker keeps its system packages. Allow
5 GB of free disk space. Check memory/CPU headroom before compiling and retain
at least 500 MB available memory beyond the planned workload.

The BySkap executor currently caps each job at 600 seconds. This is separate
from its two-hour worker auto-stop and 15-minute idle cleanup. Boat supports
longer detached commands, but that requires executor support; increasing a
worker's auto-stop does not extend a running job's timeout.

## Cross-sheet transfer acceptance

The CLI-only tests use the installed libraries and disposable project files:

```bash
export KONNECT_TEST_KICAD_CLI=/usr/bin/kicad-cli
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --locked -p konnect-core --lib transfer_native_netlist_and_erc_acceptance -- --ignored --nocapture --test-threads=1
cargo test --locked -p konnect-core --lib transfer_native_public_api_consolidates_both_circuit_groups -- --ignored --nocapture --test-threads=1
```

The full native group also includes an existing-PCB update test. KiCad 10 IPC
requires the PCB Editor, so run the group under a private virtual display:

```bash
xvfb-run -a cargo test --locked -p konnect-core --lib transfer_native -- --ignored --nocapture --test-threads=1
```

The tests create synthetic startup-limit and fuse/bypass circuits through the
public API. They export native netlists and ERC reports, exercise dry-run/apply,
and inspect persisted identities and geometry. The PCB test authors a routed
fixture with KiCad's Python model, opens its own isolated PCB Editor, updates it
through the public API, saves and closes it, then reloads it with KiCad to compare
footprint, pad and track identities and placement. It does not use the shared
desktop editor or a user's hardware project.

## Reusable worker snapshots

Boat saved snapshots hold installed tools; environments hold credentials and
settings. A tooling snapshot must not include repository source, job files,
private fixtures or credentials. Save only after the executor confirms the job
process tree and uploaded source have been cleaned.

The BySkap build client currently exposes `run`, `status` and `fetch`, not a
saved-snapshot operation. It can provision the worker with this setup script
without accessing the Boat account key. If an authorized operator saves a clean
worker as a template, configure future worker creation to use it through the
executor rather than creating unmanaged sandboxes or bypassing account limits.

References: [Boat tools](https://docs.boat.dev/machines/tools),
[saved snapshots](https://docs.boat.dev/snapshots/templates), and
[choosing an environment](https://docs.boat.dev/environments/choose).
