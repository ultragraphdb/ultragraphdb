/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod address_display;

use std::path::Path;

use crate::errors::transport_error::TransportError;
use crate::types::scheme::Scheme;

/// The scheme of a TCP address.
const TCP: &str = "tcp://";

/// The scheme of a Unix-domain socket's address.
const UNIX: &str = "unix://";

/// The longest address taken, in bytes: a DNS name's 253 and a port, or a socket path, with room to spare.
pub const ADDRESS_LEN_MAX: usize = 1024;

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
    /// # Errors
    ///
    /// [`TransportError::address`] naming `text` and the rule it breaks: longer than [`ADDRESS_LEN_MAX`], a scheme
    /// other than `tcp://` and `unix://`, a TCP address without a host or a port, or with a port that is no number
    /// below 65,536, a Unix path that is not absolute or names only the root, or either holding a control character
    /// or whitespace.
    pub fn parse(text: &str) -> Result<Self, TransportError> {
        let refused = |reason| TransportError::address(text, reason);
        if text.len() > ADDRESS_LEN_MAX {
            return Err(refused("longer than ADDRESS_LEN_MAX"));
        }
        if text.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(refused("a control character or whitespace"));
        }
        let (scheme, target) = if let Some(target) = text.strip_prefix(TCP) {
            (Scheme::Tcp, target)
        } else if let Some(target) = text.strip_prefix(UNIX) {
            (Scheme::Unix, target)
        } else {
            return Err(refused("a scheme other than tcp:// and unix://"));
        };
        let checked = match scheme {
            Scheme::Tcp => check_host_port(target),
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

/// A TCP address's `host:port`: a host, without `/` or `@`, then a port of digits below 65,536; an IPv6 host is
/// bracketed, `[::1]:9080`.
fn check_host_port(target: &str) -> Result<(), &'static str> {
    let (host, port) = if target.starts_with('[') {
        let Some(end) = target.find(']') else {
            return Err("a TCP address without a port");
        };
        let (host, rest) = target.split_at(end.saturating_add(1));
        let Some(port) = rest.strip_prefix(':') else {
            return Err("a TCP address without a port");
        };
        (host, port)
    } else {
        let Some((host, port)) = target.rsplit_once(':') else {
            return Err(if target.is_empty() {
                "a TCP address without a host"
            } else {
                "a TCP address without a port"
            });
        };
        (host, port)
    };
    if host.is_empty() {
        return Err("a TCP address without a host");
    }
    if host.contains(['/', '@']) {
        return Err("a TCP host holding '/' or '@'");
    }
    if port.is_empty() {
        return Err("a TCP address without a port");
    }
    // `u16::from_str` takes a leading `+`, which no port is written with.
    if !port.bytes().all(|b| b.is_ascii_digit()) || port.parse::<u16>().is_err() {
        return Err("a port that is no number below 65,536");
    }
    Ok(())
}

/// A Unix address's path: absolute, naming a file rather than a directory.
fn check_path(target: &str) -> Result<(), &'static str> {
    if !target.starts_with('/') || target.ends_with('/') {
        return Err("a Unix path that is not absolute or names no file");
    }
    Ok(())
}
