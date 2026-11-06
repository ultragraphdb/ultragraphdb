/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The kind of request an Error refused, which decides what its code runs again (`PROTOCOL.md` §9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefusedRequest {
    /// A Read, Execute or Query.
    Read,
    /// A Commit.
    Commit,
    /// A Drop, DirectWrite or ConditionalWrite: a frame that holds what it does.
    OneFrame,
    /// Any other request: a Begin, Write, Operation, Prepare or Describe among them.
    Other,
}
