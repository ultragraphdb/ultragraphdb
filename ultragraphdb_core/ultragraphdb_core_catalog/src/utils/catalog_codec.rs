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

/// The kind of a catalog entry: the byte after the namespace in its key, and the byte after the format in its value.
pub const PREDICATE: u8 = 1;
pub const ENTITY_TYPE: u8 = 2;
pub const RELATION: u8 = 3;
/// The entry kind of `declaration`.
pub fn kind_of(declaration: &Declaration) -> u8 {
    match declaration {
        Declaration::Predicate(_) => PREDICATE,
        Declaration::EntityType(_) => ENTITY_TYPE,
        Declaration::Relation(_) => RELATION,
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
            list(&mut w, p.tokenizers().iter().map(String::as_str));
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
            list(&mut w, p.domains().iter().map(NamedNode::as_str));
        }
        Declaration::EntityType(t) => {
            string(&mut w, t.iri().as_str());
            optional(&mut w, t.parent().map(NamedNode::as_str));
            w.push(u8::from(t.is_abstract()));
        }
        Declaration::Relation(r) => {
            string(&mut w, r.iri().as_str());
            list(&mut w, r.roles().iter().map(NamedNode::as_str));
        }
    }
    w
}

/// The declaration stored as `bytes` under `key`, refused, naming the key, when it cannot be read.
pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Declaration, CatalogError> {
    let mut r = Reader { key, bytes, at: 0 };
    let format = r.u8()?;
    if format != FORMAT {
        return Err(CatalogError::Corrupted(
            key,
            format!("unknown format {format}"),
        ));
    }
    let declaration = match r.u8()? {
        PREDICATE => {
            let mut p = PredicateDeclaration::new(r.iri()?);
            p = p.with_kind(match r.u8()? {
                0 => PredicateKind::Edge,
                1 => PredicateKind::Property,
                2 => PredicateKind::Either,
                other => {
                    return Err(CatalogError::Corrupted(
                        key,
                        format!("unknown predicate kind {other}"),
                    ));
                }
            });
            p = p.with_cardinality(match r.u8()? {
                0 => Cardinality::Single,
                1 => Cardinality::Multi,
                other => {
                    return Err(CatalogError::Corrupted(
                        key,
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
            Declaration::Predicate(p.with_domains(domains))
        }
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
            return Err(CatalogError::Corrupted(
                key,
                format!("unknown entry kind {other}"),
            ));
        }
    };
    if r.at != bytes.len() {
        return Err(CatalogError::Corrupted(key, "bytes follow the declaration"));
    }
    Ok(declaration)
}

fn string(w: &mut Vec<u8>, s: &str) {
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
    w.extend_from_slice(&(n as u16).to_be_bytes());
}

fn list<'a>(w: &mut Vec<u8>, items: impl ExactSizeIterator<Item = &'a str>) {
    count(w, items.len());
    items.for_each(|s| string(w, s));
}

struct Reader<'a> {
    key: &'a [u8],
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], CatalogError> {
        let part = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or_else(|| CatalogError::Corrupted(self.key, "the entry ends early"))?;
        self.at += n;
        Ok(part)
    }

    fn u8(&mut self) -> Result<u8, CatalogError> {
        Ok(self.take(1)?[0])
    }

    fn flag(&mut self) -> Result<bool, CatalogError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(CatalogError::Corrupted(
                self.key,
                format!("flag byte {other}"),
            )),
        }
    }

    fn u16(&mut self) -> Result<u16, CatalogError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn count(&mut self) -> Result<usize, CatalogError> {
        self.u16().map(usize::from)
    }

    fn string(&mut self) -> Result<String, CatalogError> {
        let b = self.take(4)?;
        let len = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
        let key = self.key;
        let text = self.take(len)?.to_vec();
        String::from_utf8(text).map_err(|_| CatalogError::Corrupted(key, "a string is not UTF-8"))
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
        NamedNode::new(self.string()?).map_err(|e| CatalogError::Corrupted(key, e.to_string()))
    }
}
