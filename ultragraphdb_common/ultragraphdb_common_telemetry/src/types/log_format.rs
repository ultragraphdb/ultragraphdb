/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How events are written to stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogFormat {
    /// One line of text per event, without colour codes: time, level, target, message and fields.
    Plain,
    /// One JSON object per line, with `timestamp`, `level`, `target` and `fields` (the message among them),
    /// as `tracing-subscriber`'s JSON formatter writes it.
    Json,
}
