/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The check of an invariant, Ultra Style rule 5 (`openspec/protocols/ULTRA_STYLE.md`): [`invariant`] and the
//! [`InvariantError`] it returns. An invariant fails only through a bug in UltraGraphDB, never through input, so a
//! violation in a debug build panics at the caller, for tests, simulation and fuzzing, and in a release build is
//! returned, so the component it was found in stops (fail-stop) while code under `src/` keeps its rule of never
//! panicking. The crate depends on nothing, so every crate checking an invariant can depend on it.

mod errors;
mod utils;

pub use crate::errors::invariant_error::{InvariantError, InvariantErrorEnum};
pub use crate::utils::invariant::invariant;
