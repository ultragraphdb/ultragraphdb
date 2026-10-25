/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use core::fmt::{Display, Formatter};

use super::{UltraGraphError, UltraGraphErrorEnum};

impl Display for UltraGraphError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match &self.0 {
            UltraGraphErrorEnum::ConnectionString(err) => write!(f, "{err}"),
            UltraGraphErrorEnum::Connect(err) => write!(f, "{err}"),
            UltraGraphErrorEnum::Auth(err) => write!(f, "{err}"),
            UltraGraphErrorEnum::Transaction(err) => write!(f, "{err}"),
            UltraGraphErrorEnum::Rpc { code, message } => {
                write!(f, "RPC Error: {code:?}: {message}")
            }
        }
    }
}
