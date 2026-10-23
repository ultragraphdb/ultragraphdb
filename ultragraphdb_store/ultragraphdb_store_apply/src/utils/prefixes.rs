/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (prefixesToDeleteTokensFor, prefixesToDropCountIndex,
 * prefixesToDropReverseEdges, prefixesToDropVectorIndexEdges, DeleteData, DeletePredicate) and x/keys.go
 * (ParsedKey's prefixes). Changed for ultragraphdb: translated to Rust; a predicate's and a namespace's drops pair
 * every prefix with its split twin, where Dgraph's `DeletePredicate` and `DeleteData` drop the default prefix
 * alone (ledger 7).
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, BYTE_SPLIT,
    BYTE_TYPE, data_prefix, namespace_to_bytes, predicate_prefix, schema_key,
};
use ultragraphdb_core_tok::{
    VEC_DEAD, VEC_ENTRY, VEC_KEYWORD, get_tokenizer, get_tokenizer_for_lang,
};

use crate::errors::apply_error::ApplyError;

/// The levels of the per-level predicates an older HNSW kept, which a vector index's drop removes too: Dgraph's
/// `VectorIndexMaxLevels`.
const VECTOR_INDEX_MAX_LEVELS: usize = 5;

/// `prefix` with its first byte the split byte: the prefix of the parts of the lists under `prefix`.
pub(crate) fn split_twin(prefix: &[u8]) -> Vec<u8> {
    let mut p = prefix.to_vec();
    if let Some(first) = p.first_mut() {
        *first = BYTE_SPLIT;
    }
    p
}

/// `prefixes` each followed by its split twin.
pub(crate) fn with_twins(prefixes: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    prefixes
        .into_iter()
        .flat_map(|p| {
            let twin = split_twin(&p);
            [p, twin]
        })
        .collect()
}

/// The prefix of the keys of `attr` of the kind `byte_type`.
pub(crate) fn kind_prefix(attr: &str, byte_type: u8) -> Result<Vec<u8>, ApplyError> {
    let mut p = predicate_prefix(attr)?;
    p.push(byte_type);
    Ok(p)
}

/// The prefix of the data keys of `attr`.
pub(crate) fn attr_data_prefix(attr: &str) -> Result<Vec<u8>, ApplyError> {
    kind_prefix(attr, BYTE_DATA)
}

/// The prefix of the reverse keys of `attr`.
pub(crate) fn attr_reverse_prefix(attr: &str) -> Result<Vec<u8>, ApplyError> {
    kind_prefix(attr, BYTE_REVERSE)
}

/// Dgraph's `prefixesToDeleteTokensFor`: the prefix of the index keys `tokenizer_name` makes for `attr`, the language
/// identifier's with `has_lang`, and its split twin.
///
/// # Errors
///
/// `Could not find valid tokenizer for <name>` for a tokenizer the registry does not hold.
pub fn prefixes_to_delete_tokens_for(
    attr: &str,
    tokenizer_name: &str,
    has_lang: bool,
) -> Result<Vec<Vec<u8>>, ApplyError> {
    let mut tokenizer = get_tokenizer(tokenizer_name)
        .ok_or_else(|| ApplyError::UnknownTokenizer(tokenizer_name))?;
    if has_lang {
        // The exact tokenizer's identifier with a language is the same for every language.
        tokenizer = get_tokenizer_for_lang(&tokenizer, "en");
    }
    let mut prefix = kind_prefix(attr, BYTE_INDEX)?;
    prefix.push(tokenizer.identifier());
    Ok(with_twins(vec![prefix]))
}

/// Dgraph's `appendTokenizerPrefixesToDelete`: the prefixes of `tokenizer`'s index keys, its language identifier's
/// too for `exact`.
pub(crate) fn tokenizer_prefixes(attr: &str, tokenizer: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    let mut prefixes = prefixes_to_delete_tokens_for(attr, tokenizer, false)?;
    if tokenizer == "exact" {
        prefixes.extend(prefixes_to_delete_tokens_for(attr, tokenizer, true)?);
    }
    Ok(prefixes)
}

/// Dgraph's `prefixesToDropCountIndex`: the count and reverse count prefixes with their twins.
pub(crate) fn count_prefixes(attr: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    Ok(with_twins(vec![
        kind_prefix(attr, BYTE_COUNT)?,
        kind_prefix(attr, BYTE_COUNT_REV)?,
    ]))
}

/// The reverse count prefix with its twin.
pub(crate) fn reverse_count_prefixes(attr: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    Ok(with_twins(vec![kind_prefix(attr, BYTE_COUNT_REV)?]))
}

/// Dgraph's `prefixesToDropReverseEdges`: the reverse prefix with its twin.
pub(crate) fn reverse_prefixes(attr: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    Ok(with_twins(vec![attr_reverse_prefix(attr)?]))
}

/// The predicates a vector index keeps beside `attr`: the entry point, the neighbour lists, the dead list.
pub(crate) fn vector_predicates(attr: &str) -> [String; 3] {
    [
        format!("{attr}{VEC_ENTRY}"),
        format!("{attr}{VEC_KEYWORD}"),
        format!("{attr}{VEC_DEAD}"),
    ]
}

/// Dgraph's `prefixesToDropVectorIndexEdges`: the prefixes of the predicates a vector index keeps, and of the
/// per-level predicates of older indexes, each with its twin.
pub(crate) fn vector_edge_prefixes(attr: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    let mut preds: Vec<String> = vector_predicates(attr).into_iter().collect();
    preds.extend((0..VECTOR_INDEX_MAX_LEVELS).map(|i| format!("{attr}{VEC_KEYWORD}{i}")));
    let prefixes = preds
        .iter()
        .map(|p| predicate_prefix(p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(with_twins(prefixes))
}

/// The prefixes a predicate's drop removes: every key of the predicate, its parts and its schema key.
pub(crate) fn predicate_drop_prefixes(attr: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    let mut prefixes = with_twins(vec![predicate_prefix(attr)?]);
    prefixes.push(schema_key(attr)?);
    Ok(prefixes)
}

/// The prefixes of a namespace's data, its parts included (ledger 7).
pub(crate) fn namespace_data_prefixes(ns: u64) -> Vec<Vec<u8>> {
    with_twins(vec![data_prefix(ns)])
}

/// The prefixes of everything a namespace holds: its data, parts, schema and types.
pub(crate) fn namespace_prefixes(ns: u64) -> Vec<Vec<u8>> {
    let mut prefixes = namespace_data_prefixes(ns);
    for byte in [BYTE_SCHEMA, BYTE_TYPE] {
        let mut p = vec![byte];
        p.extend_from_slice(&namespace_to_bytes(ns));
        prefixes.push(p);
    }
    prefixes
}
