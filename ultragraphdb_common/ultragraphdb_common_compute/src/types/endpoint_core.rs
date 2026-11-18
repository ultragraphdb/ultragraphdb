/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::IpAddr;
use std::rc::Rc;

use ultragraphdb_common_role::OpenRequests;

use crate::traits::authorizer::Authorizer;
use crate::types::compute_core::ComputeCore;
use crate::types::endpoint_limits::EndpointLimits;
use crate::types::one_frame_phases::OneFramePhases;
use crate::types::role_links::RoleLinks;
use crate::types::system_link::SystemLink;
use crate::types::tenant_gate::TenantGate;
use crate::types::throttle_handle::ThrottleHandle;

/// What a client session shares on its endpoint core: its link to the system group, the way to the throttle, the connections
/// to the other roles, the gate, the limits, the count of open requests a stop waits on, the phases of the one-frame writes
/// it relays, and the core's connections to the partitions its clients' reads run over.
#[derive(Debug, Clone)]
pub struct EndpointCore<A> {
    system: SystemLink,
    logins: ThrottleHandle,
    links: RoleLinks,
    gate: TenantGate<A>,
    limits: EndpointLimits,
    open: OpenRequests,
    phases: OneFramePhases,
    /// Shared by an `Rc` with every session on this core, so their reads share its links (Ultra rule 9).
    compute: Option<Rc<ComputeCore>>,
}

impl<A: Authorizer> EndpointCore<A> {
    /// The core asking the system group through `system`, counting its authentications at `logins`, reaching the other roles
    /// through `links`, each request passing `gate`.
    #[must_use]
    pub fn new(
        system: SystemLink,
        logins: ThrottleHandle,
        links: RoleLinks,
        gate: TenantGate<A>,
        limits: EndpointLimits,
    ) -> Self {
        Self {
            system,
            logins,
            links,
            gate,
            limits,
            open: OpenRequests::default(),
            phases: OneFramePhases::default(),
            compute: None,
        }
    }

    /// The core numbered `core` among the frontend's endpoint cores, which its phases' log names.
    #[must_use]
    pub fn with_core(mut self, core: usize) -> Self {
        self.phases = self.phases.with_core(core);
        self
    }

    /// The core executing its clients' reads on `compute`, this core's connections to the partitions (design D3); a core
    /// holding none answers a read code 17, as one with no compute process did.
    #[must_use]
    pub fn with_compute(mut self, compute: Rc<ComputeCore>) -> Self {
        self.compute = Some(compute);
        self
    }

    /// The core counting what its sessions relay and have not yet answered in `open`, the count its connections' server keeps,
    /// so a stop waits for those answers too.
    #[must_use]
    pub fn with_open(mut self, open: OpenRequests) -> Self {
        self.open = open;
        self
    }
}

impl<A> EndpointCore<A> {
    /// The core's link to the system group.
    #[must_use]
    pub fn system(&self) -> &SystemLink {
        &self.system
    }

    /// The way to the throttle, which counts the failed authentications of every endpoint core (design D15).
    #[must_use]
    pub fn logins(&self) -> &ThrottleHandle {
        &self.logins
    }

    /// The connections to the other roles.
    #[must_use]
    pub fn links(&self) -> &RoleLinks {
        &self.links
    }

    /// The core's connections to the partitions its clients' reads run over; `None` for a core executing no read.
    pub(crate) fn compute(&self) -> Option<&Rc<ComputeCore>> {
        self.compute.as_ref()
    }

    /// The gate.
    #[must_use]
    pub fn gate(&self) -> &TenantGate<A> {
        &self.gate
    }

    /// The limits.
    #[must_use]
    pub fn limits(&self) -> &EndpointLimits {
        &self.limits
    }

    /// Whether a connection accepted from `peer` may go on: one with no address, over a Unix socket, always; one from a banned
    /// source not, as its throttle says (design D15). The frontend asks before TLS or any frame. A dropped future
    /// leaves what [`ThrottleHandle`] says.
    pub async fn admits(&self, peer: Option<IpAddr>) -> bool {
        match peer {
            Some(peer) => self.logins.admits(peer).await,
            None => true,
        }
    }

    /// The count of open requests a stop waits on.
    #[must_use]
    pub fn open(&self) -> &OpenRequests {
        &self.open
    }

    /// The phases of the one-frame writes its sessions relay, which [`EndpointCore::report_phases`] logs.
    #[must_use]
    pub fn phases(&self) -> &OneFramePhases {
        &self.phases
    }

    /// Logs the phases of the one-frame writes its sessions relayed, once they are due ([`OneFramePhases::report`]).
    pub fn report_phases(&self) {
        let now = crate::types::client_session::now();
        self.phases.report(now);
        self.system.round_trips().report(now);
    }
}
