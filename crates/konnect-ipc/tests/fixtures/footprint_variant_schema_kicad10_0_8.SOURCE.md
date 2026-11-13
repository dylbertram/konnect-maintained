# KiCad 10.0.8 raw footprint variant inspection

This is provenance for the read-only model in
`examples/native_footprint_children_capture.rs`. It does not update production
protobuf definitions or generate a payload fixture.

The upstream annotated tag `10.0.8`, object
`7aca893cfa151bbf2f04375b686d1b905d583e56`, peels to commit
`e432beac2668303194ba2040274f9e5efe773fc0`.

- [Pinned schema source](https://raw.githubusercontent.com/KiCad/kicad-source-mirror/e432beac2668303194ba2040274f9e5efe773fc0/api/proto/board/board_types.proto)
- SHA-256 of the complete upstream `board_types.proto` bytes:
  `fa7b175e769215541e8adb61200be2dafe473f3ec9f18d54801bada9be866587`
- The upstream file's license header is GPL-3.0-or-later.

In that schema, `FootprintInstance.variants` is repeated `FootprintVariant`
at field 20. Each length-delimited payload uses `name` at field 1,
`do_not_populate` at field 2, `exclude_from_bill_of_materials` at field 3,
`exclude_from_position_files` at field 4 and the string-to-string `fields`
map at field 6. Field 5 is reserved. The same schema names the point Any
message `kiapi.board.types.ReferencePoint`.

The example saves every actual received Any.value before old-schema
FootprintInstance decoding. It extracts each actual field 20 payload, persists
its full bytes and decodes that payload with this test-only read model before
decoding the containing footprint with the old schema. Capture JSON records
the payload file, byte count, decoded variant fields, source URL and hash.
The native helper must observe the actual raw name `NoTimer` and DNP true,
independently of the saved native string and public refusal diagnostic.

There is no fabricated or re-encoded expected payload. A tag count alone is
not variant-name evidence. Native execution and acceptance remain pending.
