/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

use oxrdf::NamedNode;
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration, VectorIndexSpec,
};

use crate::errors::catalog_error::CatalogError;

/// The format of a catalog entry's value: a version byte, then the declaration. A string is a u32 big-endian length
/// and its UTF-8 bytes, an optional string a presence byte before it, a list a u16 big-endian count before its items.
/// Format 2 holds a vector index's dimension, u16 big-endian after its factory, 0 for none.
const FORMAT: u8 = 2;

/// The kind of a predicate's entry. An entry's kind is the byte after the tenant in its key, and the byte after the
/// format in its value.
pub const PREDICATE: u8 = 1;
/// The kind of an entity type's entry.
pub const ENTITY_TYPE: u8 = 2;
/// The kind of a relation's entry.
pub const RELATION: u8 = 3;
/// The entry kind of `declaration`.
pub fn kind_of(declaration: &Declaration) -> u8 {
    match declaration {
        Declaration::Predicate(_) => PREDICATE,
        Declaration::EntityType(_) => ENTITY_TYPE,
        Declaration::Relation(_) => RELATION,
    }
}

/// The longest list a declaration's encoding holds, its count being a u16.
pub(crate) const LIST_MAX: usize = u16::MAX as usize;
/// The longest string a declaration's encoding holds, its length being a u32.
pub(crate) const STRING_MAX: usize = u32::MAX as usize;

/// Whether every string and list of `declaration` fits the encoding: [`encode`] writes a declaration that does
/// without loss.
///
/// # Errors
///
/// [`CatalogError::too_large`] naming the first string or list past its maximum.
pub(crate) fn check_lengths(declaration: &Declaration) -> Result<(), CatalogError> {
    match declaration {
        Declaration::Predicate(p) => {
            let iri = p.predicate().as_str();
            strings(iri, "IRI bytes", [iri])?;
            strings(iri, "datatype bytes", p.datatype().map(NamedNode::as_str))?;
            list(iri, "tokenizers", p.tokenizers().len())?;
            strings(
                iri,
                "tokenizer bytes",
                p.tokenizers().iter().map(String::as_str),
            )?;
            if let Some(spec) = p.vector_index() {
                strings(iri, "vector index bytes", [spec.factory()])?;
                list(iri, "vector index options", spec.options().len())?;
                let options = spec.options().iter();
                strings(
                    iri,
                    "option bytes",
                    options.flat_map(|(k, v)| [k.as_str(), v.as_str()]),
                )?;
            }
            list(iri, "domains", p.domains().len())?;
            strings(
                iri,
                "domain bytes",
                p.domains().iter().map(NamedNode::as_str),
            )
        }
        Declaration::EntityType(t) => {
            let iri = t.iri().as_str();
            strings(iri, "IRI bytes", [iri])?;
            strings(iri, "parent bytes", t.parent().map(NamedNode::as_str))
        }
        Declaration::Relation(r) => {
            let iri = r.iri().as_str();
            strings(iri, "IRI bytes", [iri])?;
            list(iri, "roles", r.roles().len())?;
            strings(iri, "role bytes", r.roles().iter().map(NamedNode::as_str))
        }
    }
}

/// Refuses the first of `items` longer than [`STRING_MAX`], as `what` of the declaration of `iri`.
fn strings<'a>(
    iri: &str,
    what: &'static str,
    items: impl IntoIterator<Item = &'a str>,
) -> Result<(), CatalogError> {
    items
        .into_iter()
        .find(|s| s.len() > STRING_MAX)
        .map_or(Ok(()), |s| {
            Err(CatalogError::too_large(iri, what, s.len(), STRING_MAX))
        })
}

/// Refuses a list of `len` items longer than [`LIST_MAX`], as `what` of the declaration of `iri`.
fn list(iri: &str, what: &'static str, len: usize) -> Result<(), CatalogError> {
    if len > LIST_MAX {
        Err(CatalogError::too_large(iri, what, len, LIST_MAX))
    } else {
        Ok(())
    }
}

/// The stored value of `declaration`.
pub fn encode(declaration: &Declaration) -> Vec<u8> {
    let mut w = vec![FORMAT, kind_of(declaration)];
    match declaration {
        Declaration::Predicate(p) => {
            string(&mut w, p.predicate().as_str());
            w.push(match p.kind() {
                PredicateKind::Edge => 0,
                PredicateKind::Property => 1,
                PredicateKind::Either => 2,
            });
            w.push(match p.cardinality() {
                Cardinality::Single => 0,
                Cardinality::Multi => 1,
            });
            optional(&mut w, p.datatype().map(NamedNode::as_str));
            w.push(u8::from(p.unique()));
            write_list(&mut w, p.tokenizers().iter().map(String::as_str));
            w.push(u8::from(p.count_index()));
            match p.vector_index() {
                Some(spec) => {
                    w.push(1);
                    string(&mut w, spec.factory());
                    let dimension = spec.dimension().map_or(0, NonZeroU16::get);
                    w.extend_from_slice(&dimension.to_be_bytes());
                    count(&mut w, spec.options().len());
                    spec.options().iter().for_each(|(k, v)| {
                        string(&mut w, k);
                        string(&mut w, v);
                    });
                }
                None => w.push(0),
            }
            write_list(&mut w, p.domains().iter().map(NamedNode::as_str));
        }
        Declaration::EntityType(t) => {
            string(&mut w, t.iri().as_str());
            optional(&mut w, t.parent().map(NamedNode::as_str));
            w.push(u8::from(t.is_abstract()));
        }
        Declaration::Relation(r) => {
            string(&mut w, r.iri().as_str());
            write_list(&mut w, r.roles().iter().map(NamedNode::as_str));
        }
    }
    // Paired with the load's decode: what is written reads back as the declaration it was written from.
    debug_assert!(decode(&[], &w).is_ok_and(|read| read == *declaration));
    w
}

/// The declaration stored as `bytes` under `key`, refused, naming the key, when it cannot be read.
pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Declaration, CatalogError> {
    let mut r = Reader { key, bytes, at: 0 };
    let format = r.u8()?;
    if format != FORMAT {
        return Err(CatalogError::corrupted(
            key,
            format!("unknown format {format}"),
        ));
    }
    let declaration = match r.u8()? {
        PREDICATE => Declaration::Predicate(predicate(&mut r)?),
        ENTITY_TYPE => {
            let iri = r.iri()?;
            let parent = r.optional()?.map(NamedNode::new_unchecked);
            Declaration::EntityType(TypeDeclaration::new(iri, parent, r.flag()?))
        }
        RELATION => {
            let iri = r.iri()?;
            let roles = (0..r.count()?).map(|_| r.iri()).collect::<Result<_, _>>()?;
            Declaration::Relation(RelationDeclaration::new(iri, roles))
        }
        other => {
            return Err(CatalogError::corrupted(
                key,
                format!("unknown entry kind {other}"),
            ));
        }
    };
    if r.at != bytes.len() {
        return Err(CatalogError::corrupted(key, "bytes follow the declaration"));
    }
    Ok(declaration)
}

/// A predicate's declaration, read after its entry kind.
fn predicate(r: &mut Reader<'_>) -> Result<PredicateDeclaration, CatalogError> {
    let mut p = PredicateDeclaration::new(r.iri()?);
    p = p.with_kind(match r.u8()? {
        0 => PredicateKind::Edge,
        1 => PredicateKind::Property,
        2 => PredicateKind::Either,
        other => {
            return Err(CatalogError::corrupted(
                r.key,
                format!("unknown predicate kind {other}"),
            ));
        }
    });
    p = p.with_cardinality(match r.u8()? {
        0 => Cardinality::Single,
        1 => Cardinality::Multi,
        other => {
            return Err(CatalogError::corrupted(
                r.key,
                format!("unknown cardinality {other}"),
            ));
        }
    });
    if let Some(datatype) = r.optional()? {
        p = p.with_datatype(NamedNode::new_unchecked(datatype));
    }
    p = p.with_unique(r.flag()?);
    let tokenizers = (0..r.count()?)
        .map(|_| r.string())
        .collect::<Result<_, _>>()?;
    p = p.with_tokenizers(tokenizers);
    p = p.with_count_index(r.flag()?);
    if r.flag()? {
        let factory = r.string()?;
        let dimension = NonZeroU16::new(r.u16()?);
        let options = (0..r.count()?)
            .map(|_| Ok((r.string()?, r.string()?)))
            .collect::<Result<_, CatalogError>>()?;
        let spec = VectorIndexSpec::new(factory, options);
        p = p.with_vector_index(match dimension {
            Some(d) => spec.with_dimension(d),
            None => spec,
        });
    }
    let domains = (0..r.count()?).map(|_| r.iri()).collect::<Result<_, _>>()?;
    Ok(p.with_domains(domains))
}

// The lengths fit their widths for every declaration [`check_lengths`] passes, which every declaration the catalog
// takes and every version encoded is checked by: the conversions are lossless there.
fn string(w: &mut Vec<u8>, s: &str) {
    debug_assert!(s.len() <= STRING_MAX);
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

fn optional(w: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            w.push(1);
            string(w, s);
        }
        None => w.push(0),
    }
}

fn count(w: &mut Vec<u8>, n: usize) {
    debug_assert!(n <= LIST_MAX);
    w.extend_from_slice(&(n as u16).to_be_bytes());
}

fn write_list<'a>(w: &mut Vec<u8>, items: impl ExactSizeIterator<Item = &'a str>) {
    count(w, items.len());
    items.for_each(|s| string(w, s));
}

struct Reader<'a> {
    key: &'a [u8],
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    /// The next `n` bytes, refused when the entry ends before them; the end is computed checked, so a length read
    /// from the entry cannot wrap past it.
    fn take(&mut self, n: usize) -> Result<&[u8], CatalogError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| CatalogError::corrupted(self.key, "the entry ends early"))?;
        let part = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| CatalogError::corrupted(self.key, "the entry ends early"))?;
        self.at = end;
        debug_assert!(self.at <= self.bytes.len());
        Ok(part)
    }

    /// The next `N` bytes as an array, read without indexing.
    fn array<const N: usize>(&mut self) -> Result<[u8; N], CatalogError> {
        let key = self.key;
        <[u8; N]>::try_from(self.take(N)?)
            .map_err(|_| CatalogError::corrupted(key, "the entry ends early"))
    }

    fn u8(&mut self) -> Result<u8, CatalogError> {
        let [byte] = self.array::<1>()?;
        Ok(byte)
    }

    fn flag(&mut self) -> Result<bool, CatalogError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(CatalogError::corrupted(
                self.key,
                format!("flag byte {other}"),
            )),
        }
    }

    fn u16(&mut self) -> Result<u16, CatalogError> {
        self.array::<2>().map(u16::from_be_bytes)
    }

    fn count(&mut self) -> Result<usize, CatalogError> {
        self.u16().map(usize::from)
    }

    fn string(&mut self) -> Result<String, CatalogError> {
        let key = self.key;
        let len = usize::try_from(u32::from_be_bytes(self.array::<4>()?))
            .map_err(|_| CatalogError::corrupted(key, "a string's length exceeds memory"))?;
        let text = self.take(len)?.to_vec();
        String::from_utf8(text).map_err(|_| CatalogError::corrupted(key, "a string is not UTF-8"))
    }

    fn optional(&mut self) -> Result<Option<String>, CatalogError> {
        if self.flag()? {
            self.string().map(Some)
        } else {
            Ok(None)
        }
    }

    fn iri(&mut self) -> Result<NamedNode, CatalogError> {
        let key = self.key;
        NamedNode::new(self.string()?).map_err(|e| CatalogError::corrupted(key, e.to_string()))
    }
}
