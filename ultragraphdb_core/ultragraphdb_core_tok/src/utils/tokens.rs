/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tokens.go (GetTokens and the token helpers of the query functions). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::Value;

use crate::errors::tok_error::TokError;
use crate::types::tokenizer::tokenizer_tokens::ngram_tokens;
use crate::types::tokenizer::{Kind, Tokenizer};
use crate::utils::ident::{IDENT_NGRAM, IDENT_TERM};
use crate::utils::tok::{
    build_ngram_query_tokens, build_tokens, encode_token, get_tokenizer_by_id,
};

fn one_arg<S: AsRef<str>>(func_args: &[S]) -> Result<Value, TokError> {
    if func_args.len() != 1 {
        return Err(TokError::arguments(format!(
            "Function requires 1 arguments, but got {}",
            func_args.len()
        )));
    }
    Ok(Value::str(func_args[0].as_ref()))
}

/// Dgraph's `GetTokens`: the tokens of the one string argument by the tokenizer with identifier `id`.
pub fn get_tokens<S: AsRef<str>>(id: u8, func_args: &[S]) -> Result<Vec<Vec<u8>>, TokError> {
    let v = one_arg(func_args)?;
    let t = get_tokenizer_by_id(id).ok_or_else(|| {
        TokError::unknown_tokenizer(format!("No tokenizer was found with id {id}"))
    })?;
    build_tokens(&v, &t)
}

/// Dgraph's `GetTermTokens`.
pub fn get_term_tokens<S: AsRef<str>>(func_args: &[S]) -> Result<Vec<Vec<u8>>, TokError> {
    get_tokens(IDENT_TERM, func_args)
}

/// Dgraph's `GetFullTextTokens`: the fulltext tokens of the one argument for `lang`.
pub fn get_full_text_tokens<S: AsRef<str>>(
    func_args: &[S],
    lang: &str,
) -> Result<Vec<Vec<u8>>, TokError> {
    let v = one_arg(func_args)?;
    build_tokens(&v, &Tokenizer::new(Kind::FullText(lang.to_string())))
}

/// Dgraph's `GetNGramTokens`: the ngram tokens of a string for `lang`, sorted. Go reads the first function argument
/// without checking the count; this port takes the argument.
pub fn get_ngram_tokens(arg: impl AsRef<str>, lang: &str) -> Vec<Vec<u8>> {
    // build_tokens with the ngram tokenizer of lang, whose tokens take every value.
    ngram_tokens(lang, &Value::str(arg))
        .into_iter()
        .map(|tok| encode_token(&tok, IDENT_NGRAM))
        .collect()
}

/// Dgraph's `GetNGramQueryTokens`: the ngram query tokens of a string for `lang`, sorted.
pub fn get_ngram_query_tokens(arg: impl AsRef<str>, lang: &str) -> Vec<Vec<u8>> {
    build_ngram_query_tokens(&Value::str(arg), lang)
}
