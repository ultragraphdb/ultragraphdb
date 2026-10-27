/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod cancel_body_builder;

/// The frame's stream is cancelled: no further frame of it is sent. No command-header fields, no sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Cancel;
