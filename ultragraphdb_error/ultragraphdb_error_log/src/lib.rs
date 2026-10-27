/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `LogError`, why an operation of the ordered log failed, split by what the caller does about it.

mod errors;

pub use crate::errors::log_error::{LogError, LogErrorEnum};
