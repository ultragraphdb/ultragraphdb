/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::Path;

use ultragraphdb_protocol_transport::{
    ADDRESS_LEN_MAX, Address, Scheme, TransportError, TransportErrorEnum,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// The rule `text` is refused by; a [`TestError`] when it parses.
fn refusal(text: &str) -> TestResult<&'static str> {
    match Address::parse(text) {
        Ok(address) => Err(TestError::new(format!("{text:?} parsed as {address:?}"))),
        Err(TransportError(TransportErrorEnum::Address { address, reason })) => {
            assert_eq!(address, text, "the refusal names the address as given");
            Ok(reason)
        }
        Err(other) => Err(TestError::new(format!("{text:?}: {other:?}"))),
    }
}

#[test]
fn test_a_tcp_address_names_its_host_and_port() -> TestResult {
    for (text, host_port) in [
        ("tcp://localhost:9080", "localhost:9080"),
        ("tcp://127.0.0.1:0", "127.0.0.1:0"),
        ("tcp://[::1]:65535", "[::1]:65535"),
        (
            "tcp://frontend.ultragraph.svc:1",
            "frontend.ultragraph.svc:1",
        ),
    ] {
        let address = Address::parse(text)?;
        assert_eq!(address.scheme(), Scheme::Tcp, "{text}");
        assert_eq!(address.host_port(), Some(host_port), "{text}");
        assert_eq!(address.path(), None, "{text}");
        assert_eq!(address.to_string(), text, "{text}");
    }
    Ok(())
}

#[test]
fn test_a_unix_address_names_its_socket_path() -> TestResult {
    for (text, path) in [
        (
            "unix:///run/ultragraph/frontend.sock",
            "/run/ultragraph/frontend.sock",
        ),
        ("unix:///s", "/s"),
    ] {
        let address = Address::parse(text)?;
        assert_eq!(address.scheme(), Scheme::Unix, "{text}");
        assert_eq!(address.path(), Some(Path::new(path)), "{text}");
        assert_eq!(address.host_port(), None, "{text}");
        assert_eq!(address.to_string(), text, "{text}");
    }
    Ok(())
}

#[test]
fn test_an_address_parses_back_from_its_text() -> TestResult {
    for text in ["tcp://h:1", "unix:///run/x.sock"] {
        let address = Address::parse(text)?;
        assert_eq!(Address::parse(&address.to_string())?, address);
    }
    assert_ne!(Address::parse("tcp://h:1")?, Address::parse("tcp://h:2")?);
    Ok(())
}

#[test]
fn test_a_scheme_other_than_tcp_and_unix_is_refused() -> TestResult {
    for text in [
        "",
        "h:1",
        "http://h:1",
        "TCP://h:1",
        "Unix:///s",
        "tcp:/h:1",
        "unix:/s",
        "udp://h:1",
    ] {
        assert_eq!(
            refusal(text)?,
            "a scheme other than tcp:// and unix://",
            "{text:?}"
        );
    }
    Ok(())
}

#[test]
fn test_a_tcp_address_without_a_host_or_a_port_is_refused() -> TestResult {
    for text in ["tcp://", "tcp://h", "tcp://:80", "tcp://h:", "tcp://[::1]"] {
        let reason = refusal(text)?;
        assert!(
            reason == "a TCP address without a host" || reason == "a TCP address without a port",
            "{text:?}: {reason}"
        );
    }
    assert_eq!(refusal("tcp://:80")?, "a TCP address without a host");
    assert_eq!(refusal("tcp://h:")?, "a TCP address without a port");
    Ok(())
}

#[test]
fn test_a_port_that_is_no_number_below_65536_is_refused() -> TestResult {
    for text in [
        "tcp://h:65536",
        "tcp://h:99999999999999999999",
        "tcp://h:x",
        "tcp://h:+80",
        "tcp://h:-1",
        "tcp://h:8 0",
        "tcp://h:80/path",
    ] {
        let reason = refusal(text)?;
        assert!(
            reason == "a port that is no number below 65,536"
                || reason == "a control character or whitespace",
            "{text:?}: {reason}"
        );
    }
    assert_eq!(
        refusal("tcp://h:65536")?,
        "a port that is no number below 65,536"
    );
    assert_eq!(
        refusal("tcp://h:+80")?,
        "a port that is no number below 65,536"
    );
    Ok(())
}

#[test]
fn test_a_tcp_host_holding_a_path_or_a_user_is_refused() -> TestResult {
    for text in ["tcp://a/b:80", "tcp://user@h:80"] {
        assert_eq!(refusal(text)?, "a TCP host holding '/' or '@'", "{text:?}");
    }
    Ok(())
}

#[test]
fn test_a_unix_path_that_is_not_absolute_or_names_no_file_is_refused() -> TestResult {
    for text in ["unix://", "unix://run/x.sock", "unix:///", "unix:///run/"] {
        assert_eq!(
            refusal(text)?,
            "a Unix path that is not absolute or names no file",
            "{text:?}"
        );
    }
    Ok(())
}

#[test]
fn test_control_characters_and_whitespace_are_refused() -> TestResult {
    for text in [
        "unix:///run/a b.sock",
        "unix:///run/a\0.sock",
        "unix:///run/a\n",
        "tcp://h :80",
        "tcp://h:80\t",
        " tcp://h:80",
    ] {
        let reason = refusal(text)?;
        assert!(
            reason == "a control character or whitespace"
                || reason == "a scheme other than tcp:// and unix://",
            "{text:?}: {reason}"
        );
    }
    assert_eq!(
        refusal("unix:///run/a b.sock")?,
        "a control character or whitespace"
    );
    assert_eq!(
        refusal("tcp://h:80\t")?,
        "a control character or whitespace"
    );
    Ok(())
}

#[test]
fn test_an_address_is_taken_up_to_its_longest_and_refused_past_it() -> TestResult {
    let prefix = "unix:///";
    let longest = format!("{prefix}{}", "a".repeat(ADDRESS_LEN_MAX - prefix.len()));
    assert_eq!(longest.len(), ADDRESS_LEN_MAX);
    assert_eq!(Address::parse(&longest)?.scheme(), Scheme::Unix);
    let past = format!("{longest}a");
    assert_eq!(refusal(&past)?, "longer than ADDRESS_LEN_MAX");
    Ok(())
}
