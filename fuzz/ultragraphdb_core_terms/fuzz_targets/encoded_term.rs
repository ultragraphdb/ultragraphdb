/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a term's encoding received from a peer (`PROTOCOL.md` section 10.1): the whole-encoding check never
//! panics, and an encoding it accepts is one the store's length rules take, of the same bytes, that a reader opens
//! without the dictionary, an ID that is no valid system ID aside, and turns into a literal or refuses as a UID, never a
//! panic; a Result's reader opens every literal encoding the check accepts. The store's own parse of a key holding a
//! term takes the encoding the bytes begin with, never more bytes than they hold.

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
    // A Result's reader takes a datatype's 16 bytes as the hash they are there, so it opens every literal encoding the
    // check accepts; an IRI's or a blank node's, which a Result names by its hash, it refuses.
    match term.result_view() {
        Ok(view) => {
            assert_ne!(data[0], 0x01, "a Result's reader refuses an IRI's tag");
            let _ = view.to_literal(
                |uid| Err::<_, TermError>(TermError::not_a_literal(uid)),
                |_| Ok("http://ex.org/the-text-the-hash-names".to_string()),
            );
        }
        Err(refusal) => assert_eq!(
            refusal,
            TermError::malformed(
                data.to_vec(),
                "an IRI or a blank node, which a Result names by its hash"
            ),
            "a Result's reader opens every literal encoding the check accepts"
        ),
    }
    // The check is §10.1's whole encoding, where a Result names an IRI or a datatype by a hash; the store's reader
    // takes those 16 bytes as a system ID and refuses one that is no valid ID, and opens every other encoding.
    let view = match term.view() {
        Ok(view) => view,
        Err(refusal) => {
            assert_eq!(
                refusal,
                TermError::malformed(data.to_vec(), "an ID that is no valid system ID"),
                "a reader opens every encoding the check accepts but an invalid system ID"
            );
            return;
        }
    };
    let _ = view.to_literal(
        |uid| Err::<_, TermError>(TermError::not_a_literal(uid)),
        |_| Ok("the text the hash names".to_string()),
    );
});
