/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_protocol_scram::{ScramError, ScramErrorEnum};

#[test]
fn test_each_constructor_builds_its_variant_and_names_it() {
    let cases = [
        (
            ScramError::Malformed("nonce"),
            ScramErrorEnum::Malformed { part: "nonce" },
            "a SCRAM message's nonce does not parse",
        ),
        (
            ScramError::ChannelBinding(),
            ScramErrorEnum::ChannelBinding,
            "channel binding is not offered without TLS",
        ),
        (
            ScramError::Prepare(),
            ScramErrorEnum::Prepare,
            "SASLprep refuses the text",
        ),
        (
            ScramError::Iterations(9),
            ScramErrorEnum::Iterations { iterations: 9 },
            "the iteration count 9 is outside 4,096 to 10,000,000",
        ),
        (
            ScramError::Nonce(),
            ScramErrorEnum::Nonce,
            "the server's nonce does not extend the client's",
        ),
        (
            ScramError::Proof(),
            ScramErrorEnum::Proof,
            "the client's proof does not verify",
        ),
        (
            ScramError::ServerSignature(),
            ScramErrorEnum::ServerSignature,
            "the server's signature does not verify",
        ),
        (
            ScramError::Server("invalid-proof"),
            ScramErrorEnum::Server {
                reason: "invalid-proof".to_string(),
            },
            "the server refused: invalid-proof",
        ),
        (
            ScramError::OutOfStep(),
            ScramErrorEnum::OutOfStep,
            "a SCRAM message out of step",
        ),
        (
            ScramError::Random("no entropy"),
            ScramErrorEnum::Random {
                reason: "no entropy".to_string(),
            },
            "the random source failed: no entropy",
        ),
    ];
    for (error, variant, text) in cases {
        assert_eq!(error.0, variant);
        assert_eq!(error.to_string(), text);
    }
    let boxed: Box<dyn Error> = Box::new(ScramError::Proof());
    assert!(boxed.source().is_none());
}
