/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as an address from configuration: refused, or taken as a TCP or a Unix address whose text parses back
//! to it, within `ADDRESS_LEN_MAX`, naming its target by its scheme alone; nothing panics.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_transport::{ADDRESS_LEN_MAX, Address, Scheme, TransportErrorEnum};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    match Address::parse(text) {
        Ok(address) => {
            assert!(text.len() <= ADDRESS_LEN_MAX);
            assert_eq!(
                address.to_string(),
                text,
                "an address prints as it was given"
            );
            assert_eq!(Address::parse(text).ok(), Some(address.clone()));
            assert!(!address.target().is_empty());
            match address.scheme() {
                Scheme::Tcp => {
                    assert_eq!(address.host_port(), Some(address.target()));
                    assert!(address.path().is_none());
                }
                Scheme::Unix => {
                    assert!(address.host_port().is_none());
                    assert!(address.path().is_some_and(|path| path.is_absolute()));
                }
            }
        }
        Err(error) => {
            let TransportErrorEnum::Address { address, .. } = error.0 else {
                panic!("an address is refused as an address: {error:?}");
            };
            assert_eq!(address, text);
        }
    }
});
