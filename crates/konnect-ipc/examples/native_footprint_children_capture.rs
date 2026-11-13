//! Coordinator-only native A8 evidence helper. Never mutates a board.
//! `capture SOCKET BOARD OUT` saves raw Any.value before old-schema decode.
//! `proxy LISTEN UPSTREAM OUT` records/forwards exact IPC bytes until stopped
//! by its owning native helper. Use only on a disposable isolated editor.

use anyhow::{bail, Context, Result};
use konnect_ipc::{gen::kiapi, KiCadIpcClient};
use nng::options::Options;
use prost::Message;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["capture", socket, board, out] => capture(socket, Path::new(board), Path::new(out)),
        ["proxy", listen, upstream, out] => proxy(listen, upstream, Path::new(out)),
        _ => bail!("usage: native_footprint_children_capture capture SOCKET BOARD OUT | proxy LISTEN UPSTREAM OUT"),
    }
}

fn capture(socket: &str, board: &Path, out: &Path) -> Result<()> {
    fs::create_dir(out)?;
    let client = KiCadIpcClient::new(socket);
    let version = client.get_kicad_version()?;
    if (version.major, version.minor, version.patch) != (10, 0, 8) {
        bail!("native gate requires editor IPC 10.0.8, got {version:?}");
    }
    let document = client.find_open_board(board)?;
    fs::write(out.join("bound-document.pb"), document.encode_to_vec())?;
    let native = client.save_document_to_string_in(document.clone())?;
    fs::write(out.join("native-board.txt"), &native)?;
    let items = client.get_items_in(
        document,
        kiapi::common::types::KiCadObjectType::KotPcbFootprint,
    )?;
    // Persist ALL raw payloads first. The vendored schema cannot recover field
    // 20 after FootprintInstance decoding, so decoding is not capture evidence.
    for (i, item) in items.iter().enumerate() {
        fs::write(out.join(format!("item-{i}.any.pb")), item.encode_to_vec())?;
        fs::write(out.join(format!("item-{i}.any.value")), &item.value)?;
    }
    let root = konnect_sexp::parse_sexp(&native)?;
    if root.head() != Some("kicad_pcb") {
        bail!("native snapshot is not a board");
    }
    let native_children = root.find_all("footprint").into_iter().map(|footprint| {
        json!({
            "kiid": footprint.find_str("uuid"),
            "library_id": footprint.get(1).and_then(konnect_sexp::SexpNode::as_str),
            "group": footprint.find_all("group").len(),
            "point": footprint.find_all("point").len(),
            "variant": footprint.find_all("variant").len(),
            "variant_names": footprint.find_all("variant").iter()
                .map(|v| v.find_str("name")).collect::<Vec<_>>(),
        })
    }).collect::<Vec<_>>();
    let mut raw = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let fields = wire_fields(&item.value)?;
        let decoded = kiapi::board::types::FootprintInstance::decode(item.value.as_slice())?;
        let children = decoded.definition.iter().flat_map(|d| d.items.iter()).collect::<Vec<_>>();
        let native_footprint = root.find_all("footprint").into_iter().find(|footprint| {
            footprint.find_str("uuid") == decoded.id.as_ref().map(|id| id.value.as_str())
        }).context("raw footprint missing from same bound native snapshot")?;
        let group_ids = native_footprint.find_all("group").into_iter()
            .filter_map(|group| group.find_str("uuid")).collect::<Vec<_>>();
        let group_kiid_present = group_ids.iter().map(|id| {
            json!({
                "native_group_kiid": id,
                // KIID.value is a protobuf string. Keep the raw bytes too,
                // rather than treating an old-schema decode as omission proof.
                "utf8_in_raw_value": item.value.windows(id.len()).any(|bytes| bytes == id.as_bytes()),
            })
        }).collect::<Vec<_>>();
        raw.push(json!({
            "index": i, "type_url": item.type_url, "bytes": item.value.len(),
            "kiid": decoded.id.as_ref().map(|id| id.value.as_str()),
            "reference": decoded.reference_field.as_ref().and_then(|f| f.text.as_ref())
                .and_then(|t| t.text.as_ref()).map(|t| t.text.as_str()),
            "wire_fields": fields,
            "variant_field_20_occurrences": fields.iter().filter(|(field, _)| *field == 20).count(),
            "definition_child_type_urls": children.iter().map(|a| a.type_url.as_str()).collect::<Vec<_>>(),
            "group_any_count": children.iter().filter(|a| a.type_url.ends_with(".Group")).count(),
            "native_group_kiid_raw_presence": group_kiid_present,
            "point_any_count": children.iter().filter(|a| a.type_url.ends_with(".Point") || a.type_url.ends_with(".BoardPoint")).count(),
        }));
    }
    fs::write(out.join("capture.json"), serde_json::to_vec_pretty(&json!({
        "editor_ipc_version": version, "native_children": native_children, "raw_items": raw,
        "capture_order": "bound document, native string, all raw Any.value files, then old-schema decode",
        "evidence": "native IPC, not echoed mutation protobuf",
    }))?)?;
    Ok(())
}

/// Read protobuf tags without an old FootprintInstance schema.
fn wire_fields(mut bytes: &[u8]) -> Result<Vec<(u32, u8)>> {
    fn varint(bytes: &mut &[u8]) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *bytes.first().context("truncated wire varint")?;
            *bytes = &bytes[1..];
            if shift == 63 && byte > 1 {
                bail!("overflow wire varint");
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        bail!("overflow wire varint")
    }
    let mut fields = Vec::new();
    while !bytes.is_empty() {
        let tag = varint(&mut bytes)?;
        let field = u32::try_from(tag >> 3)?;
        let wire = (tag & 7) as u8;
        if field == 0 {
            bail!("zero wire field");
        }
        fields.push((field, wire));
        let size = match wire {
            0 => { varint(&mut bytes)?; 0 }
            1 => 8,
            2 => usize::try_from(varint(&mut bytes)?)?,
            5 => 4,
            _ => bail!("unsupported wire type {wire}"),
        };
        bytes = bytes.get(size..).context("truncated wire field")?;
    }
    Ok(fields)
}

fn proxy(listen: &str, upstream: &str, out: &Path) -> Result<()> {
    fs::create_dir(out)?;
    let server = nng::Socket::new(nng::Protocol::Rep0)?;
    server.listen(listen)?;
    let mut log = fs::File::create(out.join("rpc.jsonl"))?;
    fs::write(out.join("ready"), b"listening")?;
    for index in 0u64.. {
        let request = server.recv()?;
        // Persist request before forwarding, including unexpected mutations.
        fs::write(out.join(format!("{index:05}.request.pb")), request.as_slice())?;
        let decoded = kiapi::common::ApiRequest::decode(request.as_slice())?;
        let name = decoded.message.as_ref().context("missing IPC message")?.type_url.clone();
        writeln!(log, "{}", json!({"index": index, "command": name, "phase": "request"}))?;
        log.flush()?;
        // A fresh upstream req socket matches the production client's
        // request/response connection lifetime and avoids stale req state.
        let upstream_socket = nng::Socket::new(nng::Protocol::Req0)?;
        upstream_socket.set_opt::<nng::options::SendTimeout>(Some(Duration::from_secs(3)))?;
        upstream_socket.set_opt::<nng::options::RecvTimeout>(Some(Duration::from_secs(12)))?;
        upstream_socket.dial(upstream)?;
        upstream_socket.send(nng::Message::from(request.as_slice())).map_err(|(_, e)| e)?;
        let response = upstream_socket.recv()?;
        fs::write(out.join(format!("{index:05}.response.pb")), response.as_slice())?;
        writeln!(log, "{}", json!({"index": index, "phase": "response"}))?;
        log.flush()?;
        server.send(response).map_err(|(_, e)| e)?;
    }
    unreachable!()
}
