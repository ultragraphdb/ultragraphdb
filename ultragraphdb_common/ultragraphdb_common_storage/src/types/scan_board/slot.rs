/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// An open Scan, and what it waits on, each of which names it in the board's sets while it waits.
#[derive(Debug)]
pub(super) struct Slot<T> {
    pub(super) scan: T,
    pub(super) timer: Option<u64>,
    pub(super) epoch: Option<u64>,
}
