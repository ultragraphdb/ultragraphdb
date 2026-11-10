/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The `N` bytes of `bytes` at `at`, or `None` when they run past its end, so a short input is an absent value and
/// never a panic.
pub(crate) fn bytes_at<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at.checked_add(N)?)?.try_into().ok()
}
