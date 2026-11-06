/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Action;

use crate::traits::authorizer::Authorizer;

/// The authorizer of this stage (`SECURITY.md`, *Where it is built*): every authenticated principal holds every privilege the
/// gate asks of it, until stage 7's grants fill it in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EveryAuthenticated;

impl Authorizer for EveryAuthenticated {
    fn allows(&self, _principal: u64, _action: Action, _tenant: u64) -> bool {
        true
    }
}
