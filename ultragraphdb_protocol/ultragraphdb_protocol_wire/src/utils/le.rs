/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The `N` bytes of `bytes` at `at`, or `None` when they run past its end: the one way this crate reads a field,
/// so a short input is an absent value and never a panic. A field is then `u64::from_le_bytes(bytes_at(b, at)?)`.
pub(crate) fn bytes_at<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at.checked_add(N)?)?.try_into().ok()
}
