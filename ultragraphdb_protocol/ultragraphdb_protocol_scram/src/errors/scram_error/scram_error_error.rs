/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use crate::errors::scram_error::ScramError;

impl Error for ScramError {}
