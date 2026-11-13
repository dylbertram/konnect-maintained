# Placed schematic private properties

Bounded child of `cleanup/main` at `b1c0b3c`, on
`cleanup/fix/private-fields`. Existing Kata `konnect-maintained#27jb` and
`konnect-maintained#we5v` remain open pending coordinator verification and
parent integration. No catalog, count or cleanup-plan changes belong here.
The child subsequently merged parent `410675b` to reuse the verified export
supervisor. The production private-property implementation is unchanged.

## Source verification

Read the current plan, A5 packet, review notes, both Kata records and the
archived snapshot's complete #924/#931 entries before patching. Both archived
items have empty comments, reviews and review-comments arrays. The archived
candidate `cd2535da0d0419420d985ba81139f339df2bf5f7` is not merge approval.
Its "No public compatibility impact" claim is incorrect for a new field on an
exhaustively constructible public Rust struct.

At the maintained base, `Property::from_sexp` reads arguments zero and one,
then retains only list children. `to_sexp` emits quoted name/value without a
private prefix. `component_target_from_source` independently reads children
one and two. Two real private names therefore collide as `private`.
The raw placed-field setters also search only an ordinary quoted-name prefix.
They must recognize private headers to avoid treating an existing private
field as missing and adding a duplicate.

Verified against KiCad tag `10.0.6`, commit
`caf7377e9cb6fa1535ec3596dcb8c99bf44a996e`:

- [`saveField`, lines 1017-1019](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr.cpp#L1017-L1019)
  emits bare `private` before the quoted name/value when `IsPrivate()` is true.
- [Placed-field parser, lines 2291-2295](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr_parser.cpp#L2291-L2295)
  consumes that flag before reading the name. Lines 2386-2387 apply it to user
  fields.
- [`SCH_SYMBOL`, line 1445](https://github.com/KiCad/kicad-source-mirror/blob/caf7377e9cb6fa1535ec3596dcb8c99bf44a996e/eeschema/sch_symbol.cpp#L1445)
  copies the library field's private flag to the placed field on update.

Only `Atom("private")` in the prefix position is a flag, not
`Str("private")`. The typed model retains trailing children in their original
order. The targeted setters locate direct property children and change only
the value or requested placement child, not nested property-looking text.

## Compatibility choice

Keep `Property::new`, existing field types and all tool schemas/responses.
Represent privacy as a typed public boolean, default false in the constructor.
No existing production call site in this repository constructs `Property`
outside its own implementation, as checked by a struct-literal search.

Downstream exhaustive literals/destructuring do break. Add `private: false`
to ordinary literals, or use `Property::new`; add `..` to destructuring where
appropriate. There is no blanket downstream compatibility claim.

Storing a hidden marker in `sub_nodes` would preserve struct-literal syntax,
but would silently reserve a child value and change the meaning of callers'
existing child vectors. This patch does not use that convention or a global
side table. See `docs/API_MIGRATIONS.md`.

## Fixture provenance and the remaining native gate

Reuses the existing, unchanged
`crates/konnect-core/tests/fixtures/component_fields_kicad10.kicad_sch`.
Its README records creation through Konnect with stock libraries, followed by
`kicad-cli sch upgrade --force` on KiCad 10.0.6. Its SHA-256 is
`80d57b769749c8b9828be52a09ad947e7a0aa55715e404805f80b70d688d9f54`.
It contains R1 and three valid U1 units with per-unit LCSC/MPN copies.

That native fixture does NOT already contain private flags. No checked-in
schematic fixture did. An upstream 10.0.6 fixture search also found no private
property example in the 389 schematic files inspected, up to 700 KB per file.
This is not a claim about every larger file or every KiCad version.

`tests/fixtures/private_fields.rs` generates explicit synthetic variants by
inserting flags on the native placements' existing MPN/LCSC properties.
It renames the existing placed Description field to the ordinary quoted name
`"private"` and optionally adds an unknown property child. Native source
files are neither hand-authored nor changed by this patch. Synthetic variants
are not presented as native private-field captures.

The ignored native regression prepares that variant WITHOUT the unknown
future child, requires an explicitly selected KiCad CLI, and force-resaves it
before invoking served move/readback. It logs the native version and command.
KiCad would rightly reject the synthetic unknown child, so it is used only
for the preservation tests, not the native input.

Both native calls use the existing
`tests/support/pcb_export_supervisor.py` from parent `410675b`, on Linux with
Python 3.9+. Version discovery has its own 10-second native deadline; force
upgrade has its own 45-second deadline. The wrapper owns process-group
termination and subreaper cleanup independently of the Rust future.
Rust awaits the wrapper with `kill_on_drop(false)` rather than blocking on
`std::process::Command::output` or relying on an outer async timeout.
Each phase must produce a finished audit with `group_reaped: true`, no timeout
and no interruption. Native output still needs a successful exit status.

With `KONNECT_TEST_NATIVE_ARTIFACT_DIR` set, the test copies the native CLI's
output bytes and a provenance JSON with version, command, the base-fixture
SHA-256 and the native-output SHA-256 into that directory. It does not record
the prepared synthetic input's hash. The JSON also embeds the version/upgrade
supervision audits and their deadlines. This happens before Konnect's move.
Retrieve `private_fields_native.kicad_sch` and
`private_fields_native.provenance.json` from the admitted job; they describe
the native capture, not a captured or hashed prepared input.

No native execution or new native capture has happened in this child session.
The coordinator must admit and run the native case, and retain its output or
capture a native-written private-field fixture, before claiming that gate.
No live CAD, editor, deployment, settings or branch protections were touched.

## Tests and execution status

No dependency install, build, typecheck, test or Boat job was run here.
The only formatter invocation read source on stdin and emitted formatted text
to stdout using the already-installed Rust 1.96.0 formatter. Edits were
applied with the patch tool. `git diff --check` is a whitespace inspection,
not a test result.

Coordinator positive commands for the corrected snapshot, not executed here:

```sh
cargo test -p konnect-schematic-editor --locked --test private_properties
cargo test -p konnect-core --locked --lib tools::sch_components::private_fields_tests -- --nocapture --test-threads=1
cargo fmt --all -- --check
```

The model target covers ordinary/private/quoted-private names, escaped
names/values, missing names/values, empty values, ordered known/unknown
children, whole-file save and reload, and the unchanged native no-flag case.
The served module covers get/list, an exact-byte harmless edit, private and
quoted-name updates on all U1 units, private field placement, whole-file moves
of R1 and U1, duplicate-real-name refusal with unchanged source, and an
explicit readback-only edit with empty changes and unchanged source.

The coordinator's pre-pause job `ee04d7f0-b3c1-43de-ba67-c73a192cedc9`
tested `93d3db5`: four model tests passed; six served tests passed, one failed
and one was ignored. The failure was the harmless-edit test's own precondition,
before its served edit call. It searched R1's placed block for
`(property "Value" "R"` even though the native fixture and failure output
both contain `(property "Value" "10k"`. Its text replacement therefore changed
nothing. This is a test-fixture mismatch, not evidence of a production no-op.

The corrected regression pins exactly one old `"10k"` header and requests
`"11k"`. It retains the non-no-op preconditions, compares the complete written
file against the independently constructed single-span expected source, and
checks served readback plus all private fields. A separate readback-only
request verifies empty changes and exact original bytes. The control runner's
before/restored positives now include both tests, and its test-source pin was
deliberately updated. No production source changed for this correction.
No corrected tests have run here. The failed job never reached native capture
or the five controls; missing artifact errors do not establish native results.

Native command, only on a coordinator-admitted target with KiCad provisioned:

```sh
KONNECT_TEST_NATIVE_ARTIFACT_DIR=native-artifacts KONNECT_TEST_KICAD_CLI=/path/to/kicad-cli cargo test -p konnect-core --locked --lib tools::sch_components::private_fields_tests::served_private_fields_native_resave_and_readback -- --ignored --exact --nocapture --test-threads=1
```

Broader component/batch-edit tests and workspace formatting/Clippy/tests remain
the coordinator's integration gate, not claimed passes on this child.

## Negative controls

Run each control in a disposable, admitted source copy. Keep the added test
files and the new boolean so a compile failure is NOT mistaken for a useful
negative control. Each control must compile, fail its named regression, and
then be absent from the positive snapshot.

1. **Original wrong parse.** In `Property::from_sexp` only, set `offset = 0`,
   keep the old name/value reads, filter trailing children to lists, and
   initialize the stored flag to false. Run the model target and
   `served_private_properties_read_real_names_on_single_and_multiunit_parts`.
   Expect the first real name/value assertions to fail, not a compiler error.
2. **Writer flag loss.** Remove only the `if self.private` prefix emission in
   `Property::to_sexp`, keeping correct parsing and binding. Run the model
   target and `served_whole_file_move_keeps_unselected_private_values_and_accepts_two_flags`.
   The independent parsed property-node comparison must fail even if a
   self-roundtrip alone seems consistent.
3. **Original binding.** Set only the offset inside
   `component_target_from_source` to zero. Run the served whole-file-move
   regression. It must fail on an `ambiguous_target` refusal from two distinct
   private names.
4. **Quoted-name confusion.** Replace the model's atom-only flag predicate
   with `args.first().and_then(|node| node.text()) == Some("private")`.
   Run the model target and served get/list regression. The ordinary quoted
   field must fail, while the positive snapshot retains it.
5. **Old targeted setter.** Restore ordinary-name-only matching in the
   placed-property locator, without changing model parsing/binding. Run
   `served_private_and_quoted_field_updates_preserve_headers_on_every_unit`
   and `served_private_field_placement_changes_only_its_at_child`. Missing or
   duplicate private fields and any byte change outside the intended value/at
   span must fail the assertions.

No negative control was executed in this child session. Record exact remote
job IDs, commands and logs at coordinator verification.

### Coordinator-only runner

Run only in the coordinator's capacity-admitted disposable Linux worker, with
exclusive source ownership and dependencies/toolchain already provisioned.
The admission flag is an explicit acknowledgement, not a capacity check or
permission to run locally. This runner has not been executed here.

```sh
python3 crates/konnect-core/tests/private_field_controls.py all --coordinator-admitted --output-dir private-field-controls
```

The five controls are `wrong-parse`, `writer-flag-loss`, `wrong-binding`,
`quoted-name-confusion` and `ordinary-only-setter`. A single control name can
replace `all`. The runner pins the reviewed production from `90b9bc4`,
regressions including the bounded native-test follow-up, fixtures and the
supervisor imported from `410675b` with SHA-256 preconditions. It then requires
unique source snippets within exact function boundaries. The native-test pin
was deliberately updated for this follow-up; the production pins are unchanged.
Source changes require a separate runner review and updated preconditions,
not an automatic hash refresh.

Only eight exact focused model/served regressions are eligible. For `all`, it
runs eight positive prerequisites, nine expected-failure invocations and eight
restored positives. It requires the exact named test failure, one-test harness
summary and exit 101; compiler/build failures, missing tests, timeouts and log
overflow fail the runner rather than count as reproduction. Cargo runs with
`--locked --offline`; missing provisioned dependencies fail the prerequisite.
No native test, fixture capture, whole workspace test or dependency install is
invoked.

Each command defaults to 120 seconds, with a 180-second maximum. The work
phase is capped at 360 seconds, with a separate 180-second recovery budget.
Each invocation is aborted if its log exceeds 4 MiB, checked every 100 ms.
Owned Cargo/test process groups are terminated on completion or failure before
restoring the two mutable source files byte-for-byte in `finally`. All seven
protected inputs are compared after
restoration, and focused positives run again even after a control error.
Catchable interrupts take the cleanup path; SIGKILL, worker loss or a second
interrupt during cleanup cannot guarantee restoration. Discard the disposable
worker on any uncertain cleanup. The runner never resets Git or edits CAD.

Retrieve `private-field-controls/*.log` separately from the native artifacts.
The runner and this documentation correction are test infrastructure only;
they do not change the reviewed production implementation.

## Limits

This is a placed-property fix, not a serializer rewrite or a library-property
tool fix. It retains property-child order and unknown children. Exact
unrelated-byte checks cover targeted field edits. Whole-file regressions
compare untouched property nodes and embedded library nodes independently of
the model, including privacy, names and values.

The existing whole-file serializer can still reorder unrelated symbol
attributes and normalize formatting, including older-format forms. This patch
does not claim whole-file byte identity or fix the separate A5 canonical-symbol
and text-preservation packets. Missing name/value tests concern property
parsing; they do not establish whole-document refusal for every malformed
schematic. Native execution, deterministic failing-before/passing-after
evidence and parent integration are still required. Do not close either Kata
record on this source-only handoff.
