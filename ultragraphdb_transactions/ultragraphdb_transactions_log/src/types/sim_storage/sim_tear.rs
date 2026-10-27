/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How a torn write leaves the file's length: as far as the whole write, as the file system recorded it while the
/// drive kept a prefix, or only as far as the prefix, as after a crash during an unsynced write extending the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimTear {
    WholeLength,
    ShortLength,
}
