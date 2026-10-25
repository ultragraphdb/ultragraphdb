/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::UltraGraphError;

// No `source()`: `std::error::Error::source` returns `&dyn Error`, which the
// static-dispatch rule in rust/README_RUST.md forbids. The nested error is reachable
// through the public field `.0` instead, which is both typed and cheaper.
impl core::error::Error for UltraGraphError {}
