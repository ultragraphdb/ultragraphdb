/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::transport_error::TransportError;
use crate::traits::login::Login;
use crate::types::anonymous::Anonymous;

impl Login for Anonymous {
    type Mechanism = Anonymous;

    fn mechanism(
        &mut self,
        _offered: &[String],
    ) -> Result<Option<Self::Mechanism>, TransportError> {
        Ok(None)
    }
}
