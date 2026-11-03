/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod address_rule_display;

/// The rule an address from configuration breaks, one per check [`crate::Address::parse`] makes, in the order it makes
/// them: the text as a whole, its scheme, a TCP address's characters, its host, then its port; or a Unix address's
/// path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressRule {
    /// Longer than [`crate::ADDRESS_LEN_MAX`] bytes.
    TooLong,
    /// A control character, NUL, CR and LF among them.
    ControlCharacter,
    /// Whitespace, Unicode's included.
    Whitespace,
    /// A scheme other than `tcp://` and `unix://`, exactly as written.
    UnknownScheme,
    /// A TCP address naming a user: `@` before the host.
    Userinfo,
    /// A `%`: percent-encoding, or an IPv6 zone, neither of which a role address is written with.
    PercentEncoded,
    /// A `/`, `\`, `?` or `#` in a TCP address: a path, a query or a fragment, none of which it has.
    PathQueryOrFragment,
    /// A TCP address with no host, `[]` included.
    EmptyHost,
    /// An IPv6 host without its brackets: a `:` in the host, `tcp://::1:80`.
    UnbracketedIpv6,
    /// A bracketed host that is no IPv6 address, an unclosed bracket, or a bracket followed by anything but `:port`.
    MalformedIpv6,
    /// A host whose last label is all digits that is no dotted-quad IPv4 address of four decimal octets from 0 to
    /// 255 without leading zeros: `010.0.0.1` is octal to some resolvers and decimal to others, `127.1`, `2130706433`
    /// and `0x7f.0.0.1` name 127.0.0.1 to `inet_aton`, so none is taken.
    InvalidIpv4,
    /// A DNS name longer than 253 bytes.
    DnsNameTooLong,
    /// A DNS name with an empty label: a leading, trailing or doubled dot.
    EmptyLabel,
    /// A DNS label longer than 63 bytes.
    LabelTooLong,
    /// A DNS name holding a character other than an ASCII letter, digit, hyphen or dot, a Unicode look-alike or `_`
    /// among them.
    InvalidHostCharacter,
    /// A DNS label starting or ending with a hyphen.
    HyphenAtLabelEdge,
    /// A TCP address without a port.
    MissingPort,
    /// A port holding anything but ASCII digits, a sign included.
    PortNotNumber,
    /// A port above 65,535.
    PortOutOfRange,
    /// Port 0, which no peer can be dialled at; [`crate::Address::any_port`] names a listener's port the system
    /// chooses.
    PortZero,
    /// A Unix path that is not absolute.
    UnixNotAbsolute,
    /// A Unix path naming a directory, the root included, rather than a file.
    UnixNamesNoFile,
    /// A Unix socket bound to no path, which no address names.
    UnixUnbound,
}
