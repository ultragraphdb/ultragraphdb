/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The carrier an [`Address`](crate::Address) names by its scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// `tcp://host:port`: a TCP socket, across hosts.
    Tcp,
    /// `unix:///path`: a Unix-domain socket, within a host or a container.
    Unix,
}
