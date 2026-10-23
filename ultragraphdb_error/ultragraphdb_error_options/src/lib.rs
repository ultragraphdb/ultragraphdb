/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `OptionsError`, why an index option was refused: raised by the index options of `ultragraphdb_traits_index`
//! and by the vector index factory of `ultragraphdb_core_tok`.

mod errors;

pub use crate::errors::options_error::{OptionsError, OptionsErrorEnum};
