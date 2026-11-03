/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any text as a hostile address from configuration: an address taken prints as given, parses back equal, and its host
//! and port keep the syntax rules, checked here independently of the parser; a refusal is an `Address` error naming
//! the text and one of the rules a text can break, the first one by the parser's order where this target can tell.
//! Nothing panics.

#![no_main]

use std::net::{Ipv4Addr, Ipv6Addr};

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_transport::{
    ADDRESS_LEN_MAX, Address, AddressRule, Scheme, TransportErrorEnum,
};

/// Whether `name` is a DNS name of letters, digits and hyphens: at most 253 bytes, labels of 1 to 63 bytes with no
/// hyphen at an edge, the last not all digits.
fn is_dns_name(name: &str) -> bool {
    let labels: Vec<&str> = name.split('.').collect();
    name.len() <= 253
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
        && labels
            .last()
            .is_some_and(|last| !last.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `host_port`, a TCP address's target, holds a valid host and a port from 1 to 65,535.
fn is_host_port(host_port: &str) -> bool {
    let Some((host, port)) = host_port.rsplit_once(':') else {
        return false;
    };
    let port_ok = !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|p| p != 0);
    let host_ok = match host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        Some(inner) => inner.parse::<Ipv6Addr>().is_ok(),
        None => !host.contains(':') && (host.parse::<Ipv4Addr>().is_ok() || is_dns_name(host)),
    };
    port_ok && host_ok && host_port.is_ascii()
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    match Address::parse(text) {
        Ok(address) => {
            assert!(text.len() <= ADDRESS_LEN_MAX);
            assert!(!text.chars().any(|c| c.is_control() || c.is_whitespace()));
            assert_eq!(address.to_string(), text, "an address prints as given");
            assert_eq!(
                Address::parse(&address.to_string()).ok(),
                Some(address.clone()),
                "an address parses back equal"
            );
            match address.scheme() {
                Scheme::Tcp => {
                    let host_port = address
                        .host_port()
                        .expect("a TCP address has a host and a port");
                    assert!(
                        !host_port.contains(['@', '%', '/', '\\', '?', '#']),
                        "{text:?}"
                    );
                    assert!(
                        is_host_port(host_port),
                        "a host or port breaking the syntax taken: {text:?}"
                    );
                    assert!(address.path().is_none());
                }
                Scheme::Unix => {
                    assert!(address.host_port().is_none());
                    let path = address.path().expect("a Unix address has a path");
                    assert!(
                        path.is_absolute() && !address.target().ends_with('/'),
                        "{text:?}"
                    );
                }
            }
        }
        Err(error) => {
            let TransportErrorEnum::Address { address, rule } = error.0 else {
                panic!("an address is refused as an address: {error:?}");
            };
            assert_eq!(address, text, "the refusal names the text");
            assert_ne!(
                rule,
                AddressRule::UnixUnbound,
                "no text is an unbound socket"
            );
            // The parser's first checks, which this target can tell apart.
            let expected = if text.len() > ADDRESS_LEN_MAX {
                Some(AddressRule::TooLong)
            } else if text.chars().any(char::is_control) {
                Some(AddressRule::ControlCharacter)
            } else if text.chars().any(char::is_whitespace) {
                Some(AddressRule::Whitespace)
            } else if !text.starts_with("tcp://") && !text.starts_with("unix://") {
                Some(AddressRule::UnknownScheme)
            } else {
                None
            };
            if let Some(expected) = expected {
                assert_eq!(rule, expected, "{text:?}");
            }
            // A refused TCP address breaks a rule this target finds too.
            if let (None, Some(host_port)) = (expected, text.strip_prefix("tcp://")) {
                assert!(
                    !is_host_port(host_port) || host_port.contains(['@', '%', '/', '\\', '?', '#']),
                    "a valid TCP address refused by {rule:?}: {text:?}"
                );
            }
        }
    }
});
