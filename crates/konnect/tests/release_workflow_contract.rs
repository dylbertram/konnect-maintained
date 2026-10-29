//! Guard the small active workflow set and the archived release acceptance edges.
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

fn archived_release() -> serde_json::Value {
    workflow("../disabled-workflows/release.yml")
}

#[test]
fn archived_release_preserves_real_kicad_acceptance() {
    let release = archived_release();

    assert_eq!(
        release["jobs"]["real-kicad-acceptance"]["uses"], "./.github/workflows/e2e-kicad.yml",
        "release.yml must call the real-KiCad acceptance workflow"
    );
    assert_eq!(
        release["jobs"]["release"]["needs"],
        serde_json::json!(["real-kicad-acceptance", "build", "pcm-package"]),
        "the release publication job must require real-KiCad acceptance and both artifact jobs"
    );
}

#[test]
fn release_workflow_has_a_non_publishing_pre_tag_entry_point() {
    let release = archived_release();

    assert!(
        release["on"].get("workflow_dispatch").is_some(),
        "maintainers need to exercise the release graph before creating a tag"
    );
    assert_eq!(
        release["jobs"]["release"]["if"],
        "github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')",
        "a manual release smoke run must never publish a GitHub release"
    );
}

#[test]
fn release_pcm_validation_uses_the_effective_release_version() {
    let release = archived_release();
    let validation = release["jobs"]["pcm-package"]["steps"]
        .as_array()
        .expect("PCM package steps must be an array")
        .iter()
        .find(|step| step["name"] == "Validate PCM package against KiCAD schema (release gate)")
        .expect("release workflow must validate PCM packages");
    let script = validation["run"]
        .as_str()
        .expect("PCM validation step must have a shell script");

    assert!(
        script.contains("version=\"${RELEASE_VERSION#v}\""),
        "PCM validation must use the effective version shared by tag and dry-run builds"
    );
    assert!(
        !script.contains("GITHUB_REF_NAME"),
        "a workflow_dispatch run uses the main branch ref, not the synthetic package version"
    );
}

#[test]
fn workflows_do_not_depend_on_node20_setup_protoc() {
    for name in [
        "ci.yml",
        "e2e-kicad.yml",
        "../disabled-workflows/release.yml",
    ] {
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
fn real_kicad_workflow_is_manual_or_explicitly_reused_only() {
    let e2e = workflow("e2e-kicad.yml");

    let triggers = &e2e["on"];
    assert!(
        triggers.get("workflow_call").is_some(),
        "retain reusable acceptance for a future reviewed release process"
    );
    assert_eq!(
        triggers.as_object().unwrap().len(),
        2,
        "E2E must have only manual and reusable entry points, never scheduled/PR runs"
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
    assert_eq!(names, ["ci.yml", "e2e-kicad.yml"]);

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
