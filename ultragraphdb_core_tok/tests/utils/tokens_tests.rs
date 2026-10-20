/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{IDENT_TERM, IDENT_TRIGRAM, get_term_tokens, get_tokens};

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
    assert_eq!(
        get_tokens(IDENT_TERM, &["a", "b"]).unwrap_err().to_string(),
        "Function requires 1 arguments, but got 2"
    );
    assert_eq!(
        get_tokens(0x7f, &["a"]).unwrap_err().to_string(),
        "No tokenizer was found with id 127"
    );
}
