/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The order a [`crate::Scan`] reports its keys in: ascending byte order, or descending. A key's versions come
/// newest first either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    #[default]
    Forward,
    Reverse,
}
