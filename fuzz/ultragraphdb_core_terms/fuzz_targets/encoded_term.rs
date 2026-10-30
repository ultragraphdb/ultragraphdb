/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a term's encoding received from a peer (`PROTOCOL.md` section 10.1): the whole-encoding check never
//! panics, and an encoding it accepts is one the store's length rules take, of the same bytes, that a reader opens
//! without the dictionary and turns into a literal or refuses as a UID, never a panic. The store's own parse of a key
//! holding a term takes the encoding the bytes begin with, never more bytes than they hold.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_terms::{EncodedTerm, TermError};

fuzz_target!(|data: &[u8]| {
    if let Ok((term, rest)) = EncodedTerm::read(data) {
        assert_eq!(term.as_bytes().len() + rest.len(), data.len());
        assert_eq!(&data[..term.as_bytes().len()], term.as_bytes());
    }
    if EncodedTerm::check(data).is_err() {
        return;
    }
    let term = EncodedTerm::from_bytes(data).expect("a whole encoding holds the length rules");
    assert_eq!(term.as_bytes(), data, "an encoding is its bytes");
    let view = term
        .view()
        .expect("a reader opens every encoding the check accepts");
    let _ = view.to_literal(
        |uid| Err::<_, TermError>(TermError::NotALiteral(uid)),
        |_| Ok("the text the hash names".to_string()),
    );
});
