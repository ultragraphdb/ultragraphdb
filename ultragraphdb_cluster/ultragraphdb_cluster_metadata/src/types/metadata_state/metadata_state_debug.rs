/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::metadata_state::MetadataState;

impl fmt::Debug for MetadataState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetadataState")
            .field("tenants", &self.tenants)
            .field(
                "unknown_secret",
                &self.unknown_secret.map(|_| format_args!("<redacted>")),
            )
            .field("catalogs", &self.catalogs)
            .field("checking", &self.checking)
            .field("defaults", &self.defaults)
            .field("high_water", &self.high_water)
            .field("placement", &self.placement)
            .field("placement_pending", &self.placement_pending)
            .field("placement_version", &self.placement_version)
            .field("untrimmed", &self.untrimmed)
            .field("principals", &self.principals)
            .field("next_principal", &self.next_principal)
            .field("sessions", &self.sessions)
            .field("owners", &self.owners)
            .field("evictions", &self.evictions)
            .field("learned", &self.learned)
            .finish()
    }
}
