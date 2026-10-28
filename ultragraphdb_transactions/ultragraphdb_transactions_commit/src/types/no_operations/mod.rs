/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod no_operations_operator;

/// The operator of a commit step that runs no commit-time operations: a proposal asking for one is refused, and
/// nothing is kept.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoOperations;
