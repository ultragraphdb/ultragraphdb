/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_metadata::CatalogCheck;
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::owner::owner_answer::refused_or_stop;
use crate::types::owner::{Durable, Owner};
use crate::types::report_asked::ReportAsked;
use crate::utils::declarations::{restored, version_bytes};

/// Why a version being checked is withdrawn.
enum Withdrawal {
    /// A partition reported the data refuses it.
    Refused,
    /// This partition reported nothing within `catalog_check_ms`.
    Silent(u32),
}

impl<M: OrderedLog, S: OrderedLog> Owner<M, S> {
    /// A partition's report on a catalog version, answered once the group holds it; of a version not being checked it changes
    /// nothing (`PROTOCOL.md` §13.19).
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    pub(super) fn report(
        &mut self,
        report: ReportAsked,
        reply: Sender<Result<(), ErrorReply>>,
    ) -> Result<(), RoleError> {
        let (held, version) = (report.tenant(), report.version());
        let partition = match PartitionId::new(report.partition()) {
            Ok(partition) => partition,
            Err(error) => {
                let refusal = ErrorReply::new(ErrorCode::Protocol, error.to_string());
                drop(reply.send(Err(refusal)));
                return Ok(());
            }
        };
        let refused = report.into_refused();
        tracing::info!(
            tenant = held.get(),
            version,
            partition = partition.get(),
            refused = refused.is_some(),
            "a partition reported on a catalog version"
        );
        let is_refused = refused.is_some();
        if let Some(refused) = refused {
            let _first = self
                .check_refusals
                .entry((held, version))
                .or_insert(refused);
        }
        match self
            .metadata
            .report_check(held, version, partition, is_refused)
        {
            Ok(_position) => self.durable.push(Durable::Registered(reply)),
            Err(failed) => drop(reply.send(Err(refused_or_stop(&failed)?))),
        }
        Ok(())
    }

    /// Answers what stands, then withdraws each version being checked that a partition's data refused or whose wait, counted from
    /// the first turn to see its epoch assigned, ran out at `now_us`: a restarted owner takes up what its log holds this way.
    pub(super) fn run_checks(&mut self, now_us: u64) -> Result<(), RoleError> {
        self.answer_versions();
        let check_us = self
            .metadata
            .config()
            .catalog_check_ms()
            .saturating_mul(1_000);
        let checking: Vec<(Tenant, u64)> = self.metadata.state().checking().collect();
        for (tenant, version) in checking {
            let state = self.metadata.state();
            let Some(record) = state.version_record(tenant, version) else {
                continue;
            };
            let Some(epoch) = record.effective() else {
                continue;
            };
            let CatalogCheck::Checking { passed, refused } = record.check() else {
                continue;
            };
            let withdrawal = if *refused {
                Some(Withdrawal::Refused)
            } else {
                let due = *self
                    .check_deadlines
                    .entry((tenant, version))
                    .or_insert_with(|| now_us.saturating_add(check_us));
                let silent = state
                    .holders(tenant, epoch)
                    .into_iter()
                    .find(|holder| !passed.contains(holder));
                silent
                    .filter(|_| now_us >= due)
                    .map(|partition| Withdrawal::Silent(partition.get()))
            };
            if let Some(withdrawal) = withdrawal {
                self.withdraw(tenant, version, &withdrawal)?;
            }
        }
        Ok(())
    }

    /// Offers the version restoring what `version` changed, which withdraws it, and refuses the Commit waiting on it once that is
    /// durable. An offer the group refuses now, a full table or a version this turn got ahead of, is offered again next turn.
    fn withdraw(
        &mut self,
        tenant: Tenant,
        version: u64,
        withdrawal: &Withdrawal,
    ) -> Result<(), RoleError> {
        let number = tenant.get();
        let current = self.metadata.state().current_version(tenant);
        let unheld = |refusal: ErrorReply| RoleError::software(refusal.message().to_string());
        let before = self.contents(number, version - 1).map_err(unheld)?;
        let withdrawn = self.contents(number, version).map_err(unheld)?;
        let held = self.contents(number, current).map_err(unheld)?;
        let next = restored(&before, &withdrawn, held);
        let bytes = version_bytes(number, current + 1, next).map_err(unheld)?;
        match self
            .metadata
            .offer_restoring(tenant, current, version, &bytes)
        {
            Ok((restoring, _position)) => {
                let _forgotten = self.check_deadlines.remove(&(tenant, version));
                let refusal = self.refusal(tenant, version, restoring, withdrawal);
                let waiter = self
                    .versions_waited
                    .iter()
                    .position(|(of, waited, _)| *of == tenant && *waited == version)
                    .map(|at| self.versions_waited.swap_remove(at));
                tracing::warn!(
                    tenant = number,
                    version,
                    restoring,
                    message = refusal.message(),
                    "a tightening is withdrawn"
                );
                let told = waiter.map(|(_, _, reply)| (reply, refusal));
                self.durable.push(Durable::Restored(told));
            }
            Err(refused) => {
                let reason = refused_or_stop(&refused)?;
                tracing::warn!(
                    version,
                    reason = reason.message(),
                    "a restoring version waits"
                );
            }
        }
        Ok(())
    }

    /// What the Commit of `version` hears, `restoring` the version restoring it.
    fn refusal(
        &mut self,
        tenant: Tenant,
        version: u64,
        restoring: u64,
        withdrawal: &Withdrawal,
    ) -> ErrorReply {
        let named = self.check_refusals.remove(&(tenant, version));
        match withdrawal {
            Withdrawal::Refused => {
                let (predicate, at_fault) = named.unwrap_or_default();
                ErrorReply::new(
                    ErrorCode::Refused,
                    format!(
                        "the data refuses catalog version {version}: <{predicate}> does not hold of {at_fault}, and version {restoring} restores the previous declaration"
                    ),
                )
            }
            Withdrawal::Silent(partition) => ErrorReply::new(
                ErrorCode::TimedOut,
                format!(
                    "partition {partition} reported no check of catalog version {version} within {} ms, and version {restoring} restores the previous declaration",
                    self.metadata.config().catalog_check_ms()
                ),
            )
            .with_detail(u64::from(*partition)),
        }
    }
}
