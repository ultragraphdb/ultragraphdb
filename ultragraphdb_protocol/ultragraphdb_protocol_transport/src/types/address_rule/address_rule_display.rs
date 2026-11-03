/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::AddressRule;

impl fmt::Display for AddressRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "longer than ADDRESS_LEN_MAX",
            Self::ControlCharacter => "a control character",
            Self::Whitespace => "whitespace",
            Self::UnknownScheme => "a scheme other than tcp:// and unix://",
            Self::Userinfo => "a TCP address naming a user",
            Self::PercentEncoded => "a '%' in a TCP address",
            Self::PathQueryOrFragment => "a path, query or fragment in a TCP address",
            Self::EmptyHost => "a TCP address without a host",
            Self::UnbracketedIpv6 => "an IPv6 host without brackets",
            Self::MalformedIpv6 => "a bracketed host that is no IPv6 address",
            Self::InvalidIpv4 => "a numeric host that is no dotted-quad IPv4 address",
            Self::DnsNameTooLong => "a host name longer than 253 bytes",
            Self::EmptyLabel => "a host name with an empty label",
            Self::LabelTooLong => "a host name label longer than 63 bytes",
            Self::InvalidHostCharacter => {
                "a host name character other than a letter, digit, hyphen or dot"
            }
            Self::HyphenAtLabelEdge => "a host name label starting or ending with a hyphen",
            Self::MissingPort => "a TCP address without a port",
            Self::PortNotNumber => "a port that is no number",
            Self::PortOutOfRange => "a port above 65,535",
            Self::PortZero => "port 0",
            Self::UnixNotAbsolute => "a Unix path that is not absolute",
            Self::UnixNamesNoFile => "a Unix path that names no file",
            Self::UnixUnbound => "a Unix socket bound to no path",
        })
    }
}
