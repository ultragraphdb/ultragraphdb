/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The packed ID list's decoder over arbitrary bytes. It returns a pack or a `CodecError`, and never panics; a pack it
//! accepts encodes back to the bytes it was decoded from, decodes to strictly increasing system IDs, as many as its
//! blocks count, and packs again from them to blocks of the same IDs.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_codec::{UidPack, decode, exact_len};

fuzz_target!(|data: &[u8]| {
    let Ok(pack) = UidPack::decode(data) else {
        return;
    };
    assert_eq!(
        pack.encode(),
        data,
        "an accepted pack encodes back to its bytes"
    );
    let ids = decode(&pack, 0);
    assert_eq!(
        ids.len(),
        exact_len(&pack),
        "every ID the blocks count decodes"
    );
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "the IDs strictly increase"
    );
    for id in ids.iter().step_by(17) {
        let from = decode(&pack, id.to_u128());
        assert_eq!(from.first(), Some(id), "a decode from an ID starts at it");
    }
});
