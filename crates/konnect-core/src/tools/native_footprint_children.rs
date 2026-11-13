//! Read-only compatibility gate for whole-instance replacements.
//!
//! The typed footprint snapshot is not an inventory of native-owned children.
//! Read the exact bound editor document instead. This does not extend either
//! replacement path's preservation contract or lock the GUI across requests.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use konnect_ipc::{gen::kiapi::common::types::DocumentSpecifier, KiCadIpcClient};
use konnect_sexp::SexpNode;
use serde::Serialize;

pub(super) const DIAGNOSTIC: &str = "footprint_children_not_carried";

#[derive(Debug, Clone)]
pub(super) struct Target {
    pub reference: String,
    pub kiid: String,
    pub library_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(super) struct Children {
    pub groups: usize,
    pub points: usize,
    pub variants: usize,
    pub variant_names: Vec<String>,
}

impl Children {
    fn kinds(&self) -> Vec<String> {
        let mut kinds = Vec::new();
        if self.groups > 0 {
            kinds.push("group".to_string());
        }
        if self.points > 0 {
            kinds.push("point".to_string());
        }
        if self.variants > 0 {
            kinds.push("variant".to_string());
        }
        kinds
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Details {
    pub kiid: String,
    pub library_id: String,
    /// Empty only when inventory failed, not evidence that a target is safe.
    pub kinds: Vec<String>,
    pub native_children: Option<Children>,
    pub inventory_error: Option<String>,
}

pub(super) struct Refusal {
    pub reference: String,
    pub details: Details,
}

impl Refusal {
    pub fn message(&self) -> String {
        let reason = match &self.details.inventory_error {
            Some(error) => format!("native child inventory is unknown: {error}"),
            None => format!(
                "owns unsupported native children: {}",
                self.details.kinds.join(", ")
            ),
        };
        format!(
            "{} (KIID {}, library {}) {reason}. Whole-footprint replacement \
             cannot guarantee preservation of these children and is refused. \
             This is not a claim that every KiCad version loses every child kind.",
            self.reference, self.details.kiid, self.details.library_id
        )
    }
}

/// Inventory only actual existing replacement targets. No disk fallback,
/// reference matching, or reads for noop/add-only plans.
pub(super) fn preflight(
    client: &KiCadIpcClient,
    document: &DocumentSpecifier,
    targets: &[Target],
) -> Vec<Refusal> {
    if targets.is_empty() {
        return Vec::new();
    }
    let target_kiids = targets.iter().map(|target| target.kiid.as_str()).collect();
    let inventory = client
        .save_document_to_string_in(document.clone())
        .context("bound live SaveDocumentToString failed")
        .and_then(|source| inventory(&source, &target_kiids));
    targets
        .iter() // A8 all replacement targets
        .filter_map(|target| {
            let result = inventory
                .as_ref()
                .map_err(|error| format!("{error:#}"))
                .and_then(|entries| {
                    if target.kiid.is_empty() {
                        return Err("replacement target has no KIID".to_string());
                    }
                    match entries.get(&target.kiid).map(Vec::as_slice) {
                        Some([children]) => Ok(children.clone()),
                        Some(_) => Err(format!("duplicate top-level footprint KIID {}", target.kiid)),
                        None => Err(format!("missing top-level footprint KIID {}", target.kiid)),
                    }
                });
            let (children, error) = match result {
                Ok(children) if children.kinds().is_empty() => return None,
                Ok(children) => (Some(children), None),
                Err(error) => (None, Some(error)),
            };
            Some(Refusal {
                reference: target.reference.clone(),
                details: Details {
                    kiid: target.kiid.clone(),
                    library_id: target.library_id.clone(),
                    kinds: children.as_ref().map(Children::kinds).unwrap_or_default(),
                    native_children: children,
                    inventory_error: error,
                },
            })
        })
        .collect()
}

fn inventory(
    source: &str,
    target_kiids: &BTreeSet<&str>,
) -> Result<BTreeMap<String, Vec<Children>>> {
    let root = konnect_sexp::parse_sexp(source).context("malformed native board")?;
    // parse_sexp wraps multiple roots. Requiring this head also refuses
    // trailing malformed input rather than accepting its first valid root.
    if root.head() != Some("kicad_pcb") {
        bail!("native document root must be kicad_pcb");
    }
    if root
        .children()
        .unwrap()
        .iter()
        .skip(1)
        .any(|n| n.head().is_none())
    {
        bail!("malformed native board child");
    }
    let mut entries = BTreeMap::<String, Vec<Children>>::new();
    for footprint in root.find_all("footprint") {
        let ids = footprint.find_all("uuid");
        let [id] = ids.as_slice() else {
            bail!("native footprint must have exactly one direct uuid");
        };
        let id = id
            .get(1)
            .and_then(SexpNode::as_str)
            .filter(|id| !id.is_empty())
            .context("native footprint has an invalid uuid")?;
        if ids[0].children().unwrap().len() != 2 {
            bail!("malformed native footprint uuid");
        }
        if !target_kiids.contains(id) {
            continue;
        }
        if footprint.get(1).and_then(SexpNode::as_str).is_none()
            || footprint
                .children()
                .unwrap()
                .iter()
                .skip(2)
                .any(|n| n.head().is_none())
        {
            bail!("malformed native footprint");
        }
        let variants = footprint.find_all("variant");
        let mut variant_names = Vec::new();
        for variant in &variants {
            let names = variant.find_all("name");
            let [name] = names.as_slice() else {
                bail!("native footprint variant must have exactly one name");
            };
            let value = name
                .get(1)
                .and_then(SexpNode::as_str)
                .filter(|name| !name.is_empty())
                .context("native footprint variant has an invalid name")?;
            if name.children().unwrap().len() != 2 {
                bail!("malformed native footprint variant name");
            }
            variant_names.push(value.to_string());
        }
        entries.entry(id.to_string()).or_default().push(Children {
            groups: footprint.find_all("group").len(),
            points: footprint.find_all("point").len(),
            variants: variants.len(),
            variant_names,
        });
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "native_footprint_children_tests.rs"]
mod tests;
