/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The process-wide registry of [`ReservedNamespace`]s, as Dgraph keeps it in package variables.
//!
//! With no registration the registry is empty and every check reports false, so a stock build keeps the pristine
//! behavior: nothing under `dgraph.` may be created except the pre-defined names.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{PoisonError, RwLock, RwLockReadGuard};

use ultragraphdb_common_utils::go_strings;

use crate::errors::key_error::KeyError;
use crate::keys::NS_SEPARATOR;
use crate::keys::namespace_attr::parse_attr;
use crate::types::reserved_namespace::ReservedNamespace;
use crate::types::trust_marker::TrustMarker;

struct Registry {
    prefixes: Vec<String>,
    predicates: BTreeSet<String>,
    types: BTreeSet<String>,
    // lowercased bare predicate -> TrustMarker
    value_locked: BTreeMap<String, TrustMarker>,
    // a locked predicate prefix with the TrustMarker that authorizes writing predicates under it
    value_locked_prefixes: Vec<(String, TrustMarker)>,
}

static REGISTRY: RwLock<Registry> = RwLock::new(Registry {
    prefixes: Vec::new(),
    predicates: BTreeSet::new(),
    types: BTreeSet::new(),
    value_locked: BTreeMap::new(),
    value_locked_prefixes: Vec::new(),
});

// No registration panics while holding the lock; a poisoned lock is read through regardless.
fn read() -> RwLockReadGuard<'static, Registry> {
    REGISTRY.read().unwrap_or_else(PoisonError::into_inner)
}

/// Records a plugin's ownership of names under the reserved `dgraph.` prefix. Call it at startup, before requests
/// are handled; it is safe for concurrent use.
///
/// # Errors
///
/// With Dgraph's panic texts, so a misconfiguration trips at startup rather than silently at mutation time:
/// [`TrustMarkerMissing`](crate::KeyErrorEnum::TrustMarkerMissing) when a value lock is set without a trust marker,
/// [`QualifiedReservedName`](crate::KeyErrorEnum::QualifiedReservedName) when a name carries the namespace separator
/// (the lookups match bare names, so a qualified one would never match and leave a locked predicate publicly
/// writable), [`EmptyValueLockedPrefix`](crate::KeyErrorEnum::EmptyValueLockedPrefix) when a value-locked prefix is
/// empty, and [`AlreadyRegistered`](crate::KeyErrorEnum::AlreadyRegistered) when a predicate, type, value lock or
/// value-locked prefix is already registered. A reserved name has a single owner; for value locks last-writer-wins
/// would let registration order pick the marker. What a refused registration recorded before the refusal stays,
/// as what Dgraph's records before it panics does.
pub fn register_reserved_namespace(ns: ReservedNamespace) -> Result<(), KeyError> {
    let marker = match ns.trust_marker() {
        Some(marker) => Some(marker),
        None if !ns.value_locked().is_empty() || !ns.value_locked_prefixes().is_empty() => {
            return Err(KeyError::TrustMarkerMissing());
        }
        None => None,
    };
    let groups = [
        ns.predicates(),
        ns.types(),
        ns.value_locked(),
        ns.value_locked_prefixes(),
    ];
    let mut names = std::iter::once(ns.predicate_prefix())
        .chain(groups.iter().flat_map(|g| g.iter().map(String::as_str)));
    if let Some(name) = names.find(|name| name.contains(NS_SEPARATOR)) {
        return Err(KeyError::QualifiedReservedName(name.to_string()));
    }

    let mut reg = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);
    if !ns.predicate_prefix().is_empty() {
        reg.prefixes
            .push(go_strings::to_lower(ns.predicate_prefix()));
    }
    for p in ns.predicates() {
        let key = go_strings::to_lower(p);
        if reg.predicates.contains(&key) {
            return Err(KeyError::AlreadyRegistered("predicate", key));
        }
        reg.predicates.insert(key);
    }
    for t in ns.types() {
        let key = go_strings::to_lower(t);
        if reg.types.contains(&key) {
            return Err(KeyError::AlreadyRegistered("type", key));
        }
        reg.types.insert(key);
    }
    // A value lock is only reached with a trust marker, checked above.
    let Some(marker) = marker else {
        return Ok(());
    };
    for p in ns.value_locked() {
        let key = go_strings::to_lower(p);
        if reg.value_locked.contains_key(&key) {
            return Err(KeyError::AlreadyRegistered("value-locked predicate", key));
        }
        reg.value_locked.insert(key, marker);
    }
    for p in ns.value_locked_prefixes() {
        let key = go_strings::to_lower(p);
        if key.is_empty() {
            // An empty prefix matches every predicate, locking the whole cluster to
            // this namespace's marker. It fails closed rather than open, so it is not
            // a bypass — but it is never intended, and PredicatePrefix above already
            // rejects the same mistake.
            return Err(KeyError::EmptyValueLockedPrefix());
        }
        if reg
            .value_locked_prefixes
            .iter()
            .any(|(existing, _)| *existing == key)
        {
            return Err(KeyError::AlreadyRegistered("value-locked prefix", key));
        }
        reg.value_locked_prefixes.push((key, marker));
    }
    Ok(())
}

/// Whether the namespaced predicate is owned by a registered [`ReservedNamespace`], by dynamic prefix or exact
/// name, in any case. The alter validator and the no-schema mutation guard allow such predicates through even
/// though they are not pre-defined.
///
/// # Errors
///
/// As [`parse_attr`](crate::parse_attr), and alike for [`is_registered_reserved_type`].
pub fn is_registered_reserved_predicate(pred: &str) -> Result<bool, KeyError> {
    let p = go_strings::to_lower(parse_attr(pred)?);
    let reg = read();
    Ok(reg
        .prefixes
        .iter()
        .any(|prefix| p.starts_with(prefix.as_str()))
        || reg.predicates.contains(&p))
}

/// Whether the namespaced type is owned by a registered [`ReservedNamespace`], in any case.
pub fn is_registered_reserved_type(typ: &str) -> Result<bool, KeyError> {
    Ok(read()
        .types
        .contains(&go_strings::to_lower(parse_attr(typ)?)))
}

/// The trust marker that authorizes writing the bare predicate's value, when its value is locked. The
/// mutation-value guard rejects a write of a locked predicate when the request does not carry that marker.
///
/// The predicate is matched case-insensitively, so a value lock cannot be bypassed by changing the case of an owned
/// name. It is bare: the guard passes the predicate without its namespace, as it consults
/// [`is_other_reserved_predicate`](crate::is_other_reserved_predicate).
///
/// Exact names win over prefixes, and among prefixes the longest match wins, so registration order never decides
/// which marker governs a predicate: one namespace may own a prefix while another pins a predicate inside it.
pub fn reserved_predicate_value_lock(pred: &str) -> Option<TrustMarker> {
    let p = go_strings::to_lower(pred);
    let reg = read();
    if let Some(marker) = reg.value_locked.get(&p) {
        return Some(*marker);
    }
    let mut best: Option<(usize, TrustMarker)> = None;
    for (prefix, marker) in &reg.value_locked_prefixes {
        if p.starts_with(prefix.as_str()) && best.is_none_or(|(len, _)| prefix.len() > len) {
            best = Some((prefix.len(), *marker));
        }
    }
    best.map(|(_, marker)| marker)
}
