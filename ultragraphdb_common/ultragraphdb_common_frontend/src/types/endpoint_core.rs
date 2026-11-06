/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::{Rc, Weak};

use ultragraphdb_common_role::OpenRequests;

use crate::traits::authorizer::Authorizer;
use crate::types::auth_throttle::AuthThrottle;
use crate::types::endpoint_limits::EndpointLimits;
use crate::types::owner_handle::OwnerHandle;
use crate::types::role_links::RoleLinks;
use crate::types::tenant_gate::TenantGate;

/// What a client session shares on its endpoint core: the way to the owner, the connections to the other roles, the gate, the
/// limits, the throttle and the count of open requests a stop waits on. The way to the owner is shared by an `Rc` on the core, so what outlives the core's sessions can hold it
/// weakly and keep no owner from stopping.
#[derive(Debug, Clone)]
pub struct EndpointCore<A> {
    owner: Rc<OwnerHandle>,
    links: RoleLinks,
    gate: TenantGate<A>,
    limits: EndpointLimits,
    throttle: AuthThrottle,
    open: OpenRequests,
}

impl<A: Authorizer> EndpointCore<A> {
    /// The core asking `owner`, reaching the other roles through `links`, each request passing `gate`.
    #[must_use]
    pub fn new(
        owner: OwnerHandle,
        links: RoleLinks,
        gate: TenantGate<A>,
        limits: EndpointLimits,
    ) -> Self {
        Self {
            owner: Rc::new(owner),
            links,
            gate,
            limits,
            throttle: AuthThrottle::new(limits.refusal_interval()),
            open: OpenRequests::default(),
        }
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
    /// The way to the owner.
    #[must_use]
    pub fn owner(&self) -> &OwnerHandle {
        self.owner.as_ref()
    }

    /// The way to the owner while the core lasts, for what outlives its sessions.
    pub(crate) fn owner_weak(&self) -> Weak<OwnerHandle> {
        Rc::downgrade(&self.owner)
    }

    /// The connections to the other roles.
    #[must_use]
    pub fn links(&self) -> &RoleLinks {
        &self.links
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

    /// The throttle of failed authentications.
    #[must_use]
    pub fn throttle(&self) -> &AuthThrottle {
        &self.throttle
    }

    /// The count of open requests a stop waits on.
    #[must_use]
    pub fn open(&self) -> &OpenRequests {
        &self.open
    }
}
