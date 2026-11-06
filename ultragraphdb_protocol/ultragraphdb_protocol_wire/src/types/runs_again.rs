/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What runs again after a retryable refusal (`PROTOCOL.md` §9.4), since the code says what, and its request where the
/// code alone does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunsAgain {
    /// The same request, same session and request number, on a new stream after a delay.
    Request,
    /// The whole transaction on a new Begin, since its stream has ended or its read version is fixed.
    Transaction,
    /// The read, in the same transaction or a new one, from its start or a resume key, its earlier rows discarded.
    Read,
    /// Nothing yet: the request is in doubt, and a resend through its session says whether it committed (§11.5).
    InDoubt,
}
