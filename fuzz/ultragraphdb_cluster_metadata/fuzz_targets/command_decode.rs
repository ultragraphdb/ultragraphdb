/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata group's command decoder over arbitrary bytes, as a round's body read back from its log. Nothing
//! panics; bytes are decoded or refused as malformed or as a length or count past its limit. An accepted command
//! encodes back to exactly its bytes, the encoding having one form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_cluster_metadata::MetadataCommand;
use ultragraphdb_error_cluster::ClusterErrorEnum;

fn check(data: &[u8]) {
    let command = match MetadataCommand::decode(data) {
        Ok(command) => command,
        Err(error) => {
            let typed = match error.0 {
                ClusterErrorEnum::Malformed { offset, .. } => offset <= data.len(),
                ClusterErrorEnum::TooMany { .. } => true,
                _ => false,
            };
            assert!(typed, "{error:?}");
            return;
        }
    };
    let mut encoded = Vec::with_capacity(data.len());
    command.encode(&mut encoded);
    assert_eq!(encoded, data, "an accepted command re-encodes to its bytes");
}

fuzz_target!(|data: &[u8]| check(data));
