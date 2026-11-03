/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The `address_parse` fuzz target's seeds and committed regressions, replayed under Cargo and Bazel. A seed's name
//! states what it is, `<rule>__<what>.txt`: `Ok` for an address taken, `NotUtf8` for bytes that are no text, which the
//! target skips, else the `AddressRule` it is refused by.

use std::collections::BTreeSet;

use ultragraphdb_protocol_transport::{Address, AddressRule, TransportError, TransportErrorEnum};
use ultragraphdb_tool_bdd::{
    Context, TestError, TestResult, fuzz_regressions, fuzz_regressions_in, testdata_dir,
};

/// Every rule `Address::parse` refuses by; `UnixUnbound` is a listener's, never a text's.
const PARSE_RULES: [AddressRule; 22] = [
    AddressRule::TooLong,
    AddressRule::ControlCharacter,
    AddressRule::Whitespace,
    AddressRule::UnknownScheme,
    AddressRule::Userinfo,
    AddressRule::PercentEncoded,
    AddressRule::PathQueryOrFragment,
    AddressRule::EmptyHost,
    AddressRule::UnbracketedIpv6,
    AddressRule::MalformedIpv6,
    AddressRule::InvalidIpv4,
    AddressRule::DnsNameTooLong,
    AddressRule::EmptyLabel,
    AddressRule::LabelTooLong,
    AddressRule::InvalidHostCharacter,
    AddressRule::HyphenAtLabelEdge,
    AddressRule::MissingPort,
    AddressRule::PortNotNumber,
    AddressRule::PortOutOfRange,
    AddressRule::PortZero,
    AddressRule::UnixNotAbsolute,
    AddressRule::UnixNamesNoFile,
];

/// What `text` parses to: `Ok` for an address that prints as given and parses back equal, else the rule it is refused
/// by, its refusal naming it.
fn outcome(text: &str) -> TestResult<String> {
    match Address::parse(text) {
        Ok(address) => {
            assert_eq!(address.to_string(), text, "{text:?} prints as given");
            assert_eq!(Address::parse(&address.to_string())?, address);
            Ok("Ok".to_string())
        }
        Err(TransportError(TransportErrorEnum::Address { address, rule })) => {
            assert_eq!(address, text, "the refusal names the address");
            Ok(format!("{rule:?}"))
        }
        Err(other) => Err(TestError::new(format!("{text:?}: {other:?}"))),
    }
}

#[test]
fn test_each_seed_parses_as_its_name_says_and_the_seeds_cover_every_rule() -> TestResult {
    let dir = testdata_dir()?.join("fuzz_seeds").join("address_parse");
    let seeds = fuzz_regressions_in(&dir)?;
    assert!(seeds.len() >= PARSE_RULES.len(), "{} seeds", seeds.len());
    let mut seen = BTreeSet::new();
    for (name, bytes) in &seeds {
        let (expected, _) = name
            .split_once("__")
            .context("a seed named <rule>__<what>")?;
        let Ok(text) = std::str::from_utf8(bytes) else {
            assert_eq!(expected, "NotUtf8", "{name}: bytes that are no text");
            continue;
        };
        assert_eq!(outcome(text)?, expected, "{name}");
        let _new: bool = seen.insert(expected.to_string());
    }
    for rule in PARSE_RULES {
        assert!(
            seen.contains(&format!("{rule:?}")),
            "no seed is refused by {rule:?}"
        );
    }
    assert!(seen.contains("Ok"));
    Ok(())
}

#[test]
fn test_each_committed_regression_parses_or_is_refused_by_a_rule() -> TestResult {
    for (name, bytes) in fuzz_regressions("address_parse")? {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            let found = outcome(text)?;
            assert!(
                found == "Ok" || PARSE_RULES.iter().any(|rule| format!("{rule:?}") == found),
                "{name}: {found}"
            );
        }
    }
    Ok(())
}
