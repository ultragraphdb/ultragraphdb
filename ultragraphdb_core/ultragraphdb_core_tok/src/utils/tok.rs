/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok.go (the registry, BuildTokens, the token encodings) and tok/tokens.go
 * (GetTokenizerForLang). Changed for ultragraphdb: translated to Rust; the registry is the fixed set Dgraph
 * registers at init, custom tokenizer plugins left out.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_text::{Collator, Tag};
use ultragraphdb_core_types::Value;

use crate::errors::tok_error::TokError;
use crate::types::tokenizer::tokenizer_tokens::ngram_query_tokens;
use crate::types::tokenizer::{ExactLang, Kind, Tokenizer};
use crate::types::vector_index_factory::VectorIndexFactory;
use crate::utils::ident::{IDENT_GEO, IDENT_NGRAM, IDENT_TRIGRAM};
use crate::utils::langbase::lang_base;

/// The names of the tokenizers Dgraph registers, the vector index factory included.
pub const TOKENIZER_NAMES: [&str; 17] = [
    "bigfloat", "hnsw", "geo", "int", "float", "year", "hour", "month", "day", "exact", "bool",
    "trigram", "hash", "term", "fulltext", "ngram", "sha256",
];

/// Dgraph's `GetTokenizer`: the tokenizer registered under `name`.
///
/// ```
/// use ultragraphdb_core_tok::get_tokenizer;
///
/// assert_eq!(get_tokenizer("term").map(|t| t.identifier()), Some(0x01));
/// assert!(get_tokenizer("Term").is_none());
/// ```
pub fn get_tokenizer(name: &str) -> Option<Tokenizer> {
    let kind = match name {
        "bigfloat" => Kind::BigFloat,
        "hnsw" => Kind::Factory(VectorIndexFactory::hnsw()),
        "geo" => Kind::Geo,
        "int" => Kind::Int,
        "float" => Kind::Float,
        "year" => Kind::Year,
        "hour" => Kind::Hour,
        "month" => Kind::Month,
        "day" => Kind::Day,
        "exact" => Kind::Exact(None),
        "bool" => Kind::Bool,
        "trigram" => Kind::Trigram,
        "hash" => Kind::Hash,
        "term" => Kind::Term(String::new()),
        "fulltext" => Kind::FullText(String::new()),
        "ngram" => Kind::NGram(String::new()),
        "sha256" => Kind::Sha256,
        _ => return None,
    };
    Some(Tokenizer::new(kind))
}

/// Dgraph's `GetTokenizerByID`: the registered tokenizer with identifier `id`.
pub fn get_tokenizer_by_id(id: u8) -> Option<Tokenizer> {
    TOKENIZER_NAMES
        .iter()
        .filter_map(|n| get_tokenizer(n))
        .find(|t| t.identifier() == id)
}

/// Dgraph's `GetTokenizers`: the tokenizers named, or `Invalid tokenizer <name>` for the first unknown name.
pub fn get_tokenizers<S: AsRef<str>>(names: &[S]) -> Result<Vec<Tokenizer>, TokError> {
    names
        .iter()
        .map(|n| {
            get_tokenizer(n.as_ref()).ok_or_else(|| {
                TokError::unknown_tokenizer(format!("Invalid tokenizer {}", n.as_ref()))
            })
        })
        .collect()
}

/// Dgraph's `GetTokenizerForLang`: the term and fulltext tokenizers for `lang`, and the exact tokenizer with the
/// collator of `lang` (English when the tag does not parse). Every other tokenizer, and every tokenizer when
/// `lang` is empty, comes back unchanged; so does ngram, whose language only [`get_ngram_tokens`](crate::get_ngram_tokens)
/// and [`get_ngram_query_tokens`](crate::get_ngram_query_tokens) take.
pub fn get_tokenizer_for_lang(t: &Tokenizer, lang: &str) -> Tokenizer {
    if lang.is_empty() {
        return t.clone();
    }
    match t.kind() {
        Kind::FullText(_) => Tokenizer::new(Kind::FullText(lang.to_string())),
        Kind::Term(_) => Tokenizer::new(Kind::Term(lang.to_string())),
        Kind::Exact(_) => {
            let tag = Tag::parse(lang)
                .or_else(|_| Tag::parse("en"))
                .expect("the constant tag en parses");
            Tokenizer::new(Kind::Exact(Some(Box::new(ExactLang {
                lang_base: lang_base(lang),
                collator: Collator::new(&tag),
            }))))
        }
        _ => t.clone(),
    }
}

/// Dgraph's `BuildTokens`: the tokens of a value, each prefixed with the tokenizer's identifier, ready to become
/// index keys.
///
/// ```
/// use ultragraphdb_core_tok::{TokError, build_tokens, get_tokenizer};
/// use ultragraphdb_core_types::Value;
///
/// # fn main() -> Result<(), TokError> {
/// let term = get_tokenizer("term").ok_or(TokError::UnknownTokenizer("term".into()))?;
/// let tokens = build_tokens(&Value::str("Tokenizer works works!"), &term)?;
/// assert_eq!(tokens, vec![b"\x01tokenizer".to_vec(), b"\x01works".to_vec()]);
/// # Ok(())
/// # }
/// ```
pub fn build_tokens(val: &Value, t: &Tokenizer) -> Result<Vec<Vec<u8>>, TokError> {
    let id = t.identifier();
    Ok(t.tokens(val)?
        .into_iter()
        .map(|tok| encode_token(&tok, id))
        .collect())
}

/// Dgraph's `BuildNGramQueryTokens` for `NGramTokenizer{lang: lang}`: the ngram query tokens of a value, prefixed
/// with the ngram identifier, sorted.
pub fn build_ngram_query_tokens(val: &Value, lang: &str) -> Vec<Vec<u8>> {
    ngram_query_tokens(lang, val)
        .into_iter()
        .map(|tok| encode_token(&tok, IDENT_NGRAM))
        .collect()
}

/// Dgraph's `encodeToken`: the identifier byte, then the token.
pub(crate) fn encode_token(tok: &[u8], typ: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(tok.len() + 1);
    out.push(typ);
    out.extend_from_slice(tok);
    out
}

/// Dgraph's `EncodeGeoTokens`: each token prefixed with the geo identifier.
pub fn encode_geo_tokens(tokens: &mut [Vec<u8>]) {
    tokens.iter_mut().for_each(|t| t.insert(0, IDENT_GEO));
}

/// Dgraph's `EncodeRegexTokens`: each token prefixed with the trigram identifier.
pub fn encode_regex_tokens(tokens: &mut [Vec<u8>]) {
    tokens.iter_mut().for_each(|t| t.insert(0, IDENT_TRIGRAM));
}
