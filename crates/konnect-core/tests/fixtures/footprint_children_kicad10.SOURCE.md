# Footprint child fixture provenance

The adjacent board and original README were imported verbatim from
[`davidtdab/konnect-contrib`, commit `8fdc5ba6f494b7349b632de093d623cb4d7288cf`](https://github.com/davidtdab/konnect-contrib/tree/8fdc5ba6f494b7349b632de093d623cb4d7288cf/crates/konnect-core/tests/fixtures).
The coordinator reviewed provenance before supplying these files to this child.
This note does not replace or amend the original README.

Original sources:

- [Board](https://raw.githubusercontent.com/davidtdab/konnect-contrib/8fdc5ba6f494b7349b632de093d623cb4d7288cf/crates/konnect-core/tests/fixtures/footprint_children_kicad10.kicad_pcb)
- [README](https://raw.githubusercontent.com/davidtdab/konnect-contrib/8fdc5ba6f494b7349b632de093d623cb4d7288cf/crates/konnect-core/tests/fixtures/footprint_children_kicad10.README.md)

SHA-256:

```text
44a89de8509f6aade944ee7c4f100960672b6a34c5cfb1aef1997ad9989b73a3  footprint_children_kicad10.kicad_pcb
190ddc8ea9040acd6039900733229df36eae37e2c1d36f7d8bd2e8c805a20eb8  footprint_children_kicad10.README.md
0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0  repository LICENSE
```

The source repository declares AGPL-3.0-only, matching this repository.
The coordinator checked that its license bytes match the hash above.

The README describes GUI creation and a pcbnew 10.0.6 save. The board header
identifies 10.0, not an independently verified 10.0.8 capture. The imported
fixture alone is not a native 10.0.8 serialization or loss oracle.

U1, KIID `c7901245-d0c4-476f-8215-85f1183d04de`, directly owns one group,
one point and variant `NoTimer`. R1, KIID
`20a44d6b-d6bb-4ec1-ac0c-f1d3d7e20db2`, owns none. A board-level group
contains both footprints. Board-level variant declarations are separate.

Single-kind reductions and malformed responses live only as explicitly
synthetic in-memory test inputs. Tests import the original board bytes to a
temporary file without changing them. The native helper imports the same
bytes, then changes only disposable accepted copies through native pcbnew
operations or public Konnect tools. It never patches CAD text.
