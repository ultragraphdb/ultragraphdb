/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_protocol_scram::{MAX_ITERATIONS, MIN_ITERATIONS, ScramVerifier};
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_group::MetadataGroup;

impl<L: OrderedLog> MetadataGroup<L> {
    /// Creates the principal `name`, authenticating with `verifier`, holding the administrator role when
    /// `administrator`; returns its identity, one past the last created, and the position it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::principal_name`] for a name of 0 or more than 255 bytes; [`ClusterError::too_many`] for a salt
    /// of more than 255 bytes; [`ClusterError::principal_exists`] naming `name`; [`ClusterError::full`] at
    /// [`MetadataState::PRINCIPALS_MAX`](crate::MetadataState::PRINCIPALS_MAX); [`ClusterError::log`].
    pub fn create_principal(
        &mut self,
        name: &str,
        verifier: ScramVerifier,
        administrator: bool,
    ) -> Result<(u64, Position), ClusterError> {
        let id = self.head.next_principal;
        let position = self.append(MetadataCommand::CreatePrincipal {
            id,
            name: name.to_string(),
            verifier,
            administrator,
        })?;
        Ok((id, position))
    }

    /// Creates the bootstrap administrator `name` at the cluster's first start from `secret_file`, the bytes of the
    /// secret file the operator supplies: UTF-8 text whose password is the whole file, one trailing line ending (`\n`
    /// or `\r\n`) left out. The group keeps only the verifier derived from it with `salt` over `iterations`, never the
    /// password. Returns the principal's identity and the position it was appended at.
    ///
    /// # Errors
    ///
    /// In this order: [`ClusterError::bootstrap`] when the group holds a principal, durable or not;
    /// [`ClusterError::principal_name`] for a name of 0 or more than 255 bytes; [`ClusterError::bootstrap`] naming what
    /// is missing: a file of UTF-8 text, a password in it, an iteration count from 4,096 to 10,000,000, a password
    /// SASLprep accepts; the refusals of [`create_principal`](Self::create_principal).
    pub fn bootstrap(
        &mut self,
        name: &str,
        secret_file: &[u8],
        salt: &[u8],
        iterations: u32,
    ) -> Result<(u64, Position), ClusterError> {
        if self.head.principals() > 0 {
            return Err(ClusterError::bootstrap(
                "a group holding no principal, at the cluster's first start",
            ));
        }
        if name.is_empty() || name.len() > MetadataCommand::NAME_BYTES_MAX {
            return Err(ClusterError::principal_name(name.len()));
        }
        let text = std::str::from_utf8(secret_file)
            .map_err(|_| ClusterError::bootstrap("a secret file of UTF-8 text"))?;
        let password = text
            .strip_suffix("\r\n")
            .or_else(|| text.strip_suffix('\n'))
            .unwrap_or(text);
        if password.is_empty() {
            return Err(ClusterError::bootstrap("a password in the secret file"));
        }
        if !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&iterations) {
            return Err(ClusterError::bootstrap(
                "an iteration count from 4,096 to 10,000,000",
            ));
        }
        // The iteration count is in range, so SASLprep refusing the password is derive's one refusal left.
        let verifier = ScramVerifier::derive(password, salt, iterations)
            .map_err(|_| ClusterError::bootstrap("a password SASLprep accepts"))?;
        self.create_principal(name, verifier, true)
    }

    /// Removes the principal `name` and evicts every session registered for it, so none outlives it; returns the
    /// sessions evicted, oldest first, which the sequencer's next closes carry, and the position it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::unknown_principal`] naming `name`; [`ClusterError::last_administrator`] naming it when it is the
    /// only principal holding the administrator role; [`ClusterError::full`] when its sessions' evictions would pass
    /// [`MetadataState::EVICTIONS_PENDING_MAX`](crate::MetadataState::EVICTIONS_PENDING_MAX); [`ClusterError::log`].
    pub fn remove_principal(&mut self, name: &str) -> Result<(Vec<u128>, Position), ClusterError> {
        let id = self
            .head
            .principal(name)
            .map(|held| held.id())
            .ok_or_else(|| ClusterError::unknown_principal(name))?;
        let evicts: Vec<u128> = self
            .head
            .sessions
            .iter()
            .filter(|(_, owner)| *owner == id)
            .map(|(session, _)| *session)
            .collect();
        let position = self.append(MetadataCommand::RemovePrincipal {
            name: name.to_string(),
            evicts: evicts.clone(),
        })?;
        Ok((evicts, position))
    }

    /// Registers `session` for the principal `principal`, evicting the least recently registered sessions when the
    /// registry holds `sessions_max`, at most 4,096 at once; returns the sessions evicted, which the sequencer's next
    /// close carries, and the position it was appended at. The registration is answered once a sync makes that position
    /// durable.
    ///
    /// # Errors
    ///
    /// [`ClusterError::unknown_principal`] naming `principal`; [`ClusterError::session_exists`] naming `session`;
    /// [`ClusterError::full`] when the evictions awaiting a close would pass
    /// [`MetadataState::EVICTIONS_PENDING_MAX`](crate::MetadataState::EVICTIONS_PENDING_MAX); [`ClusterError::log`].
    pub fn register_session(
        &mut self,
        session: u128,
        principal: &str,
    ) -> Result<(Vec<u128>, Position), ClusterError> {
        let id = self
            .head
            .principal(principal)
            .map(|held| held.id())
            .ok_or_else(|| ClusterError::unknown_principal(principal))?;
        let max = self.config.sessions_max() as usize;
        let over = (self.head.sessions() + 1).saturating_sub(max);
        let evicts: Vec<u128> = self
            .head
            .sessions
            .iter()
            .take(over.min(MetadataCommand::EVICTIONS_MAX))
            .map(|(held, _)| *held)
            .collect();
        let position = self.append(MetadataCommand::RegisterSession {
            session,
            principal: id,
            evicts: evicts.clone(),
        })?;
        Ok((evicts, position))
    }
}
