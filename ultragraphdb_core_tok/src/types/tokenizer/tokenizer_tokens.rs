/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok.go (each tokenizer's Tokens and NGramTokenizer.QueryTokens) and
 * tok/index_factory.go (tokensForExpectedVFloat). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashSet;

use blake2::{Blake2b256, Digest};
use ultragraphdb_core_types::{DateTime, Value, index_geo_tokens};

use crate::errors::tok_error::TokError;
use crate::types::analysis_token::AnalysisToken;
use crate::types::tokenizer::{Kind, Tokenizer};
use crate::utils::bleve::{analyze, remove_duplicates, unique_terms};
use crate::utils::ident::IDENT_DELIMITER;
use crate::utils::langbase::lang_base;
use crate::utils::stemmers::filter_stemmers;
use crate::utils::stopwords::filter_stopwords;

/// Shingles of this many bytes or more are replaced by their BLAKE2b-256 digest.
const MAX_SHINGLE_LEN: usize = 30;

impl Tokenizer {
    /// Go's `Tokens`: the tokens of a value, without the identifier byte ([`build_tokens`](crate::build_tokens)
    /// adds it).
    ///
    /// The ngram tokenizer's tokens are in no particular order in Dgraph, which collects them in a map; this port
    /// returns them sorted. Geo tokens come in cell order.
    pub fn tokens(&self, v: &Value) -> Result<Vec<Vec<u8>>, TokError> {
        match self.kind() {
            Kind::BigFloat => match v {
                Value::BigFloat(b) => Ok(vec![encode_int(b.to_i64())]),
                _ => Err(wrong(v, "big.Float")),
            },
            Kind::Geo => match v {
                Value::Geo(g) => Ok(index_geo_tokens(g)?
                    .into_iter()
                    .map(String::into_bytes)
                    .collect()),
                _ => Err(wrong(v, "geom.T")),
            },
            Kind::Int => match v {
                Value::Int(i) => Ok(vec![encode_int(*i)]),
                _ => Err(wrong(v, "int64")),
            },
            // Go's int64(float64) on arm64 and Rust's `as` both saturate, and take NaN to 0.
            Kind::Float => match v {
                Value::Float(f) => Ok(vec![encode_int(*f as i64)]),
                _ => Err(wrong(v, "float64")),
            },
            Kind::Year => date_token(v, 1),
            Kind::Month => date_token(v, 2),
            Kind::Day => date_token(v, 3),
            Kind::Hour => date_token(v, 4),
            Kind::Term(lang) => Ok(term_tokens(lang, v)),
            Kind::Exact(lang) => {
                let Value::Str(s) = v else {
                    return Err(TokError::wrong_value(
                        "Exact indices only supported for string types",
                    ));
                };
                let Some(l) = lang else {
                    return Ok(vec![s.clone()]);
                };
                // Go keys the string's bytes, which need not be valid UTF-8.
                let key = l.collator.key_bytes(s);
                let mut term = Vec::with_capacity(l.lang_base.len() + 2 + key.len());
                term.extend_from_slice(l.lang_base.as_bytes());
                term.push(IDENT_DELIMITER);
                term.extend_from_slice(&key);
                Ok(vec![term])
            }
            Kind::Bool => match v {
                Value::Bool(b) => Ok(vec![encode_int(i64::from(*b))]),
                _ => Err(wrong(v, "bool")),
            },
            Kind::Trigram => {
                let Value::Str(s) = v else {
                    return Err(TokError::wrong_value(
                        "Trigram indices only supported for string types",
                    ));
                };
                if s.len() < 3 {
                    return Ok(Vec::new());
                }
                Ok(remove_duplicates(
                    s.windows(3).map(<[u8]>::to_vec).collect(),
                ))
            }
            Kind::Hash => {
                let Value::Str(s) = v else {
                    return Err(TokError::wrong_value(
                        "Hash tokenizer only supported for string types",
                    ));
                };
                Ok(vec![Blake2b256::digest(s).to_vec()])
            }
            Kind::FullText(lang) => {
                let Some(tokens) = analyzed(lang, v) else {
                    return Ok(Vec::new());
                };
                Ok(unique_terms(tokens))
            }
            Kind::NGram(lang) => Ok(ngram_tokens(lang, v)),
            Kind::Sha256 => match v {
                // The first 64 characters hold a query's SHA-256 in hex.
                Value::Str(s) if s.len() >= 64 => Ok(vec![s[..64].to_vec()]),
                _ => Ok(Vec::new()),
            },
            // A vector predicate takes one mutation at a time; its one token says so.
            Kind::Factory(_) => Ok(vec![b"float".to_vec()]),
        }
    }
}

/// The text of a string value, `None` for the empty string and any other type.
fn text(v: &Value) -> Option<&[u8]> {
    match v {
        Value::Str(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Go's panic text when a tokenizer's type assertion fails, as an error.
fn wrong(v: &Value, want: &str) -> TokError {
    TokError::wrong_value(format!(
        "interface conversion: interface {{}} is {}, not {want}",
        v.go_type()
    ))
}

/// Dgraph's `encodeInt`: a sign byte (0 for negative, 1 otherwise) and the value as u64 big-endian, so that the
/// byte order of the tokens is the order of the values.
pub(crate) fn encode_int(val: i64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(9);
    buf.push(u8::from(val >= 0));
    buf.extend_from_slice(&(val as u64).to_be_bytes());
    buf
}

/// The year, month, day and hour tokens: the first `fields` of the UTC year, month, day and hour, each a big-endian
/// u16 (the year truncated to 16 bits, as Go's `uint16` does).
fn date_token(v: &Value, fields: usize) -> Result<Vec<Vec<u8>>, TokError> {
    let Value::DateTime(t) = v else {
        return Err(wrong(v, "time.Time"));
    };
    let (year, month, day, hour) = utc_fields(t);
    let all = [year as u16, month as u16, day as u16, hour as u16];
    Ok(vec![
        all[..fields].iter().flat_map(|f| f.to_be_bytes()).collect(),
    ])
}

/// Go's `UTC().Year()`, `Month()`, `Day()` and `Hour()`, from the proleptic Gregorian calendar.
fn utc_fields(t: &DateTime) -> (i64, i64, i64, i64) {
    let unix = i128::from(t.unix());
    let days = unix.div_euclid(86_400);
    let hour = (unix.rem_euclid(86_400) / 3600) as i64;
    // Days since 1970-01-01 to a civil date (H. Hinnant's days_to_civil).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i128::from(month <= 2);
    (year as i64, month as i64, day as i64, hour)
}

/// The term tokenizer: the value split on spaces for the languages written without them, its analyzed terms for
/// every other, sorted and deduplicated. A value that is empty or not a string gives one empty token, as Dgraph's
/// does.
fn term_tokens(lang: &str, v: &Value) -> Vec<Vec<u8>> {
    let Some(s) = text(v) else {
        return vec![Vec::new()];
    };
    match lang_base(lang).as_str() {
        // Chinese, Japanese, Thai, Lao, Burmese, Tibetan and Khmer (km, kxm) do not use spaces as delimiters.
        "zh" | "ja" | "th" | "lo" | "my" | "bo" | "km" | "kxm" => {
            remove_duplicates(s.split(|&b| b == b' ').map(<[u8]>::to_vec).collect())
        }
        _ => unique_terms(analyze(s)),
    }
}

/// The fulltext and ngram analysis: analyzed, without the language's stop words, stemmed. `None` for a value that
/// is empty or not a string.
fn analyzed(lang: &str, v: &Value) -> Option<Vec<AnalysisToken>> {
    let s = text(v)?;
    let lang = lang_base(lang);
    Some(filter_stemmers(&lang, filter_stopwords(&lang, analyze(s))))
}

/// Dgraph's `generateNGrams`: the shingles of `terms` at the `(first word, words)` spans, each of 30 bytes or more
/// as its BLAKE2b-256 digest, deduplicated and, in this port, sorted.
fn shingles(terms: &[&[u8]], spans: impl Iterator<Item = (usize, usize)>) -> Vec<Vec<u8>> {
    let mut set = HashSet::new();
    for (i, n) in spans {
        let shingle = terms[i..i + n].join(&b' ');
        if shingle.len() < MAX_SHINGLE_LEN {
            set.insert(shingle);
        } else {
            set.insert(Blake2b256::digest(&shingle).to_vec());
        }
    }
    let mut out: Vec<Vec<u8>> = set.into_iter().collect();
    out.sort();
    out
}

/// The ngram tokenizer's `Tokens`: shingles of one to four words.
pub(crate) fn ngram_tokens(lang: &str, v: &Value) -> Vec<Vec<u8>> {
    let Some(tokens) = analyzed(lang, v) else {
        return Vec::new();
    };
    let terms: Vec<&[u8]> = tokens.iter().map(AnalysisToken::term).collect();
    let n = terms.len();
    shingles(
        &terms,
        (0..n).flat_map(|i| (1..=4).filter(move |k| i + k <= n).map(move |k| (i, k))),
    )
}

/// The ngram tokenizer's `QueryTokens`: shingles of three words, or of all words when there are fewer.
pub(crate) fn ngram_query_tokens(lang: &str, v: &Value) -> Vec<Vec<u8>> {
    let Some(tokens) = analyzed(lang, v) else {
        return Vec::new();
    };
    let terms: Vec<&[u8]> = tokens.iter().map(AnalysisToken::term).collect();
    let n = terms.len();
    let gram = n.min(3);
    shingles(&terms, (0..n).filter(|i| i + gram <= n).map(|i| (i, gram)))
}
