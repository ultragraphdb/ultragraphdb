/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The OTLP client's response decoder over arbitrary bytes, the bytes a collector, or anything answering on its
//! port, sends back. It returns a response or an error and never panics; a body it accepts is never longer than the
//! input, and a body framed by `Content-Length` alone is exactly as long as announced.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_common_telemetry::decode_response;

fuzz_target!(|data: &[u8]| {
    let Ok(response) = decode_response(data) else {
        return;
    };
    let body = response.body();
    assert!(body.len() <= data.len(), "the body is part of the input");
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_ascii_lowercase)
    };
    let chunked = header("transfer-encoding").is_some_and(|value| value.contains("chunked"));
    if !chunked && let Some(length) = header("content-length") {
        assert_eq!(
            Some(body.len()),
            length.trim().parse::<usize>().ok(),
            "a Content-Length body is as long as announced"
        );
    }
});
