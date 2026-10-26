/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::LiteralRef;
use ultragraphdb_core_tok::{TokError, get_tokenizer, get_tokenizer_for_lang};

use crate::utils::token_values::token_value;

/// The value-index tokens of `literal` under each of `tokenizers`, each with its tokenizer's identifier: computed from
/// the literal's value, by its language's tokenizer when it is tagged. A literal of a datatype a tokenizer does not
/// take gives that tokenizer no token.
pub(crate) fn literal_tokens(
    tokenizers: &[String],
    literal: LiteralRef<'_>,
) -> Result<BTreeSet<(u8, Vec<u8>)>, TokError> {
    let lang = literal.language().unwrap_or_default();
    let mut tokens = BTreeSet::new();
    for name in tokenizers {
        let (Some(tokenizer), Some(value)) = (get_tokenizer(name), token_value(literal, name))
        else {
            continue;
        };
        let tokenizer = get_tokenizer_for_lang(&tokenizer, lang);
        for token in tokenizer.tokens(&value)? {
            tokens.insert((tokenizer.identifier(), token));
        }
    }
    Ok(tokens)
}
