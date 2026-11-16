/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the frontend reads of the books besides the registry: the default-entry records a Describe names, the catalog
//! version a client may read at a closed epoch, and the two halves of a client's proof (`PROTOCOL.md` §13.7 to §13.9,
//! §13.28). The stored key and the server key are read here and leave in no answer.

use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_scram::{DEFAULT_ITERATIONS, ScramVerifier};
use ultragraphdb_protocol_wire::{Catalog, Defaults, ErrorCode, ErrorReply};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::system_group::SystemGroup;

/// The refusal of a request naming a tenant the group does not hold: code 7, `detail` the tenant (`PROTOCOL.md` §13.27, §13.28).
pub(super) fn unknown_tenant(tenant: u64) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Refused,
        format!("the metadata group holds no tenant {tenant}"),
    )
    .with_detail(tenant)
}

impl<M: OrderedLog, S: OrderedLog> SystemGroup<M, S> {
    /// The predicates `tenant` holds default-entry records for, in the order of their texts' bytes, at the log position read.
    pub(super) fn defaults(&self, tenant: u64) -> Result<Defaults, ErrorReply> {
        let state = self.metadata.state();
        let held = Tenant::new(tenant);
        if !state.holds_tenant(held) {
            return Err(unknown_tenant(tenant));
        }
        // The records are kept by (tenant, predicate), so in the order of the predicates' texts' bytes.
        let predicates = state
            .default_entries(held)
            .map(|(predicate, _)| predicate.to_string())
            .collect();
        Ok(Defaults::reply(
            tenant,
            self.metadata.applied().get(),
            predicates,
        ))
    }

    /// The version of `tenant`'s catalog a client may read at the closed `epoch`, or the current one for `epoch` 0.
    pub(super) fn catalog_at(&self, tenant: u64, epoch: u64) -> Result<Catalog, ErrorReply> {
        if epoch == 0 {
            return self.catalog(tenant, 0);
        }
        if epoch > self.closed {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!(
                    "a Catalog request at epoch {epoch}, above the latest close {}",
                    self.closed
                ),
            ));
        }
        let at = Epoch::new(epoch)
            .map_err(|error| ErrorReply::new(ErrorCode::Protocol, error.to_string()))?;
        let state = self.metadata.state();
        let record = state.settled_at(Tenant::new(tenant), at).ok_or_else(|| {
            ErrorReply::new(
                ErrorCode::Refused,
                format!(
                    "tenant {tenant} holds no catalog version a client may read at epoch {epoch}"
                ),
            )
            .with_detail(0)
        })?;
        Ok(Catalog::reply(
            tenant,
            record.version(),
            record.effective().map_or(0, Epoch::get),
            self.metadata.applied().get(),
            record.bytes().to_vec(),
        ))
    }

    /// The iteration count and salt a Challenge for `name` answers.
    pub(super) fn challenge(&self, name: &str) -> Result<(u32, Vec<u8>), ErrorReply> {
        let verifier = self.verifier_of(name)?;
        Ok((verifier.iterations(), verifier.salt().to_vec()))
    }

    /// The verifier of `name`: the principal's, or for a name no principal holds the one derived from the secret the group drew,
    /// at the count it derives a principal's verifier with, so an unknown name costs what a held one costs (`PROTOCOL.md` §13.8).
    fn verifier_of(&self, name: &str) -> Result<ScramVerifier, ErrorReply> {
        let state = self.metadata.state();
        if let Some(principal) = state.principal(name) {
            return Ok(principal.verifier().clone());
        }
        let secret = state.unknown_secret().ok_or_else(|| {
            ErrorReply::new(
                ErrorCode::Unavailable,
                "the metadata group holds no secret for an unknown name",
            )
        })?;
        Ok(ScramVerifier::unknown(secret, name, DEFAULT_ITERATIONS))
    }

    /// The principal and ServerSignature a Verify answers when `proof` holds for `name` over `auth_message`.
    pub(super) fn verify(
        &self,
        name: &str,
        auth_message: &str,
        proof: &[u8; 32],
    ) -> Result<Option<(u64, [u8; 32])>, ErrorReply> {
        // An unknown name's proof is checked against its derived verifier as a held one's is, so both cost alike and neither
        // holds.
        let signature = self.verifier_of(name)?.sign(auth_message, proof);
        let principal = self.metadata.state().principal(name).map(|held| held.id());
        Ok(principal.zip(signature))
    }
}
