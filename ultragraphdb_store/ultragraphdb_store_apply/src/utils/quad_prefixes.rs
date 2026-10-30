/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The prefixes of a predicate's keys in the quad keyspaces: every keyspace whose keys name it, and each index a
//! declaration names, split parts included, in every graph.

use ultragraphdb_core_keys::{CountDirection, Keyspace};
use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_tok::{get_tokenizer, get_tokenizer_for_lang};

use crate::types::predicate_prefix::PredicatePrefix;

/// The identifiers the value-index keys of tokenizer `name` carry: its own and, where a language tokenizer differs,
/// the language one's.
pub(crate) fn tokenizer_ids(name: &str) -> Vec<u8> {
    let Some(tokenizer) = get_tokenizer(name) else {
        return Vec::new();
    };
    let own = tokenizer.identifier();
    let lang = get_tokenizer_for_lang(&tokenizer, "en").identifier();
    if own == lang {
        vec![own]
    } else {
        vec![own, lang]
    }
}

/// The value-index keys of tokenizer `name` for predicate `predicate`.
pub(crate) fn tokenizer_prefixes(
    namespace: u64,
    predicate: u64,
    name: &str,
) -> Vec<PredicatePrefix> {
    tokenizer_ids(name)
        .into_iter()
        .flat_map(|id| {
            PredicatePrefix::with_parts(Keyspace::ValueIndex, namespace, predicate, vec![id])
        })
        .collect()
}

/// The count keys of `predicate`, both directions.
pub(crate) fn count_prefixes(namespace: u64, predicate: u64) -> Vec<PredicatePrefix> {
    [CountDirection::Forward, CountDirection::Reverse]
        .into_iter()
        .flat_map(|d| {
            PredicatePrefix::with_parts(Keyspace::CountIndex, namespace, predicate, vec![d.byte()])
        })
        .collect()
}

/// The HNSW lists of `predicate`.
pub(crate) fn vector_prefixes(namespace: u64, predicate: u64) -> Vec<PredicatePrefix> {
    PredicatePrefix::with_parts(Keyspace::VectorIndex, namespace, predicate, Vec::new())
}

/// The keys of every index `declaration` names for `predicate`.
pub(crate) fn declared_prefixes(
    namespace: u64,
    predicate: u64,
    declaration: &PredicateDeclaration,
) -> Vec<PredicatePrefix> {
    let mut prefixes: Vec<PredicatePrefix> = declaration
        .tokenizers()
        .iter()
        .flat_map(|t| tokenizer_prefixes(namespace, predicate, t))
        .collect();
    if declaration.count_index() {
        prefixes.extend(count_prefixes(namespace, predicate));
    }
    if declaration.vector_index().is_some() {
        prefixes.extend(vector_prefixes(namespace, predicate));
    }
    prefixes
}

/// The keys of every index of `predicate` that `declaration` does not name: each value-index identifier but its
/// tokenizers', and the count and vector indexes it does not declare. What an undone build removes, whatever the
/// build was making (7.9).
pub(crate) fn undeclared_prefixes(
    namespace: u64,
    predicate: u64,
    declaration: &PredicateDeclaration,
) -> Vec<PredicatePrefix> {
    let declared: Vec<u8> = declaration
        .tokenizers()
        .iter()
        .flat_map(|t| tokenizer_ids(t))
        .collect();
    let mut prefixes: Vec<PredicatePrefix> = (0..=u8::MAX)
        .filter(|id| !declared.contains(id))
        .flat_map(|id| {
            PredicatePrefix::with_parts(Keyspace::ValueIndex, namespace, predicate, vec![id])
        })
        .collect();
    if !declaration.count_index() {
        prefixes.extend(count_prefixes(namespace, predicate));
    }
    if declaration.vector_index().is_none() {
        prefixes.extend(vector_prefixes(namespace, predicate));
    }
    prefixes
}

/// The keys of `predicate` in every keyspace whose keys name it.
pub(crate) fn predicate_prefixes(namespace: u64, predicate: u64) -> Vec<PredicatePrefix> {
    [
        Keyspace::Forward,
        Keyspace::Reverse,
        Keyspace::ValueIndex,
        Keyspace::CountIndex,
        Keyspace::VectorIndex,
        Keyspace::Statistics,
    ]
    .into_iter()
    .flat_map(|k| PredicatePrefix::with_parts(k, namespace, predicate, Vec::new()))
    .collect()
}
