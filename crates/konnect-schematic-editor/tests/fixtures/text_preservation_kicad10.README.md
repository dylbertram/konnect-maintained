# Native text preservation fixture

`text_preservation_kicad10.kicad_sch` is actual Linux KiCad 10.0.8 output,
installed unchanged by the coordinator. It is not a hand-shaped schematic or
an LF-normalized copy of a Windows file.

The input was the repository's
`crates/konnect-core/tests/fixtures/multichannel_mixer.kicad_sch`, copied into
a disposable native capture directory. Its existing provenance in
`crates/konnect-core/tests/fixtures/annotate_duplicates.README.md` identifies
it as KiCad's verbatim multichannel demo root, distributed with KiCad 10.0.5.
The input is CRLF. KiCad 10.0.8's `sch upgrade --force` produced the committed
LF output; no subsequent newline normalization was applied.

- Input SHA-256: `95d79da90c27ca4600071da822b864603f1004751bf7e0e8fcfc3a2810f2a061`
- Native output SHA-256: `1bbd268c32a780f2314fb0a280405be92800086ff2ac1a5cf50b15f5ca266398`
- Native format: `20260306`, generator `eeschema` `10.0`
- Capture revision and details: unchanged adjacent `.provenance.json`
- Coordinator command reference: `boatcmd_wk7jitojaaxx`
- Coordinator evidence in the parent worktree:
  `.remote-build-jobs/boat-route-verification/text-native-capture/` and
  `text-native-capture-result.json` in the same job directory.

The coordinator reported supervised native upgrade and native `kicadsexpr`
netlist acceptance, both exit 0, and an explicit no-CR assertion on the actual
output. This child did not execute or independently rerun those native commands.

Both top-level texts carry `(exclude_from_sim no)` before `at`:

- `7a1204f2-6377-4d90-b301-42e19a80a973`
- `fdd0a080-44f5-467e-8b25-1ae4c1590a2a`

The first includes font colour; both include font size. The tests match blocks
by UUID and compare exact source slices, without folding line endings.
They do not compare the complete schematic.

The `yes`, missing-flag and unknown-child cases are controlled synthetic
derivatives created in memory by `tests/support/text_preservation.rs`. They
are not native captures. Invented unknown syntax is never passed to KiCad.
The committed source fixture and provenance must not be rewritten by tests.
