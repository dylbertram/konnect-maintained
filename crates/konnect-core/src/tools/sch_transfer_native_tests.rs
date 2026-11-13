//! Opt-in acceptance against installed KiCad, using only synthetic projects.
use super::*;
use crate::mcp::handler::McpHandler;

async fn handler(cli: &str, socket: &str) -> McpHandler {
    McpHandler::new(crate::tools::ServerConfig {
        kicad_cli: cli.into(),
        ipc_address: socket.into(),
        eager_toolsets: true,
        ..Default::default()
    })
    .await
    .unwrap()
}

async fn call(handler: &McpHandler, name: &str, args: Value) -> Value {
    let reply = handler
        .handle_message(json!({
            "jsonrpc":"2.0", "id":1, "method":"tools/call",
            "params":{"name":name,"arguments":args}
        }))
        .await
        .unwrap();
    assert!(reply.error.is_none(), "{name}: {:?}", reply.error);
    let result = reply.result.unwrap();
    assert_ne!(result["isError"], true, "{name}: {result:#}");
    let text = result["content"][0]["text"].as_str().unwrap();
    if name == "save_project" {
        assert_eq!(text, "Board saved successfully.");
        return json!(text);
    }
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{name}: {error}; response: {text}"))
}

struct CircuitGroup {
    source: PathBuf,
    destination: PathBuf,
    source_instance: String,
    destination_instance: String,
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    groups: Vec<CircuitGroup>,
}

fn pin(path: &Path, reference: &str, number: &str) -> (f64, f64) {
    let tree = parse_sexp(&read_consistent(path).unwrap()).unwrap();
    let pins = crate::tools::placed_pins_by_reference(&tree);
    let (_, placed) = pins
        .iter()
        .find(|(symbol, _)| symbol.reference == reference)
        .unwrap();
    let (pin, transform) = placed.iter().find(|(pin, _)| pin.number == number).unwrap();
    konnect_sexp::schematic::pin_endpoint(pin, *transform)
}

async fn fixture(api: &McpHandler) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    call(
        api,
        "create_project",
        json!({"path":directory.path(),"name":"transfer_acceptance"}),
    )
    .await;
    let root = directory.path().join("transfer_acceptance.kicad_sch");
    for name in [
        "pd-startup-limit",
        "pd-controller",
        "control-fuse-bypass",
        "logic-regulator",
    ] {
        call(
            api,
            "add_hierarchical_sheet",
            json!({"schematic":root,"sheet_file":format!("{name}.kicad_sch"),"sheet_name":name}),
        )
        .await;
    }
    call(api,"batch_place_components",json!({"schematic":root,"components":[
        {"lib_id":"Device:R","reference":"R901","x":200.66,"y":149.86,"footprint":"Resistor_SMD:R_0603_1608Metric"}
    ]})).await;
    let tree = parse_sexp(&read_consistent(&root).unwrap()).unwrap();
    let root_uuid = tree.find_str("uuid").unwrap();
    let instance = |name: &str| {
        let sheet = tree
            .find_all("sheet")
            .into_iter()
            .find(|sheet| property(sheet, "Sheetname") == Some(name))
            .unwrap();
        format!("/{root_uuid}/{}", sheet.find_str("uuid").unwrap())
    };
    let groups = [
        ("pd-startup-limit", "pd-controller"),
        ("control-fuse-bypass", "logic-regulator"),
    ]
    .into_iter()
    .map(|(source, destination)| CircuitGroup {
        source: directory.path().join(format!("{source}.kicad_sch")),
        destination: directory.path().join(format!("{destination}.kicad_sch")),
        source_instance: instance(source),
        destination_instance: instance(destination),
    })
    .collect::<Vec<_>>();
    for (group, lib_id, reference, x, y) in [
        (
            &groups[0],
            "Interface_USB:STUSB4500QTR",
            "U201",
            139.7,
            101.6,
        ),
        (
            &groups[1],
            "Regulator_Linear:AP2112K-3.3",
            "U202",
            180.34,
            119.38,
        ),
    ] {
        call(
            api,
            "batch_place_components",
            json!({"schematic":group.destination,"components":[
                {"lib_id":lib_id,"reference":reference,"x":x,"y":y,"footprint":""}
            ]}),
        )
        .await;
    }
    call(api,"batch_place_components",json!({"schematic":groups[0].source,"components":[
        {"lib_id":"Device:R","reference":"R101","x":50.8,"y":50.8,"footprint":"Resistor_SMD:R_0603_1608Metric"},
        {"lib_id":"Device:R","reference":"R102","x":50.8,"y":76.2,"footprint":"Resistor_SMD:R_0603_1608Metric"}
    ]})).await;
    call(
        api,
        "connect_pins",
        json!({"schematic":groups[0].source,"ref1":"R101","pin1":"2","ref2":"R102","pin2":"1"}),
    )
    .await;
    let (x, y) = pin(&groups[0].source, "R101", "1");
    call(
        api,
        "add_no_connect",
        json!({"schematic":groups[0].source,"x":x,"y":y}),
    )
    .await;
    call(api,"connect_to_net",json!({"schematic":groups[0].source,"reference":"R102","pin_number":"2","net":"STARTUP_RETURN","label_type":"global_label"})).await;
    let a = pin(&groups[0].source, "R101", "2");
    let b = pin(&groups[0].source, "R102", "1");
    let midpoint = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    call(
        api,
        "add_junction",
        json!({"schematic":groups[0].source,"x":midpoint.0,"y":midpoint.1}),
    )
    .await;
    call(
        api,
        "add_schematic_net_label",
        json!({"schematic":groups[0].source,"x":midpoint.0,"y":midpoint.1,"net":"STARTUP_LIMIT"}),
    )
    .await;
    call(api,"batch_place_components",json!({"schematic":groups[1].source,"components":[
        {"lib_id":"Device:Fuse","reference":"F101","x":50.8,"y":50.8,"footprint":"Resistor_SMD:R_0603_1608Metric"},
        {"lib_id":"Jumper:SolderJumper_2_Open","reference":"JP101","x":76.2,"y":50.8,"rotation":90,"footprint":"Resistor_SMD:R_0603_1608Metric"}
    ]})).await;
    for number in ["1", "2"] {
        call(api,"connect_pins",json!({"schematic":groups[1].source,"ref1":"F101","pin1":number,"ref2":"JP101","pin2":number})).await;
        call(api,"connect_to_net",json!({"schematic":groups[1].source,"reference":"F101","pin_number":number,
            "net":if number=="1" {"CONTROL_INPUT"} else {"CONTROL_OUTPUT"},"label_type":"global_label"})).await;
    }
    call(api,"batch_edit_schematic_components",json!({"schematic":groups[1].source,"create_missing":true,"edits":[
        {"reference":"JP101","dnp":true,"fields":{"MPN":"native-transfer-fixture","LCSC":"fixture-not-for-ordering"}}
    ]})).await;
    for group in &groups {
        call(
            api,
            "add_schematic_text",
            json!({"schematic":group.source,"text":"Consolidation fixture","x":40.64,"y":30.48}),
        )
        .await;
    }
    Fixture {
        _directory: directory,
        root,
        groups,
    }
}

fn project_bytes(fixture: &Fixture) -> BTreeMap<PathBuf, String> {
    let mut paths = vec![
        fixture.root.clone(),
        fixture.root.with_extension("kicad_pro"),
        fixture.root.with_extension("kicad_pcb"),
    ];
    for group in &fixture.groups {
        paths.extend([group.source.clone(), group.destination.clone()]);
    }
    paths
        .into_iter()
        .map(|path| {
            let source = read_consistent(&path).unwrap();
            (path, source)
        })
        .collect()
}

async fn consolidate(api: &McpHandler, fixture: &Fixture) -> Vec<Value> {
    let mut reports = Vec::new();
    for group in &fixture.groups {
        let before = project_bytes(fixture);
        let objects = object_blocks(&before[&group.source]).unwrap();
        let args = json!({"schematic":fixture.root,"source_sheet_instance":group.source_instance,
            "destination_sheet_instance":group.destination_instance,"object_uuids":objects.keys().collect::<Vec<_>>(),
            "offset_x_mm":25.4,"offset_y_mm":12.7,"dry_run":true});
        let planned = call(api, "move_schematic_objects_between_sheets", args.clone()).await;
        assert_eq!(planned["status"], "planned");
        assert_eq!(
            before,
            project_bytes(fixture),
            "dry run changed project files"
        );
        let mut apply = args;
        apply["dry_run"] = json!(false);
        let moved = call(api, "move_schematic_objects_between_sheets", apply).await;
        assert_eq!(moved["status"], "moved");
        assert_eq!(moved["path_mapping"].as_object().unwrap().len(), 2);
        let remaining = object_blocks(&read_consistent(&group.source).unwrap()).unwrap();
        assert!(remaining.is_empty(), "source circuit objects remain");
        let after = read_consistent(&group.destination).unwrap();
        let destination_objects = object_blocks(&after).unwrap();
        for (uuid, (start, end)) in objects {
            let old = parse_sexp(&before[&group.source][start..end]).unwrap();
            let (start, end) = destination_objects[&uuid];
            let new = parse_sexp(&after[start..end]).unwrap();
            let mut old_ids = BTreeSet::new();
            let mut new_ids = BTreeSet::new();
            collect_uuids(&old, &mut old_ids).unwrap();
            collect_uuids(&new, &mut new_ids).unwrap();
            assert_eq!(old_ids, new_ids);
            if old.head() == Some("symbol") {
                for field in ["Reference", "Value", "Footprint", "MPN", "LCSC"] {
                    assert_eq!(property(&old, field), property(&new, field));
                }
                for field in ["unit", "in_bom", "on_board", "dnp"] {
                    assert_eq!(old.find_str(field), new.find_str(field));
                }
                let a = old.find("at").unwrap();
                let b = new.find("at").unwrap();
                assert!((b.get_f64(1).unwrap() - a.get_f64(1).unwrap() - 25.4).abs() < 0.000001);
                assert!((b.get_f64(2).unwrap() - a.get_f64(2).unwrap() - 12.7).abs() < 0.000001);
            }
        }
        assert_eq!(
            before[&fixture.root],
            read_consistent(&fixture.root).unwrap(),
            "hierarchy changed"
        );
        reports.push(moved);
    }
    reports
}

#[tokio::test]
#[ignore = "requires installed KiCad 10 CLI and symbol libraries"]
async fn transfer_native_public_api_consolidates_both_circuit_groups() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").expect("KONNECT_TEST_KICAD_CLI is required");
    let api = handler(&cli, "").await;
    let fixture = fixture(&api).await;
    let export = fixture._directory.path().join("before.net");
    cli::export_netlist(&cli, &fixture.root, &export, "kicadsexpr")
        .await
        .unwrap();
    let before = netlist_partitions(&std::fs::read_to_string(&export).unwrap()).unwrap();
    let reports = consolidate(&api, &fixture).await;
    assert_eq!(reports.len(), 2);
    assert!(reports[0]["connectivity_impacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|impact| impact["old_name"]
            .as_str()
            .unwrap()
            .ends_with("/STARTUP_LIMIT")));
    let export = fixture._directory.path().join("after.net");
    cli::export_netlist(&cli, &fixture.root, &export, "kicadsexpr")
        .await
        .unwrap();
    assert_eq!(
        before,
        netlist_partitions(&std::fs::read_to_string(export).unwrap()).unwrap()
    );
    println!("Native public API: startup limiter and fuse/bypass consolidation preserve identities, geometry, connectivity and ERC");
}

#[tokio::test]
#[ignore = "requires installed KiCad 10 CLI and symbol libraries"]
async fn repair_native_public_api_restores_instance_roots_without_circuit_changes() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").expect("KONNECT_TEST_KICAD_CLI is required");
    let api = handler(&cli, "").await;
    let fixture = fixture(&api).await;
    let baseline = project_bytes(&fixture);
    let root = parse_sexp(&baseline[&fixture.root])
        .unwrap()
        .find_str("uuid")
        .unwrap()
        .to_owned();
    let old = "cd385c7a-53db-42a0-815e-a24ef952404d";
    for (path, source) in &baseline {
        if path.extension().is_some_and(|ext| ext == "kicad_sch") {
            std::fs::write(
                path,
                source.replace(&format!("\"/{root}"), &format!("\"/{old}")),
            )
            .unwrap();
        }
    }
    let stale = project_bytes(&fixture);
    let args = json!({"schematic":fixture.root,"old_root_uuid":old,"dry_run":true});
    let planned = call(&api, "repair_schematic_instance_roots", args.clone()).await;
    assert_eq!(planned["status"], "planned");
    assert_eq!(planned["canonical_root_uuid"], root);
    assert_eq!(planned["symbol_instance_records"], 7);
    assert_eq!(planned["sheet_instance_records"], 4);
    assert_eq!(
        stale,
        project_bytes(&fixture),
        "repair dry-run changed a project file"
    );
    let mut apply = args;
    apply["dry_run"] = json!(false);
    apply["expected_plan_revision"] = planned["plan_revision"].clone();
    let repaired = call(&api, "repair_schematic_instance_roots", apply).await;
    assert_eq!(repaired["status"], "repaired");
    assert_eq!(
        baseline,
        project_bytes(&fixture),
        "repair changed bytes beyond the stale path prefixes"
    );
    let noop = call(
        &api,
        "repair_schematic_instance_roots",
        json!({
            "schematic":fixture.root,"old_root_uuid":old
        }),
    )
    .await;
    assert_eq!(noop["status"], "noop");
    consolidate(&api, &fixture).await;
    println!("Native instance-root repair: exact original bytes restored; canonical root and circuit identities preserved; transfer dry-run/apply works afterward in the synthetic fixture");
}

fn native_board(mode: &str, board: &Path, spec: Option<&Path>) -> Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/native_transfer_board.py");
    let mut command = std::process::Command::new("/usr/bin/python3");
    command.arg(script).arg(mode).arg(board);
    if let Some(spec) = spec {
        command.arg(spec);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "Native board {mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

async fn seed_board(cli: &str, fixture: &Fixture) -> Value {
    let export = fixture._directory.path().join("board-before.net");
    cli::export_netlist(cli, &fixture.root, &export, "kicadsexpr")
        .await
        .unwrap();
    let tree = parse_sexp(&std::fs::read_to_string(export).unwrap()).unwrap();
    let mut nets = BTreeSet::new();
    let mut pins: BTreeMap<(String, String), String> = BTreeMap::new();
    for net in tree.find("nets").unwrap().find_all("net") {
        let name = net.find_str("name").unwrap();
        nets.insert(name);
        for node in net.find_all("node") {
            pins.insert(
                (
                    node.find_str("ref").unwrap().into(),
                    node.find_str("pin").unwrap().into(),
                ),
                name.into(),
            );
        }
    }
    let components=tree.find("components").unwrap().find_all("comp").into_iter().filter_map(|component| {
        let footprint=component.find_str("footprint")?;
        let reference=component.find_str("ref").unwrap();
        let stamp=component.find_str("tstamps").unwrap().trim_matches('/');
        let sheet=component.find("sheetpath").unwrap().find_str("tstamps").unwrap().trim_matches('/');
        let path=format!("/{sheet}/{stamp}").replace("//","/");
        let pads=pins.iter().filter(|((owner,_),_)|owner==reference).map(|((_,pin),net)|(pin.clone(),net.clone())).collect::<BTreeMap<_,_>>();
        Some(json!({"reference":reference,"value":component.find_str("value").unwrap(),"footprint":footprint,"path":path,"pads":pads}))
    }).collect::<Vec<_>>();
    let specification = fixture._directory.path().join("board-specification.json");
    std::fs::write(
        &specification,
        serde_json::to_vec(&json!({"components":components,"nets":nets})).unwrap(),
    )
    .unwrap();
    native_board(
        "seed",
        &fixture.root.with_extension("kicad_pcb"),
        Some(&specification),
    )
}

struct Editor {
    child: std::process::Child,
    _control: tempfile::TempDir,
    socket: String,
    log: PathBuf,
    board: PathBuf,
}

impl Editor {
    async fn open(board: &Path) -> Self {
        Self::open_with_close_mode(board, "").await
    }

    async fn open_with_close_mode(board: &Path, mode: &str) -> Self {
        use std::process::{Command, Stdio};
        let bridge_root = std::env::var_os("KONNECT_BRIDGE_DIR")
            .expect("native editor tests require an isolated KONNECT_BRIDGE_DIR");
        let control = tempfile::Builder::new()
            .prefix("kt-")
            .tempdir_in("/tmp")
            .unwrap();
        let config = control.path().join("config");
        let versioned = config.join("10.0");
        std::fs::create_dir_all(&versioned).unwrap();
        std::fs::write(
            versioned.join("kicad_common.json"),
            serde_json::to_vec(&json!({
                "meta":{"version":6},"api":{"enable_server":true},"graphics":{"canvas_type":2},
                "do_not_show_again":{"data_collection_prompt":true,"update_check_prompt":true}
            }))
            .unwrap(),
        )
        .unwrap();
        for name in ["sym-lib-table", "fp-lib-table"] {
            std::fs::copy(
                Path::new("/usr/share/kicad/template").join(name),
                versioned.join(name),
            )
            .unwrap();
        }
        // KiCad 10's first-run wizard validates all three library tables,
        // including design blocks even when only the PCB editor is used.
        std::fs::write(
            versioned.join("design-block-lib-table"),
            "(design_block_lib_table)\n",
        )
        .unwrap();
        let plugins = versioned.join("scripting/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/native_bridge.py"),
            plugins.join("native_bridge.py"),
        )
        .unwrap();
        std::fs::write(
            plugins.join("zz_shutdown_fixture.py"),
            "import atexit\nimport pcbnew\nimport wx\n\
             from native_bridge import NativeSpecctraBridge\n\
             bridge = NativeSpecctraBridge(pcbnew, wx, allow_editor_shutdown=True)\n\
             bridge.start()\natexit.register(bridge.stop)\n\
             import os\n\
             mode = os.environ.get('KONNECT_TEST_CLOSE_MODE', '')\n\
             original = bridge.editor_operation\n\
             injected = [False]\n\
             def operation(board, close, requester_pid=None):\n\
                 \x20   if close and mode and not injected[0]:\n\
                 \x20       injected[0] = True\n\
                 \x20       frame = next(w for w in wx.GetTopLevelWindows() if w.GetName() == 'PcbFrame')\n\
                 \x20       def intercept(event):\n\
                 \x20           frame.Unbind(wx.EVT_CLOSE, handler=intercept)\n\
                 \x20           if mode == 'veto':\n\
                 \x20               event.Veto()\n\
                 \x20       frame.Bind(wx.EVT_CLOSE, intercept)\n\
                 \x20   return original(board, close, requester_pid)\n\
             bridge.editor_operation = operation\n",
        )
        .unwrap();
        let log = control.path().join("pcbnew.log");
        let stdout = std::fs::File::create(&log).unwrap();
        let stderr = stdout.try_clone().unwrap();
        let child = Command::new("pcbnew")
            .arg(board)
            .env("KICAD_CONFIG_HOME", &config)
            .env("TMPDIR", control.path())
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("KONNECT_TEST_CLOSE_MODE", mode)
            .env("KONNECT_BRIDGE_DIR", bridge_root)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap();
        let socket = format!("ipc://{}/kicad/api.sock", control.path().display());
        let mut editor = Self {
            child,
            _control: control,
            socket,
            log,
            board: board.to_path_buf(),
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
        loop {
            let client = konnect_ipc::KiCadIpcClient::new(&editor.socket);
            let ping = client.ping();
            let footprints = client.list_footprints();
            if ping.as_ref().is_ok_and(|ready| *ready)
                && footprints.as_ref().is_ok_and(|items| items.len() == 5)
            {
                return editor;
            }
            assert!(
                editor.child.try_wait().unwrap().is_none(),
                "PCB editor exited: {}",
                std::fs::read_to_string(&editor.log).unwrap()
            );
            assert!(
                std::time::Instant::now() < deadline,
                "PCB editor was not ready: ping={ping:?}, footprints={footprints:?}, log={}",
                std::fs::read_to_string(&editor.log).unwrap()
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    async fn close(mut self) {
        let api = handler("", &self.socket).await;
        let target = call(
            &api,
            "get_editor_shutdown_target",
            json!({"expected_board":self.board}),
        )
        .await["target"]
            .clone();
        let result = call(
            &api,
            "shutdown_editor",
            json!({"target":target,"timeout_seconds":15}),
        )
        .await;
        assert_eq!(result["status"], "closed");
        let repeated = call(&api, "shutdown_editor", json!({"target":target})).await;
        assert_eq!(repeated["status"], "already_closed");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Test editor did not close cleanly"
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        // Only a disposable test editor owned by this test. A panic must not
        // leave it running beyond the remote command's lifetime.
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[tokio::test]
#[ignore = "requires Linux KiCad 10.0.8, pcbnew Python and Xvfb DISPLAY"]
async fn transfer_native_existing_pcb_update_preserves_footprints_and_routing() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").expect("KONNECT_TEST_KICAD_CLI is required");
    assert!(
        std::env::var_os("DISPLAY").is_some(),
        "run native IPC tests under xvfb-run"
    );
    let api = handler(&cli, "").await;
    let fixture = fixture(&api).await;
    let before = seed_board(&cli, &fixture).await;
    assert_eq!(before["footprints"].as_object().unwrap().len(), 5);
    assert!(!before["tracks"].as_object().unwrap().is_empty());
    let reports = consolidate(&api, &fixture).await;
    let editor = Editor::open(&fixture.root.with_extension("kicad_pcb")).await;
    let live = handler(&cli, &editor.socket).await;
    let planned=call(&live,"update_pcb_from_schematic",json!({"schematic":fixture.root,"board":fixture.root.with_extension("kicad_pcb"),"dry_run":true})).await;
    assert!(
        matches!(planned["status"].as_str(), Some("ready" | "noop")),
        "{planned:#}"
    );
    assert_eq!(planned["coverage"]["footprints_added"]["planned"], 0);
    let applied=call(&live,"update_pcb_from_schematic",json!({"schematic":fixture.root,"board":fixture.root.with_extension("kicad_pcb"),"dry_run":false,"expected_plan_revision":planned["plan_revision"]})).await;
    assert!(
        matches!(applied["status"].as_str(), Some("applied" | "noop")),
        "{applied:#}"
    );
    call(&live, "save_project", json!({})).await;
    editor.close().await;
    let after = native_board("inspect", &fixture.root.with_extension("kicad_pcb"), None);
    let mut expected = before;
    for report in reports {
        for (old, new) in report["pcb_path_mapping"].as_object().unwrap() {
            for footprint in expected["footprints"].as_object_mut().unwrap().values_mut() {
                if footprint["path"] == *old {
                    footprint["path"] = new.clone();
                }
            }
        }
        for change in report["connectivity_impacts"].as_array().unwrap() {
            for footprint in expected["footprints"].as_object_mut().unwrap().values_mut() {
                for pad in footprint["pads"].as_object_mut().unwrap().values_mut() {
                    if pad["net"] == change["old_name"] {
                        pad["net"] = change["new_name"].clone();
                    }
                }
            }
            for track in expected["tracks"].as_object_mut().unwrap().values_mut() {
                if track["net"] == change["old_name"] {
                    track["net"] = change["new_name"].clone();
                }
            }
        }
    }
    assert_eq!(expected,after,"PCB update replaced, duplicated or moved an existing object, or changed routed connectivity");
    println!("Native PCB update: all footprint/pad/track UUIDs, placement, orientation, layers and routing retained");
}

#[tokio::test]
#[ignore = "requires Linux KiCad 10.0.8, pcbnew Python and Xvfb DISPLAY"]
async fn transfer_native_editor_shutdown_refuses_unsaved_and_wrong_target() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").unwrap();
    let api = handler(&cli, "").await;
    let fixture = fixture(&api).await;
    seed_board(&cli, &fixture).await;
    let editor = Editor::open(&fixture.root.with_extension("kicad_pcb")).await;
    let api = handler(&cli, &editor.socket).await;
    let target = call(
        &api,
        "get_editor_shutdown_target",
        json!({"expected_board":editor.board}),
    )
    .await["target"]
        .clone();
    let wrong_board = fixture.root.with_file_name("wrong.kicad_pcb");
    std::fs::copy(&editor.board, &wrong_board).unwrap();
    let before = std::fs::read(&editor.board).unwrap();
    for (board, expected) in [
        (wrong_board, "wrong_target"),
        (editor.board.clone(), "save_required"),
    ] {
        if expected == "save_required" {
            konnect_ipc::KiCadIpcClient::new(&editor.socket)
                .run_commit("shutdown dirty-state acceptance", |client| {
                    client.move_footprint("R101", 80.0, 60.0)
                })
                .unwrap();
        }
        let mut requested = target.clone();
        requested["board_path"] = json!(board);
        let reply = api
            .handle_message(json!({
                "jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"shutdown_editor","arguments":{"target":requested}}
            }))
            .await
            .unwrap()
            .result
            .unwrap();
        assert_eq!(reply["isError"], true, "{reply:#}");
        let text = reply["content"][0]["text"].as_str().unwrap();
        let value: Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["error"]["status"], expected, "{value:#}");
        assert_eq!(std::fs::read(&editor.board).unwrap(), before);
        assert!(konnect_ipc::KiCadIpcClient::new(&editor.socket)
            .ping()
            .unwrap());
    }
    // Explicit, separately verified save. Shutdown itself never saves.
    call(&api, "save_project", json!({})).await;
    assert_ne!(std::fs::read(&editor.board).unwrap(), before);
    editor.close().await;
}

#[tokio::test]
#[ignore = "requires Linux KiCad 10.0.8, pcbnew Python and Xvfb DISPLAY"]
async fn transfer_native_editor_shutdown_veto_timeout_and_other_instance() {
    let cli = std::env::var("KONNECT_TEST_KICAD_CLI").unwrap();
    let api = handler(&cli, "").await;
    let fixture = fixture(&api).await;
    seed_board(&cli, &fixture).await;
    let other_board = fixture.root.with_file_name("other.kicad_pcb");
    std::fs::copy(fixture.root.with_extension("kicad_pcb"), &other_board).unwrap();
    let other = Editor::open(&other_board).await;
    for (mode, expected) in [("veto", "refused"), ("swallow", "timed_out")] {
        let editor =
            Editor::open_with_close_mode(&fixture.root.with_extension("kicad_pcb"), mode).await;
        let target = call(
            &api,
            "get_editor_shutdown_target",
            json!({"expected_board":editor.board}),
        )
        .await["target"]
            .clone();
        let reply = api.handle_message(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"shutdown_editor","arguments":{"target":target,"timeout_seconds":1}}
        })).await.unwrap().result.unwrap();
        assert_eq!(reply["isError"], true, "{reply:#}");
        let value: Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(value["error"]["status"], expected, "{value:#}");
        assert!(konnect_ipc::KiCadIpcClient::new(&editor.socket)
            .ping()
            .unwrap());
        editor.close().await;
        assert!(konnect_ipc::KiCadIpcClient::new(&other.socket)
            .ping()
            .unwrap());
    }
    other.close().await;
}
