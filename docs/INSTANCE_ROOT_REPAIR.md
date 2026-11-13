# Repairing stale hierarchy-root prefixes

`repair_schematic_instance_roots` in `sch_hierarchy` repairs one explicit obsolete
root UUID in a saved project's symbol and sheet instance paths. It does not
change the root schematic UUID, object or pin UUIDs, references, units,
properties, geometry, nets, or PCB content.

Use this only after establishing the canonical root from the actual saved
hierarchy and project history. An old instance prefix is not authority to change
the root to match it. Other stale metadata needs a different repair.

## Dry-run and apply

Close the requested project's editors cleanly. A project or document lock, an
uncertain/live matching PCB, or a pending transaction blocks the operation.
An unrelated open PCB is not closed or changed.

```json
{
  "schematic": "/projects/controller/controller.kicad_sch",
  "old_root_uuid": "cd385c7a-53db-42a0-815e-a24ef952404d",
  "dry_run": true
}
```

The old UUID must be the exact observed obsolete prefix. The plan names the
canonical root, changed symbol/sheet records and affected files. Apply with
`"dry_run": false` and the exact returned `"expected_plan_revision"`. Any file or
plan change requires a new dry-run. A fully repaired hierarchy returns `noop`.

## Preconditions and proof

- One saved project owns every reachable file. Sheet files are contained within
  its directory, readable, acyclic, and not reused.
- Every symbol and sheet has exactly one instance entry for that project.
  Reference, unit and the entire path after the root already match the saved
  hierarchy. Object/pin UUIDs are valid and unique.
- All entries use the explicit old root, or all are already canonical.
  Mixed roots, foreign projects, legacy symbol tables, missing entries and
  ambiguous metadata are refused, not normalized.
- Only the root-prefix byte spans in those instance paths are changed. All other
  bytes are retained. The PCB, project settings and local library tables are
  unchanged read-set guards.

The original inconsistent hierarchy is not a valid connectivity baseline.
Instead, Konnect stages a private **coherent legacy-root reference**: it changes
only the root UUID in that temporary copy so the original instance paths resolve.
It compares that copy with the proposed canonical-root candidate through native
KiCad netlist and ERC exports. All expected component/reference/UUID identities,
pin-net partitions, and ERC rule/severity/item multisets must match. Temporary
reference changes never touch the source project.

Unavailable or undecodable native evidence blocks repair. Equivalent ERC does
not mean a design is ERC-clean, electrically correct, or ready for fabrication.

## Persistence and follow-up

Apply revision-checks and locks the complete read set, then uses the existing
version-2 reversible transaction. Failure restores before images; interrupted
uncommitted transactions recover backward. Divergent edits or uncertain rollback
retain recovery evidence and require inspection before retry.

Committed files are read back byte-for-byte. Reload the saved project before
using an editor. Repeat any previously blocked transfer **dry-run** afterward;
repair does not apply a circuit move or waive label-scope/boundary checks.
