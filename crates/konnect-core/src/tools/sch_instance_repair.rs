//! Repair one proved obsolete root prefix, never arbitrary hierarchy metadata.
//! The canonical root, every object block outside the changed path bytes and
//! the saved PCB are immutable. A coherent legacy-root copy is the native
//! connectivity/ERC reference; the inconsistent original is not called valid.
use super::sch_transfer::{
    closed_board, collect_hierarchy, collect_uuids, load_document, netlist_partitions, property,
    validate_instances, Document, ErcIdentityIndex,
};
use super::{cli, invalid_arg, ToolContext, ToolDef};
use crate::mcp::{error::ToolErrorKind, protocol::CallToolResult};
use crate::tool;
use anyhow::{ensure, Context, Result};
use konnect_sexp::writer::find_direct_child_blocks;
use konnect_sexp::{apply_edits, parse_sexp, read_consistent, FileTransition, SexpEdit, SexpNode};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const OPERATION: &str = "repair_schematic_instance_roots";

pub(super) fn tool() -> ToolDef {
    tool!(
        "repair_schematic_instance_roots",
        "Repair one explicitly named obsolete root UUID in saved symbol and sheet instance \
         paths. Preserves the canonical schematic root, object UUIDs, references, units, \
         properties, geometry and PCB bytes. Only a uniform root-prefix mismatch with \
         otherwise exact hierarchy paths is supported; mixed roots, foreign projects, \
         reused sheets and any other stale metadata are refused. Defaults to dry-run. \
         Apply requires the exact latest plan revision and closed editors. Native KiCad \
         compares the candidate to a coherent legacy-root copy before reversible apply.",
        json!({
            "type":"object","additionalProperties":false,
            "properties":{
                "schematic":{"type":"string","description":"Root .kicad_sch beside its owning .kicad_pro"},
                "old_root_uuid":{"type":"string","description":"Exact obsolete root UUID observed in every stale instance path; never inferred"},
                "dry_run":{"type":"boolean","default":true},
                "expected_plan_revision":{"type":"string","description":"Required on apply; exact revision from the latest dry-run"}
            },
            "required":["schematic","old_root_uuid"]
        }),
        |args, ctx| async move { handle_repair_schematic_instance_roots(args, ctx).await }
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schematic: PathBuf,
    old_root_uuid: String,
    #[serde(default = "default_dry_run")]
    dry_run: bool,
    expected_plan_revision: Option<String>,
}

fn default_dry_run() -> bool {
    true
}

fn blocked(reason: impl ToString) -> CallToolResult {
    let reason = reason.to_string();
    CallToolResult::error_kind(
        ToolErrorKind::PlanBlocked {
            operation: OPERATION.into(),
            reasons: vec![reason.clone()],
        },
        format!("Instance-root repair was not applied. {reason}"),
    )
}

async fn handle_repair_schematic_instance_roots(
    args: &Value,
    ctx: &ToolContext,
) -> Result<CallToolResult> {
    use crate::outcome::{self, OutcomeStatus};
    let result = repair(args, ctx)
        .await
        .unwrap_or_else(|error| blocked(format!("{error:#}")));
    let status = if crate::mcp::error::extract_error_kind(&result).as_deref()
        == Some("mutation_outcome_uncertain")
    {
        OutcomeStatus::Uncertain
    } else if result.is_error {
        OutcomeStatus::Failed
    } else {
        OutcomeStatus::Complete
    };
    let retry = match status {
        OutcomeStatus::Uncertain => Some(outcome::inspect_before_retry()),
        OutcomeStatus::Failed => Some(outcome::retry_whole_request()),
        _ => None,
    };
    Ok(outcome::attach(
        result,
        outcome::summary(
            status,
            args["schematic"].as_str().unwrap_or("unresolved schematic"),
            "saved_project",
            1,
            usize::from(status == OutcomeStatus::Complete),
            usize::from(status != OutcomeStatus::Complete),
            retry,
        ),
    ))
}

struct Plan {
    root: PathBuf,
    old_root: String,
    documents: BTreeMap<PathBuf, Document>,
    board: Option<PathBuf>,
    components: BTreeSet<(String, String)>,
    revision: String,
    report: Value,
}

async fn repair(args: &Value, ctx: &ToolContext) -> Result<CallToolResult> {
    let request: Request = match serde_json::from_value(args.clone()) {
        Ok(request) => request,
        Err(error) => return Ok(invalid_arg("arguments", &error.to_string())),
    };
    if !request.dry_run
        && request
            .expected_plan_revision
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Ok(invalid_arg(
            "expected_plan_revision",
            "apply requires the latest dry-run revision",
        ));
    }
    let mut plan = Plan::prepare(&request)?;
    if !request.dry_run {
        ensure!(
            request.expected_plan_revision.as_deref() == Some(plan.revision.as_str()),
            "Stale plan revision; request a new dry-run before applying"
        );
    }
    if let Some(refusal) = plan.closed(ctx).await? {
        return Ok(refusal);
    }
    let changed = plan.documents.values().any(|doc| doc.before != doc.after);
    if changed {
        plan.report["verification"] = verify(&plan, &ctx.config.kicad_cli).await?;
    } else {
        plan.report["verification"] =
            json!({"source":"saved_project","status":"not_needed_no_changes"});
    }
    if !request.dry_run && changed {
        if let Some(refusal) = plan.closed(ctx).await? {
            return Ok(refusal);
        }
        let transitions = plan
            .documents
            .iter()
            .map(|(path, doc)| FileTransition::replace(path, &doc.before, &doc.after))
            .collect();
        match konnect_sexp::commit_reversible_file_transaction(
            plan.root.parent().unwrap(),
            transitions,
        ) {
            Ok(commit) => plan.report["transaction_id"] = json!(commit.id),
            Err(error) => {
                let unchanged = plan.documents.iter().all(|(path, doc)| {
                    read_consistent(path).is_ok_and(|source| source == doc.before)
                });
                let no_journal =
                    konnect_sexp::inspect_file_transactions(plan.root.parent().unwrap())
                        .is_ok_and(|journals| journals.is_empty());
                if unchanged && no_journal {
                    return Ok(blocked(error));
                }
                return Ok(super::mutation_outcome_uncertain(
                    &plan.root,
                    OPERATION,
                    error.to_string(),
                ));
            }
        }
        for (path, doc) in &plan.documents {
            if !read_consistent(path).is_ok_and(|source| source == doc.after) {
                return Ok(super::mutation_outcome_uncertain(
                    path,
                    OPERATION,
                    "Committed-file readback differs from the verified root-prefix-only candidate",
                ));
            }
        }
    }
    plan.report["dry_run"] = json!(request.dry_run);
    plan.report["status"] = json!(if !changed {
        "noop"
    } else if request.dry_run {
        "planned"
    } else {
        "repaired"
    });
    Ok(CallToolResult::json(&plan.report))
}

impl Plan {
    async fn closed(&self, ctx: &ToolContext) -> Result<Option<CallToolResult>> {
        if let Some(refusal) = super::pcb_board::refuse_if_project_locked_by_kicad(
            &self.root.with_extension("kicad_pro"),
            "instance-root repair",
        ) {
            return Ok(Some(refusal));
        }
        for path in self.documents.keys() {
            if let Some(lock) = konnect_sexp::writer::kicad_editor_lock_path(path) {
                match std::fs::symlink_metadata(&lock) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                    _ => return Ok(Some(blocked(format!(
                        "Cannot prove document '{}' is closed; editor lock '{}' is present or uninspectable",
                        path.display(), lock.display())))),
                }
            }
        }
        if let Some(board) = &self.board {
            return closed_board(ctx, board).await;
        }
        Ok(None)
    }

    fn prepare(request: &Request) -> Result<Self> {
        let old = uuid::Uuid::parse_str(&request.old_root_uuid).context("Invalid old_root_uuid")?;
        ensure!(
            old.to_string() == request.old_root_uuid,
            "old_root_uuid must use canonical UUID spelling"
        );
        let root = std::fs::canonicalize(&request.schematic)?;
        ensure!(
            root.extension().is_some_and(|ext| ext == "kicad_sch"),
            "Expected a root .kicad_sch"
        );
        let directory = root.parent().unwrap();
        ensure!(
            root.with_extension("kicad_pro").is_file(),
            "Root must have a same-named .kicad_pro"
        );
        ensure!(
            konnect_sexp::inspect_file_transactions(directory)?.is_empty(),
            "Pending project transaction; recover it explicitly first"
        );
        let project = root
            .file_stem()
            .and_then(|name| name.to_str())
            .context("Non-Unicode project name")?;
        let mut documents = BTreeMap::new();
        let mut instances = BTreeMap::new();
        collect_hierarchy(
            &root,
            None,
            directory,
            &mut documents,
            &mut instances,
            &mut BTreeSet::new(),
        )?;
        let root_uuid = parse_sexp(&documents[&root].before)?
            .find_str("uuid")
            .context("Root lacks UUID")?
            .to_owned();
        ensure!(
            root_uuid != request.old_root_uuid,
            "The requested old root is the canonical root; no migration is defined"
        );
        let mut paths = BTreeMap::new();
        for (instance, path) in &instances {
            ensure!(
                paths.insert(path.clone(), instance.clone()).is_none(),
                "Repeated sheet file '{}' is unsupported",
                path.display()
            );
        }
        let mut ids = BTreeSet::new();
        let mut seen_roots = BTreeSet::new();
        let mut changes = Vec::new();
        let mut component_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut units = BTreeSet::new();
        for (path, instance) in &paths {
            let owner = super::resolve_schematic_ownership(path)
                .map_err(|error| anyhow::anyhow!("Cannot prove project ownership: {error:?}"))?
                .context("No owning project")?;
            ensure!(
                super::same_schematic_document(&owner.root_schematic, &root),
                "Foreign owning project for '{}'",
                path.display()
            );
            let doc = documents.get_mut(path).unwrap();
            let tree = parse_sexp(&doc.before)?;
            ensure!(
                tree.find("symbol_instances").is_none(),
                "Legacy symbol-instance tables are unsupported"
            );
            for node in tree.children().unwrap_or_default() {
                if node.head() != Some("lib_symbols") {
                    collect_uuids(node, &mut ids)?;
                }
            }
            let mut edits = Vec::new();
            for (start, end) in find_direct_child_blocks(&doc.before, "kicad_sch") {
                let block = &doc.before[start..end];
                let node = parse_sexp(block)?;
                let Some(kind @ ("symbol" | "sheet")) = node.head() else {
                    continue;
                };
                let uuid = node.find_str("uuid").context("Instance owner lacks UUID")?;
                uuid::Uuid::parse_str(uuid).context("Invalid object UUID")?;
                let reference = if kind == "symbol" {
                    let reference =
                        property(&node, "Reference").context("Symbol lacks Reference")?;
                    let unit = node.find_str("unit").context("Symbol lacks unit")?;
                    ensure!(
                        units.insert((reference.to_owned(), unit.to_owned())),
                        "Duplicate reference/unit {reference}/{unit}"
                    );
                    if !reference.starts_with('#') && node.find_str("on_board") != Some("no") {
                        component_ids
                            .entry(reference.into())
                            .or_default()
                            .insert(uuid.into());
                    }
                    Some((reference, unit))
                } else {
                    None
                };
                let edit = instance_edit(
                    block,
                    instance,
                    project,
                    &root_uuid,
                    &request.old_root_uuid,
                    reference,
                )?;
                seen_roots.insert(edit.observed_root);
                if let Some((offset, length)) = edit.root_span {
                    edits.push(SexpEdit::replace(
                        start + offset,
                        start + offset + length,
                        root_uuid.clone(),
                    ));
                    changes.push(json!({"file":path.display().to_string(),"kind":kind,"uuid":uuid,
                        "reference":reference.map(|(name,_)|name),"old_path":edit.old_path,"new_path":instance}));
                }
            }
            doc.after = apply_edits(doc.before.clone(), edits);
            validate_instances(path, &doc.after, project, instance, path != &root)?;
        }
        ensure!(
            seen_roots.len() <= 1,
            "Mixed old/current instance roots are unsupported; no partial migration is guessed"
        );
        load_document(&root.with_extension("kicad_pro"), &mut documents)?;
        for name in ["sym-lib-table", "fp-lib-table"] {
            let path = directory.join(name);
            if path.exists() {
                load_document(&path, &mut documents)?;
            }
        }
        let board = root.with_extension("kicad_pcb");
        let board = if board.exists() {
            ensure!(
                std::fs::canonicalize(&board)? == board,
                "Project PCB must not be a symlink"
            );
            load_document(&board, &mut documents)?;
            Some(board)
        } else {
            None
        };
        let mut hash = Sha256::new();
        hash.update(b"instance-root-repair-v1\0");
        hash.update(request.old_root_uuid.as_bytes());
        for (path, doc) in &documents {
            let path_text = path.to_string_lossy();
            for part in [
                path_text.as_bytes(),
                doc.before.as_bytes(),
                doc.after.as_bytes(),
            ] {
                hash.update((part.len() as u64).to_le_bytes());
                hash.update(part);
            }
        }
        let revision = format!("{:x}", hash.finalize());
        let affected = documents
            .iter()
            .filter(|(_, doc)| doc.before != doc.after)
            .map(|(path, _)| path.display().to_string())
            .collect::<Vec<_>>();
        let report = json!({"source":"saved_project","plan_revision":revision,
            "old_root_uuid":request.old_root_uuid,"canonical_root_uuid":root_uuid,
            "root_uuid_changed":false,"pcb_changed":false,"changes":changes,"affected_files":affected,
            "symbol_instance_records":changes.iter().filter(|item|item["kind"]=="symbol").count(),
            "sheet_instance_records":changes.iter().filter(|item|item["kind"]=="sheet").count(),
            "limitations":["Only a uniform root-prefix migration is supported",
                "Native reference is a coherent legacy-root copy, not a claim that the original metadata was valid",
                "No circuit transfer, annotation, or source-sheet deletion is performed"]});
        let components = component_ids
            .into_iter()
            .map(|(reference, ids)| (reference, ids.into_iter().collect::<Vec<_>>().join(" ")))
            .collect();
        Ok(Self {
            root,
            old_root: request.old_root_uuid.clone(),
            documents,
            board,
            components,
            revision,
            report,
        })
    }
}

struct InstanceEdit {
    root_span: Option<(usize, usize)>,
    observed_root: String,
    old_path: String,
}

fn instance_edit(
    block: &str,
    expected: &str,
    project_name: &str,
    root: &str,
    old_root: &str,
    reference: Option<(&str, &str)>,
) -> Result<InstanceEdit> {
    let tree = parse_sexp(block)?;
    let blocks = find_direct_child_blocks(block, tree.head().unwrap());
    let instances = blocks
        .into_iter()
        .filter(|&(start, end)| {
            parse_sexp(&block[start..end]).is_ok_and(|node| node.head() == Some("instances"))
        })
        .collect::<Vec<_>>();
    ensure!(
        instances.len() == 1,
        "Object must have exactly one instances block"
    );
    let (start, end) = instances[0];
    let source = &block[start..end];
    let entries = find_direct_child_blocks(source, "instances");
    ensure!(
        entries.len() == 1,
        "Foreign or ambiguous project instance entries"
    );
    let (project_start, project_end) = entries[0];
    let project_source = &source[project_start..project_end];
    let project = parse_sexp(project_source)?;
    ensure!(
        project.head() == Some("project")
            && project.get(1).and_then(SexpNode::as_str) == Some(project_name),
        "Instance belongs to a foreign project"
    );
    let paths = find_direct_child_blocks(project_source, "project");
    ensure!(paths.len() == 1, "Missing or repeated instance paths");
    let (path_start, path_end) = paths[0];
    let path_source = &project_source[path_start..path_end];
    let path = parse_sexp(path_source)?;
    ensure!(path.head() == Some("path"), "Malformed instance path");
    let observed = path
        .get(1)
        .and_then(SexpNode::as_str)
        .context("Invalid instance path")?;
    let old_expected = expected.replacen(&format!("/{root}"), &format!("/{old_root}"), 1);
    ensure!(observed==expected || observed==old_expected,
        "Instance path '{observed}' differs beyond the explicitly named root prefix; expected '{expected}'");
    if let Some((reference, unit)) = reference {
        ensure!(
            path.find_str("reference") == Some(reference) && path.find_str("unit") == Some(unit),
            "Saved reference/unit disagrees with the placed symbol {reference}/{unit}"
        );
    } else {
        ensure!(path.find_str("page").is_some(), "Sheet instance lacks page");
    }
    let root_span = if observed == old_expected {
        let literal = format!("\"{observed}\"");
        let quoted = path_source
            .find(&literal)
            .context("Only quoted native instance paths are supported")?;
        ensure!(
            path_source[1..quoted].trim() == "path",
            "Unsupported instance path syntax"
        );
        Some((
            start + project_start + path_start + quoted + 2,
            old_root.len(),
        ))
    } else {
        None
    };
    Ok(InstanceEdit {
        root_span,
        observed_root: if observed == expected {
            root.into()
        } else {
            old_root.into()
        },
        old_path: observed.into(),
    })
}

async fn verify(plan: &Plan, cli_path: &str) -> Result<Value> {
    ensure!(
        !cli_path.is_empty(),
        "kicad-cli is required; no unverified repair is supported"
    );
    let mut netlists = Vec::new();
    let mut violations = Vec::new();
    for after in [false, true] {
        let stage = tempfile::tempdir()?;
        for (path, doc) in &plan.documents {
            if Some(path) == plan.board.as_ref() {
                continue;
            }
            let mut source = if after {
                doc.after.clone()
            } else {
                doc.before.clone()
            };
            if !after && path == &plan.root {
                let range = find_direct_child_blocks(&source, "kicad_sch")
                    .into_iter()
                    .find(|&(start, end)| {
                        parse_sexp(&source[start..end])
                            .is_ok_and(|node| node.head() == Some("uuid"))
                    })
                    .context("Root UUID block missing")?;
                source = apply_edits(
                    source,
                    vec![SexpEdit::replace(
                        range.0,
                        range.1,
                        format!("(uuid \"{}\")", plan.old_root),
                    )],
                );
            }
            let target = stage
                .path()
                .join(path.strip_prefix(plan.root.parent().unwrap())?);
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::fs::write(target, source)?;
        }
        let root = stage.path().join(plan.root.file_name().unwrap());
        let netlist = stage.path().join("repair.net");
        cli::export_netlist(cli_path, &root, &netlist, "kicadsexpr").await?;
        let partitions = netlist_partitions(&std::fs::read_to_string(netlist)?)?;
        ensure!(
            partitions.0 == plan.components,
            "Native netlist did not preserve every expected component/reference/UUID"
        );
        netlists.push(partitions);
        let identities = ErcIdentityIndex::build_documents(&plan.documents, after)?;
        let report = cli::run_erc(cli_path, &root).await?;
        let mut counts = BTreeMap::new();
        for violation in report.violations {
            let mut ids = violation
                .items
                .iter()
                .map(|item| identities.identity(item))
                .collect::<Result<Vec<_>>>()?;
            ids.sort();
            *counts
                .entry((violation.rule, violation.severity, ids))
                .or_insert(0usize) += 1;
        }
        violations.push(counts);
    }
    ensure!(
        netlists[0] == netlists[1],
        "Native pin-net partitions differ from the coherent legacy-root reference"
    );
    ensure!(
        violations[0] == violations[1],
        "Native ERC differs from the coherent legacy-root reference"
    );
    Ok(
        json!({"source":"kicad_cli","reference":"coherent_legacy_root_copy",
        "netlist":"equivalent_components_and_pin_partitions","erc":"equivalent_rule_severity_item_multisets"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::handler::McpHandler;
    use std::path::Path;

    const ROOT: &str = "a73252f6-017c-453c-8321-f1201f92efed";
    const OLD: &str = "cd385c7a-53db-42a0-815e-a24ef952404d";
    const NATIVE: &str = include_str!("../../tests/fixtures/component_fields_kicad10.kicad_sch");

    fn fixture() -> (tempfile::TempDir, Request, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("component_fields_kicad10.kicad_sch");
        let sheet = konnect_sexp::schematic::format_hierarchical_sheet(
            konnect_sexp::schematic::HierarchicalSheetSpec {
                name: "Child",
                file: "child.kicad_sch",
                x: 10.0,
                y: 10.0,
                width: 30.0,
                height: 20.0,
                project_name: "component_fields_kicad10",
                parent_instance_path: &format!("/{ROOT}"),
                page: "2",
            },
        );
        let source = apply_edits(
            NATIVE.into(),
            vec![SexpEdit::insert(NATIVE.rfind(')').unwrap(), sheet)],
        );
        std::fs::write(
            &path,
            source.replace(&format!("\"/{ROOT}"), &format!("\"/{OLD}")),
        )
        .unwrap();
        std::fs::write(path.with_extension("kicad_pro"), "{}").unwrap();
        std::fs::write(
            dir.path().join("child.kicad_sch"),
            super::super::blank_schematic_template(),
        )
        .unwrap();
        let request = Request {
            schematic: path,
            old_root_uuid: OLD.into(),
            dry_run: true,
            expected_plan_revision: None,
        };
        (dir, request, source)
    }

    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect()
    }

    #[test]
    fn repair_plan_changes_only_the_proved_prefix_bytes_and_preserves_canonical_root() {
        let (dir, request, expected) = fixture();
        let before = snapshot(dir.path());
        let plan = Plan::prepare(&request).unwrap();
        assert_eq!(before, snapshot(dir.path()));
        assert_eq!(plan.documents[&request.schematic].after, expected);
        assert_eq!(
            parse_sexp(&plan.documents[&request.schematic].after)
                .unwrap()
                .find_str("uuid"),
            Some(ROOT)
        );
        assert_eq!(plan.report["symbol_instance_records"], 4);
        assert_eq!(plan.report["sheet_instance_records"], 1);
        assert_eq!(plan.report["root_uuid_changed"], false);
    }

    #[test]
    fn repair_refuses_wrong_or_mixed_roots_foreign_projects_and_nonroot_staleness() {
        for case in [
            "wrong_root",
            "mixed",
            "foreign",
            "reference",
            "unit",
            "tail",
            "missing",
        ] {
            let (dir, mut request, _) = fixture();
            let source = std::fs::read_to_string(&request.schematic).unwrap();
            let changed = match case {
                "wrong_root" => {
                    request.old_root_uuid = uuid::Uuid::new_v4().to_string();
                    source
                }
                "mixed" => source.replacen(&format!("\"/{OLD}\""), &format!("\"/{ROOT}\""), 1),
                "foreign" => source.replace(
                    "(project \"component_fields_kicad10\"",
                    "(project \"other\"",
                ),
                "reference" => source.replace("(reference \"R1\")", "(reference \"R99\")"),
                "unit" => source.replacen("(unit 1)", "(unit 2)", 1),
                "tail" => source.replace(
                    &format!("\"/{OLD}\""),
                    &format!("\"/{OLD}/{}\"", uuid::Uuid::new_v4()),
                ),
                "missing" => source.replace("(instances", "(unsupported_instances"),
                _ => unreachable!(),
            };
            std::fs::write(&request.schematic, changed).unwrap();
            let before = snapshot(dir.path());
            assert!(Plan::prepare(&request).is_err(), "{case}");
            assert_eq!(before, snapshot(dir.path()), "{case}");
        }
    }

    #[test]
    fn repair_is_noop_after_repair_and_revision_binds_unchanged_project_guards() {
        let (dir, request, expected) = fixture();
        let first = Plan::prepare(&request).unwrap();
        std::fs::write(
            request.schematic.with_extension("kicad_pro"),
            "{\"meta\":{}}",
        )
        .unwrap();
        let next = Plan::prepare(&request).unwrap();
        assert_ne!(first.revision, next.revision);
        std::fs::write(&request.schematic, expected).unwrap();
        let before = snapshot(dir.path());
        let noop = Plan::prepare(&request).unwrap();
        assert!(noop.documents.values().all(|doc| doc.before == doc.after));
        assert_eq!(noop.report["changes"], json!([]));
        assert_eq!(before, snapshot(dir.path()));
    }

    #[test]
    fn repair_preserves_a_native_pcb_byte_for_byte_as_a_revision_guard() {
        let (_dir, request, _) = fixture();
        let board = request.schematic.with_extension("kicad_pcb");
        let native = include_str!(
            "../../../konnect-sexp/tests/fixtures/placement/placement_fixture.kicad_pcb"
        );
        std::fs::write(&board, native).unwrap();
        let plan = Plan::prepare(&request).unwrap();
        assert_eq!(plan.documents[&board].before, native);
        assert_eq!(plan.documents[&board].after, native);
        std::fs::write(&board, format!("{native}\n")).unwrap();
        assert_ne!(plan.revision, Plan::prepare(&request).unwrap().revision);
    }

    #[test]
    fn repair_refuses_missing_cyclic_and_reused_sheet_files_without_writes() {
        for case in ["missing", "cycle", "reused"] {
            let (dir, request, _) = fixture();
            let child = dir.path().join("child.kicad_sch");
            if case == "missing" {
                std::fs::remove_file(&child).unwrap();
            } else {
                let file = if case == "cycle" {
                    request.schematic.file_name().unwrap().to_str().unwrap()
                } else {
                    "child.kicad_sch"
                };
                let sheet = konnect_sexp::schematic::format_hierarchical_sheet(
                    konnect_sexp::schematic::HierarchicalSheetSpec {
                        name: "Another",
                        file,
                        x: 60.0,
                        y: 10.0,
                        width: 30.0,
                        height: 20.0,
                        project_name: "component_fields_kicad10",
                        parent_instance_path: &format!("/{OLD}"),
                        page: "3",
                    },
                );
                let path = if case == "cycle" {
                    &child
                } else {
                    &request.schematic
                };
                let source = std::fs::read_to_string(path).unwrap();
                let close = source.rfind(')').unwrap();
                std::fs::write(
                    path,
                    apply_edits(source, vec![SexpEdit::insert(close, sheet)]),
                )
                .unwrap();
            }
            let before = snapshot(dir.path());
            assert!(Plan::prepare(&request).is_err(), "{case}");
            assert_eq!(before, snapshot(dir.path()), "{case}");
        }
    }

    async fn invoke(handler: &McpHandler, request: &Request) -> Value {
        let mut arguments = json!({"schematic":request.schematic,"old_root_uuid":request.old_root_uuid,
            "dry_run":request.dry_run});
        if let Some(revision) = &request.expected_plan_revision {
            arguments["expected_plan_revision"] = json!(revision);
        }
        let reply = handler
            .handle_message(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                    "name":OPERATION,"arguments":arguments
                }}),
            )
            .await
            .unwrap();
        assert!(reply.error.is_none(), "{:?}", reply.error);
        let result = reply.result.unwrap();
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn repair_public_dispatch_fails_closed_for_locks_unverified_apply_and_stale_revision() {
        let (dir, mut request, _) = fixture();
        let handler = McpHandler::new(super::super::ServerConfig {
            kicad_cli: String::new(),
            ipc_address: String::new(),
            eager_toolsets: true,
            ..Default::default()
        })
        .await
        .unwrap();
        let before = snapshot(dir.path());
        let refusal = invoke(&handler, &request).await;
        assert_eq!(refusal["error"]["kind"], "plan_blocked");
        assert!(refusal["message"]
            .as_str()
            .unwrap()
            .contains("kicad-cli is required"));
        request.dry_run = false;
        let refusal = invoke(&handler, &request).await;
        assert_eq!(refusal["error"]["kind"], "invalid_argument");
        request.expected_plan_revision = Some("obsolete".into());
        let refusal = invoke(&handler, &request).await;
        assert!(refusal["message"]
            .as_str()
            .unwrap()
            .contains("Stale plan revision"));
        assert_eq!(before, snapshot(dir.path()));
        request.dry_run = true;
        request.expected_plan_revision = None;
        let lock = konnect_sexp::writer::kicad_project_lock_path(
            &request.schematic.with_extension("kicad_pro"),
        )
        .unwrap();
        std::fs::write(&lock, "test-owned").unwrap();
        let locked = snapshot(dir.path());
        let refusal = invoke(&handler, &request).await;
        assert_eq!(refusal["error"]["kind"], "unsafe_file_fallback");
        assert_eq!(locked, snapshot(dir.path()));
    }

    #[test]
    fn repair_reversible_read_set_refuses_concurrent_project_changes_without_partial_writes() {
        let (dir, request, _) = fixture();
        let plan = Plan::prepare(&request).unwrap();
        std::fs::write(
            request.schematic.with_extension("kicad_pro"),
            "{\"external\":\"edit\"}",
        )
        .unwrap();
        let before = snapshot(dir.path());
        let transitions = plan
            .documents
            .iter()
            .map(|(path, doc)| FileTransition::replace(path, &doc.before, &doc.after))
            .collect();
        assert!(konnect_sexp::commit_reversible_file_transaction(dir.path(), transitions).is_err());
        assert_eq!(before, snapshot(dir.path()));
    }
}
