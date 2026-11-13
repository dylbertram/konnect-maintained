//! Bounded export regressions. Fake CLI checks prove dispatch/refusal, not KiCad
//! artifact correctness. The ignored native acceptance below supplies that gate.
use super::*;
use crate::mcp::handler::McpHandler;
use serde_json::Value;

const FRONT: [&str; 4] = ["F.Cu", "F.Mask", "F.SilkS", "Edge.Cuts"];
const BOARD: &str = include_str!("../../tests/fixtures/specctra_two_resistors.kicad_pcb");

async fn handler(cli: &str) -> McpHandler {
    McpHandler::new(crate::tools::ServerConfig {
        kicad_cli: cli.into(),
        ipc_address: String::new(),
        eager_toolsets: true,
        ..Default::default()
    })
    .await
    .unwrap()
}

async fn call(api: &McpHandler, name: &str, args: Value) -> (bool, Value) {
    let reply = api
        .handle_message(json!({
            "jsonrpc": "2.0", "id": 968, "method": "tools/call",
            "params": {"name": name, "arguments": args}
        }))
        .await
        .unwrap();
    assert!(reply.error.is_none(), "{name}: {:?}", reply.error);
    let result = reply.result.unwrap();
    let body = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    (result["isError"] == true, body)
}

struct Fixture {
    directory: tempfile::TempDir,
    board: PathBuf,
    log: PathBuf,
    executable: PathBuf,
    api: McpHandler,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let board = directory.path().join("board.kicad_pcb");
        std::fs::write(&board, BOARD).unwrap();
        let log = directory.path().join("argv.txt");
        let executable = cli::test_support::write_script(
            directory.path(),
            "export-cli",
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n\
                 while [ $# -gt 0 ]; do\n\
                 if [ \"$1\" = --output ]; then printf 'artifact\\n' > \"$2\"; fi\n\
                 shift\n\
                 done\n",
                log.display()
            ),
            &format!(
                "@echo off\r\ntype nul > \"{}\"\r\n\
                 :next\r\necho(%~1>> \"{}\"\r\n\
                 if \"%~1\"==\"--output\" (echo artifact> \"%~2\")\r\n\
                 shift\r\nif not \"%~1\"==\"\" goto next\r\nexit /b 0\r\n",
                log.display(),
                log.display()
            ),
        );
        let api = handler(executable.to_str().unwrap()).await;
        Self {
            directory,
            board,
            log,
            executable,
            api,
        }
    }

    fn output(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(|line| line.trim().to_string())
            .collect()
    }

    fn assert_board_unchanged(&self) {
        assert_eq!(std::fs::read_to_string(&self.board).unwrap(), BOARD);
    }
}

fn option<'a>(argv: &'a [String], name: &str) -> &'a str {
    let index = argv.iter().position(|arg| arg == name).unwrap();
    &argv[index + 1]
}

#[test]
fn front_view_is_filtered_and_drawn_in_order_not_layer_table_order() {
    let native = include_str!("../../tests/fixtures/six_layer_power_planes_kicad10.kicad_pcb");
    assert_eq!(front_view_layers(native).unwrap(), FRONT);
    assert_eq!(
        front_view_layers(
            r#"(kicad_pcb (layers (25 "Edge.Cuts" user) (5 "F.SilkS" user)
                (2 "B.Cu" signal) (0 "F.Cu" signal)))"#
        )
        .unwrap(),
        ["F.Cu", "F.SilkS", "Edge.Cuts"]
    );
    for source in [
        "(kicad_pcb)",
        "(kicad_pcb (layers))",
        r#"(kicad_pcb (layers (2 "B.Cu" signal)))"#,
        r#"(kicad_sch (layers (0 "F.Cu" signal)))"#,
        "(malformed",
    ] {
        assert!(front_view_layers(source).is_err(), "{source}");
    }
}

#[tokio::test]
async fn served_catalog_describes_front_view_and_supported_netlist_formats() {
    let api = handler("").await;
    let catalog = api
        .handle_message(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}))
        .await
        .unwrap()
        .result
        .unwrap();
    let tools = catalog["tools"].as_array().unwrap();
    for name in ["export_pdf", "export_svg"] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        let schema = &tool["inputSchema"];
        assert!(!schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("layers")));
        let description = schema["properties"]["layers"]["description"]
            .as_str()
            .unwrap();
        assert!(description.contains("F.Cu, F.Mask, F.SilkS, Edge.Cuts"));
        assert!(description.contains("saved board"));
        assert!(!description.contains("visible layers"));
    }
    let netlist = tools
        .iter()
        .find(|tool| tool["name"] == "export_netlist")
        .unwrap();
    assert_eq!(
        netlist["inputSchema"]["properties"]["format"]["enum"],
        json!(["kicad", "ipc", "ipcd356"])
    );
}

#[tokio::test]
async fn served_omitted_and_empty_layers_select_front_view_for_both_plotters() {
    for tool in ["export_pdf", "export_svg"] {
        for layers in [None, Some(json!([]))] {
            let fixture = Fixture::new().await;
            let output = fixture.output("plot");
            let mut args = json!({"board":fixture.board,"output":output});
            if let Some(layers) = layers {
                args["layers"] = layers;
            }
            let (failed, body) = call(&fixture.api, tool, args).await;
            assert!(!failed, "{tool}: {body}");
            assert_eq!(body["layers"], json!(FRONT));
            assert_eq!(body["output"], json!(output));
            assert!(output.is_file());
            let argv = fixture.argv();
            assert_eq!(option(&argv, "--layers"), "F.Cu,F.Mask,F.SilkS,Edge.Cuts");
            assert_eq!(argv.iter().filter(|arg| *arg == "--layers").count(), 1);
            assert!(argv.iter().any(|arg| arg == "--mode-single"));
            fixture.assert_board_unchanged();
        }
    }
}

#[tokio::test]
async fn served_explicit_layers_keep_order_and_monochrome_option() {
    for tool in ["export_pdf", "export_svg"] {
        let fixture = Fixture::new().await;
        let (failed, body) = call(
            &fixture.api,
            tool,
            json!({"board":fixture.board,"output":fixture.output("explicit"),
                "layers":["Edge.Cuts","B.Cu"],"black_and_white":true}),
        )
        .await;
        assert!(!failed, "{body}");
        assert_eq!(body["layers"], json!(["Edge.Cuts", "B.Cu"]));
        assert_eq!(body["black_and_white"], true);
        let argv = fixture.argv();
        assert_eq!(option(&argv, "--layers"), "Edge.Cuts,B.Cu");
        assert!(argv.iter().any(|arg| arg == "--black-and-white"));
        fixture.assert_board_unchanged();
    }
}

#[tokio::test]
async fn served_schema_rejects_malformed_layers_before_handler() {
    for tool in ["export_pdf", "export_svg"] {
        for (layers, field) in [
            (json!(null), "layers"),
            (json!("F.Cu"), "layers"),
            (json!({}), "layers"),
            (json!(["F.Cu", 7]), "layers[1]"),
            (json!([null]), "layers[0]"),
        ] {
            let fixture = Fixture::new().await;
            let output = fixture.output("sentinel");
            std::fs::write(&output, "keep").unwrap();
            let (failed, body) = call(
                &fixture.api,
                tool,
                json!({"board":fixture.board,"output":output,"layers":layers}),
            )
            .await;
            assert!(failed, "{tool}: {body}");
            assert_eq!(body["error"]["kind"], "invalid_argument");
            assert_eq!(body["error"]["field"], field);
            assert!(!fixture.log.exists(), "CLI must not run");
            assert_eq!(std::fs::read_to_string(output).unwrap(), "keep");
            fixture.assert_board_unchanged();
        }
    }
}

/// Bypass McpHandler's schema validator deliberately. Served calls alone cannot
/// detect a regression that restores filter_map inside either plot handler.
#[tokio::test]
async fn direct_handlers_reject_malformed_layers_without_cli_or_write() {
    for tool in ["export_pdf", "export_svg"] {
        for (layers, field) in [
            (json!(null), "layers"),
            (json!("F.Cu"), "layers"),
            (json!({}), "layers"),
            (json!(["F.Cu", 7]), "layers[1]"),
            (json!([null]), "layers[0]"),
        ] {
            let fixture = Fixture::new().await;
            let output = fixture.output("direct-sentinel");
            std::fs::write(&output, "keep").unwrap();
            let ctx = ToolContext::new(
                crate::tools::ServerConfig {
                    kicad_cli: fixture.executable.to_str().unwrap().into(),
                    ipc_address: String::new(),
                    ..Default::default()
                },
                std::sync::Arc::new(crate::router::ToolRouter::new()),
            );
            let args = json!({"board":fixture.board,"output":output,"layers":layers});
            let result = if tool == "export_pdf" {
                handle_export_pdf(&args, &ctx).await
            } else {
                handle_export_svg(&args, &ctx).await
            }
            .expect("malformed layers return a structured refusal");
            assert!(result.is_error, "{tool} must refuse malformed layers");
            let crate::mcp::protocol::ToolContent::Text { text } = &result.content[0] else {
                panic!("expected structured text error");
            };
            let body: Value = serde_json::from_str(text).unwrap();
            assert_eq!(body["error"]["kind"], "invalid_argument");
            assert_eq!(body["error"]["field"], field);
            assert!(!fixture.log.exists(), "handler must not invoke CLI");
            assert_eq!(std::fs::read_to_string(output).unwrap(), "keep");
            fixture.assert_board_unchanged();
        }
    }
}

#[tokio::test]
async fn served_missing_default_board_names_path_without_cli_or_output() {
    for tool in ["export_pdf", "export_svg"] {
        let fixture = Fixture::new().await;
        let missing = fixture.output("missing.kicad_pcb");
        let output = fixture.output("missing-output");
        let (failed, body) = call(
            &fixture.api,
            tool,
            json!({"board":missing,"output":output}),
        )
        .await;
        assert!(failed, "{body}");
        assert!(body.to_string().contains(missing.to_str().unwrap()));
        assert!(!fixture.log.exists());
        assert!(!output.exists());
    }
}

#[tokio::test]
async fn served_ipc_aliases_dispatch_to_pcb_ipcd356() {
    for format in ["ipc", "ipcd356"] {
        let fixture = Fixture::new().await;
        let output = fixture.output("board.ipc");
        let (failed, body) = call(
            &fixture.api,
            "export_netlist",
            json!({"board":fixture.board,"output":output,"format":format}),
        )
        .await;
        assert!(!failed, "{body}");
        assert_eq!(body["format"], format);
        assert_eq!(body["output"], json!(output));
        let argv = fixture.argv();
        assert_eq!(&argv[..3], ["pcb", "export", "ipcd356"]);
        assert_eq!(option(&argv, "--output"), output.to_str().unwrap());
        assert_eq!(argv.last().unwrap(), fixture.board.to_str().unwrap());
        assert!(output.is_file());
        fixture.assert_board_unchanged();
    }
}

#[tokio::test]
async fn served_unknown_netlist_formats_and_pcb_kicad_refuse_without_guessing() {
    for format in [
        None,
        Some(json!("kicad")),
        Some(json!("bogus")),
        Some(json!(null)),
        Some(json!(7)),
    ] {
        let fixture = Fixture::new().await;
        // A sibling exists deliberately. The PCB call must not silently use it.
        let sibling = fixture.board.with_extension("kicad_sch");
        std::fs::write(&sibling, "schematic sentinel").unwrap();
        let output = fixture.output("netlist");
        std::fs::write(&output, "output sentinel").unwrap();
        let mut args = json!({"board":fixture.board,"output":output});
        if let Some(value) = &format {
            args["format"] = value.clone();
        }
        let (failed, body) = call(&fixture.api, "export_netlist", args).await;
        assert!(failed, "{body}");
        assert_eq!(body["error"]["kind"], "invalid_argument");
        assert_eq!(body["error"]["field"], "format");
        if format.is_none() || format == Some(json!("kicad")) {
            assert!(body["message"].as_str().unwrap().contains("generate_netlist"));
            assert!(body["message"].as_str().unwrap().contains("No sibling"));
        }
        assert!(!fixture.log.exists());
        assert_eq!(std::fs::read_to_string(output).unwrap(), "output sentinel");
        assert_eq!(std::fs::read_to_string(sibling).unwrap(), "schematic sentinel");
        fixture.assert_board_unchanged();
    }
}

#[tokio::test]
async fn served_explicit_schematic_keeps_kicad_dispatch_but_refuses_ipc() {
    let fixture = Fixture::new().await;
    let schematic = fixture.board.with_extension("kicad_sch");
    std::fs::write(&schematic, "explicit schematic").unwrap();
    let output = fixture.output("schematic.net");
    let (failed, body) = call(
        &fixture.api,
        "export_netlist",
        json!({"board":schematic,"output":output}),
    )
    .await;
    assert!(!failed, "{body}");
    let argv = fixture.argv();
    assert_eq!(&argv[..3], ["sch", "export", "netlist"]);
    assert_eq!(option(&argv, "--format"), "kicadsexpr");
    assert_eq!(argv.last().unwrap(), schematic.to_str().unwrap());
    let previous_log = std::fs::read(&fixture.log).unwrap();
    let (failed, body) = call(
        &fixture.api,
        "export_netlist",
        json!({"board":schematic,"output":fixture.output("refused.ipc"),"format":"ipc"}),
    )
    .await;
    assert!(failed, "{body}");
    assert_eq!(body["error"]["field"], "board");
    assert_eq!(std::fs::read(&fixture.log).unwrap(), previous_log);
    assert!(!fixture.output("refused.ipc").exists());
    assert_eq!(
        std::fs::read_to_string(schematic).unwrap(),
        "explicit schematic"
    );
}

fn native_supervisor(directory: &Path, program: &str, seconds: u32) -> PathBuf {
    assert!(
        cfg!(target_os = "linux"),
        "native export acceptance supervision is supported on Linux only"
    );
    let wrapper = directory.join(format!("supervisor-{}.py", uuid::Uuid::new_v4()));
    std::fs::write(
        &wrapper,
        include_str!("../../tests/support/pcb_export_supervisor.py"),
    )
    .unwrap();
    std::fs::write(
        wrapper.with_extension("json"),
        serde_json::to_vec(&json!({
            "program": program,
            "seconds": seconds,
            "audit": wrapper.with_extension("audit"),
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    wrapper
}

/// `program` is always a native_supervisor wrapper, including Poppler controls.
/// Do not kill the supervisor when this future is dropped. It owns the native
/// process group's independent deadline and reaps it even after cancellation.
async fn native_output(program: &str, args: &[&str]) -> std::process::Output {
    tokio::process::Command::new(program)
        .args(args)
        .kill_on_drop(false)
        .output()
        .await
        .unwrap_or_else(|error| panic!("cannot execute {program}: {error}"))
}

async fn native_command(program: &str, args: &[&str]) -> std::process::Output {
    let output = native_output(program, args).await;
    assert!(
        output.status.success(),
        "{program} {args:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn hanging_native_stub(directory: &Path) -> PathBuf {
    let program = directory.join("hanging-native.py");
    std::fs::write(
        &program,
        "#!/usr/bin/env python3\n\
         import json, os, signal, subprocess, sys, time\n\
         from pathlib import Path\n\
         signal.signal(signal.SIGTERM, signal.SIG_IGN)\n\
         child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(5)'])\n\
         marker = Path(__file__).with_suffix('.pids.tmp')\n\
         marker.write_text(json.dumps([os.getpid(), child.pid]))\n\
         marker.replace(Path(__file__).with_suffix('.pids.json'))\n\
         time.sleep(5)\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    program
}

/// Observation only, not the process-lifetime mechanism. Even a failed control
/// stub self-exits after five seconds; the supervisor owns the two-second kill.
async fn wait_for_test_file(path: &Path) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no lifecycle evidence at {}", path.display()));
}

async fn assert_supervisor_cleanup(wrapper: &Path, stub: &Path) {
    let pids_path = stub.with_extension("pids.json");
    wait_for_test_file(&pids_path).await;
    let pids: Vec<u32> =
        serde_json::from_slice(&std::fs::read(&pids_path).unwrap()).unwrap();
    let audit = wrapper.with_extension("audit");
    let invocation = std::fs::read_dir(audit)
        .unwrap()
        .next()
        .expect("supervisor invocation")
        .unwrap()
        .path();
    let finished = invocation.join("finished.json");
    wait_for_test_file(&finished).await;
    let finished: Value =
        serde_json::from_slice(&std::fs::read(finished).unwrap()).unwrap();
    assert_eq!(finished["timed_out"], true);
    assert_eq!(finished["status"], -9, "native leader must be killed");
    assert_eq!(finished["group_reaped"], true);
    assert!(
        finished["descendants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| child["pid"] == pids[1] && child["status"] == -9),
        "native descendant must be killed and reaped, not left to finish naturally"
    );
    for pid in pids {
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "test-owned native process {pid} remains after supervisor cleanup"
        );
    }
    // A dropped tools/call future must not leave the supervisor running either.
    let supervisor = finished["supervisor"].as_u64().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while Path::new(&format!("/proc/{supervisor}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("supervisor must exit and be reaped after cleanup");
}

#[tokio::test]
#[ignore = "requires Linux and Python 3; coordinator must admit lifecycle subprocess tests"]
async fn native_supervisor_terminates_direct_and_served_children() {
    for served in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let stub = hanging_native_stub(directory.path());
        let wrapper = native_supervisor(directory.path(), stub.to_str().unwrap(), 2);
        if served {
            let api = handler(wrapper.to_str().unwrap()).await;
            let board = directory.path().join("board.kicad_pcb");
            std::fs::write(&board, BOARD).unwrap();
            let output = directory.path().join("refused.ipc");
            let (failed, body) = call(
                &api,
                "export_netlist",
                json!({"board":board,"output":output,"format":"ipc"}),
            )
            .await;
            assert!(failed, "{body}");
            assert!(body.to_string().contains("child runtime limit exceeded"));
            assert!(!output.exists());
            assert_eq!(std::fs::read_to_string(board).unwrap(), BOARD);
        } else {
            let result = native_output(wrapper.to_str().unwrap(), &[]).await;
            assert_eq!(result.status.code(), Some(124));
        }
        assert_supervisor_cleanup(&wrapper, &stub).await;
    }
}

#[tokio::test]
#[ignore = "requires Linux and Python 3; coordinator must admit lifecycle subprocess tests"]
async fn native_supervisor_survives_cancelled_served_call() {
    let directory = tempfile::tempdir().unwrap();
    let stub = hanging_native_stub(directory.path());
    let wrapper = native_supervisor(directory.path(), stub.to_str().unwrap(), 2);
    let api = handler(wrapper.to_str().unwrap()).await;
    let board = directory.path().join("board.kicad_pcb");
    std::fs::write(&board, BOARD).unwrap();
    let output = directory.path().join("cancelled.ipc");
    let mut request = Box::pin(call(
        &api,
        "export_netlist",
        json!({"board":board,"output":output,"format":"ipc"}),
    ));
    let pids = stub.with_extension("pids.json");
    tokio::select! {
        result = &mut request => panic!("expected a still-running native child: {result:?}"),
        _ = wait_for_test_file(&pids) => {}
    }
    drop(request);
    assert_supervisor_cleanup(&wrapper, &stub).await;
    assert!(!output.exists());
    assert_eq!(std::fs::read_to_string(board).unwrap(), BOARD);
}

/// IPC-D-356 has fixed-width fields. Ignore comments and coordinates, but require
/// the fixture's exact net/reference/pin partition, not merely a nonempty file.
fn ipc_partition(path: &Path) -> std::collections::BTreeSet<(String, String, String)> {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.lines().any(|line| line.trim() == "999"), "{text}");
    let mut records = std::collections::BTreeSet::new();
    // This fixture has four SMD pads, represented by KiCad's 327 records.
    for line in text.lines().filter(|line| line.starts_with("327")) {
        assert!(line.is_ascii() && line.len() >= 31, "{line}");
        assert_eq!(&line[26..27], "-", "{line}");
        assert!(records.insert((
            line[3..17].trim().to_string(),
            line[20..26].trim().to_string(),
            line[27..31].trim().to_string(),
        )));
    }
    records
}

fn expected_partition() -> std::collections::BTreeSet<(String, String, String)> {
    [
        ("VCC", "R1", "1"),
        ("GND", "R1", "2"),
        ("GND", "R2", "1"),
        ("VCC", "R2", "2"),
    ]
    .into_iter()
    .map(|(net, reference, pin)| (net.into(), reference.into(), pin.into()))
    .collect()
}

fn svg_pixels(path: &Path) -> Vec<u8> {
    let svg = std::fs::read(path).unwrap();
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).unwrap();
    assert!(!tree.root().children().is_empty(), "empty SVG drawing");
    konnect_render::svg_to_png(&svg, 1200).unwrap().png
}

async fn pdf_pixels(path: &Path, prefix: &Path) -> Vec<u8> {
    let info_wrapper = native_supervisor(
        prefix.parent().unwrap(),
        &std::env::var("KONNECT_TEST_PDFINFO").unwrap_or_else(|_| "pdfinfo".into()),
        60,
    );
    let info = native_command(
        info_wrapper.to_str().unwrap(),
        &[path.to_str().unwrap()],
    )
    .await;
    let info = String::from_utf8(info.stdout).unwrap();
    let pages = info
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .expect("pdfinfo must report the page count");
    assert_eq!(pages.trim(), "1", "{info}");
    let raster_wrapper = native_supervisor(
        prefix.parent().unwrap(),
        &std::env::var("KONNECT_TEST_PDFTOPPM").unwrap_or_else(|_| "pdftoppm".into()),
        60,
    );
    native_command(
        raster_wrapper.to_str().unwrap(),
        &[
            "-f",
            "1",
            "-singlefile",
            "-scale-to",
            "1200",
            "-png",
            path.to_str().unwrap(),
            prefix.to_str().unwrap(),
        ],
    )
    .await;
    let png = std::fs::read(prefix.with_extension("png")).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    png
}

/// Coordinator-owned admission only. This test fails when its native tools are
/// missing; it never silently skips, installs tools, upgrades CAD or opens PCB
/// Editor. All commands read disposable copies of committed native fixtures.
#[tokio::test]
#[ignore = "requires Linux, Python 3, KiCad 10 CLI and Poppler; coordinator must admit execution"]
async fn native_pcb_export_artifact_acceptance() {
    let real_cli = std::env::var("KONNECT_TEST_KICAD_CLI")
        .expect("KONNECT_TEST_KICAD_CLI must name the real KiCad 10 CLI");
    let directory = tempfile::tempdir().unwrap();
    let wrapper = native_supervisor(directory.path(), &real_cli, 60);
    let cli = wrapper.to_str().unwrap().to_string();
    let version = native_command(&cli, &["--version"]).await;
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(
        version.trim().starts_with("10."),
        "require KiCad 10: {version}"
    );
    eprintln!("Native PCB export acceptance: {}", version.trim());
    let board = directory.path().join("native.kicad_pcb");
    std::fs::write(&board, BOARD).unwrap();
    let api = handler(&cli).await;

    let control_ipc = directory.path().join("control.ipc");
    native_command(
        &cli,
        &[
            "pcb",
            "export",
            "ipcd356",
            "--output",
            control_ipc.to_str().unwrap(),
            board.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(ipc_partition(&control_ipc), expected_partition());
    for format in ["ipc", "ipcd356"] {
        let output = directory.path().join(format!("{format}.ipc"));
        let (failed, body) = call(
            &api,
            "export_netlist",
            json!({"board":board,"output":output,"format":format}),
        )
        .await;
        assert!(!failed, "{body}");
        assert_eq!(ipc_partition(&output), expected_partition());
    }

    for format in ["svg", "pdf"] {
        // Baseline CLI refusal behind the original omitted-layers bug.
        let refused = directory.path().join(format!("no-layers.{format}"));
        let refusal = native_output(
            &cli,
            &[
                "pcb",
                "export",
                format,
                "--output",
                refused.to_str().unwrap(),
                "--mode-single",
                board.to_str().unwrap(),
            ],
        )
        .await;
        assert!(!refusal.status.success());
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&refusal.stdout),
            String::from_utf8_lossy(&refusal.stderr)
        );
        assert!(diagnostics.contains("At least one layer"), "{diagnostics}");
        assert!(
            !refused.exists(),
            "CLI refusal unexpectedly wrote an artifact"
        );
        let control = directory.path().join(format!("control.{format}"));
        native_command(
            &cli,
            &[
                "pcb",
                "export",
                format,
                "--output",
                control.to_str().unwrap(),
                "--mode-single",
                "--layers",
                "F.Cu,F.Mask,F.SilkS,Edge.Cuts",
                board.to_str().unwrap(),
            ],
        )
        .await;
        let control_pixels = if format == "svg" {
            svg_pixels(&control)
        } else {
            pdf_pixels(&control, &directory.path().join("control-pdf")).await
        };
        for (label, layers) in [("omitted", None), ("empty", Some(json!([])))] {
            let output = directory.path().join(format!("{label}.{format}"));
            let mut args = json!({"board":board,"output":output});
            if let Some(layers) = layers {
                args["layers"] = layers;
            }
            let (failed, body) = call(&api, &format!("export_{format}"), args).await;
            assert!(!failed, "{body}");
            assert_eq!(body["layers"], json!(FRONT));
            assert_eq!(body["output"], json!(output));
            let pixels = if format == "svg" {
                svg_pixels(&output)
            } else {
                pdf_pixels(&output, &directory.path().join(format!("{label}-pdf"))).await
            };
            assert!(pixels == control_pixels, "{format} {label} view differs");
        }
    }

    // A successful but wrong copper-only SVG cannot satisfy the visual oracle.
    let copper = directory.path().join("copper-only.svg");
    let (failed, body) = call(
        &api,
        "export_svg",
        json!({"board":board,"output":copper,"layers":["F.Cu"]}),
    )
    .await;
    assert!(!failed, "{body}");
    assert!(
        svg_pixels(&copper) != svg_pixels(&directory.path().join("control.svg")),
        "fixture must distinguish copper-only from the documented front view"
    );

    // Keep the native schematic's project name so its instance entries resolve.
    let schematic = directory.path().join("component_fields_kicad10.kicad_sch");
    let source = include_str!("../../tests/fixtures/component_fields_kicad10.kicad_sch");
    std::fs::write(&schematic, source).unwrap();
    let output = directory.path().join("schematic.net");
    let (failed, body) = call(
        &api,
        "export_netlist",
        json!({"board":schematic,"output":output,"format":"kicad"}),
    )
    .await;
    assert!(!failed, "{body}");
    let netlist = std::fs::read_to_string(output).unwrap();
    let netlist = konnect_sexp::parser::parse_sexp(&netlist).unwrap();
    assert_eq!(netlist.head(), Some("export"));
    assert!(!netlist.find("components").unwrap().find_all("comp").is_empty());
    assert_eq!(std::fs::read_to_string(schematic).unwrap(), source);
    assert_eq!(std::fs::read_to_string(board).unwrap(), BOARD);
}
