/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What one HTTP/1.1 request asks of the observability listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthRequest {
    /// The bytes so far end no request, and more may.
    Incomplete,
    /// `GET /health`.
    Health,
    /// `GET /ready`.
    Ready,
    /// A `GET` of anything else.
    NotFound,
    /// A well-formed request of any other method.
    MethodNotAllowed,
    /// Bytes that are no request, or a request past the bound.
    BadRequest,
}
