//! Guard the small active workflow set.
//! Publication must stay inactive until a separate release-policy review.

use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("konnect crate must live below the repository root")
        .to_path_buf()
}

fn workflow(name: &str) -> serde_json::Value {
    let source = std::fs::read_to_string(repository_root().join(".github/workflows").join(name))
        .unwrap_or_else(|error| panic!("failed to read {name}: {error}"));
    serde_yaml_ng::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {name} as YAML: {error}"))
}

#[test]
fn workflows_do_not_depend_on_node20_setup_protoc() {
    for name in ["ci.yml", "e2e-kicad.yml", "rename-platforms.yml"] {
        let source =
            std::fs::read_to_string(repository_root().join(".github/workflows").join(name))
                .unwrap_or_else(|error| panic!("failed to read {name}: {error}"));

        assert!(
            !source.contains("arduino/setup-protoc"),
            "{name} must not reintroduce the Node-20 setup-protoc action"
        );
        assert!(
            source.contains("tool: protoc@3.23.4") && source.contains("fallback: none"),
            "{name} must install the reviewed protoc version without an unreviewed fallback"
        );
    }
}

#[test]
fn real_kicad_workflow_is_manual_only() {
    let e2e = workflow("e2e-kicad.yml");

    let triggers = &e2e["on"];
    assert_eq!(
        triggers.as_object().unwrap().len(),
        1,
        "E2E must have only a manual entry point, never scheduled/PR runs"
    );
    assert!(
        triggers.get("workflow_dispatch").is_some(),
        "manual real-KiCad entry point must remain available"
    );
}

#[test]
fn active_workflows_cannot_publish_or_close_issues() {
    let directory = repository_root().join(".github/workflows");
    let mut names: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["ci.yml", "e2e-kicad.yml", "rename-platforms.yml"]);

    for name in names {
        let config = workflow(&name);
        assert_eq!(
            config["permissions"],
            serde_json::json!({"contents": "read"})
        );
        assert_eq!(config["concurrency"]["cancel-in-progress"], true);
        let triggers = config["on"].as_object().unwrap();
        assert!(!triggers.contains_key("schedule"));

        for job in config["jobs"].as_object().unwrap().values() {
            assert!(
                job.get("permissions").is_none(),
                "no job-level privilege escalation"
            );
            assert!(
                job.get("strategy").is_none(),
                "no automatic matrix expansion"
            );
            assert!(job["timeout-minutes"].as_u64().unwrap() <= 60);
            for step in job["steps"].as_array().unwrap() {
                if let Some(action) = step["uses"].as_str() {
                    let (repository, sha) = action.split_once('@').unwrap();
                    assert_eq!(sha.len(), 40, "{action} must be commit-pinned");
                    assert!(sha.bytes().all(|byte| byte.is_ascii_hexdigit()));
                    if repository == "actions/checkout" {
                        assert_eq!(step["with"]["persist-credentials"], false);
                    }
                    if repository == "actions/upload-artifact" {
                        assert_eq!(step["with"]["retention-days"], 7);
                    }
                }
            }
        }
    }

    let ci = workflow("ci.yml");
    assert_eq!(
        ci["on"],
        serde_json::json!({
            "push": {"branches": ["main"]},
            "pull_request": {"branches": ["main"]},
            "workflow_dispatch": null
        }),
        "ordinary CI must not respond to tags or privileged workflow events"
    );
}

#[test]
fn core_ci_keeps_the_required_checks_in_one_bounded_linux_job() {
    let ci = workflow("ci.yml");
    assert_eq!(ci["jobs"].as_object().unwrap().len(), 1);
    let job = &ci["jobs"]["checks"];
    assert_eq!(job["name"], "Core checks");
    assert_eq!(job["runs-on"], "ubuntu-latest");
    assert_eq!(job["timeout-minutes"], 45);
    assert!(
        job.get("if").is_none(),
        "the required job must not be conditional"
    );
    assert!(job.get("continue-on-error").is_none());
    let steps = job["steps"].as_array().unwrap();
    for step in steps {
        assert!(
            step.get("if").is_none(),
            "core steps must not be conditional"
        );
        assert!(step.get("continue-on-error").is_none());
    }
    for command in [
        "cargo fmt --all -- --check",
        "cargo clippy --workspace --locked --all-targets -- -D warnings",
        "cargo test --workspace --locked --lib --tests",
        "cargo test --workspace --locked --doc",
    ] {
        assert!(
            steps.iter().any(|step| {
                step["run"] == command
                    && step.get("if").is_none()
                    && step.get("continue-on-error").is_none()
            }),
            "missing unconditional gate: {command}"
        );
    }
    let python = steps
        .iter()
        .find(|step| step["name"] == "Python tests")
        .unwrap();
    let script = python["run"].as_str().unwrap();
    assert!(script.contains("python -m unittest discover -s plugin/tests -v"));
    assert!(script.contains("python -m unittest discover -s scripts/tests -v"));
    assert!(steps.iter().any(|step| {
        step["uses"]
            .as_str()
            .is_some_and(|action| action.starts_with("EmbarkStudios/cargo-deny-action@"))
            && step["with"]["command"] == "check licenses"
    }));
}

#[test]
fn rename_validation_is_manual_read_only_and_tests_an_exact_source_commit() {
    let config = workflow("rename-platforms.yml");
    let triggers = config["on"].as_object().unwrap();
    assert_eq!(triggers.len(), 1);
    let input = &triggers["workflow_dispatch"]["inputs"]["source_sha"];
    assert_eq!(input["required"], true);
    assert_eq!(input["type"], "string");
    assert_eq!(config["env"]["CARGO_BUILD_JOBS"], 2);
    assert_eq!(
        config["env"]["TESTED_SOURCE_SHA"],
        "${{ inputs.source_sha }}"
    );
    assert_eq!(
        config["env"]["VALIDATION_WORKFLOW_SHA"],
        "${{ github.workflow_sha }}"
    );
    let jobs = config["jobs"].as_object().unwrap();
    assert_eq!(jobs.len(), 3);
    for (id, runner) in [
        ("windows", "windows-2025"),
        ("macos-intel", "macos-15-intel"),
        ("macos-arm", "macos-15"),
    ] {
        let job = &jobs[id];
        assert_eq!(job["runs-on"], runner);
        assert_eq!(job["timeout-minutes"], 60);
        assert!(job.get("if").is_none());
        assert!(job.get("continue-on-error").is_none());
        let steps = job["steps"].as_array().unwrap();
        assert_eq!(
            steps[0]["name"],
            "Validate input and runner capacity before setup"
        );
        assert!(steps[0].get("uses").is_none());
        let checkout = steps
            .iter()
            .find(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("actions/checkout@"))
            })
            .unwrap();
        assert_eq!(checkout["with"]["ref"], "${{ inputs.source_sha }}");
        assert_eq!(checkout["with"]["persist-credentials"], false);
        let core = steps
            .iter()
            .find(|step| step["name"] == "Build core and run native filesystem tests")
            .unwrap();
        assert!(core["run"]
            .as_str()
            .unwrap()
            .contains("scripts/rename_platform_checks.py"));
        assert!(core["run"].as_str().unwrap().contains("--mode core"));
        assert!(core.get("if").is_none());
        assert!(core.get("continue-on-error").is_none());
        let artifact = steps.last().unwrap();
        assert_eq!(artifact["if"], "always()");
        assert!(artifact["uses"]
            .as_str()
            .unwrap()
            .starts_with("actions/upload-artifact@"));
        assert_eq!(artifact["with"]["retention-days"], 7);
        assert_eq!(artifact["with"]["if-no-files-found"], "error");
    }
    let unc = jobs["windows"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["name"] == "Test an owned loopback UNC share")
        .unwrap();
    let script = unc["run"].as_str().unwrap();
    for required in [
        "New-SmbShare",
        "Remove-SmbShare",
        "finally",
        "--mode unc",
        "unc-share-lifecycle.json",
    ] {
        assert!(script.contains(required), "missing UNC boundary: {required}");
    }
    assert!(unc.get("if").is_none());
    assert!(unc.get("continue-on-error").is_none());
}
