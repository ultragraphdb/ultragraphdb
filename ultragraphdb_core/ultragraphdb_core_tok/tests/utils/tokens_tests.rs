/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{
    IDENT_EXACT, IDENT_EXACT_LANG, IDENT_INT, IDENT_TERM, IDENT_TRIGRAM, TokErrorEnum,
    get_term_tokens, get_tokens,
};

#[test]
fn test_get_tokens() {
    assert_eq!(
        get_term_tokens(&["Hello World"]).unwrap(),
        vec![b"\x01hello".to_vec(), b"\x01world".to_vec()]
    );
    assert_eq!(
        get_tokens(IDENT_TRIGRAM, &["abcd"]).unwrap(),
        vec![b"\x0aabc".to_vec(), b"\x0abcd".to_vec()]
    );
    // The registered exact tokenizer has no language, so its value is kept as it is.
    assert_eq!(
        get_tokens(IDENT_EXACT, &["Ab C"]).unwrap(),
        vec![b"\x02Ab C".to_vec()]
    );
    // An empty string is one empty term, as Dgraph's term tokenizer gives it.
    assert_eq!(get_term_tokens(&[""]).unwrap(), vec![vec![IDENT_TERM]]);
}

/// The errors of Dgraph's `GetTokens` and `GetTermTokens`, as Dgraph v25.5.0 words them; the argument count is checked
/// before the identifier.
#[test]
fn test_get_tokens_errors() {
    let cases: [(u8, &[&str], TokErrorEnum); 6] = [
        (
            IDENT_TERM,
            &["a", "b"],
            TokErrorEnum::Arguments("Function requires 1 arguments, but got 2".into()),
        ),
        (
            IDENT_TERM,
            &[],
            TokErrorEnum::Arguments("Function requires 1 arguments, but got 0".into()),
        ),
        (
            0x7f,
            &[],
            TokErrorEnum::Arguments("Function requires 1 arguments, but got 0".into()),
        ),
        (
            0x7f,
            &["a"],
            TokErrorEnum::UnknownTokenizer("No tokenizer was found with id 127".into()),
        ),
        (
            IDENT_EXACT_LANG,
            &["a"],
            TokErrorEnum::UnknownTokenizer("No tokenizer was found with id 3".into()),
        ),
        // Go panics on the int tokenizer's type assertion; the port returns its text.
        (
            IDENT_INT,
            &["a"],
            TokErrorEnum::WrongValue(
                "interface conversion: interface {} is string, not int64".into(),
            ),
        ),
    ];
    for (id, args, want) in cases {
        let e = get_tokens(id, args).unwrap_err();
        assert_eq!(e.0, want, "{id:#04x} {args:?}");
    }
    let e = get_term_tokens::<&str>(&[]).unwrap_err();
    assert_eq!(e.to_string(), "Function requires 1 arguments, but got 0");
}
