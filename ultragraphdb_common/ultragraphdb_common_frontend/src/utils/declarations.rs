/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_protocol_wire::{
    Declared, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation, Describe,
    ErrorCode, ErrorReply,
};

use crate::types::owner::owner_catalog::CATALOG_CAP;

fn iri(text: &str) -> Result<NamedNode, ErrorReply> {
    NamedNode::new(text).map_err(|error| {
        ErrorReply::new(ErrorCode::Refused, format!("{text} is not an IRI: {error}"))
    })
}

fn predicate_of(held: &DeclaredPredicate) -> Result<PredicateDeclaration, ErrorReply> {
    let mut predicate = PredicateDeclaration::new(iri(held.predicate())?)
        .with_kind(match held.kind() {
            DeclaredKind::Edge => PredicateKind::Edge,
            DeclaredKind::Property => PredicateKind::Property,
            DeclaredKind::Either => PredicateKind::Either,
        })
        .with_cardinality(if held.single() {
            Cardinality::Single
        } else {
            Cardinality::Multi
        })
        .with_unique(held.unique())
        .with_tokenizers(held.tokenizers().to_vec())
        .with_count_index(held.count_index())
        .with_domains(
            held.domains()
                .iter()
                .map(|domain| iri(domain))
                .collect::<Result<_, _>>()?,
        );
    if let Some(datatype) = held.datatype() {
        predicate = predicate.with_datatype(iri(datatype)?);
    }
    if let Some((factory, options)) = held.vector_index() {
        predicate =
            predicate.with_vector_index(VectorIndexSpec::new(factory.clone(), options.clone()));
    }
    Ok(predicate)
}

/// The store's declaration of what a Write declared.
pub(crate) fn declaration_of(declared: &Declared) -> Result<Declaration, ErrorReply> {
    Ok(match declared {
        Declared::Predicate(held) => Declaration::Predicate(predicate_of(held)?),
        Declared::EntityType(held) => Declaration::EntityType(TypeDeclaration::new(
            iri(held.iri())?,
            held.parent().map(iri).transpose()?,
            held.is_abstract(),
        )),
        Declared::Relation(held) => Declaration::Relation(RelationDeclaration::new(
            iri(held.iri())?,
            held.roles()
                .iter()
                .map(|role| iri(role))
                .collect::<Result<_, _>>()?,
        )),
    })
}

/// The declared IRI of `declaration`, and its kind's place: what one replaces in a later version.
fn name_of(declaration: &Declaration) -> (u8, &str) {
    match declaration {
        Declaration::Predicate(held) => (0, held.predicate().as_str()),
        Declaration::EntityType(held) => (1, held.iri().as_str()),
        Declaration::Relation(held) => (2, held.iri().as_str()),
    }
}

/// `held` with `declared` over it: a declaration of a name held replaces it.
pub(crate) fn merged(
    held: Vec<Declaration>,
    declared: &[Declared],
) -> Result<Vec<Declaration>, ErrorReply> {
    let mut next = held;
    for one in declared {
        let declaration = declaration_of(one)?;
        let name = name_of(&declaration);
        match next.iter().position(|old| name_of(old) == name) {
            Some(at) => {
                if let Some(slot) = next.get_mut(at) {
                    *slot = declaration;
                }
            }
            None => next.push(declaration),
        }
    }
    Ok(next)
}

/// Whether `old` to `new` narrows what the predicate takes, and whether it widens it (`PROTOCOL.md` §11.13). A change to an
/// index, to uniqueness or to a domain does neither.
fn narrows_and_widens(old: &PredicateDeclaration, new: &PredicateDeclaration) -> (bool, bool) {
    let single = new.cardinality() == Cardinality::Single;
    let was_single = old.cardinality() == Cardinality::Single;
    let kind_changed = new.kind() != old.kind();
    let datatype_changed = new.datatype() != old.datatype();
    (
        (single && !was_single)
            || (kind_changed && new.kind() != PredicateKind::Either)
            || (datatype_changed && new.datatype().is_some()),
        (was_single && !single)
            || (kind_changed && old.kind() != PredicateKind::Either)
            || (datatype_changed && old.datatype().is_some()),
    )
}

/// Whether `declared` tightens a predicate over `held`, a predicate `held` does not declare standing as its default entry;
/// declarations tightening one thing and loosening another are refused with code 7, `detail` the index among `declared` of the
/// first that loosens (`PROTOCOL.md` §11.13).
pub(crate) fn tightens(held: &[Declaration], declared: &[Declared]) -> Result<bool, ErrorReply> {
    let mut tightening = false;
    let mut loosening = None;
    for (index, one) in declared.iter().enumerate() {
        let Declared::Predicate(one) = one else {
            continue;
        };
        let new = predicate_of(one)?;
        let default = PredicateDeclaration::new(new.predicate().clone());
        let old = held
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Predicate(old) if old.predicate() == new.predicate() => Some(old),
                Declaration::Predicate(_)
                | Declaration::EntityType(_)
                | Declaration::Relation(_) => None,
            })
            .unwrap_or(&default);
        let (narrows, widens) = narrows_and_widens(old, &new);
        tightening |= narrows;
        if widens && loosening.is_none() {
            loosening = Some((index, new.predicate().as_str().to_string()));
        }
    }
    match loosening {
        Some((index, predicate)) if tightening => Err(ErrorReply::new(
            ErrorCode::Refused,
            format!(
                "these declarations tighten one thing and loosen <{predicate}>, which a check of the first could not undo: commit the loosening, then the tightening"
            ),
        )
        .with_detail(index as u64)),
        Some(_) | None => Ok(tightening),
    }
}

/// `current` with what `withdrawn` changed from `before` back as `before` held it, each declaration a later version changed since
/// left as it is (`PROTOCOL.md` §11.13).
pub(crate) fn restored(
    before: &[Declaration],
    withdrawn: &[Declaration],
    current: Vec<Declaration>,
) -> Vec<Declaration> {
    let named = |held: &[Declaration], name: (u8, &str)| {
        held.iter()
            .position(|declaration| name_of(declaration) == name)
    };
    let mut next = current;
    for declaration in withdrawn {
        let name = name_of(declaration);
        let previous = named(before, name).and_then(|at| before.get(at));
        if previous == Some(declaration) {
            continue;
        }
        let unchanged = named(&next, name).filter(|at| next.get(*at) == Some(declaration));
        match (unchanged, previous) {
            (Some(at), Some(previous)) => {
                if let Some(slot) = next.get_mut(at) {
                    slot.clone_from(previous);
                }
            }
            (Some(at), None) => drop(next.remove(at)),
            (None, Some(_) | None) => {}
        }
    }
    next
}

/// The bytes of version `version` of `tenant`, holding `declarations`, refused past what one Catalog frame holds with code 7.
pub(crate) fn version_bytes(
    tenant: u64,
    version: u64,
    declarations: Vec<Declaration>,
) -> Result<Vec<u8>, ErrorReply> {
    let bytes = CatalogContents::new(tenant, version, declarations)
        .encode()
        .map_err(|error| ErrorReply::new(ErrorCode::Refused, error.to_string()))?;
    if bytes.len() > CATALOG_CAP {
        return Err(ErrorReply::new(
            ErrorCode::Refused,
            format!(
                "a catalog version of {} bytes passes what one Catalog frame holds",
                bytes.len()
            ),
        )
        .with_detail(CATALOG_CAP as u64));
    }
    Ok(bytes)
}

fn declared_predicate(held: &PredicateDeclaration) -> DeclaredPredicate {
    let mut predicate = DeclaredPredicate::new(held.predicate().as_str())
        .with_kind(match held.kind() {
            PredicateKind::Edge => DeclaredKind::Edge,
            PredicateKind::Property => DeclaredKind::Property,
            PredicateKind::Either => DeclaredKind::Either,
        })
        .with_single(held.cardinality() == Cardinality::Single)
        .with_unique(held.unique())
        .with_tokenizers(held.tokenizers().to_vec())
        .with_count_index(held.count_index())
        .with_domains(
            held.domains()
                .iter()
                .map(|d| d.as_str().to_string())
                .collect(),
        );
    if let Some(datatype) = held.datatype() {
        predicate = predicate.with_datatype(datatype.as_str());
    }
    if let Some(spec) = held.vector_index() {
        predicate = predicate.with_vector_index(spec.factory(), spec.options().to_vec());
    }
    predicate
}

/// Each entity type's preorder number, from 1: each type before its children, roots and siblings in their IRIs' order.
fn preorder(types: &[&TypeDeclaration]) -> BTreeMap<String, u32> {
    let mut children: BTreeMap<Option<&str>, BTreeSet<&str>> = BTreeMap::new();
    for held in types {
        let _new = children
            .entry(held.parent().map(NamedNode::as_str))
            .or_default()
            .insert(held.iri().as_str());
    }
    let mut numbers = BTreeMap::new();
    let mut stack: Vec<&str> = children
        .get(&None)
        .map(|roots| roots.iter().rev().copied().collect())
        .unwrap_or_default();
    let mut next = 1;
    // Each turn pops one type and pushes its children, and a type has one parent, so each is taken once.
    while let Some(held) = stack.pop() {
        let _new = numbers.insert(held.to_string(), next);
        next += 1;
        if let Some(below) = children.get(&Some(held)) {
            stack.extend(below.iter().rev().copied());
        }
    }
    numbers
}

/// The relationship types and property keys of `predicates` and of `recorded`, the default entries, each of kind either.
fn kinds<'a>(
    predicates: &[&PredicateDeclaration],
    recorded: impl Iterator<Item = &'a str>,
) -> (Vec<String>, Vec<String>) {
    let mut edges: BTreeSet<String> = BTreeSet::new();
    let mut properties: BTreeSet<String> = BTreeSet::new();
    for held in predicates {
        let name = held.predicate().as_str().to_string();
        match held.kind() {
            PredicateKind::Edge => {
                let _new = edges.insert(name);
            }
            PredicateKind::Property => {
                let _new = properties.insert(name);
            }
            PredicateKind::Either => {
                let _edge = edges.insert(name.clone());
                let _property = properties.insert(name);
            }
        }
    }
    for predicate in recorded {
        let declared = predicates
            .iter()
            .any(|held| held.predicate().as_str() == predicate);
        if !declared {
            let _edge = edges.insert(predicate.to_string());
            let _property = properties.insert(predicate.to_string());
        }
    }
    (
        edges.into_iter().collect(),
        properties.into_iter().collect(),
    )
}

/// The answer to a Describe of `tenant` at `read_version`: `declarations` by name, and `recorded` predicates among its kinds.
pub(crate) fn describe_of<'a>(
    tenant: u64,
    read_version: u64,
    declarations: &[Declaration],
    recorded: impl Iterator<Item = &'a str>,
) -> Describe {
    let mut predicates: Vec<&PredicateDeclaration> = Vec::new();
    let mut types: Vec<&TypeDeclaration> = Vec::new();
    let mut relations: Vec<&RelationDeclaration> = Vec::new();
    for declaration in declarations {
        match declaration {
            Declaration::Predicate(held) => predicates.push(held),
            Declaration::EntityType(held) => types.push(held),
            Declaration::Relation(held) => relations.push(held),
        }
    }
    predicates.sort_by(|a, b| a.predicate().as_str().cmp(b.predicate().as_str()));
    types.sort_by(|a, b| a.iri().as_str().cmp(b.iri().as_str()));
    relations.sort_by(|a, b| a.iri().as_str().cmp(b.iri().as_str()));
    let numbers = preorder(&types);
    let (edges, properties) = kinds(&predicates, recorded);
    Describe::new(tenant, read_version)
        .with_labels(types.iter().map(|t| t.iri().as_str().to_string()).collect())
        .with_relationship_types(edges)
        .with_property_keys(properties)
        .with_predicates(predicates.iter().map(|p| declared_predicate(p)).collect())
        .with_entity_types(
            types
                .iter()
                .map(|t| {
                    let number = numbers.get(t.iri().as_str()).copied().unwrap_or(0);
                    (
                        DeclaredEntityType::new(
                            t.iri().as_str(),
                            t.parent().map(|parent| parent.as_str().to_string()),
                            t.is_abstract(),
                        ),
                        number,
                    )
                })
                .collect(),
        )
        .with_relations(
            relations
                .iter()
                .map(|r| {
                    DeclaredRelation::new(
                        r.iri().as_str(),
                        r.roles()
                            .iter()
                            .map(|role| role.as_str().to_string())
                            .collect(),
                    )
                })
                .collect(),
        )
}
