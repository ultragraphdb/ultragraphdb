/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::Path;

use ultragraphdb_protocol_transport::{
    ADDRESS_LEN_MAX, Address, AddressRule, Scheme, TransportError, TransportErrorEnum,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// The rule `text` is refused by; a [`TestError`] when it parses.
fn refusal(text: &str) -> TestResult<AddressRule> {
    match Address::parse(text) {
        Ok(address) => Err(TestError::new(format!("{text:?} parsed as {address:?}"))),
        Err(TransportError(TransportErrorEnum::Address { address, rule })) => {
            assert_eq!(address, text, "the refusal names the address as given");
            Ok(rule)
        }
        Err(other) => Err(TestError::new(format!("{text:?}: {other:?}"))),
    }
}

/// Each of `cases` refused by its rule.
fn refused_by(cases: &[(&str, AddressRule)]) -> TestResult {
    for (text, rule) in cases {
        assert_eq!(refusal(text)?, *rule, "{text:?}");
    }
    Ok(())
}

/// Each of `texts` parses, as a TCP address naming its text after the scheme.
fn tcp_parses(texts: &[&str]) -> TestResult {
    for text in texts {
        let address = Address::parse(text)?;
        assert_eq!(address.scheme(), Scheme::Tcp, "{text:?}");
        assert_eq!(address.host_port(), text.strip_prefix("tcp://"), "{text:?}");
        assert_eq!(address.to_string(), *text, "{text:?}");
    }
    Ok(())
}

/// `tcp://` and a host of `len` bytes, labels of 63 bytes and a shorter last one, then `:80`.
fn name_of(len: usize) -> String {
    let mut name = String::new();
    while name.len() < len {
        if !name.is_empty() {
            name.push('.');
        }
        let room = (len - name.len()).min(63);
        name.push_str(&"a".repeat(room));
    }
    format!("tcp://{name}:80")
}

#[test]
fn test_a_tcp_address_names_its_host_and_port() -> TestResult {
    for (text, host_port) in [
        ("tcp://localhost:9080", "localhost:9080"),
        ("tcp://127.0.0.1:1", "127.0.0.1:1"),
        ("tcp://[::1]:65535", "[::1]:65535"),
        ("tcp://[::ffff:127.0.0.1]:80", "[::ffff:127.0.0.1]:80"),
        (
            "tcp://frontend.ultragraph.svc:1",
            "frontend.ultragraph.svc:1",
        ),
        ("tcp://Host-1.Example.COM:80", "Host-1.Example.COM:80"),
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
        // A path is the file system's: `%`, `@`, `?` and `#` are characters of a file's name.
        ("unix:///run/a%20b@c?d#e", "/run/a%20b@c?d#e"),
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
    for text in ["tcp://h:1", "tcp://[::1]:2", "unix:///run/x.sock"] {
        let address = Address::parse(text)?;
        assert_eq!(Address::parse(&address.to_string())?, address);
    }
    assert_ne!(Address::parse("tcp://h:1")?, Address::parse("tcp://h:2")?);
    Ok(())
}

#[test]
fn test_a_scheme_other_than_tcp_and_unix_is_refused() -> TestResult {
    refused_by(&[
        ("", AddressRule::UnknownScheme),
        ("h:1", AddressRule::UnknownScheme),
        ("http://h:1", AddressRule::UnknownScheme),
        ("TCP://h:1", AddressRule::UnknownScheme),
        ("Unix:///s", AddressRule::UnknownScheme),
        ("tcp:/h:1", AddressRule::UnknownScheme),
        ("unix:/s", AddressRule::UnknownScheme),
        ("udp://h:1", AddressRule::UnknownScheme),
    ])
}

#[test]
fn test_control_characters_and_whitespace_are_refused_before_the_scheme() -> TestResult {
    refused_by(&[
        ("unix:///run/a\0.sock", AddressRule::ControlCharacter),
        ("unix:///run/a\n", AddressRule::ControlCharacter),
        ("tcp://h:80\r\n", AddressRule::ControlCharacter),
        ("tcp://h:80\u{7f}", AddressRule::ControlCharacter),
        ("tcp://h:80\t", AddressRule::ControlCharacter),
        ("unix:///run/a b.sock", AddressRule::Whitespace),
        ("tcp://h :80", AddressRule::Whitespace),
        (" tcp://h:80", AddressRule::Whitespace),
        ("tcp://h:80\u{a0}", AddressRule::Whitespace),
        ("tcp://h:8 0", AddressRule::Whitespace),
    ])
}

#[test]
fn test_userinfo_percent_encoding_and_paths_are_refused_in_a_tcp_address() -> TestResult {
    refused_by(&[
        ("tcp://user@h:80", AddressRule::Userinfo),
        ("tcp://user:pw@h:80", AddressRule::Userinfo),
        ("tcp://h:80@evil.example:80", AddressRule::Userinfo),
        ("tcp://h%2e:80", AddressRule::PercentEncoded),
        ("tcp://[fe80::1%25eth0]:80", AddressRule::PercentEncoded),
        ("tcp://[fe80::1%eth0]:80", AddressRule::PercentEncoded),
        ("tcp://a/b:80", AddressRule::PathQueryOrFragment),
        ("tcp://h:80/path", AddressRule::PathQueryOrFragment),
        ("tcp://h:80?q", AddressRule::PathQueryOrFragment),
        ("tcp://h:80#f", AddressRule::PathQueryOrFragment),
        ("tcp://h\\evil:80", AddressRule::PathQueryOrFragment),
    ])
}

#[test]
fn test_a_tcp_address_without_a_host_is_refused() -> TestResult {
    refused_by(&[
        ("tcp://", AddressRule::EmptyHost),
        ("tcp://:80", AddressRule::EmptyHost),
        ("tcp://[]:80", AddressRule::EmptyHost),
        ("tcp://[]", AddressRule::EmptyHost),
    ])
}

#[test]
fn test_an_ipv6_host_is_bracketed_and_well_formed() -> TestResult {
    refused_by(&[
        ("tcp://::1:80", AddressRule::UnbracketedIpv6),
        ("tcp://fe80::1:80", AddressRule::UnbracketedIpv6),
        ("tcp://[::1", AddressRule::MalformedIpv6),
        ("tcp://[::1:80", AddressRule::MalformedIpv6),
        ("tcp://[::1]80", AddressRule::MalformedIpv6),
        ("tcp://[::1]x:80", AddressRule::MalformedIpv6),
        ("tcp://[::g]:80", AddressRule::MalformedIpv6),
        ("tcp://[1:2:3:4:5:6:7:8:9]:80", AddressRule::MalformedIpv6),
        ("tcp://[127.0.0.1]:80", AddressRule::MalformedIpv6),
        ("tcp://[localhost]:80", AddressRule::MalformedIpv6),
        ("tcp://[[::1]]:80", AddressRule::MalformedIpv6),
    ])?;
    tcp_parses(&["tcp://[1:2:3:4:5:6:7:8]:80", "tcp://[::]:80"])
}

#[test]
fn test_a_numeric_host_is_a_dotted_quad_or_refused() -> TestResult {
    refused_by(&[
        // Leading zeros: octal to `inet_aton`, decimal elsewhere.
        ("tcp://010.0.0.1:80", AddressRule::InvalidIpv4),
        ("tcp://127.0.0.01:80", AddressRule::InvalidIpv4),
        // Out of range octets.
        ("tcp://256.0.0.1:80", AddressRule::InvalidIpv4),
        ("tcp://1.2.3.999:80", AddressRule::InvalidIpv4),
        // Short forms, and one number for the whole address, which `inet_aton` reads as 127.0.0.1.
        ("tcp://127.1:80", AddressRule::InvalidIpv4),
        ("tcp://127.0.1:80", AddressRule::InvalidIpv4),
        ("tcp://2130706433:80", AddressRule::InvalidIpv4),
        ("tcp://1.2.3.4.5:80", AddressRule::InvalidIpv4),
        // Hexadecimal octets, and a numeric last label after a name.
        ("tcp://0x7f.0.0.1:80", AddressRule::InvalidIpv4),
        ("tcp://host.1:80", AddressRule::InvalidIpv4),
        // Dots alone, and a trailing dot.
        ("tcp://...:80", AddressRule::InvalidIpv4),
        ("tcp://1.2.3.4.:80", AddressRule::InvalidIpv4),
    ])?;
    tcp_parses(&[
        "tcp://0.0.0.0:80",
        "tcp://255.255.255.255:80",
        "tcp://10.0.0.1:80",
    ])
}

#[test]
fn test_a_dns_name_keeps_its_length_bounds() -> TestResult {
    // A label of 63 bytes is taken, of 64 refused; a name of 253 bytes is taken, of 254 refused.
    let label_63 = format!("tcp://{}.example:80", "a".repeat(63));
    let label_64 = format!("tcp://{}.example:80", "a".repeat(64));
    let (name_253, name_254) = (name_of(253), name_of(254));
    assert_eq!(name_253.len(), "tcp://".len() + 253 + ":80".len());
    tcp_parses(&[&label_63, &name_253, "tcp://a:80", "tcp://x-1.y:80"])?;
    refused_by(&[
        (&label_64, AddressRule::LabelTooLong),
        (&name_254, AddressRule::DnsNameTooLong),
    ])
}

#[test]
fn test_a_dns_name_is_letters_digits_and_hyphens_in_labels() -> TestResult {
    refused_by(&[
        ("tcp://.example:80", AddressRule::EmptyLabel),
        ("tcp://example.:80", AddressRule::EmptyLabel),
        ("tcp://a..b:80", AddressRule::EmptyLabel),
        ("tcp://a_b:80", AddressRule::InvalidHostCharacter),
        ("tcp://a*b:80", AddressRule::InvalidHostCharacter),
        // A Cyrillic 'а' for the Latin 'a': a homoglyph.
        (
            "tcp://ex\u{430}mple.com:80",
            AddressRule::InvalidHostCharacter,
        ),
        ("tcp://bücher.example:80", AddressRule::InvalidHostCharacter),
        ("tcp://-a.example:80", AddressRule::HyphenAtLabelEdge),
        ("tcp://a-.example:80", AddressRule::HyphenAtLabelEdge),
        ("tcp://a.-b:80", AddressRule::HyphenAtLabelEdge),
    ])
}

#[test]
fn test_a_port_is_a_number_from_1_to_65535() -> TestResult {
    refused_by(&[
        ("tcp://h", AddressRule::MissingPort),
        ("tcp://h:", AddressRule::MissingPort),
        ("tcp://[::1]", AddressRule::MissingPort),
        ("tcp://[::1]:", AddressRule::MissingPort),
        ("tcp://h:x", AddressRule::PortNotNumber),
        ("tcp://h:+80", AddressRule::PortNotNumber),
        ("tcp://h:-1", AddressRule::PortNotNumber),
        ("tcp://h:٨٠", AddressRule::PortNotNumber),
        ("tcp://h:65536", AddressRule::PortOutOfRange),
        ("tcp://h:99999999999999999999", AddressRule::PortOutOfRange),
        ("tcp://[::1]:65536", AddressRule::PortOutOfRange),
        ("tcp://h:0", AddressRule::PortZero),
        ("tcp://h:00", AddressRule::PortZero),
    ])?;
    // The bounds 1 and 65,535 are ports, and leading zeros still name a number in range.
    tcp_parses(&["tcp://h:1", "tcp://h:65535", "tcp://h:00080"])
}

#[test]
fn test_a_unix_path_is_absolute_and_names_a_file() -> TestResult {
    refused_by(&[
        ("unix://", AddressRule::UnixNotAbsolute),
        ("unix://run/x.sock", AddressRule::UnixNotAbsolute),
        ("unix:///", AddressRule::UnixNamesNoFile),
        ("unix:///run/", AddressRule::UnixNamesNoFile),
    ])
}

#[test]
fn test_the_first_rule_broken_is_the_one_named() -> TestResult {
    // Several rules broken at once: the earliest check names its rule.
    refused_by(&[
        ("tcp://user@::1:0", AddressRule::Userinfo),
        ("tcp://h%:x", AddressRule::PercentEncoded),
        ("tcp://-a:0", AddressRule::HyphenAtLabelEdge),
        ("tcp://::1:x", AddressRule::UnbracketedIpv6),
        ("ftp://a b", AddressRule::Whitespace),
    ])
}

#[test]
fn test_an_address_is_taken_up_to_its_longest_and_refused_past_it() -> TestResult {
    let prefix = "unix:///";
    let longest = format!("{prefix}{}", "a".repeat(ADDRESS_LEN_MAX - prefix.len()));
    assert_eq!(longest.len(), ADDRESS_LEN_MAX);
    assert_eq!(Address::parse(&longest)?.scheme(), Scheme::Unix);
    let past = format!("{longest}a");
    assert_eq!(refusal(&past)?, AddressRule::TooLong);
    Ok(())
}

#[test]
fn test_any_port_names_a_host_at_port_0_for_a_listener() -> TestResult {
    let any = Address::any_port("127.0.0.1")?;
    assert_eq!(any.host_port(), Some("127.0.0.1:0"));
    assert_eq!(any.to_string(), "tcp://127.0.0.1:0");
    // Its text names port 0, which no peer is dialled at.
    assert_eq!(refusal(&any.to_string())?, AddressRule::PortZero);
    assert_eq!(Address::any_port("[::1]")?.host_port(), Some("[::1]:0"));
    for (host, rule) in [
        ("", AddressRule::EmptyHost),
        ("::1", AddressRule::UnbracketedIpv6),
        ("user@h", AddressRule::Userinfo),
        ("h:80", AddressRule::UnbracketedIpv6),
        ("a b", AddressRule::Whitespace),
        ("a\n", AddressRule::ControlCharacter),
    ] {
        let Err(TransportError(TransportErrorEnum::Address {
            address,
            rule: found,
        })) = Address::any_port(host)
        else {
            return Err(TestError::new(format!("{host:?} was taken as a host")));
        };
        assert_eq!(
            (address, found),
            (format!("tcp://{host}:0"), rule),
            "{host:?}"
        );
    }
    let long = "a".repeat(ADDRESS_LEN_MAX);
    let Err(error) = Address::any_port(&long) else {
        return Err(TestError::new("a host past ADDRESS_LEN_MAX was taken"));
    };
    assert!(matches!(
        error.0,
        TransportErrorEnum::Address {
            rule: AddressRule::TooLong,
            ..
        }
    ));
    Ok(())
}

#[test]
fn test_each_rule_names_itself() {
    for (rule, text) in [
        (AddressRule::TooLong, "longer than ADDRESS_LEN_MAX"),
        (AddressRule::ControlCharacter, "a control character"),
        (AddressRule::Whitespace, "whitespace"),
        (
            AddressRule::UnknownScheme,
            "a scheme other than tcp:// and unix://",
        ),
        (AddressRule::Userinfo, "a TCP address naming a user"),
        (AddressRule::PercentEncoded, "a '%' in a TCP address"),
        (
            AddressRule::PathQueryOrFragment,
            "a path, query or fragment in a TCP address",
        ),
        (AddressRule::EmptyHost, "a TCP address without a host"),
        (
            AddressRule::UnbracketedIpv6,
            "an IPv6 host without brackets",
        ),
        (
            AddressRule::MalformedIpv6,
            "a bracketed host that is no IPv6 address",
        ),
        (
            AddressRule::InvalidIpv4,
            "a numeric host that is no dotted-quad IPv4 address",
        ),
        (
            AddressRule::DnsNameTooLong,
            "a host name longer than 253 bytes",
        ),
        (AddressRule::EmptyLabel, "a host name with an empty label"),
        (
            AddressRule::LabelTooLong,
            "a host name label longer than 63 bytes",
        ),
        (
            AddressRule::InvalidHostCharacter,
            "a host name character other than a letter, digit, hyphen or dot",
        ),
        (
            AddressRule::HyphenAtLabelEdge,
            "a host name label starting or ending with a hyphen",
        ),
        (AddressRule::MissingPort, "a TCP address without a port"),
        (AddressRule::PortNotNumber, "a port that is no number"),
        (AddressRule::PortOutOfRange, "a port above 65,535"),
        (AddressRule::PortZero, "port 0"),
        (
            AddressRule::UnixNotAbsolute,
            "a Unix path that is not absolute",
        ),
        (
            AddressRule::UnixNamesNoFile,
            "a Unix path that names no file",
        ),
        (AddressRule::UnixUnbound, "a Unix socket bound to no path"),
    ] {
        assert_eq!(rule.to_string(), text);
    }
}
