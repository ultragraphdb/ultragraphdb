/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_common_service::{HealthRequest, parse_request, response};

// Whatever arrives on the observability port gets an answer that is a complete HTTP response, and a request's outcome does
// not change with what follows its headers.
fuzz_target!(|bytes: &[u8]| {
    let request = parse_request(bytes);
    let text = response(request, false);
    assert!(text.starts_with("HTTP/1.1 "));
    assert!(text.contains("\r\n\r\n"));
    if request != HealthRequest::Incomplete && request != HealthRequest::BadRequest {
        let mut extended = bytes.to_vec();
        extended.extend_from_slice(b"trailing");
        assert_eq!(parse_request(&extended), request);
    }
});
