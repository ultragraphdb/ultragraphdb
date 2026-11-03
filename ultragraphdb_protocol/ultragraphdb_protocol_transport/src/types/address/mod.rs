/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod address_display;

use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

use crate::errors::transport_error::TransportError;
use crate::types::address_rule::AddressRule;
use crate::types::scheme::Scheme;

/// The scheme of a TCP address.
const TCP: &str = "tcp://";

/// The scheme of a Unix-domain socket's address.
const UNIX: &str = "unix://";

/// The longest address taken, in bytes: a DNS name's 253 and a port, or a socket path, with room to spare.
pub const ADDRESS_LEN_MAX: usize = 1024;

/// The longest DNS name, in bytes, without a final dot (RFC 1035 §2.3.4, as RFC 1123 §2.1 reads it).
const NAME_LEN_MAX: usize = 253;

/// The longest DNS label, in bytes (RFC 1035 §2.3.4).
const LABEL_LEN_MAX: usize = 63;

/// Where a role listens or a peer is dialled, from configuration, its carrier chosen by its scheme (`PROTOCOL.md`
/// §11.12): `tcp://host:port` across hosts, `unix:///path` within a container. The frames are the same on both
/// carriers, so no role asks which one it runs on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    scheme: Scheme,
    // `host:port` of a TCP address, the absolute path of a Unix one.
    target: String,
}

impl Address {
    /// The address `text` names.
    ///
    /// A TCP address is `tcp://host:port`: the host a DNS name of letters, digits and hyphens (RFC 1123 §2.1, at most
    /// 253 bytes, each label 1 to 63 bytes, no hyphen at a label's edge), a dotted-quad IPv4 address, or an IPv6
    /// address in brackets, `[::1]`; the port 1 to 65,535 in decimal digits. Nothing else is taken: no user, no
    /// percent-encoding, no path, query or fragment, and a host whose last label is all digits only as a dotted quad,
    /// so no resolver reads a name as an address it does not show. A Unix address is `unix:///path`, absolute and
    /// naming a file.
    ///
    /// # Errors
    ///
    /// [`TransportError::address`] naming `text` and the first [`AddressRule`] it breaks, checked in the rules' order.
    pub fn parse(text: &str) -> Result<Self, TransportError> {
        let refused = |rule| TransportError::address(text, rule);
        if text.len() > ADDRESS_LEN_MAX {
            return Err(refused(AddressRule::TooLong));
        }
        if text.chars().any(char::is_control) {
            return Err(refused(AddressRule::ControlCharacter));
        }
        if text.chars().any(char::is_whitespace) {
            return Err(refused(AddressRule::Whitespace));
        }
        let (scheme, target) = if let Some(target) = text.strip_prefix(TCP) {
            (Scheme::Tcp, target)
        } else if let Some(target) = text.strip_prefix(UNIX) {
            (Scheme::Unix, target)
        } else {
            return Err(refused(AddressRule::UnknownScheme));
        };
        let checked = match scheme {
            Scheme::Tcp => check_host_port(target, false),
            Scheme::Unix => check_path(target),
        };
        checked.map_err(refused)?;
        debug_assert!(!target.is_empty());
        debug_assert!(text.len() <= ADDRESS_LEN_MAX);
        Ok(Self {
            scheme,
            target: target.to_string(),
        })
    }

    /// The TCP address of `host` at port 0, where a listener's system chooses its port, which the listener reads back
    /// from its socket. Its text does not parse back, since no peer is dialled at port 0.
    ///
    /// # Errors
    ///
    /// [`TransportError::address`] naming `tcp://host:0` and the rule its host breaks, as [`Address::parse`] checks
    /// a host.
    pub fn any_port(host: &str) -> Result<Self, TransportError> {
        let target = format!("{host}:0");
        let text = format!("{TCP}{target}");
        let refused = |rule| TransportError::address(&text, rule);
        if text.len() > ADDRESS_LEN_MAX {
            return Err(refused(AddressRule::TooLong));
        }
        if text.chars().any(char::is_control) {
            return Err(refused(AddressRule::ControlCharacter));
        }
        if text.chars().any(char::is_whitespace) {
            return Err(refused(AddressRule::Whitespace));
        }
        check_host_port(&target, true).map_err(refused)?;
        Ok(Self {
            scheme: Scheme::Tcp,
            target,
        })
    }

    /// The carrier the scheme chose.
    #[must_use]
    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// A TCP address's `host:port`, as a resolver takes it; `None` for a Unix address.
    #[must_use]
    pub fn host_port(&self) -> Option<&str> {
        match self.scheme {
            Scheme::Tcp => Some(&self.target),
            Scheme::Unix => None,
        }
    }

    /// A Unix address's socket path; `None` for a TCP address.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self.scheme {
            Scheme::Tcp => None,
            Scheme::Unix => Some(Path::new(&self.target)),
        }
    }

    /// The scheme's prefix, as the address is written.
    pub(crate) fn prefix(&self) -> &'static str {
        match self.scheme {
            Scheme::Tcp => TCP,
            Scheme::Unix => UNIX,
        }
    }

    /// What the scheme names: a TCP address's `host:port`, a Unix address's socket path.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }
}

/// A TCP address's `host:port`, by [`AddressRule`]'s order: its characters, the split of host and port, the host,
/// then the port, 0 taken only when `any_port`.
fn check_host_port(target: &str, any_port: bool) -> Result<(), AddressRule> {
    if target.contains('@') {
        return Err(AddressRule::Userinfo);
    }
    if target.contains('%') {
        return Err(AddressRule::PercentEncoded);
    }
    if target.contains(['/', '\\', '?', '#']) {
        return Err(AddressRule::PathQueryOrFragment);
    }
    let (host, port) = split_host_port(target)?;
    check_host(host)?;
    let port = port
        .filter(|port| !port.is_empty())
        .ok_or(AddressRule::MissingPort)?;
    // `u16::from_str` takes a leading `+`, which no port is written with.
    if !port.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AddressRule::PortNotNumber);
    }
    match port.parse::<u16>() {
        Err(_) => Err(AddressRule::PortOutOfRange),
        Ok(0) if !any_port => Err(AddressRule::PortZero),
        Ok(_) => Ok(()),
    }
}

/// `target` as its host, brackets kept, and its port after the last `:`, `None` when it names none.
fn split_host_port(target: &str) -> Result<(&str, Option<&str>), AddressRule> {
    if target.starts_with('[') {
        let end = target.find(']').ok_or(AddressRule::MalformedIpv6)?;
        let (host, rest) = target.split_at(end.saturating_add(1));
        if rest.is_empty() {
            return Ok((host, None));
        }
        let port = rest.strip_prefix(':').ok_or(AddressRule::MalformedIpv6)?;
        return Ok((host, Some(port)));
    }
    Ok(match target.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (target, None),
    })
}

/// A host: a bracketed IPv6 address, a dotted-quad IPv4 address when its last label is all digits, else a DNS name.
fn check_host(host: &str) -> Result<(), AddressRule> {
    if host.is_empty() || host == "[]" {
        return Err(AddressRule::EmptyHost);
    }
    if let Some(bracketed) = host.strip_prefix('[') {
        let inner = bracketed
            .strip_suffix(']')
            .ok_or(AddressRule::MalformedIpv6)?;
        return inner
            .parse::<Ipv6Addr>()
            .map(drop)
            .map_err(|_| AddressRule::MalformedIpv6);
    }
    if host.contains(':') {
        return Err(AddressRule::UnbracketedIpv6);
    }
    let last = host.rsplit('.').next().unwrap_or(host);
    let numeric = !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit());
    if numeric || host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        // std refuses a leading zero, fewer or more than four octets and an octet past 255.
        return host
            .parse::<Ipv4Addr>()
            .map(drop)
            .map_err(|_| AddressRule::InvalidIpv4);
    }
    check_name(host)
}

/// A DNS name of letters, digits and hyphens (RFC 1123 §2.1): at most 253 bytes, each label 1 to 63 bytes, no hyphen
/// at a label's edge.
fn check_name(name: &str) -> Result<(), AddressRule> {
    if name.len() > NAME_LEN_MAX {
        return Err(AddressRule::DnsNameTooLong);
    }
    for label in name.split('.') {
        if label.is_empty() {
            return Err(AddressRule::EmptyLabel);
        }
        if label.len() > LABEL_LEN_MAX {
            return Err(AddressRule::LabelTooLong);
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(AddressRule::InvalidHostCharacter);
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(AddressRule::HyphenAtLabelEdge);
        }
    }
    Ok(())
}

/// A Unix address's path: absolute, naming a file rather than a directory.
fn check_path(target: &str) -> Result<(), AddressRule> {
    if !target.starts_with('/') {
        return Err(AddressRule::UnixNotAbsolute);
    }
    if target.ends_with('/') {
        return Err(AddressRule::UnixNamesNoFile);
    }
    Ok(())
}
