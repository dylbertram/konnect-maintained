# PCB export acceptance

Scope: existing Kata `pe7h`, `459k`, `xsd6`. This is a proposed execution gate,
not a record of tests run. The implementation child does not admit jobs.
The coordinator must verify capacity and choose the approved environment before
running anything below, following `byskap-builds`. Do not run on live CAD.

## Maintained-source findings

At child base `16bb336`, including PR #11, `handle_export_netlist` always calls
`cli::export_netlist`. That wrapper always uses `sch export netlist`; a separate
`cli::export_ipcd356` already builds `pcb export ipcd356` but has no caller.
The PDF/SVG handlers discard malformed layer entries and forward empty arrays.
Their single-file command builder then omits `--layers`.

The three existing Kata bodies and all archived comments/reviews were read.
Issue #465 has one comment extending the defect to PDF and rejecting an
all-copper default. PR #919 has no archived review or inline comments. Its
implementation and historical validation are reference evidence, not evidence
for this child. No other export packet is included here.

This change keeps the saved-file CLI policy and existing output publication
paths. It does not save or query the editor, weaken live-board authority in
other tools, add multipage mode, or change Gerber/manufacturing selection.
It does not claim board-copper continuity analysis from IPC-D-356 pad/net records.

## Fixtures and independent oracles

- `crates/konnect-core/tests/fixtures/specctra_two_resistors.kicad_pcb` is existing
  KiCad 10 `pcbnew` output. It has four SMD terminals:
  `VCC/R1/1`, `GND/R1/2`, `GND/R2/1`, `VCC/R2/2`, and a rectangular outline.
- `six_layer_power_planes_kicad10.kicad_pcb` and its README establish the native
  six-layer table used by the unit oracle. Inner/back/documentation layers must
  not enter the default. Small in-code structural cases test filtering/refusal;
  they are not represented as native fixtures.
- `component_fields_kicad10.kicad_sch` supplies explicit schematic compatibility
  acceptance. Its exported artifact must parse as a nonempty KiCad netlist.

No CAD fixture is changed. Native acceptance copies fixture bytes into one
disposable directory and checks the original copied bytes after exporting.

The native IPC oracle reads fixed-width net/reference/pin fields of KiCad's
`327` SMD records and requires the exact four-terminal partition and `999`
terminator. It also checks a direct CLI control against that independent oracle.
The field layout and SMD record code were cross-checked against KiCad's
[`export_d356.cpp`](https://github.com/KiCad/kicad-source-mirror/blob/10.0/pcbnew/exporters/export_d356.cpp).

The CLI command is documented in the
[KiCad 10 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html).
Direct native controls plot the literal front-view layer list, not a constant
read from production code. Served omitted/empty requests must rasterize exactly
like those controls. SVG is parsed by `usvg` and rasterized by `konnect-render`.
PDF must parse with `pdfinfo`, have exactly one page, and rasterize identically
through `pdftoppm`. A copper-only SVG must differ from the default, preventing
an empty or uninformative fixture from satisfying the visual comparison.

## Commands for an admitted coordinator job

Focused regressions include served catalogs, dispatch, explicit input,
schema refusals, missing board paths, no sibling guessing, and no-write
refusals. The separate `direct_handlers_reject_malformed_layers_without_cli_or_write`
test bypasses schema validation and checks each plot handler's array/index
refusal, absent CLI log, unchanged board and existing output sentinel.

```sh
cargo test -p konnect-core --locked --lib pcb_export_contract_tests
cargo test -p konnect-core --locked --lib pcb_plot_export_tests
cargo test -p konnect-core --locked --lib required_layers_tests
cargo test -p konnect-core --locked --lib fabrication_option_tests
```

The fake CLI tests assert transport and refusal contracts only. Their marker
artifacts are deliberately not native evidence.

`McpHandler` validates schemas before invoking handlers. Consequently the served
malformed-layer test passes even if a handler regresses to dropping bad entries.
Only the direct-handler test detects that mutation. Keep both tests, but do not
claim served schema refusal proves the handler's guard.

## Native process lifecycle and platform

Native acceptance is currently supported on **Linux only**, with Python **3.9+**,
KiCad **10.x**, and Poppler's `pdfinfo` and `pdftoppm`. It does not establish
Windows or macOS process cleanup. Tools must already exist in the admitted
environment. Missing tools or a non-Linux invocation fail, never skip or install.
Configure a real KiCad executable, not a fake CLI:

```sh
KONNECT_TEST_KICAD_CLI=/absolute/path/to/kicad-cli \
cargo test -p konnect-core --locked --lib \
  native_pcb_export_artifact_acceptance -- --ignored --nocapture
```

`KONNECT_TEST_PDFINFO` and `KONNECT_TEST_PDFTOPPM` optionally override the Poppler
executables. Every direct native command and every **served** native export runs
through a test-only Python supervisor from
`crates/konnect-core/tests/support/pcb_export_supervisor.py`. The supervisor path
is passed as `ServerConfig.kicad_cli`; the actual executable and fixed runtime
budget are in an adjacent test-owned JSON file. Arguments are forwarded without
a shell. Production CLI code and its 600-second timeout are unchanged.

Each supervisor starts its native tool in a new session/process group, acts as a
Linux child subreaper, and owns an independent **60-second child-runtime limit**.
On expiry it sends SIGKILL to that group, collects pipes, reaps the leader and
adopted group descendants, then returns 124. It also cleans up remaining group
members after normal exit. SIGTERM/SIGINT request the same cleanup. Atomic
started/finished audit records report native PIDs, exit statuses and reaping.
This is a runtime cutoff followed by cleanup, not a promise that kernel
termination/reaping completes at exactly 60 seconds.

The Rust direct-command helper deliberately does **not** kill the supervisor on
future drop. Production served calls also leave the supervisor alive on drop.
If a tools/call future is cancelled, the supervisor retains its own deadline
and still stops/reaps its native group. A Rust timeout around tools/call alone
would not provide that guarantee and is not used as the cleanup mechanism.

This is a test harness for trusted native binaries, not a sandbox. It assumes
children stay in the supervisor-created group. Deliberate session/group escape,
SIGKILL of the supervisor itself, and whole-worker crashes require the
coordinator's worker teardown; no cleanup guarantee is claimed for those cases.
The test does not open PCB Editor, use IPC, upgrade files, or load real projects.

Before native artifact acceptance, run the supervisor lifecycle tests in an
admitted Linux/Python environment. They need no KiCad or Poppler:

```sh
cargo test -p konnect-core --locked --lib native_supervisor -- --ignored --nocapture
```

`native_supervisor_terminates_direct_and_served_children` exercises both the
direct helper and the production served CLI path against a two-second budget.
`native_supervisor_survives_cancelled_served_call` drops a served request only
after the native leader and descendant have started. Both require timeout
evidence, SIGKILL status for leader and descendant, group reaping and absent
`/proc` entries. The synthetic processes self-exit after five seconds as a
secondary safety bound. Observation timeouts only fail assertions; the Python
supervisor, not those timeouts, owns termination.

Required real CLI operations, executed by that test:

```text
kicad-cli --version
kicad-cli pcb export ipcd356 --output <file> <copied-board.kicad_pcb>
kicad-cli pcb export pdf --output <file> --mode-single --layers F.Cu,F.Mask,F.SilkS,Edge.Cuts <board>
kicad-cli pcb export svg --output <file> --mode-single --layers F.Cu,F.Mask,F.SilkS,Edge.Cuts <board>
kicad-cli sch export netlist --output <file> --format kicadsexpr <copied-schematic.kicad_sch>
pdfinfo <pdf>
pdftoppm -f 1 -singlefile -scale-to 1200 -png <pdf> <prefix>
```

It also invokes both plot commands without `--layers` and requires a nonzero
exit, the native missing-layer diagnostic and no artifact. This retains the
original failure as a negative control. Save exact revision, CLI/Poppler versions,
job ID and output with the coordinator's evidence.

## Failing-before and mutation controls

Run the new tests against the base with the test module and supervisor source
attached.
The production-only `front_view_layers` unit test needs the new helper to
compile; for that base comparison omit just that unit test. Other tests do not
require it. Expected failures include both omitted/empty defaults,
**direct-handler** malformed-array refusal, IPC dispatch, PCB `kicad` guidance,
and the catalog. Served malformed-array schema refusal is already present at
the base and is expected to pass there. Supervisor lifecycle tests are harness
checks and are expected to pass on either base when attached.
Native acceptance against the base must fail on served IPC export or plotting.

For independent controls, change one guard at a time in a disposable validation
copy, never the integration tree, and retain its exact diff:

- Skip default selection in either PDF or SVG. Its served default test and
  native artifact acceptance must fail.
- Return table-order layers instead of drawing order. The structural and
  served default tests must fail.
- Restore string filtering in either plot handler. The **direct-handler**
  malformed-layer test must fail, with sentinel output and CLI-log checks.
  The served schema test is expected to keep passing and is not this control's
  oracle.
- Remove `layers` from either response. Default/explicit response tests fail.
- Route IPC aliases to the schematic helper. Dispatch and native partition
  acceptance fail.
- Let PCB `kicad` choose its sibling or reach CLI. The existing-sibling sentinel
  refusal test fails.
- Accept an empty layer list in the argument builder. Its direct guard test fails.
- In the supervisor only, replace process-group kill with leader-only kill.
  Lifecycle tests must fail because the descendant finishes naturally rather
  than reporting SIGKILL. Remove subreaper/reaping and the descendant status or
  `/proc` checks must fail. Run these mutations only against the finite synthetic
  lifecycle stubs, not real native exports.
- In the lifecycle tests only, configure each served handler with the finite
  stub path instead of the supervisor path, retaining direct supervision.
  The served-expiry and cancellation controls must fail.
- In a disposable validation copy only, set `kill_on_drop(true)` on production
  `cli::run_cli_captured`'s Command. The cancelled-served-call test must fail its
  finished-audit/reaping assertions because killing the supervisor bypasses
  cleanup. Changing only `native_output` does not exercise that served control.
  The five-second stub lifetime bounds these negative controls.

Restore and compare each validation copy before the next control. These are
required checks, not claims of results already obtained. Keep all three Kata
records open until review, native evidence and parent integration are complete.
