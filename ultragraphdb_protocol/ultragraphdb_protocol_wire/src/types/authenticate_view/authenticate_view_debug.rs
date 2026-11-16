/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::authenticate_view::AuthenticateView;

impl fmt::Debug for AuthenticateView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthenticateView")
            .field("step", &self.step)
            .field("last", &self.last)
            .field("principal", &self.principal)
            .field("expires", &self.expires)
            .field("mechanism", &self.mechanism)
            .field("message", &format_args!("<redacted>"))
            .finish()
    }
}
