/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use ultragraphdb_cluster_placement::{PartitionId, PlacementMap, TenantRun};
use ultragraphdb_common_role::{RoleClient, RoleCluster};
use ultragraphdb_core_ids::{Bucket, Tenant};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{DEFAULT_TRANSACTION_MS, Join, RoleKind};

use crate::types::role_link::RoleLink;

#[derive(Debug, Default)]
struct Held {
    open: HashMap<String, RoleLink>,
    partitions: BTreeMap<u32, String>,
    computes: Vec<String>,
    compute_turn: usize,
    partition_turn: usize,
    map: Option<PlacementMap>,
}

/// What one endpoint core knows of the other roles: who joined, a connection to each, shared by its client and role sessions on
/// that one thread, so no borrow crosses a wait. The placement map is a stale-tolerant copy: a partition's refusal corrects it.
#[derive(Debug, Clone)]
pub struct RoleLinks {
    cluster: RoleCluster,
    name: String,
    dial_ms: u32,
    held: Rc<RefCell<Held>>,
}

impl RoleLinks {
    /// What a core of `cluster` knows, dialling others as `name`; nothing has joined yet.
    #[must_use]
    pub fn new(cluster: RoleCluster, name: String) -> Self {
        Self {
            cluster,
            name,
            dial_ms: DEFAULT_TRANSACTION_MS,
            held: Rc::default(),
        }
    }

    /// What this core knows, with a dial bounded at `dial_ms` milliseconds, the Welcome's `transaction_ms`.
    #[must_use]
    pub fn with_dial_ms(mut self, dial_ms: u32) -> Self {
        self.dial_ms = dial_ms;
        self
    }

    /// The cluster's identity.
    #[must_use]
    pub fn cluster(&self) -> u64 {
        self.cluster.id()
    }

    /// The core's placement map, when it holds one.
    #[must_use]
    pub fn map(&self) -> Option<PlacementMap> {
        self.held.borrow().map.clone()
    }

    /// The core's placement map is `map` from here on.
    pub fn set_map(&self, map: PlacementMap) {
        self.held.borrow_mut().map = Some(map);
    }

    /// `join` was registered.
    pub fn joined(&self, join: &Join) {
        let mut held = self.held.borrow_mut();
        let address = join.address().to_string();
        if join.kind() == RoleKind::Compute {
            if !held.computes.contains(&address) {
                held.computes.push(address);
            }
            return;
        }
        held.partitions.retain(|_, holder| *holder != address);
        for partition in join.partitions() {
            let _replaced = held.partitions.insert(*partition, address.clone());
        }
    }

    /// `client` dialled the role at `address`: the link every session there shares, in place of one that closed.
    pub fn connected(&self, address: String, client: RoleClient) {
        let link = RoleLink::over(client, self.cluster());
        let _replaced = self.held.borrow_mut().open.insert(address, link);
    }

    fn link(&self, address: &str) -> Option<RoleLink> {
        let mut held = self.held.borrow_mut();
        if let Some(link) = held.open.get(address).filter(|link| !link.is_closed()) {
            return Some(link.clone());
        }
        let parsed = Address::parse(address).ok()?;
        let link = RoleLink::dial(
            parsed,
            self.cluster(),
            self.name.clone(),
            self.cluster.secret().password().to_string(),
            self.dial_ms,
        );
        let _replaced = held.open.insert(address.to_string(), link.clone());
        Some(link)
    }

    /// The link to `partition`'s role address; `None` when nothing has joined for it.
    #[must_use]
    pub fn partition(&self, partition: u32) -> Option<RoleLink> {
        let address = self.held.borrow().partitions.get(&partition).cloned()?;
        self.link(&address)
    }

    /// The next compute process, round-robin over those joined, leaving out `tried`: its address and link.
    #[must_use]
    pub fn next_compute(&self, tried: &[String]) -> Option<(String, RoleLink)> {
        let address = {
            let mut held = self.held.borrow_mut();
            let count = held.computes.len();
            let start = held.compute_turn;
            let found = (0..count)
                .map(|offset| (start + offset) % count.max(1))
                .find(|at| {
                    held.computes
                        .get(*at)
                        .is_some_and(|address| !tried.contains(address))
                })?;
            held.compute_turn = (found + 1) % count;
            held.computes.get(found).cloned()?
        };
        let link = self.link(&address)?;
        Some((address, link))
    }

    /// The compute processes joined.
    #[must_use]
    pub fn compute_count(&self) -> usize {
        self.held.borrow().computes.len()
    }

    /// Every partition joined, ascending.
    #[must_use]
    pub fn partitions(&self) -> Vec<u32> {
        self.held.borrow().partitions.keys().copied().collect()
    }

    /// The partition owning `bucket` by the core's map.
    #[must_use]
    pub fn owner_of(&self, bucket: Bucket) -> Option<u32> {
        let held = self.held.borrow();
        let map = held.map.as_ref()?;
        Some(map.partition_of(bucket, map.latest_change()).get())
    }

    /// The partitions holding `tenant`'s run by the core's map, ascending.
    #[must_use]
    pub fn run_partitions(&self, tenant: u64) -> Vec<u32> {
        let held = self.held.borrow();
        let Some(map) = held.map.as_ref() else {
            return Vec::new();
        };
        let run = TenantRun::of(Tenant::new(tenant)).range();
        let mut owners: Vec<PartitionId> = map
            .ranges_at(map.latest_change())
            .into_iter()
            .filter(|(range, _)| range.start() < run.end() && run.start() < range.end())
            .map(|(_, owner)| owner)
            .collect();
        owners.sort_unstable();
        owners.dedup();
        owners.iter().map(|owner| owner.get()).collect()
    }

    /// Every partition of the cluster by the core's map, ascending.
    #[must_use]
    pub fn every_partition(&self) -> Vec<u32> {
        let held = self.held.borrow();
        let Some(map) = held.map.as_ref() else {
            return Vec::new();
        };
        let mut owners: Vec<u32> = map
            .ranges_at(map.latest_change())
            .into_iter()
            .map(|(_, owner)| owner.get())
            .collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    /// The next of `tenant`'s run's partitions, in turn: where a transaction naming only new blank nodes binds.
    #[must_use]
    pub fn next_in_turn(&self, tenant: u64) -> Option<u32> {
        let owners = self.run_partitions(tenant);
        let mut held = self.held.borrow_mut();
        let at = held.partition_turn % owners.len().max(1);
        held.partition_turn = held.partition_turn.wrapping_add(1);
        owners.get(at).copied()
    }
}
