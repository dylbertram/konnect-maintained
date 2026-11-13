The `project` toolset provides `get_editor_shutdown_target` and
`shutdown_editor` for headless PCB Editor shutdown.

## Supported mechanism

This is a narrow compatibility path for **Linux KiCad 10.0.8** with the legacy
Python ActionPlugin installed. Other versions/platforms and schematic editors
are unsupported. KiCad 10's typed IPC has neither a close command nor a dirty
state query. Konnect does not substitute `RunAction`, desktop input, X window
destruction, signals or process-name matching.

The authenticated loopback native bridge runs one callback on KiCad's UI thread.
It verifies the exact canonical board path, one `PcbFrame`, no other frames or
dialogs, an enabled frame, and KiCad's clean native title before calling
`frame.Close(False)`. This invokes KiCad's vetoable application close handler.
It never calls `Destroy` and never changes the dirty flag.

The version pin matters. In KiCad 10.0.8,
[`PCB_EDIT_FRAME::UpdateTitle` and `OnModify`](https://github.com/KiCad/kicad-source-mirror/blob/10.0.8/pcbnew/pcb_edit_frame.cpp)
synchronously mirror `IsContentModified()` with a leading `*`. The same file's
`canCloseWindow` checks that dirty state before prompting. Konnect checks the
title and closes in one UI turn without yielding between them. The
[`PcbFrame` name](https://github.com/KiCad/kicad-source-mirror/blob/10.0.8/include/eda_draw_frame.h)
is native frame identity, not a localized window-title search. Requalify these
contracts and native tests before expanding the supported version.

## Enable and use

Set `"native_editor_shutdown": true` in the legacy plugin's `settings.json`
and reload the plugin or close its settings dialog. It defaults to disabled.
It is independent of `"native_specctra_bridge"`; enabling shutdown does not
enable native DSN export. Headless fixtures can start `NativeSpecctraBridge`
with `allow_editor_shutdown=True`. The bridge is not a general Python execution
service.

Run Konnect as an **external** stdio or HTTP server. A server started by the
plugin is terminated by the plugin's exit cleanup. An embedded server would
also lose its runtime and transport on editor exit. Konnect refuses to close
the editor that contains or owns the requesting MCP server. Closing an editor is not
`stop_server`, bridge shutdown, or stopping a cdylib runtime.

1. Call `get_editor_shutdown_target` with the exact `expected_board` path.
2. Pass its `target` object unchanged to `shutdown_editor`.
3. Inspect `status`. Only `closed` proves the observed process generation
   exited. `already_closed` means that generation is no longer running and
   performs no mutation. Missing sockets or registrations alone never count as
   closure.

The target includes a registration session ID, canonical board path, PID and
Linux `/proc` start ticks. Native board identity is rechecked immediately before
close. Konnect observes that specific process generation exiting, including a
zombie awaiting its parent's reap; a recycled PID does not name the old editor.
Unrelated registrations, processes and projects are not closed.

## Save policy and outcomes

Shutdown never saves or discards work. A dirty or unsaved board returns
`save_required` before the native close handler runs. Save the exact intended
board explicitly through your normal verified save workflow, then observe a new
target. There is no implicit save-and-close or discard option.

Errors use the structured `editor_shutdown` kind with `status` and `reason`.
Statuses include `unsupported`, `save_required`, `wrong_target`, `refused`,
`timed_out` and `uncertain`. Discovery failures include diagnostic reasons.
A changed board, busy bridge, unexpected title, modal dialog or ancillary frame
fails closed. An already-closed target is safe to submit again.

Discovery has a 35-second total deadline. Each native HTTP request has a
32-second deadline, including at most 30 seconds waiting for the UI thread.
An expired queued UI callback cannot close an editor later.
`timeout_seconds`, default 10 and range 1 to 60, bounds process-exit verification
after the close request. Shutdown performs a fresh read-only observation first,
so its total bound is two HTTP deadlines plus that verification interval.
If the editor exits before replying, transport loss is expected; process
generation readback still determines success. If it stays alive, Konnect reports
timeout or uncertainty and does not retry, kill, or dismiss a dialog. Inspect
the editor before retrying.

## Verification

`plugin/tests/test_native_bridge.py` covers dirty/wrong-target guards, native
veto, unsupported versions, server ownership and expired queued callbacks.
The headless `transfer_native_editor_shutdown_*` tests use isolated real
PCB Editors under Xvfb. They cover dirty refusal without file writes,
wrong-target protection, clean shutdown, already-closed retries, native veto,
an accepted-but-unclosed request timeout, and preservation of a second editor.
The existing-PCB transfer acceptance test uses these public tools instead of
`xdotool`.
