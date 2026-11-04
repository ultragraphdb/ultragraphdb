/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#![no_main]

use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use ultragraphdb_transport_http_server::{HttpLimits, HttpResponse, ParsedHead, parse_head};

// Whatever arrives on an HTTP listener is a request, a refusal or a wait. A refusal is a complete response, a whole
// request's body lies within the bytes and its bound, and neither changes with what follows.
fuzz_target!(|bytes: &[u8]| {
    let limits = HttpLimits::new(512, Duration::from_secs(1));
    let parsed = parse_head(bytes, &limits);
    match &parsed {
        ParsedHead::Incomplete => return,
        ParsedHead::Refused(status) => {
            let head = HttpResponse::new(*status).head();
            assert!(head.starts_with("HTTP/1.1 "));
            assert!(head.ends_with("\r\n\r\n"));
        }
        ParsedHead::Complete(head) => {
            assert!(head.body_len() <= limits.body());
            assert!(head.target().starts_with('/'));
        }
    }
    let mut extended = bytes.to_vec();
    extended.extend_from_slice(b"trailing");
    assert_eq!(parse_head(&extended, &limits), parsed);
});
