/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_http_client::{HttpClientError, HttpClientErrorEnum};

const URL: &str = "http://h:1";
const REASON: &str = "the reason";

/// Each failure at a URL names the URL, and the reason where it has one; only what waiting may mend is unreachable.
#[test]
fn test_each_failure_at_a_url_names_it_and_its_reason() {
    let url = URL.to_string();
    let reason = REASON.to_string();
    for (error, has_reason, unreachable) in [
        (
            HttpClientErrorEnum::Unreachable {
                url: url.clone(),
                reason: reason.clone(),
            },
            true,
            true,
        ),
        (
            HttpClientErrorEnum::TimedOut { url: url.clone() },
            false,
            true,
        ),
        (
            HttpClientErrorEnum::Tls {
                url: url.clone(),
                reason: reason.clone(),
            },
            true,
            false,
        ),
        (
            HttpClientErrorEnum::Malformed {
                url: url.clone(),
                reason: reason.clone(),
            },
            true,
            false,
        ),
    ] {
        let error = HttpClientError(error);
        let text = error.to_string();
        assert!(text.contains(URL), "{text}");
        assert_eq!(text.contains(REASON), has_reason, "{text}");
        assert_eq!(error.is_unreachable(), unreachable, "{text}");
    }
}
