/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod abort_body_builder;

/// A transaction aborts on the frame's stream. No command-header fields, no sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Abort;
