/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#[cfg(test)]
pub mod draw;
/// The unit tests' driver of the two halves, shared by path.
#[cfg(test)]
#[path = "../../utils/halves.rs"]
pub mod halves;
