# Cross-sheet transfer

`move_schematic_objects_between_sheets` in `sch_hierarchy` moves existing objects
within one saved project. It does not copy symbols or generate new symbol/pin
UUIDs. Reference designators, multi-unit membership, properties, footprints and
BOM/DNP settings travel with the original blocks. Coordinates and field positions
receive the same offset in millimetres. Unselected object blocks stay unchanged.

## Consolidating a sparse sheet

Close the schematic and project PCB editors cleanly. Use the project's root
schematic beside its same-named `.kicad_pro`. Inspect the hierarchy, symbol
instance records and object UUIDs first. A sheet instance is the root schematic
UUID followed by the chain of sheet-symbol UUIDs, not child-file root UUIDs or
page numbers.

For a two-part startup limiter going beside its controller, select both symbols
and their wires, junctions, labels, no-connects and annotations. Include every
placed unit of each selected reference. Request a plan first:

```json
{
  "schematic": "/projects/controller/controller.kicad_sch",
  "source_sheet_instance": "/ROOT_UUID/STARTUP_SHEET_UUID",
  "destination_sheet_instance": "/ROOT_UUID/PD_SHEET_UUID",
  "object_uuids": ["PART_1_UUID", "PART_2_UUID", "WIRE_UUID", "LABEL_UUID"],
  "offset_x_mm": 50.8,
  "offset_y_mm": -25.4,
  "dry_run": true
}
```

Replace the illustrative UUIDs with the actual saved UUIDs. The same operation
can move a control fuse/bypass group beside a regulator. A boundary connection
is not permission to guess a replacement connection. Resolve the reported
boundary wiring explicitly, inspect the new plan, then repeat with
`"dry_run": false`. Application rebuilds and verifies the plan against fresh
files; it does not trust an earlier plan.

The response lists selected objects, library definitions added, affected files,
the placement offset, `path_mapping` and `pcb_path_mapping`. `connectivity_impacts`
lists old/new net names observed in KiCad's exports when hierarchy scope changes.
A dry run checks the candidate but does not change project files or recover
pending transactions.

## Hierarchy and PCB association

Symbol instance paths change from `/<root>/<source-sheet>` to
`/<root>/<destination-sheet>`. The symbol UUID at the end of the full component
path stays the same. `path_mapping` contains the full old/new component paths.
Existing PCB footprint paths omit the root schematic UUID; `pcb_path_mapping`
reports those separate old/new paths. Matching footprint path fields change.
Footprint UUIDs, pad UUIDs, placement and routing remain unchanged.

When KiCad reports a hierarchy-qualified net-name change, every saved PCB pad,
track, via and zone reference to that net is renamed in the same transaction.
Legacy numeric net IDs are retained. `pcb_net_name_mapping` lists these changes.
Existing destination net names are refused rather than merged. Explicit project
netclass assignments follow the renamed net; a change in the classes matched by
netclass patterns blocks transfer for explicit review.

The same-named project PCB, when present, participates in the transaction.
Alternate boards and other projects are not searched or updated. Reconcile those
explicitly before using the operation in a multi-board workflow. Reopen/reload
the saved design before updating the PCB from its schematic.

## Connectivity checks and limits

Supported selected objects are symbols, wires, junctions, local/global labels,
no-connect flags, text and text boxes. The operation refuses:

- Source/destination files placed more than once, nonexistent instances, stale
  symbol paths, foreign project instance records, UUID/reference conflicts and
  selections that split a multi-unit component.
- Physical source boundary connections or destination electrical overlaps.
- A selected local label whose name occurs outside the selection, including
  destination local/hierarchical label collisions.
- Moving hierarchical labels, sheet ports or sheet objects. Explicit boundary
  rewiring must happen separately.
- Grouped selections. Ungroup objects explicitly before transferring them.
- Buses or legacy top-level symbol-instance tables on either affected sheet,
  derived library definitions, conflicting destination definitions, incomplete or
  cyclic hierarchies and sheets outside the project directory.
- Pending project transactions, a present/uninspectable KiCad project lock, or
  live/uncertain PCB editor state. Project locks are checked again after native
  verification and before applying changes.

Required library definitions are copied unchanged only when absent; matching
definitions are reused. Source definitions are not deleted. Global labels and
power nets retain their scope. A local label gains its destination sheet scope.
No labels are silently renamed or converted into global labels.

Before writing, Konnect stages the complete saved hierarchy and project settings
in private temporary directories. It requires `kicad-cli` to export both
netlists and run both ERC checks. Component UUID/reference identities and pin-net
partitions must match, and no new ERC rule/severity/item combination may appear.
Persisted ERC item UUIDs are compared directly. When KiCad generates a transient
pin UUID for a symbol without saved pin records, comparison requires a unique
saved symbol/reference, pin number and observed endpoint. Ambiguous or repeated
instances are refused rather than treating a fresh UUID as proof of a new error
or ignoring the violation.
Generated net codes and hierarchy-qualified local net names may change.
Transfer does not rewrite netclass pattern rules or guess different classes.
Unavailable or undecodable verification blocks application, never reports a pass.

## Failure and recovery

Application locks and revision-checks all inspected project files. Unchanged
files are read-set guards and are not rewritten. Source, destination and any
dependent PCB path edits share one reversible journal. Ordinary write failure
restores the before images. After a crash, transaction recovery rolls an
uncommitted transfer **back**, rather than completing it. Existing forward-recovery
transactions retain their original semantics.

Rollback cannot overwrite a third-party edit or repair unavailable storage.
Those failures retain the journal and report uncertain persistence. Inspect the
files and `konnect transaction status` before recovery or retry. A result claiming
refusal is issued only when all before images and absence of a journal are proven.
Older Konnect versions reject the new version-2 rollback journal; recover with a
version supporting this operation.

Source sheet deletion and hierarchy cleanup are always separate operations.
