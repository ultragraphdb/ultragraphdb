/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{ScramError, ScramRelay};
use ultragraphdb_protocol_transport::MechanismStep;

/// Where a relayed exchange stands between the session that asks the system group and the machine that answers the
/// client.
#[derive(Default)]
pub(crate) struct RelayState {
    pub relay: Option<ScramRelay>,
    pub name: Option<String>,
    /// What the next step answers, prepared by the session before the machine runs it.
    pub answer: Option<Result<MechanismStep, ScramError>>,
}
