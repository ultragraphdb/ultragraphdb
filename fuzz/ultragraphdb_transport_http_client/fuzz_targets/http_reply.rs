/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_transport_http_client::{ParsedReply, parse_reply};

// Whatever a peer answers with is a reply, a refusal or a wait, a body never past its bound, and an outcome that does not
// change with what follows it.
fuzz_target!(|bytes: &[u8]| {
    let parsed = parse_reply(bytes, 512);
    if parsed == ParsedReply::Incomplete {
        return;
    }
    if let ParsedReply::Complete(reply) = &parsed {
        assert!(reply.body().len() <= 512);
        assert!((100..=999).contains(&reply.status()));
    }
    let mut extended = bytes.to_vec();
    extended.extend_from_slice(b"trailing");
    assert_eq!(parse_reply(&extended, 512), parsed);
});
