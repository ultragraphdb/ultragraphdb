/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_protocol_wire::{
    Declared, DeclaredKind, DeclaredPredicate, ErrorCode, ErrorReply,
};

use crate::types::system_group::system_group_catalog::CATALOG_CAP;

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
fn declaration_of(declared: &Declared) -> Result<Declaration, ErrorReply> {
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

/// The version `declared` builds on `held`: whether it tightens a predicate ([`tightens`]), and its declarations
/// ([`merged`]). Each IRI of `declared` is validated once. The predicates are read first and the types and relations after
/// the tightening is judged, so a Declare is refused for the first predicate naming no IRI, then for a tightening with a
/// loosening, then for the first type or relation naming no IRI.
pub(crate) fn built(
    held: Vec<Declaration>,
    declared: &[Declared],
) -> Result<(bool, Vec<Declaration>), ErrorReply> {
    let predicates = declared
        .iter()
        .map(|one| match one {
            Declared::Predicate(one) => predicate_of(one).map(Some),
            Declared::EntityType(_) | Declared::Relation(_) => Ok(None),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tightening = tightens(&held, &predicates)?;
    let declarations = predicates
        .into_iter()
        .zip(declared)
        .map(|(predicate, one)| match predicate {
            Some(predicate) => Ok(Declaration::Predicate(predicate)),
            None => declaration_of(one),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((tightening, merged(held, declarations)))
}

/// `held` with `declarations` over it, in their order: a declaration of a name held replaces the first held of that name, and
/// one of a name not held is appended, a later declaration of the same name replacing it.
pub(crate) fn merged(held: Vec<Declaration>, declarations: Vec<Declaration>) -> Vec<Declaration> {
    let places = places_of(&held, &declarations);
    let mut next = held;
    for (declaration, place) in declarations.into_iter().zip(places) {
        match next.get_mut(place) {
            Some(slot) => *slot = declaration,
            None => {
                debug_assert_eq!(place, next.len(), "a name not held is appended in order");
                next.push(declaration);
            }
        }
    }
    next
}

/// Where each of `declarations` lands in [`merged`]: the place of the first held declaration of its name, or the place the first
/// declaration of its name takes after `held`. One pass over `held`, each name looked up among those declared.
fn places_of(held: &[Declaration], declarations: &[Declaration]) -> Vec<usize> {
    let mut places: BTreeMap<(u8, &str), Option<usize>> = declarations
        .iter()
        .map(|declaration| (name_of(declaration), None))
        .collect();
    for (at, old) in held.iter().enumerate() {
        if let Some(place) = places.get_mut(&name_of(old))
            && place.is_none()
        {
            *place = Some(at);
        }
    }
    let mut appended = held.len();
    declarations
        .iter()
        .map(|declaration| {
            *places
                .entry(name_of(declaration))
                .or_default()
                .get_or_insert_with(|| {
                    let place = appended;
                    appended += 1;
                    place
                })
        })
        .collect()
}

/// Whether `old` to `new` narrows what the predicate takes, and whether it widens it (`PROTOCOL.md` §11.13). A change to an
/// index, to uniqueness or to a domain does neither. No `old` is the default entry, as [`PredicateDeclaration::new`] holds it:
/// either kind, multi-valued, no datatype.
fn narrows_and_widens(
    old: Option<&PredicateDeclaration>,
    new: &PredicateDeclaration,
) -> (bool, bool) {
    let (old_kind, old_cardinality, old_datatype) = old.map_or(
        (PredicateKind::default(), Cardinality::default(), None),
        |old| (old.kind(), old.cardinality(), old.datatype()),
    );
    let single = new.cardinality() == Cardinality::Single;
    let was_single = old_cardinality == Cardinality::Single;
    let kind_changed = new.kind() != old_kind;
    let datatype_changed = new.datatype() != old_datatype;
    (
        (single && !was_single)
            || (kind_changed && new.kind() != PredicateKind::Either)
            || (datatype_changed && new.datatype().is_some()),
        (was_single && !single)
            || (kind_changed && old_kind != PredicateKind::Either)
            || (datatype_changed && old_datatype.is_some()),
    )
}

/// Whether `predicates`, the predicates declared at their index among a Declare's declarations, tighten a predicate over `held`,
/// a predicate `held` does not declare standing as its default entry; declarations tightening one thing and loosening another
/// are refused with code 7, `detail` the index of the first that loosens (`PROTOCOL.md` §11.13).
pub(crate) fn tightens(
    held: &[Declaration],
    predicates: &[Option<PredicateDeclaration>],
) -> Result<bool, ErrorReply> {
    // The first held declaration of each predicate declared, found in one pass over `held`.
    let mut olds: BTreeMap<&str, Option<&PredicateDeclaration>> = predicates
        .iter()
        .flatten()
        .map(|new| (new.predicate().as_str(), None))
        .collect();
    for declaration in held {
        if let Declaration::Predicate(old) = declaration
            && let Some(slot) = olds.get_mut(old.predicate().as_str())
            && slot.is_none()
        {
            *slot = Some(old);
        }
    }
    let mut tightening = false;
    let mut loosening = None;
    for (index, new) in predicates.iter().enumerate() {
        let Some(new) = new else {
            continue;
        };
        let old = olds.get(new.predicate().as_str()).copied().flatten();
        let (narrows, widens) = narrows_and_widens(old, new);
        tightening |= narrows;
        if widens && loosening.is_none() {
            loosening = Some((index, new.predicate().as_str()));
        }
    }
    match loosening {
        Some((index, predicate)) if tightening => Err(ErrorReply::new(
            ErrorCode::Refused,
            format!(
                "these declarations tighten one thing and loosen <{predicate}>, which a check of the first could not undo: commit the loosening, then the tightening"
            ),
        )
        .with_detail(index as u64)), // lossless: a `usize` is at most 64 bits
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
    let changes = restorations(before, withdrawn, &current);
    current
        .into_iter()
        .zip(changes)
        .filter_map(|(declaration, change)| match change {
            None => Some(declaration),
            Some(previous) => previous.cloned(),
        })
        .collect()
}

/// What [`restored`] does to each declaration of `current`, by its place: `None` keeps it, `Some(Some(previous))` returns it to
/// what `before` held, `Some(None)` removes it. Each withdrawn declaration is matched against the first declaration of its name
/// in `current` that it has not removed, each name found by lookup rather than by a search of `before` and `current`.
fn restorations<'a>(
    before: &'a [Declaration],
    withdrawn: &[Declaration],
    current: &[Declaration],
) -> Vec<Option<Option<&'a Declaration>>> {
    // The first declaration of each name in `before`, which a withdrawn declaration of that name returns to.
    let mut previous_of: BTreeMap<(u8, &str), &Declaration> = BTreeMap::new();
    for declaration in before {
        let _first: &mut &Declaration = previous_of
            .entry(name_of(declaration))
            .or_insert(declaration);
    }
    // The first place of each name in `current`, and after each place the next of its name, which a removal makes the first.
    let mut first: BTreeMap<(u8, &str), usize> = BTreeMap::new();
    let mut later: Vec<Option<usize>> = vec![None; current.len()];
    for (at, declaration) in current.iter().enumerate().rev() {
        if let Some(slot) = later.get_mut(at) {
            *slot = first.insert(name_of(declaration), at);
        }
    }
    let mut changes: Vec<Option<Option<&Declaration>>> = vec![None; current.len()];
    for declaration in withdrawn {
        let name = name_of(declaration);
        let previous = previous_of.get(&name).copied();
        if previous == Some(declaration) {
            continue;
        }
        let Some(at) = first.get(&name).copied() else {
            continue;
        };
        let now = match changes.get(at) {
            Some(Some(restored)) => *restored,
            Some(None) | None => current.get(at),
        };
        if now != Some(declaration) {
            continue;
        }
        if let Some(change) = changes.get_mut(at) {
            *change = Some(previous);
        }
        if previous.is_none() {
            let _removed: Option<usize> = match later.get(at).copied().flatten() {
                Some(next) => first.insert(name, next),
                None => first.remove(&name),
            };
        }
    }
    changes
}

/// The bytes of version `version` of `tenant`, holding `declarations`, refused past `catalog_bytes_max` with code 7, `detail` the
/// bound (`PROTOCOL.md` §11.9, §11.13).
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
                "a catalog version of {} bytes passes `catalog_bytes_max`, {CATALOG_CAP} bytes",
                bytes.len()
            ),
        )
        .with_detail(CATALOG_CAP as u64)); // lossless: a `usize` is at most 64 bits
    }
    Ok(bytes)
}
