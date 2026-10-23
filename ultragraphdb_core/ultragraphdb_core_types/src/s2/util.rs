/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/util.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Restricts a value to be within the range [lo, hi], as Go's `min(max(x, lo), hi)` on ints.
pub(crate) fn clamp(x: i64, lo: i64, hi: i64) -> i64 {
    x.max(lo).min(hi)
}
