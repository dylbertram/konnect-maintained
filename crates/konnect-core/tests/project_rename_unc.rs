//! Opt-in Windows localhost SMB checks. Real filesystem, served MCP, unavailable
//! IPC. No native KiCad, editor ownership or schematic/PCB parity claim.

#[cfg(windows)]
mod windows {
    use konnect_core::mcp::handler::McpHandler;
    use konnect_core::mcp::protocol::{CallToolResult, ToolContent};
    use konnect_core::tools::ServerConfig;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    const OLD: &str = "multichannel_mixer";
    const NEW: &str = "guarded_rename_源";
    const CHILD_NAME: &str = "multichannel_channel_strip.kicad_sch";
    const ROOT: &str = include_str!("fixtures/multichannel_mixer.kicad_sch");
    const CHILD: &str = include_str!("fixtures/multichannel_channel_strip.kicad_sch");
    const PRO: &str = include_str!("fixtures/multichannel_mixer.kicad_pro");
    const BOARD: &str = include_str!("fixtures/specctra_two_resistors.kicad_pcb");
    const PRL: &str = r#"{"name":"multichannel_mixer","file":"multichannel_mixer.kicad_prl","unrelated":"keep-A"}"#;

    struct Fixture {
        temp: tempfile::TempDir,
        dir: PathBuf,
    }

    impl Fixture {
        fn new(root: &Path) -> Self {
            let temp = tempfile::Builder::new()
                .prefix("served-unc-源-")
                .tempdir_in(root)
                .unwrap();
            let dir = temp.path().join(OLD);
            std::fs::create_dir(&dir).unwrap();
            for (extension, bytes) in [
                ("kicad_pro", PRO),
                ("kicad_sch", ROOT),
                ("kicad_pcb", BOARD),
                ("kicad_prl", PRL),
            ] {
                std::fs::write(dir.join(format!("{OLD}.{extension}")), bytes).unwrap();
            }
            std::fs::write(dir.join(CHILD_NAME), CHILD).unwrap();
            Self { temp, dir }
        }
    }

    fn inventory(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file(), "{entry:?}");
                (
                    entry.file_name().into_string().unwrap(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    async fn call(api: &McpHandler, fixture: &Fixture, rename_directory: bool) -> (bool, Value) {
        let input = fixture.dir.join(format!("{OLD}.kicad_pro"));
        let canonical = input.canonicalize().unwrap();
        let canonical_text = canonical.to_str().unwrap();
        assert!(
            canonical_text.starts_with(r"\\?\UNC\localhost\KonnectRename"),
            "canonical path must retain verbatim localhost UNC: {canonical_text}"
        );
        println!(
            "UNC_PATH {}",
            json!({
                "input": input,
                "canonicalPath": canonical,
                "unicode_name": NEW,
                "rename_directory": rename_directory,
                "filesystem": "real localhost SMB; IPC unavailable; no GUI claim",
            })
        );
        let response = api
            .handle_message(json!({
                "jsonrpc": "2.0", "id": 852, "method": "tools/call",
                "params": {
                    "name": "rename_project",
                    "arguments": {
                        "project": input,
                        "new_name": NEW,
                        "rename_directory": rename_directory,
                    },
                },
            }))
            .await
            .unwrap();
        assert!(response.error.is_none(), "{response:?}");
        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        let ToolContent::Text { text } = &result.content[0] else {
            panic!("expected JSON text");
        };
        let body: Value = serde_json::from_str(text).unwrap();
        println!("UNC_RESPONSE {body}");
        (result.is_error, body)
    }

    fn expected() -> BTreeMap<String, Vec<u8>> {
        let old_token = format!("(project \"{OLD}\"");
        let new_token = format!("(project \"{NEW}\"");
        assert_eq!(ROOT.matches(old_token.as_str()).count(), 29);
        assert_eq!(CHILD.matches(old_token.as_str()).count(), 36);
        let pro_token = format!("\"{OLD}.kicad_pro\"");
        assert_eq!(PRO.matches(pro_token.as_str()).count(), 1);
        assert!(!PRO.contains(format!("\"{OLD}\"").as_str()));
        assert!(!BOARD.contains(old_token.as_str()));
        // Independent byte substitutions, never the production transformer.
        BTreeMap::from([
            (
                format!("{NEW}.kicad_sch"),
                ROOT.replace(old_token.as_str(), &new_token).into_bytes(),
            ),
            (
                CHILD_NAME.to_owned(),
                CHILD.replace(old_token.as_str(), &new_token).into_bytes(),
            ),
            (
                format!("{NEW}.kicad_pro"),
                PRO.replace(pro_token.as_str(), &format!("\"{NEW}.kicad_pro\""))
                    .into_bytes(),
            ),
            (format!("{NEW}.kicad_pcb"), BOARD.as_bytes().to_vec()),
            (
                format!("{NEW}.kicad_prl"),
                format!(
                    "{{\"name\":\"{NEW}\",\"file\":\"{NEW}.kicad_prl\",\"unrelated\":\"keep-A\"}}"
                )
                .into_bytes(),
            ),
        ])
    }

    #[tokio::test]
    #[ignore = "requires an explicitly provisioned owned localhost SMB share"]
    async fn served_rename_preserves_unc_files_and_collisions() {
        let root = std::env::var("KONNECT_RENAME_UNC_ROOT")
            .expect("workflow must supply KONNECT_RENAME_UNC_ROOT");
        let share = root
            .strip_prefix(r"\\localhost\")
            .expect("only literal localhost UNC paths are allowed");
        assert!(
            share.starts_with("KonnectRename")
                && share.len() > "KonnectRename".len()
                && share
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "require the workflow-owned KonnectRename share root, no subpath"
        );
        let root = PathBuf::from(root);
        assert!(root.is_dir(), "workflow-owned UNC share is unavailable");
        // The endpoint is unique, local and deliberately has no IPC listener.
        let ipc = tempfile::tempdir().unwrap();
        let api = McpHandler::new(ServerConfig {
            kicad_cli: String::new(),
            kicad_binary: String::new(),
            ipc_address: format!("ipc://{}", ipc.path().join("unused.sock").display()),
            project_dir: None,
            jlcpcb_db_path: None,
            auto_load_toolsets: false,
            eager_toolsets: true,
        })
        .await
        .unwrap();

        let mut cases = 0;
        for rename_directory in [false, true] {
            let f = Fixture::new(&root);
            let before = inventory(&f.dir);
            assert_eq!(before.len(), 5);
            let (is_error, body) = call(&api, &f, rename_directory).await;
            assert!(!is_error, "{body}");
            assert_eq!(body["outcome"]["status"], "complete", "{body}");
            assert_eq!(body["outcome"]["requested"], 1);
            assert_eq!(body["outcome"]["completed"], 1);
            assert_eq!(body["outcome"]["failed"], 0);
            let final_dir = if rename_directory {
                assert!(!f.dir.exists());
                f.temp.path().join(NEW)
            } else {
                f.dir.clone()
            };
            assert_eq!(inventory(&final_dir), expected());
            assert_eq!(body["applied"]["moved_files"].as_array().unwrap().len(), 4);
            assert_eq!(
                body["applied"]["verified_rewrites"]
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
            let entries = body["applied"]["current_paths"].as_array().unwrap();
            assert_eq!(entries.len(), 5);
            for entry in entries {
                let reported = PathBuf::from(entry["current_path"].as_str().unwrap());
                assert_eq!(
                    reported.parent().unwrap().canonicalize().unwrap(),
                    final_dir.canonicalize().unwrap()
                );
                assert!(reported.is_file(), "{entry}");
            }
            cases += 1;
            f.temp.close().expect("remove only this owned UNC fixture");
        }

        let f = Fixture::new(&root);
        let destination = f.dir.join(format!("{NEW}.kicad_pro"));
        std::fs::write(&destination, b"existing destination must survive").unwrap();
        let before = inventory(&f.dir);
        let (is_error, body) = call(&api, &f, true).await;
        assert!(is_error, "{body}");
        assert_eq!(body["outcome"]["status"], "failed", "{body}");
        assert_eq!(body["error"]["kind"], "conflict");
        assert_eq!(body["outcome"]["completed"], 0);
        assert_eq!(body["applied"]["moved_files"], json!([]));
        assert_eq!(body["applied"]["verified_rewrites"], json!([]));
        assert_eq!(inventory(&f.dir), before);
        assert!(!f.temp.path().join(NEW).exists());
        cases += 1;
        f.temp.close().expect("remove only this owned UNC fixture");
        println!(
            "UNC_EVIDENCE {}",
            json!({
                "tested_cases": cases,
                "filename_rename": true,
                "directory_rename": true,
                "existing_file_collision_preserved": true,
                "root_project_tokens": 29,
                "child_project_tokens": 36,
                "project_filename_tokens": 1,
                "pcb_unchanged": true,
                "verbatim_unc_and_unicode": true,
                "owned_fixture_cleanup": true,
                "native_kicad_or_gui": false,
            })
        );
        ipc.close()
            .expect("remove only this owned local IPC directory");
    }
}
