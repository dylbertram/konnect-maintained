//! Identity-preserving transfer between saved, single-instance sheets.
//!
//! Planning edits raw object blocks, not typed round trips. KiCad independently
//! checks the complete candidate hierarchy before the reversible write begins.

use super::{cli, invalid_arg, ToolContext, ToolDef};
use crate::mcp::{error::ToolErrorKind, protocol::CallToolResult};
use crate::tool;
use anyhow::{ensure, Context, Result};
use konnect_sexp::writer::{find_balanced_block, find_direct_child_blocks};
use konnect_sexp::{apply_edits, parse_sexp, read_consistent, FileTransition, SexpEdit, SexpNode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "sch_transfer_native_tests.rs"]
mod native_tests;

pub(super) fn tool() -> ToolDef {
    tool!(
        "move_schematic_objects_between_sheets",
        "Move existing circuit objects between explicit hierarchy instances in one saved \
         project, preserving UUIDs, references and relative geometry. Defaults to dry-run. \
         Requires kicad-cli for before/after netlist and ERC checks. Both sheets and the \
         project PCB must be closed. Reused source/destination sheets, buses, hierarchical \
         label moves, boundary connections and local-label collisions are refused with \
         explicit rewiring diagnostics. Updates saved PCB schematic paths and renamed \
         copper nets in the same rollback-on-failure transaction. Refuses netclass pattern \
         changes and PCB net-name collisions. Never deletes a source sheet.",
        json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "schematic": {"type":"string", "description":"Root .kicad_sch beside its .kicad_pro"},
                "source_sheet_instance": {"type":"string", "description":"/<root UUID>[/<sheet UUID>...], using sheet-symbol UUIDs, not page numbers or child-file root UUIDs"},
                "destination_sheet_instance": {"type":"string"},
                "object_uuids": {"type":"array", "minItems":1, "uniqueItems":true, "items":{"type":"string"}},
                "offset_x_mm": {"type":"number"}, "offset_y_mm": {"type":"number"},
                "dry_run": {"type":"boolean", "default":true}
            },
            "required":["schematic","source_sheet_instance","destination_sheet_instance","object_uuids","offset_x_mm","offset_y_mm"]
        }),
        |args, ctx| async move { handle_move_schematic_objects_between_sheets(args, ctx).await }
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schematic: PathBuf,
    source_sheet_instance: String,
    destination_sheet_instance: String,
    object_uuids: Vec<String>,
    offset_x_mm: f64,
    offset_y_mm: f64,
    #[serde(default = "default_dry_run")]
    dry_run: bool,
}

fn default_dry_run() -> bool {
    true
}

struct Document {
    before: String,
    after: String,
}

struct Plan {
    root: PathBuf,
    documents: BTreeMap<PathBuf, Document>,
    board: Option<PathBuf>,
    netlist_identities: BTreeMap<String, BTreeSet<String>>,
    report: Value,
}

fn blocked(reason: impl ToString) -> CallToolResult {
    let reason = reason.to_string();
    CallToolResult::error_kind(
        ToolErrorKind::PlanBlocked {
            operation: "move_schematic_objects_between_sheets".into(),
            reasons: vec![reason.clone()],
        },
        format!("Transfer was not applied. {reason}"),
    )
}

async fn handle_move_schematic_objects_between_sheets(
    args: &Value,
    ctx: &ToolContext,
) -> Result<CallToolResult> {
    use crate::outcome::{self, OutcomeStatus};
    let result = match transfer(args, ctx).await {
        Ok(result) => result,
        Err(error) => blocked(format!("{error:#}")),
    };
    let uncertain = crate::mcp::error::extract_error_kind(&result)
        .is_some_and(|kind| kind == "mutation_outcome_uncertain");
    let status = if uncertain {
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

async fn transfer(args: &Value, ctx: &ToolContext) -> Result<CallToolResult> {
    let request: Request = match serde_json::from_value(args.clone()) {
        Ok(request) => request,
        Err(error) => return Ok(invalid_arg("arguments", &error.to_string())),
    };
    let mut plan = match Plan::prepare(&request) {
        Ok(plan) => plan,
        Err(error) => return Ok(blocked(format!("{error:#}"))),
    };
    if let Some(board) = &plan.board {
        if let Some(error) = closed_board(ctx, board).await? {
            return Ok(error);
        }
    }
    match verify_with_kicad(&plan, &ctx.config.kicad_cli).await {
        Ok(impacts) => {
            if let Err(error) = plan.preserve_pcb_net_names(&impacts) {
                return Ok(blocked(format!(
                    "Dependent net-name update refused: {error:#}"
                )));
            }
            plan.report["connectivity_impacts"] = json!(impacts);
        }
        Err(error) => {
            return Ok(blocked(format!(
                "KiCad candidate verification failed: {error:#}"
            )))
        }
    }
    plan.report["verification"] = json!({"source":"kicad_cli", "netlist":"equivalent_pin_partitions", "erc":"no_new_violations"});
    if request.dry_run {
        plan.report["status"] = json!("planned");
        return Ok(CallToolResult::json(&plan.report));
    }
    // A long CLI verification must not leave the board-open observation stale.
    if let Some(board) = &plan.board {
        if let Some(error) = closed_board(ctx, board).await? {
            return Ok(error);
        }
    }
    let transitions = plan
        .documents
        .iter()
        .map(|(path, doc)| FileTransition::replace(path, &doc.before, &doc.after))
        .collect();
    let commit =
        konnect_sexp::commit_reversible_file_transaction(plan.root.parent().unwrap(), transitions);
    if let Err(error) = commit {
        // Only byte-exact before images AND no retained journal prove refusal.
        let unchanged = plan
            .documents
            .iter()
            .all(|(path, doc)| read_consistent(path).is_ok_and(|source| source == doc.before));
        let no_journal = konnect_sexp::inspect_file_transactions(plan.root.parent().unwrap())
            .is_ok_and(|journals| journals.is_empty());
        if unchanged && no_journal {
            return Ok(blocked(error));
        }
        return Ok(CallToolResult::error_kind(ToolErrorKind::MutationOutcomeUncertain {
            operation:"move_schematic_objects_between_sheets".into(),
            path:plan.root.display().to_string(), reason:error.to_string(),
        },format!("Transfer persistence or rollback is uncertain. Inspect project transactions and files before retrying. {error}")));
    }
    for (path, doc) in &plan.documents {
        if !read_consistent(path).is_ok_and(|source| source == doc.after) {
            return Ok(super::mutation_outcome_uncertain(
                path,
                "move_schematic_objects_between_sheets",
                "Committed-file readback differs from the verified candidate",
            ));
        }
    }
    plan.report["status"] = json!("moved");
    plan.report["transaction_id"] = json!(commit.unwrap().id);
    Ok(CallToolResult::json(&plan.report))
}

async fn closed_board(ctx: &ToolContext, board: &Path) -> Result<Option<CallToolResult>> {
    use super::live_board::{self, LiveBoard};
    match live_board::observe(ctx, board, |_, _| Ok(())).await? {
        LiveBoard::NotOpen { .. } | LiveBoard::NeverReached(_) =>
            Ok(super::pcb_board::board_lock_refusal(board)),
        _ => Ok(Some(blocked("Cannot prove that the project PCB is closed. Close it cleanly and retry; live or uncertain board state cannot accept saved schematic-path edits."))),
    }
}

impl Plan {
    /// A sheet-local net keeps its electrical identity, but its exported name
    /// gains the destination sheet prefix. Rename every saved copper/pad net
    /// reference together so the next PCB sync does not detach routed pads.
    fn preserve_pcb_net_names(&mut self, impacts: &[Value]) -> Result<()> {
        let mut mapping = BTreeMap::new();
        let mut destinations = BTreeSet::new();
        for impact in impacts {
            let old = impact["old_name"]
                .as_str()
                .context("Missing old net name")?;
            let new = impact["new_name"]
                .as_str()
                .context("Missing new net name")?;
            ensure!(
                !old.is_empty() && !new.is_empty(),
                "Cannot rename the unconnected pseudo-net"
            );
            ensure!(
                mapping.insert(old.to_owned(), new.to_owned()).is_none()
                    && destinations.insert(new.to_owned()),
                "Net-name mapping is not one-to-one"
            );
        }
        if mapping.is_empty() {
            return Ok(());
        }
        let pro_path = self.root.with_extension("kicad_pro");
        let project = self
            .documents
            .get_mut(&pro_path)
            .context("Project settings were not bound")?;
        project.after = move_project_net_assignments(&project.after, &mapping)?;
        if let Some(board) = &self.board {
            let document = self.documents.get_mut(board).unwrap();
            document.after = move_pcb_net_names(&document.after, &mapping)?;
            self.report["pcb_net_name_mapping"] = json!(mapping);
        }
        self.report["affected_files"] = json!(self
            .documents
            .iter()
            .filter(|(_, document)| document.before != document.after)
            .map(|(path, _)| path.display().to_string())
            .collect::<Vec<_>>());
        Ok(())
    }

    fn prepare(request: &Request) -> Result<Self> {
        ensure!(
            request.offset_x_mm.is_finite() && request.offset_y_mm.is_finite(),
            "Offsets must be finite"
        );
        ensure!(
            request.offset_x_mm.abs() <= 100_000.0 && request.offset_y_mm.abs() <= 100_000.0,
            "Offsets exceed the supported schematic range"
        );
        let selected = request
            .object_uuids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        ensure!(
            !selected.is_empty() && selected.len() == request.object_uuids.len(),
            "Select unique, nonempty object UUIDs"
        );
        for uuid in &selected {
            uuid::Uuid::parse_str(uuid).context("Invalid selected UUID")?;
        }
        let root = std::fs::canonicalize(&request.schematic)?;
        ensure!(
            root.extension().is_some_and(|ext| ext == "kicad_sch"),
            "schematic must be a root .kicad_sch"
        );
        let directory = root.parent().unwrap();
        ensure!(
            root.with_extension("kicad_pro").is_file(),
            "Root schematic must have a same-named .kicad_pro"
        );
        ensure!(
            konnect_sexp::inspect_file_transactions(directory)?.is_empty(),
            "Pending project transaction; inspect and recover it first"
        );
        let project = root
            .file_stem()
            .and_then(|name| name.to_str())
            .context("Project name is not Unicode")?;
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
        let source_path = instances
            .get(&request.source_sheet_instance)
            .context("Source sheet instance does not exist in this root hierarchy")?
            .clone();
        let destination_path = instances
            .get(&request.destination_sheet_instance)
            .context("Destination sheet instance does not exist in this root hierarchy")?
            .clone();
        ensure!(
            source_path != destination_path,
            "Source and destination must be different schematic files"
        );
        for path in [&source_path, &destination_path] {
            ensure!(instances.values().filter(|value| *value == path).count() == 1,
                "Sheet '{}' has repeated instances. Moving one instance without cloning identities is unsupported", path.display());
        }
        let source = &documents[&source_path].before;
        let destination = &documents[&destination_path].before;
        let source_tree = parse_sexp(source)?;
        let destination_tree = parse_sexp(destination)?;
        for (path, instance, content) in [
            (&source_path, &request.source_sheet_instance, source),
            (
                &destination_path,
                &request.destination_sheet_instance,
                destination,
            ),
        ] {
            validate_instances(path, content, project, instance, path != &root)?;
            let owner = super::resolve_schematic_ownership(path)
                .map_err(|error| {
                    anyhow::anyhow!("Cannot prove unique project ownership: {error:?}")
                })?
                .context("Sheet has no proven owning project")?;
            ensure!(
                super::same_schematic_document(&owner.root_schematic, &root),
                "Sheet is owned by a different root project"
            );
        }
        for tree in [&source_tree, &destination_tree] {
            for group in tree.find_all("group") {
                let members = group
                    .find("members")
                    .context("Malformed schematic group")?
                    .children()
                    .context("Malformed group members")?;
                ensure!(!members.iter().skip(1).any(|member| member.as_str().is_some_and(|uuid| selected.contains(uuid))),
                    "Selected objects belong to a schematic group. Ungroup them explicitly before transferring; group membership cannot span sheet files");
            }
            for tag in ["bus", "bus_entry", "bus_alias", "symbol_instances"] {
                ensure!(tree.find(tag).is_none(), "Affected sheet uses unsupported '{tag}'; resolve bus or legacy instance-table semantics before transfer");
            }
        }
        let items = object_blocks(source)?;
        ensure!(
            selected.iter().all(|uuid| items.contains_key(uuid)),
            "Every selected UUID must identify exactly one top-level source object"
        );
        let mut source_edits = Vec::new();
        let mut moved = Vec::new();
        let mut path_mapping = BTreeMap::new();
        let mut moved_references = BTreeSet::new();
        let mut netlist_identities: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        let mut moved_ids = BTreeSet::new();
        for uuid in &selected {
            let (start, end) = items[uuid];
            let block = &source[start..end];
            let tree = parse_sexp(block)?;
            let kind = tree.head().context("Object has no type")?;
            ensure!(matches!(kind, "symbol" | "wire" | "junction" | "label" | "global_label" | "no_connect" | "text" | "text_box"),
                "Object {uuid} has unsupported type '{kind}'. Hierarchical labels and sheet ports require explicit boundary rewiring; sheets are never moved implicitly");
            collect_uuids(&tree, &mut moved_ids)?;
            let mut transferred = translate(block, request.offset_x_mm, request.offset_y_mm)?;
            if kind == "symbol" {
                let reference =
                    property(&tree, "Reference").context("Moved symbol lacks Reference")?;
                ensure!(
                    !reference.contains('?') && !reference.is_empty(),
                    "Annotate symbol {uuid} before transfer"
                );
                moved_references.insert(reference.to_owned());
                if tree.find_str("on_board") != Some("no") && !reference.starts_with('#') {
                    netlist_identities
                        .entry(reference.to_owned())
                        .or_default()
                        .insert(uuid.clone());
                }
                transferred = move_instance(
                    &transferred,
                    project,
                    &request.source_sheet_instance,
                    &request.destination_sheet_instance,
                    reference,
                )?;
                path_mapping.insert(
                    format!("{}/{uuid}", request.source_sheet_instance),
                    format!("{}/{uuid}", request.destination_sheet_instance),
                );
                let name = tree
                    .find_str("lib_name")
                    .or_else(|| tree.find_str("lib_id"))
                    .context("Symbol lacks library identity")?;
                let lib = library_definition(source, name)?
                    .context("Source symbol has no embedded library definition")?;
                ensure!(
                    parse_sexp(&lib)?.find("extends").is_none(),
                    "Derived library definitions require flattening before transfer"
                );
                if let Some(existing) = library_definition(destination, name)? {
                    ensure!(parse_sexp(&lib)? == parse_sexp(&existing)?, "Destination library definition '{name}' conflicts with the source; resolve it explicitly");
                } else {
                    definitions.insert(name.to_owned(), lib);
                }
            }
            source_edits.push(SexpEdit::delete(start, end));
            moved.push(json!({"uuid":uuid, "kind":kind}));
            moved.last_mut().unwrap()["block"] = json!(transferred);
        }
        // A physical symbol's units must stay together. Do not silently split
        // a footprint's unit association over two schematic paths.
        for (path, document) in &documents {
            let tree = parse_sexp(&document.before)?;
            for symbol in tree.find_all("symbol") {
                if property(symbol, "Reference")
                    .is_some_and(|reference| moved_references.contains(reference))
                {
                    ensure!(*path == source_path && symbol.find_str("uuid").is_some_and(|uuid| selected.contains(uuid)),
                        "All units of a moved reference must be selected in the source; a reference/unit conflict exists in '{}'", path.display());
                }
            }
            let mut ids = BTreeSet::new();
            for child in tree.children().unwrap_or_default() {
                if child.head() == Some("lib_symbols") {
                    continue;
                }
                if *path == source_path
                    && child
                        .find_str("uuid")
                        .is_some_and(|uuid| selected.contains(uuid))
                {
                    continue;
                }
                collect_uuids(child, &mut ids)?;
            }
            ensure!(
                ids.is_disjoint(&moved_ids),
                "Moved object or pin UUID conflicts with an unselected object in '{}'",
                path.display()
            );
        }
        check_boundary(&source_tree, &selected, false)?;
        let mut destination_after = destination.to_owned();
        if !definitions.is_empty() {
            let lib_range = find_direct_child_blocks(destination, "kicad_sch")
                .into_iter()
                .find(|&(start, end)| {
                    parse_sexp(&destination[start..end])
                        .is_ok_and(|node| node.head() == Some("lib_symbols"))
                })
                .context("Destination lacks lib_symbols container")?;
            let additions = definitions
                .values()
                .map(|block| format!("\n\t\t{block}"))
                .collect::<String>();
            destination_after = apply_edits(
                destination_after,
                vec![SexpEdit::insert(lib_range.1 - 1, additions)],
            );
        }
        // KiCad processes connectivity markers before placed symbols. Preserve
        // that ordering instead of appending a UUID-sorted mixture at the end.
        moved.sort_by_key(|item| item["kind"] == "symbol");
        let blocks = moved
            .iter_mut()
            .map(|item| {
                let block = item.as_object_mut().unwrap().remove("block").unwrap();
                format!("\n\t{}\n", block.as_str().unwrap())
            })
            .collect::<String>();
        let anchor = find_direct_child_blocks(&destination_after, "kicad_sch")
            .into_iter()
            .find(|&(start, end)| {
                parse_sexp(&destination_after[start..end]).is_ok_and(|node| {
                    matches!(
                        node.head(),
                        Some("symbol" | "sheet_instances" | "symbol_instances" | "embedded_fonts")
                    )
                })
            })
            .map(|(start, _)| start)
            .unwrap_or(
                find_balanced_block(&destination_after, destination_after.find('(').unwrap())
                    .context("Malformed destination")?
                    .1
                    - 1,
            );
        let line_start = destination_after[..anchor]
            .rfind('\n')
            .map_or(0, |pos| pos + 1);
        let anchor = if destination_after[line_start..anchor].trim().is_empty() {
            line_start
        } else {
            anchor
        };
        destination_after = apply_edits(destination_after, vec![SexpEdit::insert(anchor, blocks)]);
        check_boundary(&parse_sexp(&destination_after)?, &selected, true)?;
        let source_after = apply_edits(source.to_owned(), source_edits);
        parse_sexp(&source_after)?;
        validate_instances(
            &source_path,
            &source_after,
            project,
            &request.source_sheet_instance,
            source_path != root,
        )?;
        validate_instances(
            &destination_path,
            &destination_after,
            project,
            &request.destination_sheet_instance,
            destination_path != root,
        )?;
        documents.get_mut(&source_path).unwrap().after = source_after;
        documents.get_mut(&destination_path).unwrap().after = destination_after;
        // Preserve all read dependencies, including project/ERC settings.
        let pro = root.with_extension("kicad_pro");
        load_document(&pro, &mut documents)?;
        for name in ["sym-lib-table", "fp-lib-table"] {
            let path = directory.join(name);
            if path.exists() {
                load_document(&path, &mut documents)?;
            }
        }
        let board_path = root.with_extension("kicad_pcb");
        // PCB paths and exported netlist sheet stamps omit the schematic root
        // UUID. Symbol instance records include it. Never confuse the two.
        let pcb_path_mapping = path_mapping
            .iter()
            .map(|(old, new)| Ok((without_root_uuid(old)?, without_root_uuid(new)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let board = if board_path.exists() {
            let canonical = std::fs::canonicalize(&board_path)?;
            ensure!(canonical == board_path, "Project PCB must not be a symlink");
            load_document(&board_path, &mut documents)?;
            let doc = documents.get_mut(&board_path).unwrap();
            doc.after = move_pcb_paths(&doc.before, &pcb_path_mapping)?;
            Some(board_path)
        } else {
            None
        };
        let files = documents
            .iter()
            .filter(|(_, doc)| doc.before != doc.after)
            .map(|(path, _)| path.display().to_string())
            .collect::<Vec<_>>();
        let report = json!({
            "dry_run":request.dry_run, "source":"saved_project", "objects":moved,
            "source_sheet_instance":request.source_sheet_instance,
            "destination_sheet_instance":request.destination_sheet_instance,
            "offset_mm":{"x":request.offset_x_mm,"y":request.offset_y_mm},
            "path_mapping":path_mapping, "pcb_path_mapping":pcb_path_mapping,
            "library_definitions_added":definitions.keys().collect::<Vec<_>>(),
            "affected_files":files, "pcb_present":board.is_some(),
            "source_sheet_deleted":false,
            "connectivity_impacts":[], "boundary_rewiring":[],
            "limitations":["Saved-file operation; reload editors after completion", "Sheet-local net names gain the destination hierarchy prefix", "Crash recovery rolls back an uncommitted transfer"]
        });
        Ok(Self {
            root,
            documents,
            board,
            netlist_identities,
            report,
        })
    }
}

fn without_root_uuid(path: &str) -> Result<String> {
    let (_, tail) = path
        .trim_start_matches('/')
        .split_once('/')
        .context("Symbol path has no UUID after its root")?;
    Ok(format!("/{tail}"))
}

fn validate_instances(
    path: &Path,
    source: &str,
    project: &str,
    instance: &str,
    is_child: bool,
) -> Result<()> {
    let schematic = konnect_schematic_editor::Schematic::from_source(path, source)?;
    let context = super::SheetInstanceContext {
        project_name: project.into(),
        instance_path: instance.into(),
        instance_paths: vec![instance.into()],
        is_child_sheet: is_child,
    };
    super::validate_sheet_instance_state(path, &schematic, &context)
        .map_err(|error| anyhow::anyhow!("Stale hierarchy state: {error:?}"))
}

fn load_document(path: &Path, documents: &mut BTreeMap<PathBuf, Document>) -> Result<()> {
    if !documents.contains_key(path) {
        let before = read_consistent(path)?;
        documents.insert(
            path.to_path_buf(),
            Document {
                after: before.clone(),
                before,
            },
        );
    }
    Ok(())
}

fn collect_hierarchy(
    path: &Path,
    instance: Option<String>,
    directory: &Path,
    documents: &mut BTreeMap<PathBuf, Document>,
    instances: &mut BTreeMap<String, PathBuf>,
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    ensure!(
        visiting.len() < super::sch_hierarchy::MAX_HIERARCHY_DEPTH,
        "Hierarchy depth limit exceeded"
    );
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot read sheet '{}'", path.display()))?;
    ensure!(
        path.starts_with(directory),
        "Sheet escapes the root project directory"
    );
    ensure!(visiting.insert(path.clone()), "Cyclic sheet hierarchy");
    load_document(&path, documents)?;
    let tree = parse_sexp(&documents[&path].before)?;
    ensure!(
        tree.head() == Some("kicad_sch"),
        "Invalid schematic root in '{}'",
        path.display()
    );
    let root_uuid = tree.find_str("uuid").context("Schematic lacks root UUID")?;
    uuid::Uuid::parse_str(root_uuid).context("Invalid schematic root UUID")?;
    let instance = instance.unwrap_or_else(|| format!("/{root_uuid}"));
    ensure!(
        instances.insert(instance.clone(), path.clone()).is_none(),
        "Conflicting hierarchy instance {instance}"
    );
    for sheet in tree.find_all("sheet") {
        let file = property(sheet, "Sheetfile").context("Sheet lacks Sheetfile")?;
        ensure!(
            !file.contains("${") && Path::new(file).is_relative(),
            "Variable/absolute sheet paths are unsupported"
        );
        let uuid = sheet.find_str("uuid").context("Sheet lacks UUID")?;
        uuid::Uuid::parse_str(uuid).context("Invalid sheet UUID")?;
        collect_hierarchy(
            &path.parent().unwrap().join(file),
            Some(format!("{instance}/{uuid}")),
            directory,
            documents,
            instances,
            visiting,
        )?;
    }
    visiting.remove(&path);
    Ok(())
}

fn property<'a>(tree: &'a SexpNode, name: &str) -> Option<&'a str> {
    tree.find_all("property")
        .into_iter()
        .find(|node| node.get(1).and_then(SexpNode::as_str) == Some(name))?
        .get(2)?
        .as_str()
}

fn collect_uuids(tree: &SexpNode, ids: &mut BTreeSet<String>) -> Result<()> {
    if tree.head() == Some("uuid") {
        let uuid = tree
            .get(1)
            .and_then(SexpNode::as_str)
            .context("Malformed UUID")?;
        uuid::Uuid::parse_str(uuid).context("Invalid object or pin UUID")?;
        ensure!(ids.insert(uuid.to_owned()), "Duplicate UUID {uuid}");
    }
    for child in tree.children().unwrap_or_default() {
        collect_uuids(child, ids)?;
    }
    Ok(())
}

fn object_blocks(source: &str) -> Result<BTreeMap<String, (usize, usize)>> {
    let mut items = BTreeMap::new();
    for (start, end) in find_direct_child_blocks(source, "kicad_sch") {
        let node = parse_sexp(&source[start..end])?;
        if let Some(uuid) = node.find_str("uuid") {
            ensure!(
                items.insert(uuid.to_owned(), (start, end)).is_none(),
                "Duplicate top-level UUID {uuid}"
            );
        }
    }
    Ok(items)
}

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn translate(block: &str, dx: f64, dy: f64) -> Result<String> {
    let node = parse_sexp(block)?;
    let head = node.head().context("Missing object type")?;
    let mut edits = Vec::new();
    for (start, end) in find_direct_child_blocks(block, head) {
        let part = &block[start..end];
        let tree = parse_sexp(part)?;
        match tree.head() {
            Some("at" | "start" | "end" | "xy") => {
                let x = tree.get_f64(1).context("Missing X coordinate")? + dx;
                let y = tree.get_f64(2).context("Missing Y coordinate")? + dy;
                ensure!(
                    x.is_finite() && y.is_finite() && x.abs() <= 100_000.0 && y.abs() <= 100_000.0,
                    "Invalid translated geometry"
                );
                // Preserve angle and any unknown trailing coordinate fields.
                let tail = tree.children().unwrap()[3..]
                    .iter()
                    .map(|node| node.as_str().context("Unsupported coordinate metadata"))
                    .collect::<Result<Vec<_>>>()?
                    .join(" ");
                let text = format!(
                    "({} {} {}{})",
                    tree.head().unwrap(),
                    konnect_schematic_editor::types::fmt_f64(x),
                    konnect_schematic_editor::types::fmt_f64(y),
                    if tail.is_empty() {
                        String::new()
                    } else {
                        format!(" {tail}")
                    }
                );
                edits.push(SexpEdit::replace(start, end, text));
            }
            Some("property" | "pts") => {
                edits.push(SexpEdit::replace(start, end, translate(part, dx, dy)?))
            }
            _ => {}
        }
    }
    Ok(apply_edits(block.to_owned(), edits))
}

fn move_instance(
    block: &str,
    project: &str,
    old: &str,
    new: &str,
    reference: &str,
) -> Result<String> {
    let tree = parse_sexp(block)?;
    let projects = tree
        .find("instances")
        .context("Symbol lacks instances; save/repair hierarchy first")?
        .find_all("project");
    ensure!(
        projects.len() == 1 && projects[0].get(1).and_then(SexpNode::as_str) == Some(project),
        "Symbol has foreign or ambiguous project instances"
    );
    let paths = projects[0].find_all("path");
    ensure!(
        paths.len() == 1 && paths[0].get(1).and_then(SexpNode::as_str) == Some(old),
        "Symbol instance path is stale or repeated"
    );
    ensure!(
        paths[0].find_str("reference") == Some(reference),
        "Symbol reference disagrees with its instance record"
    );
    ensure!(
        paths[0].find_str("unit") == tree.find_str("unit"),
        "Symbol unit disagrees with its instance record"
    );
    let mut edits = Vec::new();
    for offset in konnect_sexp::writer::find_block_starts(block, "path") {
        let (_, end) = find_balanced_block(block, offset).context("Malformed instance path")?;
        let part = &block[offset..end];
        let path_node = parse_sexp(part)?;
        ensure!(
            path_node.get(1).and_then(SexpNode::as_str) == Some(old),
            "Unsupported additional symbol path"
        );
        let children = find_direct_child_blocks(part, "path");
        let tail = children
            .first()
            .map(|&(start, _)| &part[start..part.len() - 1])
            .unwrap_or("");
        edits.push(SexpEdit::replace(
            offset,
            end,
            format!("(path {} {tail})", quote(new)),
        ));
    }
    Ok(apply_edits(block.to_owned(), edits))
}

fn library_definition(source: &str, name: &str) -> Result<Option<String>> {
    let Some((start, end)) = find_direct_child_blocks(source, "kicad_sch")
        .into_iter()
        .find(|&(start, end)| {
            parse_sexp(&source[start..end]).is_ok_and(|node| node.head() == Some("lib_symbols"))
        })
    else {
        return Ok(None);
    };
    let library = &source[start..end];
    let mut found = None;
    for (start, end) in find_direct_child_blocks(library, "lib_symbols") {
        let block = &library[start..end];
        let node = parse_sexp(block)?;
        if node.head() == Some("symbol") && node.get(1).and_then(SexpNode::as_str) == Some(name) {
            ensure!(found.is_none(), "Duplicate library definition {name}");
            found = Some(block.to_owned());
        }
    }
    Ok(found)
}

fn move_pcb_paths(source: &str, mapping: &BTreeMap<String, String>) -> Result<String> {
    let tree = parse_sexp(source)?;
    ensure!(tree.head() == Some("kicad_pcb"), "Invalid project PCB");
    let mut seen = BTreeSet::new();
    let mut edits = Vec::new();
    for (start, end) in find_direct_child_blocks(source, "kicad_pcb") {
        let block = &source[start..end];
        let footprint = parse_sexp(block)?;
        if footprint.head() != Some("footprint") {
            continue;
        }
        for (pstart, pend) in find_direct_child_blocks(block, "footprint") {
            let path = parse_sexp(&block[pstart..pend])?;
            if path.head() != Some("path") {
                continue;
            }
            let old = path
                .get(1)
                .and_then(SexpNode::as_str)
                .context("Malformed footprint schematic path")?;
            if let Some(new) = mapping.get(old) {
                ensure!(
                    seen.insert(old.to_owned()),
                    "Multiple PCB footprints claim schematic path {old}"
                );
                edits.push(SexpEdit::replace(
                    start + pstart,
                    start + pend,
                    format!("(path {})", quote(new)),
                ));
            } else {
                ensure!(
                    !mapping.values().any(|new| new == old),
                    "PCB already contains a destination schematic path {old}"
                );
            }
        }
    }
    Ok(apply_edits(source.to_owned(), edits))
}

fn move_pcb_net_names(source: &str, mapping: &BTreeMap<String, String>) -> Result<String> {
    use konnect_sexp::net::{collect_net_keys, net_id, net_name};
    let tree = parse_sexp(source)?;
    let present = collect_net_keys(&tree);
    for (old, new) in mapping {
        ensure!(old==new || !present.contains(new),
            "PCB already contains destination net '{new}'; renaming '{old}' would merge unrelated nets");
    }
    let mut edits = Vec::new();
    for tag in ["net", "net_name", "add_net"] {
        for start in konnect_sexp::writer::find_block_starts(source, tag) {
            let (_, end) =
                find_balanced_block(source, start).context("Malformed PCB net reference")?;
            let node = parse_sexp(&source[start..end])?;
            let name = if tag == "net" {
                net_name(&node)
            } else {
                node.get(1).and_then(SexpNode::as_str)
            };
            let Some(new) = name.and_then(|old| mapping.get(old)) else {
                continue;
            };
            let replacement = if tag == "net" {
                if let Some(id) = net_id(&node) {
                    ensure!(
                        node.children().unwrap().len() == 3,
                        "Unsupported legacy net declaration"
                    );
                    format!("(net {id} {})", quote(new))
                } else {
                    ensure!(
                        node.children().unwrap().len() == 2,
                        "Unsupported named net reference"
                    );
                    format!("(net {})", quote(new))
                }
            } else {
                format!("({tag} {})", quote(new))
            };
            edits.push(SexpEdit::replace(start, end, replacement));
        }
    }
    Ok(apply_edits(source.to_owned(), edits))
}

fn move_project_net_assignments(
    source: &str,
    mapping: &BTreeMap<String, String>,
) -> Result<String> {
    let mut project: Value = serde_json::from_str(source).context("Invalid project JSON")?;
    let Some(settings) = project.get_mut("net_settings") else {
        return Ok(source.to_owned());
    };
    ensure!(
        settings.is_object(),
        "Project net_settings must be an object"
    );
    if let Some(patterns) = settings.get("netclass_patterns") {
        let patterns = patterns
            .as_array()
            .context("Project netclass_patterns must be an array")?;
        for (old, new) in mapping {
            let matching = |name: &str| -> Result<BTreeSet<String>> {
                let mut classes = BTreeSet::new();
                for rule in patterns {
                    let pattern = rule["pattern"]
                        .as_str()
                        .context("Invalid netclass pattern")?;
                    let class = rule["netclass"]
                        .as_str()
                        .context("Invalid pattern netclass")?;
                    if super::pcb_routing::wildcard_matches(pattern, name) {
                        classes.insert(class.to_owned());
                    }
                }
                Ok(classes)
            };
            ensure!(matching(old)?==matching(new)?,
                "Renaming '{old}' to '{new}' changes matching netclasses. Review the project netclass patterns explicitly before transfer");
        }
    }
    let Some(assignments) = settings.get_mut("netclass_assignments") else {
        return Ok(source.to_owned());
    };
    let assignments = assignments
        .as_object_mut()
        .context("Project netclass_assignments must be an object")?;
    let mut changed = false;
    for (old, new) in mapping {
        if let Some(value) = assignments.remove(old) {
            ensure!(
                assignments
                    .get(new)
                    .is_none_or(|existing| existing == &value),
                "Destination net '{new}' has conflicting explicit netclass assignments"
            );
            assignments.insert(new.clone(), value);
            changed = true;
        }
    }
    if changed {
        Ok(format!("{}\n", serde_json::to_string_pretty(&project)?))
    } else {
        Ok(source.to_owned())
    }
}

/// Refuse physical boundary connections and name-based local boundaries before
/// asking KiCad. Global labels and power nets are checked by the full netlist.
fn check_boundary(tree: &SexpNode, selected: &BTreeSet<String>, destination: bool) -> Result<()> {
    use konnect_sexp::geometry::{point_on_segment, points_coincident};
    use konnect_sexp::schematic::pin_endpoint;
    type Point = (f64, f64);
    struct Geometry {
        selected: bool,
        uuid: String,
        points: Vec<Point>,
        segments: Vec<(Point, Point)>,
    }
    let pins =
        super::resolved_placed_pins_by_reference(tree).context(super::UNRESOLVED_PIN_GEOMETRY)?;
    let mut geometry = Vec::new();
    let mut names: BTreeMap<String, Vec<(bool, bool)>> = BTreeMap::new();
    for node in tree.children().unwrap_or_default() {
        let kind = node.head().unwrap_or("");
        if !matches!(
            kind,
            "symbol"
                | "wire"
                | "junction"
                | "label"
                | "global_label"
                | "hierarchical_label"
                | "sheet"
                | "no_connect"
        ) {
            continue;
        }
        let uuid = node
            .find_str("uuid")
            .context("Electrical object lacks UUID")?
            .to_owned();
        let is_selected = selected.contains(&uuid);
        let mut g = Geometry {
            selected: is_selected,
            uuid: uuid.clone(),
            points: Vec::new(),
            segments: Vec::new(),
        };
        match kind {
            "symbol" => {
                let (_, placed) = pins
                    .iter()
                    .find(|(symbol, _)| symbol.uuid.as_deref() == Some(uuid.as_str()))
                    .context("Symbol pin geometry missing")?;
                g.points = placed
                    .iter()
                    .map(|(pin, transform)| pin_endpoint(pin, *transform))
                    .collect();
            }
            "wire" => {
                let pts = node.find("pts").context("Wire lacks pts")?.find_all("xy");
                ensure!(pts.len() == 2, "Only two-point wires are supported");
                let a = (
                    pts[0].get_f64(1).context("Invalid wire X")?,
                    pts[0].get_f64(2).context("Invalid wire Y")?,
                );
                let b = (
                    pts[1].get_f64(1).context("Invalid wire X")?,
                    pts[1].get_f64(2).context("Invalid wire Y")?,
                );
                g.points = vec![a, b];
                g.segments.push((a, b));
            }
            "sheet" => {
                for pin in node.find_all("pin") {
                    let at = pin.find("at").context("Sheet pin lacks at")?;
                    g.points.push((
                        at.get_f64(1).context("Invalid pin X")?,
                        at.get_f64(2).context("Invalid pin Y")?,
                    ));
                }
            }
            _ => {
                let at = node.find("at").context("Electrical object lacks at")?;
                g.points.push((
                    at.get_f64(1).context("Invalid X")?,
                    at.get_f64(2).context("Invalid Y")?,
                ));
            }
        }
        if matches!(kind, "label" | "hierarchical_label" | "global_label") {
            let name = node
                .get(1)
                .and_then(SexpNode::as_str)
                .context("Label lacks name")?;
            names
                .entry(name.into())
                .or_default()
                .push((is_selected, kind != "global_label"));
        }
        geometry.push(g);
    }
    for label in konnect_sexp::schematic::extract_power_symbol_labels(tree) {
        let is_selected = label
            .uuid
            .as_ref()
            .is_some_and(|uuid| selected.contains(uuid));
        let instance = &pins
            .iter()
            .find(|(instance, _)| instance.uuid == label.uuid)
            .context("Cannot bind power symbol scope")?
            .0;
        let libraries = tree
            .find("lib_symbols")
            .context("Missing lib_symbols")?
            .find_all("symbol");
        let definition = konnect_sexp::schematic::find_lib_symbol(&libraries, instance)
            .context("Cannot resolve power symbol scope")?;
        let local = definition
            .find("power")
            .and_then(|node| node.get(1))
            .and_then(SexpNode::as_str)
            == Some("local");
        names
            .entry(label.net)
            .or_default()
            .push((is_selected, local));
    }
    for (name, occurrences) in names {
        let crosses = occurrences.iter().any(|(is_selected, _)| *is_selected)
            && occurrences.iter().any(|(is_selected, _)| !*is_selected);
        let local = occurrences.iter().any(|(_, local)| *local);
        ensure!(!crosses || !local, "Sheet-local label or power name '{name}' crosses the {} selection boundary. Explicitly rewire or rename it before transfer", if destination {"destination"} else {"source"});
    }
    let touch = |point: Point, g: &Geometry| {
        g.points
            .iter()
            .any(|&(x, y)| points_coincident(point.0, point.1, x, y, 0.01))
            || g.segments
                .iter()
                .any(|&(a, b)| point_on_segment(point.0, point.1, a.0, a.1, b.0, b.1, 0.01))
    };
    for a in geometry.iter().filter(|g| g.selected) {
        for b in geometry.iter().filter(|g| !g.selected) {
            ensure!(!a.points.iter().any(|&p|touch(p,b)) && !b.points.iter().any(|&p|touch(p,a)),
                "Physical {} boundary connection between {} and {}. Select the complete connected geometry or explicitly rewire before transfer", if destination {"destination"} else {"source"},a.uuid,b.uuid);
        }
    }
    Ok(())
}

async fn verify_with_kicad(plan: &Plan, cli_path: &str) -> Result<Vec<Value>> {
    ensure!(
        !cli_path.is_empty(),
        "kicad-cli is required; no unverified apply is supported"
    );
    let mut netlists = Vec::new();
    let mut violations = Vec::new();
    let mut net_names = Vec::new();
    for after in [false, true] {
        let identities = ErcIdentityIndex::build(plan, after)?;
        let snapshot = tempfile::tempdir()?;
        let directory = plan.root.parent().unwrap();
        for (path, doc) in &plan.documents {
            if Some(path) == plan.board.as_ref() {
                continue;
            }
            let target = snapshot.path().join(path.strip_prefix(directory)?);
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::fs::write(target, if after { &doc.after } else { &doc.before })?;
        }
        let root = snapshot.path().join(plan.root.file_name().unwrap());
        let output = snapshot.path().join("transfer.net");
        cli::export_netlist(cli_path, &root, &output, "kicadsexpr").await?;
        let netlist_source = std::fs::read_to_string(output)?;
        let netlist = netlist_partitions(&netlist_source)?;
        net_names.push(netlist_names(&netlist_source)?);
        for (reference, uuids) in &plan.netlist_identities {
            ensure!(
                netlist
                    .0
                    .iter()
                    .any(|(observed, stamps)| observed == reference
                        && stamps.split_whitespace().any(|stamp| uuids.contains(stamp))),
                "KiCad netlist does not resolve moved component {reference} to its preserved UUIDs"
            );
        }
        netlists.push(netlist);
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
    ensure!(netlists[0] == netlists[1], "Before/after KiCad netlists differ in component identities or pin connectivity. Explicit boundary rewiring is required");
    for (key, count) in &violations[1] {
        ensure!(
            *count <= *violations[0].get(key).unwrap_or(&0),
            "Transfer introduces an ERC violation: {} ({}, items {:?})",
            key.0,
            key.1,
            key.2
        );
    }
    Ok(net_names[0].iter().filter_map(|(nodes,old)| {
        let new=net_names[1].get(nodes)?;
        (new != old).then(||json!({"kind":"net_name_changed","old_name":old,"new_name":new,"connectivity":"equivalent"}))
    }).collect())
}

struct ErcPinIdentity {
    identity: String,
    reference: String,
    number: String,
    position: (f64, f64),
    repeated: bool,
}

struct ErcIdentityIndex {
    persisted: BTreeSet<String>,
    pins: Vec<ErcPinIdentity>,
}

impl ErcIdentityIndex {
    fn build(plan: &Plan, after: bool) -> Result<Self> {
        let mut index = Self {
            persisted: BTreeSet::new(),
            pins: Vec::new(),
        };
        for (path, document) in &plan.documents {
            if path.extension().is_none_or(|ext| ext != "kicad_sch") {
                continue;
            }
            let tree = parse_sexp(if after {
                &document.after
            } else {
                &document.before
            })?;
            for child in tree.children().unwrap_or_default() {
                if child.head() != Some("lib_symbols") {
                    collect_uuids(child, &mut index.persisted)?;
                }
            }
            let symbols = tree.find_all("symbol");
            let placed = super::resolved_placed_pins_by_reference(&tree)
                .context(super::UNRESOLVED_PIN_GEOMETRY)?;
            for (symbol, pins) in placed {
                let uuid = symbol
                    .uuid
                    .as_deref()
                    .context("Symbol lacks UUID for ERC comparison")?;
                let projects = symbols
                    .iter()
                    .find(|node| node.find_str("uuid") == Some(uuid))
                    .and_then(|node| node.find("instances"))
                    .map(|node| node.find_all("project"))
                    .unwrap_or_default();
                let repeated = projects.len() != 1 || projects[0].find_all("path").len() != 1;
                for (pin, transform) in pins {
                    index.pins.push(ErcPinIdentity {
                        identity: format!("symbol:{uuid}:pin:{}", pin.number),
                        reference: symbol.reference.clone(),
                        number: pin.number.clone(),
                        position: konnect_sexp::schematic::pin_endpoint(&pin, transform),
                        repeated,
                    });
                }
            }
        }
        Ok(index)
    }

    fn identity(&self, item: &cli::ReportItem) -> Result<String> {
        if let Some(uuid) = &item.uuid {
            if self.persisted.contains(uuid) {
                return Ok(format!("uuid:{uuid}"));
            }
        }
        // KiCad generates fresh SCH_PIN UUIDs while loading a symbol with no
        // saved pin UUID records. A before/after comparison must not mistake
        // those ephemeral IDs for new violations. Bind them to one observed
        // reference and pin endpoint, never to free text or coordinates alone.
        let pos = item
            .pos
            .as_ref()
            .context("ERC item has no persisted UUID or usable pin position")?;
        let candidates = self
            .pins
            .iter()
            .filter(|pin| {
                !pin.reference.is_empty()
                    && item.description.contains(&pin.reference)
                    && item
                        .description
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|token| token == pin.number)
                    && konnect_sexp::geometry::points_coincident(
                        pos.x,
                        pos.y,
                        pin.position.0,
                        pin.position.1,
                        0.001,
                    )
            })
            .collect::<Vec<_>>();
        let identities = candidates
            .iter()
            .map(|pin| pin.identity.as_str())
            .collect::<BTreeSet<_>>();
        ensure!(identities.len()==1 && candidates.iter().all(|pin|!pin.repeated),
            "Cannot uniquely bind transient ERC item '{}' to one saved symbol pin; save/repair pin identities in KiCad before transfer",item.description);
        Ok(identities.first().unwrap().to_string())
    }
}

fn netlist_names(source: &str) -> Result<BTreeMap<BTreeSet<(String, String)>, String>> {
    let tree = parse_sexp(source)?;
    let mut names = BTreeMap::new();
    for net in tree
        .find("nets")
        .context("Netlist lacks nets")?
        .find_all("net")
    {
        let nodes = net
            .find_all("node")
            .into_iter()
            .map(|node| {
                Ok((
                    node.find_str("ref").context("Net node lacks ref")?.into(),
                    node.find_str("pin").context("Net node lacks pin")?.into(),
                ))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        if !nodes.is_empty() {
            names.insert(
                nodes,
                net.find_str("name")
                    .context("KiCad net lacks name")?
                    .to_owned(),
            );
        }
    }
    Ok(names)
}

type NetlistPartitions = (
    BTreeSet<(String, String)>,
    BTreeSet<BTreeSet<(String, String)>>,
);

fn netlist_partitions(source: &str) -> Result<NetlistPartitions> {
    let tree = parse_sexp(source)?;
    ensure!(
        tree.head() == Some("export"),
        "Unexpected KiCad netlist root"
    );
    let mut components = BTreeSet::new();
    for component in tree
        .find("components")
        .context("Netlist lacks components")?
        .find_all("comp")
    {
        let reference = component
            .find_str("ref")
            .context("Netlist component lacks ref")?;
        let mut stamps = component
            .find("tstamps")
            .context("Netlist component lacks UUID")?
            .children()
            .context("Malformed component UUIDs")?[1..]
            .iter()
            .map(|stamp| stamp.as_str().context("Malformed component UUID"))
            .collect::<Result<Vec<_>>>()?;
        ensure!(!stamps.is_empty(), "Netlist component has no UUIDs");
        stamps.sort();
        ensure!(
            components.insert((reference.into(), stamps.join(" "))),
            "Duplicate component identity in netlist"
        );
    }
    let mut nets = BTreeSet::new();
    for net in tree
        .find("nets")
        .context("Netlist lacks nets")?
        .find_all("net")
    {
        let mut nodes = BTreeSet::new();
        for node in net.find_all("node") {
            ensure!(
                nodes.insert((
                    node.find_str("ref").context("Net node lacks ref")?.into(),
                    node.find_str("pin").context("Net node lacks pin")?.into()
                )),
                "Duplicate net node"
            );
        }
        if !nodes.is_empty() {
            ensure!(nets.insert(nodes), "Repeated pin partition");
        }
    }
    Ok((components, nets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::ToolContent;

    const ROOT_UUID: &str = "a73252f6-017c-453c-8321-f1201f92efed";
    const SHEET_UUID: &str = "269824f1-3d6b-4e8b-ac46-c5f8ca062c27";
    const RESISTOR: &str = "8b9b7750-3421-4b18-92a3-4bddb63e9056";
    const NATIVE: &str = include_str!("../../tests/fixtures/component_fields_kicad10.kicad_sch");

    fn fixture() -> (tempfile::TempDir, Request) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("component_fields_kicad10.kicad_sch");
        // The units, all pin UUIDs, fields and library geometry are from the
        // KiCad 10-authored component-field fixture. Only add a destination.
        let sheet = konnect_sexp::schematic::format_hierarchical_sheet(
            konnect_sexp::schematic::HierarchicalSheetSpec {
                name: "Logic regulator",
                file: "logic.kicad_sch",
                x: 10.0,
                y: 10.0,
                width: 30.0,
                height: 20.0,
                project_name: "component_fields_kicad10",
                parent_instance_path: &format!("/{ROOT_UUID}"),
                page: "2",
            },
        );
        let tree = parse_sexp(&sheet).unwrap();
        let generated = tree.find_str("uuid").unwrap();
        let sheet = sheet.replace(generated, SHEET_UUID);
        let close = NATIVE.rfind(')').unwrap();
        std::fs::write(
            &root,
            apply_edits(
                NATIVE.into(),
                vec![SexpEdit::insert(close, format!("\n{sheet}\n"))],
            ),
        )
        .unwrap();
        std::fs::write(
            root.with_extension("kicad_pro"),
            "{\"meta\":{\"filename\":\"component_fields_kicad10.kicad_pro\"}}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("logic.kicad_sch"),
            super::super::blank_schematic_template(),
        )
        .unwrap();
        let request = Request {
            schematic: root,
            source_sheet_instance: format!("/{ROOT_UUID}"),
            destination_sheet_instance: format!("/{ROOT_UUID}/{SHEET_UUID}"),
            object_uuids: vec![RESISTOR.into()],
            offset_x_mm: -50.8,
            offset_y_mm: 25.4,
            dry_run: true,
        };
        (dir, request)
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
    fn transfer_plan_preserves_native_symbol_identity_fields_pins_and_unrelated_blocks() {
        let (dir, request) = fixture();
        let before = snapshot(dir.path());
        let plan = Plan::prepare(&request).unwrap();
        assert_eq!(before, snapshot(dir.path()), "planning is non-mutating");
        let root = &plan.documents[&request.schematic];
        let destination = &plan.documents[&dir.path().join("logic.kicad_sch")];
        let old = parse_sexp(&root.before).unwrap();
        let new = parse_sexp(&destination.after).unwrap();
        let symbol = old
            .find_all("symbol")
            .into_iter()
            .find(|node| node.find_str("uuid") == Some(RESISTOR))
            .unwrap();
        let moved = new.find_all("symbol")[0];
        let mut old_ids = BTreeSet::new();
        let mut new_ids = BTreeSet::new();
        collect_uuids(symbol, &mut old_ids).unwrap();
        collect_uuids(moved, &mut new_ids).unwrap();
        assert_eq!(old_ids, new_ids);
        for field in [
            "Reference",
            "Value",
            "Footprint",
            "Datasheet",
            "Description",
        ] {
            assert_eq!(property(symbol, field), property(moved, field));
        }
        for field in ["unit", "dnp", "in_bom", "on_board", "exclude_from_sim"] {
            assert_eq!(symbol.find_str(field), moved.find_str(field));
        }
        let at = symbol.find("at").unwrap();
        let new_at = moved.find("at").unwrap();
        assert!((new_at.get_f64(1).unwrap() - at.get_f64(1).unwrap() + 50.8).abs() < 0.000001);
        assert!((new_at.get_f64(2).unwrap() - at.get_f64(2).unwrap() - 25.4).abs() < 0.000001);
        assert_eq!(
            moved
                .find("instances")
                .unwrap()
                .find("project")
                .unwrap()
                .find("path")
                .unwrap()
                .get(1)
                .and_then(SexpNode::as_str),
            Some(request.destination_sheet_instance.as_str())
        );
        let old_blocks = object_blocks(&root.before).unwrap();
        let new_blocks = object_blocks(&root.after).unwrap();
        assert!(!new_blocks.contains_key(RESISTOR));
        for (uuid, (start, end)) in old_blocks {
            if uuid != RESISTOR {
                let (new_start, new_end) = new_blocks[&uuid];
                assert_eq!(&root.before[start..end], &root.after[new_start..new_end]);
            }
        }
        assert_eq!(
            plan.report["pcb_path_mapping"][format!("/{RESISTOR}")],
            format!("/{SHEET_UUID}/{RESISTOR}")
        );
    }

    #[test]
    fn transfer_updates_only_existing_pcb_path_not_footprint_identity_or_routing() {
        let (dir, request) = fixture();
        let board=format!("(kicad_pcb (version 20260306) (footprint \"Fixture:R\" (uuid \"footprint-id\") (at 10 20 90) (property \"Reference\" \"R1\") (path \"/{RESISTOR}\") (pad \"1\" smd rect (uuid \"pad-id\"))) (segment (start 1 2) (end 3 4) (uuid \"track-id\")))");
        std::fs::write(request.schematic.with_extension("kicad_pcb"), &board).unwrap();
        let plan = Plan::prepare(&request).unwrap();
        let doc = &plan.documents[&dir.path().join("component_fields_kicad10.kicad_pcb")];
        assert_eq!(
            doc.after,
            board.replace(
                &format!("(path \"/{RESISTOR}\")"),
                &format!("(path \"/{SHEET_UUID}/{RESISTOR}\")")
            )
        );
        assert!(plan.report["affected_files"].as_array().unwrap().len() == 3);
    }

    #[test]
    fn transfer_moves_all_units_together_and_rejects_partial_unit_selection() {
        let (dir, mut request) = fixture();
        request.object_uuids = vec!["37dfbfbc-5926-4718-b154-59425486a365".into()];
        let before = snapshot(dir.path());
        assert!(Plan::prepare(&request)
            .err()
            .unwrap()
            .to_string()
            .contains("All units"));
        assert_eq!(before, snapshot(dir.path()));
        request.object_uuids.extend([
            "734a8165-fecd-4968-a9b8-6990edf43995".into(),
            "db4e5e3c-9adc-40eb-a748-d1a1a0578a45".into(),
        ]);
        let plan = Plan::prepare(&request).unwrap();
        let moved = parse_sexp(&plan.documents[&dir.path().join("logic.kicad_sch")].after).unwrap();
        assert_eq!(moved.find_all("symbol").len(), 3);
        assert_eq!(
            moved
                .find_all("symbol")
                .iter()
                .map(|node| node.find_str("unit").unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["1", "2", "3"])
        );
    }

    #[test]
    fn transfer_refuses_unknown_destination_duplicate_selection_and_repeated_sheet() {
        let (dir, mut request) = fixture();
        let before = snapshot(dir.path());
        request.destination_sheet_instance = "/missing".into();
        assert!(Plan::prepare(&request).is_err());
        request.destination_sheet_instance = format!("/{ROOT_UUID}/{SHEET_UUID}");
        request.object_uuids.push(RESISTOR.into());
        assert!(Plan::prepare(&request).is_err());
        request.object_uuids.pop();
        assert_eq!(before, snapshot(dir.path()));
        let source = std::fs::read_to_string(&request.schematic).unwrap();
        let tree = parse_sexp(&source).unwrap();
        let sheet = tree.find("sheet").unwrap().find_str("uuid").unwrap();
        let (start, end) = object_blocks(&source).unwrap()[sheet];
        let repeated =
            source[start..end].replace(SHEET_UUID, "933d214c-5586-4ed5-8eae-875b556d8cc1");
        let close = source.rfind(')').unwrap();
        std::fs::write(
            &request.schematic,
            apply_edits(source, vec![SexpEdit::insert(close, repeated)]),
        )
        .unwrap();
        assert!(Plan::prepare(&request)
            .err()
            .unwrap()
            .to_string()
            .contains("repeated instances"));
    }

    #[test]
    fn transfer_rejects_library_and_uuid_conflicts_without_mutation() {
        let (dir, request) = fixture();
        let dest = dir.path().join("logic.kicad_sch");
        let original = std::fs::read_to_string(&dest).unwrap();
        let (start, end) = find_direct_child_blocks(&original, "kicad_sch")
            .into_iter()
            .find(|&(start, end)| {
                parse_sexp(&original[start..end]).unwrap().head() == Some("lib_symbols")
            })
            .unwrap();
        let mut source = apply_edits(
            original.clone(),
            vec![SexpEdit::replace(
                start,
                end,
                "(lib_symbols (symbol \"Device:R\" (property \"Value\" \"conflict\")))",
            )],
        );
        std::fs::write(&dest, &source).unwrap();
        let before = snapshot(dir.path());
        assert!(Plan::prepare(&request)
            .err()
            .unwrap()
            .to_string()
            .contains("conflicts"));
        assert_eq!(before, snapshot(dir.path()));
        let close = original.rfind(')').unwrap();
        source = apply_edits(
            original,
            vec![SexpEdit::insert(
                close,
                format!("(text \"duplicate\" (at 0 0 0) (uuid \"{RESISTOR}\"))"),
            )],
        );
        std::fs::write(&dest, &source).unwrap();
        assert!(Plan::prepare(&request)
            .err()
            .unwrap()
            .to_string()
            .contains("UUID conflicts"));
    }

    #[test]
    fn transfer_boundary_guards_cover_midwire_pins_local_labels_ports_and_destination_overlap() {
        let selected = BTreeSet::from(["selected".into()]);
        for remainder in [
            "(wire (pts (xy 5 0) (xy 5 10)) (uuid other))",
            "(junction (at 5 0) (uuid other))",
            "(sheet (uuid other) (pin \"PORT\" input (at 5 0 0)))",
            "(no_connect (at 5 0) (uuid other))",
        ] {
            let tree=parse_sexp(&format!("(kicad_sch (lib_symbols) (wire (pts (xy 0 0) (xy 10 0)) (uuid selected)) {remainder})")).unwrap();
            for destination in [false, true] {
                assert!(check_boundary(&tree, &selected, destination).is_err());
            }
        }
        let tree=parse_sexp("(kicad_sch (lib_symbols) (label \"SCOPE\" (at 0 0 0) (uuid selected)) (label \"SCOPE\" (at 50 50 0) (uuid other)))").unwrap();
        assert!(check_boundary(&tree, &selected, false)
            .err()
            .unwrap()
            .to_string()
            .contains("Sheet-local"));
        let tree=parse_sexp("(kicad_sch (lib_symbols) (global_label \"GND\" (at 0 0 0) (uuid selected)) (global_label \"GND\" (at 50 50 0) (uuid other)))").unwrap();
        check_boundary(&tree, &selected, false).unwrap();
    }

    #[test]
    fn transfer_translation_preserves_angles_relative_graphics_and_escaped_annotations() {
        let block = r#"(text_box "note \\\" (at 999 999)" (start 10 20) (end 30 40) (effects (font (size 1 1))) (uuid "text"))"#;
        let new = translate(block, 2.54, -1.27).unwrap();
        let tree = parse_sexp(&new).unwrap();
        assert_eq!(tree.find("start").unwrap().get_f64(1), Some(12.54));
        assert_eq!(tree.find("end").unwrap().get_f64(2), Some(38.73));
        assert_eq!(tree.get(1), parse_sexp(block).unwrap().get(1));
        let symbol=translate("(symbol (at 10 20 90) (property \"Reference\" \"R1\" (at 5 6 0)) (pin \"1\" (uuid \"pin\")))",5.0,10.0).unwrap();
        assert!(symbol.contains("(at 15 30 90)"));
        assert!(symbol.contains("(at 10 16 0)"));
        assert!(symbol.contains("(pin \"1\" (uuid \"pin\"))"));
    }

    #[test]
    fn transfer_netlist_comparison_detects_lost_or_merged_connections_but_ignores_generated_names()
    {
        let prefix =
            "(export (components (comp (ref R1) (tstamps r1)) (comp (ref R2) (tstamps r2))) (nets ";
        let a=format!("{prefix}(net (code 1) (name \"/Source/N\") (node (ref R1) (pin 1)) (node (ref R2) (pin 1)))))");
        let b = a
            .replace("/Source/N", "/Destination/N")
            .replace("code 1", "code 8");
        assert_eq!(
            netlist_partitions(&a).unwrap(),
            netlist_partitions(&b).unwrap()
        );
        let split =
            format!("{prefix}(net (node (ref R1) (pin 1))) (net (node (ref R2) (pin 1)))))");
        assert_ne!(
            netlist_partitions(&a).unwrap(),
            netlist_partitions(&split).unwrap()
        );
        assert!(netlist_partitions("(export (nets))").is_err());
    }

    #[test]
    fn transfer_pcb_net_rename_preserves_copper_geometry_and_rejects_merges() {
        let before = r#"(kicad_pcb (version 20260306)
            (footprint "R" (uuid "footprint") (pad "1" (net "/source/LIMIT")))
            (segment (uuid "track") (start 1 2) (end 3 4) (width 0.25) (net "/source/LIMIT"))
            (zone (uuid "zone") (net "/source/LIMIT"))
            (gr_text "/source/LIMIT" (at 10 20)))"#;
        let mapping = BTreeMap::from([("/source/LIMIT".into(), "/destination/LIMIT".into())]);
        let after = move_pcb_net_names(before, &mapping).unwrap();
        assert_eq!(
            after,
            before.replace("(net \"/source/LIMIT\")", "(net \"/destination/LIMIT\")")
        );
        let conflicting = before.replace("(gr_text", "(net \"/destination/LIMIT\") (gr_text");
        assert!(move_pcb_net_names(&conflicting, &mapping)
            .unwrap_err()
            .to_string()
            .contains("merge unrelated nets"));
        let legacy = r#"(kicad_pcb (net 7 "/source/LIMIT") (segment (net 7)) (zone (net 7) (net_name "/source/LIMIT")))"#;
        assert_eq!(
            move_pcb_net_names(legacy, &mapping).unwrap(),
            legacy.replace("\"/source/LIMIT\"", "\"/destination/LIMIT\"")
        );
    }

    #[test]
    fn transfer_keeps_no_connects_before_symbols_and_symbols_before_the_footer() {
        let (dir, mut request) = fixture();
        let mut source = konnect_schematic_editor::Schematic::load(&request.schematic).unwrap();
        let uuid = source.add_no_connect(101.6, 72.39).uuid.clone();
        source.overwrite().unwrap();
        request.object_uuids.push(uuid);
        let plan = Plan::prepare(&request).unwrap();
        let after = &plan.documents[&dir.path().join("logic.kicad_sch")].after;
        let tree = parse_sexp(after).unwrap();
        let kinds = tree
            .children()
            .unwrap()
            .iter()
            .filter_map(SexpNode::head)
            .collect::<Vec<_>>();
        let symbol = kinds.iter().position(|kind| *kind == "symbol").unwrap();
        assert!(kinds.iter().position(|kind| *kind == "no_connect").unwrap() < symbol);
        if let Some(footer) = kinds.iter().position(|kind| *kind == "embedded_fonts") {
            assert!(symbol < footer);
        }
    }

    #[test]
    fn transfer_netclass_rename_preserves_explicit_assignments_and_refuses_pattern_changes() {
        let source = json!({"unrelated":{"title":"keep"},"net_settings":{
            "netclass_patterns":[{"pattern":"*","netclass":"Default"}],
            "netclass_assignments":{"/source/LIMIT":["Fast"]}
        }})
        .to_string();
        let mapping = BTreeMap::from([("/source/LIMIT".into(), "/destination/LIMIT".into())]);
        let after: Value =
            serde_json::from_str(&move_project_net_assignments(&source, &mapping).unwrap())
                .unwrap();
        assert_eq!(
            after["net_settings"]["netclass_assignments"]["/destination/LIMIT"],
            json!(["Fast"])
        );
        assert!(after["net_settings"]["netclass_assignments"]
            .get("/source/LIMIT")
            .is_none());
        assert_eq!(after["unrelated"], json!({"title":"keep"}));
        let changed_pattern = source.replace("\"pattern\":\"*\"", "\"pattern\":\"/source/*\"");
        assert!(move_project_net_assignments(&changed_pattern, &mapping)
            .unwrap_err()
            .to_string()
            .contains("changes matching netclasses"));
        let collision = json!({"net_settings":{"netclass_assignments":{
            "/source/LIMIT":["Fast"],"/destination/LIMIT":["Slow"]
        }}})
        .to_string();
        assert!(move_project_net_assignments(&collision, &mapping).is_err());
    }

    #[test]
    fn transfer_nested_destination_round_trip_preserves_persisted_symbol_and_pin_ids() {
        let (dir, mut request) = fixture();
        let middle = dir.path().join("logic.kicad_sch");
        let mut schematic = konnect_schematic_editor::Schematic::load(&middle).unwrap();
        let sheet = konnect_schematic_editor::Sheet::new(
            "Destination",
            "leaf.kicad_sch",
            10.0,
            10.0,
            30.0,
            20.0,
        );
        let uuid = sheet.uuid.clone();
        schematic.add_sheet(sheet);
        schematic.overwrite().unwrap();
        std::fs::write(
            dir.path().join("leaf.kicad_sch"),
            super::super::blank_schematic_template(),
        )
        .unwrap();
        request.destination_sheet_instance.push('/');
        request.destination_sheet_instance.push_str(&uuid);
        let plan = Plan::prepare(&request).unwrap();
        konnect_sexp::commit_reversible_file_transaction(
            dir.path(),
            plan.documents
                .iter()
                .map(|(path, doc)| FileTransition::replace(path, &doc.before, &doc.after))
                .collect(),
        )
        .unwrap();
        let leaf =
            konnect_schematic_editor::Schematic::load(dir.path().join("leaf.kicad_sch")).unwrap();
        assert_eq!(leaf.symbols.len(), 1);
        assert_eq!(leaf.symbols.iter().next().unwrap().uuid, RESISTOR);
        // A normal editor save must retain all identity and instance metadata.
        leaf.overwrite().unwrap();
        std::mem::swap(
            &mut request.source_sheet_instance,
            &mut request.destination_sheet_instance,
        );
        request.offset_x_mm = 50.8;
        request.offset_y_mm = -25.4;
        let reverse = Plan::prepare(&request).unwrap();
        let root = &reverse.documents[&request.schematic];
        let tree = parse_sexp(&root.after).unwrap();
        let resistor = tree
            .find_all("symbol")
            .into_iter()
            .find(|symbol| symbol.find_str("uuid") == Some(RESISTOR))
            .unwrap();
        assert_eq!(
            resistor.find("pin").unwrap().find_str("uuid"),
            Some("2e601eb3-6f37-4903-9f86-b1aa7e0196b4")
        );
        assert_eq!(resistor.find("at").unwrap().get_f64(1), Some(101.6));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn transfer_served_dispatch_dry_run_and_apply_report_observed_persistence() {
        let (dir, request) = fixture();
        let control = tempfile::tempdir().unwrap();
        // This mock checks dispatch/transaction/readback, NOT KiCad semantics.
        let script=crate::tools::cli::test_support::write_script(control.path(),"fake-transfer-cli",
            "#!/bin/sh\nif [ \"$2\" = erc ]; then\n  printf '%s' '{\"kicad_version\":\"10.0.7\",\"sheets\":[{\"path\":\"/\",\"violations\":[]}]}' > \"$4\"\nelse\n  printf '%s' '(export (components (comp (ref R1) (tstamps 8b9b7750-3421-4b18-92a3-4bddb63e9056))) (nets (net (name mock) (node (ref R1) (pin 1)))))' > \"$5\"\nfi\n","");
        let handler = crate::mcp::handler::McpHandler::new(super::super::ServerConfig {
            kicad_cli: script.to_str().unwrap().into(),
            kicad_binary: String::new(),
            ipc_address: String::new(),
            project_dir: None,
            jlcpcb_db_path: None,
            auto_load_toolsets: false,
            eager_toolsets: true,
        })
        .await
        .unwrap();
        let before = snapshot(dir.path());
        for dry_run in [true, false] {
            let response=handler.handle_message(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"move_schematic_objects_between_sheets", "arguments":{
                    "schematic":request.schematic,"source_sheet_instance":request.source_sheet_instance,
                    "destination_sheet_instance":request.destination_sheet_instance,"object_uuids":request.object_uuids,
                    "offset_x_mm":request.offset_x_mm,"offset_y_mm":request.offset_y_mm,"dry_run":dry_run
                }
            }})).await.unwrap().result.unwrap();
            assert_ne!(response["isError"], true, "{response}");
            let result: Value =
                serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(result["outcome"]["status"], "complete");
            if dry_run {
                assert_eq!(result["status"], "planned");
                assert_eq!(before, snapshot(dir.path()));
            } else {
                assert_eq!(result["status"], "moved");
                assert_ne!(before, snapshot(dir.path()));
                let root = konnect_schematic_editor::Schematic::load(&request.schematic).unwrap();
                assert!(root.symbols.iter().all(|symbol| symbol.uuid != RESISTOR));
                let destination =
                    konnect_schematic_editor::Schematic::load(dir.path().join("logic.kicad_sch"))
                        .unwrap();
                assert_eq!(destination.symbols.iter().next().unwrap().uuid, RESISTOR);
            }
        }
    }

    #[test]
    fn transfer_erc_comparison_binds_transient_pin_ids_and_rejects_unknown_or_ambiguous_items() {
        let (_dir, request) = fixture();
        let plan = Plan::prepare(&request).unwrap();
        let before = ErcIdentityIndex::build(&plan, false).unwrap();
        let after = ErcIdentityIndex::build(&plan, true).unwrap();
        let item: cli::ReportItem = serde_json::from_value(json!({
            "description":"Symbol R1 [R], Pin 1 [~ Passive Line]", "uuid":"fresh-before",
            "pos":{"x":101.6,"y":72.39}
        }))
        .unwrap();
        let mut moved = item.clone();
        moved.uuid = Some("fresh-after".into());
        moved.pos.as_mut().unwrap().x -= 50.8;
        moved.pos.as_mut().unwrap().y += 25.4;
        assert_eq!(
            before.identity(&item).unwrap(),
            after.identity(&moved).unwrap()
        );
        moved.description = "Unidentified object".into();
        assert!(after.identity(&moved).is_err());
        let mut ambiguous = before;
        ambiguous.pins.push(ErcPinIdentity {
            identity: "different-symbol:1".into(),
            reference: "R1".into(),
            number: "1".into(),
            position: (101.6, 72.39),
            repeated: false,
        });
        assert!(ambiguous.identity(&item).is_err());
    }

    fn body(result: &CallToolResult) -> Value {
        let ToolContent::Text { text } = &result.content[0] else {
            panic!("text result")
        };
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn transfer_group_membership_and_local_global_scope_collisions_are_explicit_refusals() {
        let (dir, request) = fixture();
        let source = std::fs::read_to_string(&request.schematic).unwrap();
        let close = source.rfind(')').unwrap();
        std::fs::write(
            &request.schematic,
            apply_edits(
                source,
                vec![SexpEdit::insert(
                    close,
                    format!("(group \"startup\" (id \"group-id\") (members \"{RESISTOR}\"))"),
                )],
            ),
        )
        .unwrap();
        let before = snapshot(dir.path());
        assert!(Plan::prepare(&request)
            .err()
            .unwrap()
            .to_string()
            .contains("Ungroup"));
        assert_eq!(before, snapshot(dir.path()));
        let tree=parse_sexp("(kicad_sch (lib_symbols) (label \"GND\" (at 0 0 0) (uuid selected)) (global_label \"GND\" (at 50 50 0) (uuid other)))").unwrap();
        assert!(check_boundary(&tree, &BTreeSet::from(["selected".into()]), true).is_err());
    }

    #[tokio::test]
    async fn transfer_served_dispatch_refuses_invalid_requests_and_unavailable_verification() {
        let (dir, request) = fixture();
        let before = snapshot(dir.path());
        let handler = crate::mcp::handler::McpHandler::new(super::super::ServerConfig {
            kicad_cli: String::new(),
            kicad_binary: String::new(),
            ipc_address: String::new(),
            project_dir: None,
            jlcpcb_db_path: None,
            auto_load_toolsets: false,
            eager_toolsets: true,
        })
        .await
        .unwrap();
        let response=handler.handle_message(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"move_schematic_objects_between_sheets", "arguments":{
                "schematic":request.schematic,"source_sheet_instance":request.source_sheet_instance,
                "destination_sheet_instance":request.destination_sheet_instance,"object_uuids":request.object_uuids,
                "offset_x_mm":request.offset_x_mm,"offset_y_mm":request.offset_y_mm
            }
        }})).await.unwrap().result.unwrap();
        assert_eq!(response["isError"], true, "{response}");
        assert!(
            response["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("kicad-cli is required"),
            "{response}"
        );
        assert_eq!(before, snapshot(dir.path()));
        let error = blocked("boundary rewiring required");
        assert_eq!(body(&error)["error"]["kind"], "plan_blocked");
    }

    /// Opt-in native acceptance: never treat absence of KiCad as a pass.
    #[tokio::test]
    #[ignore = "requires KiCad 10 kicad-cli; run with KONNECT_TEST_KICAD_CLI configured"]
    async fn transfer_native_netlist_and_erc_acceptance() {
        let cli =
            std::env::var("KONNECT_TEST_KICAD_CLI").expect("KONNECT_TEST_KICAD_CLI is required");
        let (_dir, request) = fixture();
        let plan = Plan::prepare(&request).unwrap();
        verify_with_kicad(&plan, &cli).await.unwrap();
    }
}
