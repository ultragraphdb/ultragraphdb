/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::metadata_command::MetadataCommand;

impl fmt::Debug for MetadataCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OfferCatalog {
                tenant,
                predecessor,
                bytes,
            } => f
                .debug_struct("OfferCatalog")
                .field("tenant", tenant)
                .field("predecessor", predecessor)
                .field("bytes", bytes)
                .finish(),
            Self::RecordDefault {
                tenant,
                predicate,
                entry,
            } => f
                .debug_struct("RecordDefault")
                .field("tenant", tenant)
                .field("predicate", predicate)
                .field("entry", entry)
                .finish(),
            Self::LeaseBlock {
                prefix,
                partition,
                start,
                end,
            } => f
                .debug_struct("LeaseBlock")
                .field("prefix", prefix)
                .field("partition", partition)
                .field("start", start)
                .field("end", end)
                .finish(),
            Self::InitPlacement { partitions, leader } => f
                .debug_struct("InitPlacement")
                .field("partitions", partitions)
                .field("leader", leader)
                .finish(),
            Self::ChangePlacement { assignments } => f
                .debug_struct("ChangePlacement")
                .field("assignments", assignments)
                .finish(),
            Self::SetLeader { partition, node } => f
                .debug_struct("SetLeader")
                .field("partition", partition)
                .field("node", node)
                .finish(),
            Self::CreatePrincipal { .. }
            | Self::RemovePrincipal { .. }
            | Self::RegisterSession { .. }
            | Self::LearnClose { .. }
            | Self::ReportUntrimmed { .. }
            | Self::CreateTenant { .. }
            | Self::DrawSecret { .. }
            | Self::RemoveDefaults { .. }
            | Self::Chunk { .. }
            | Self::Commit { .. } => self.fmt_rest(f),
            Self::OfferChecked { .. } | Self::OfferRestoring { .. } | Self::ReportCheck { .. } => {
                self.fmt_check(f)
            }
        }
    }
}

impl MetadataCommand {
    /// The `Debug` of the access and epoch commands: [`MetadataCommand::DrawSecret`]'s secret never printed.
    fn fmt_rest(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreatePrincipal {
                id,
                name,
                verifier,
                administrator,
            } => f
                .debug_struct("CreatePrincipal")
                .field("id", id)
                .field("name", name)
                .field("verifier", verifier)
                .field("administrator", administrator)
                .finish(),
            Self::RemovePrincipal { name, evicts } => f
                .debug_struct("RemovePrincipal")
                .field("name", name)
                .field("evicts", evicts)
                .finish(),
            Self::RegisterSession {
                session,
                principal,
                evicts,
            } => f
                .debug_struct("RegisterSession")
                .field("session", session)
                .field("principal", principal)
                .field("evicts", evicts)
                .finish(),
            Self::LearnClose {
                epoch,
                versions,
                placement,
                evictions,
            } => f
                .debug_struct("LearnClose")
                .field("epoch", epoch)
                .field("versions", versions)
                .field("placement", placement)
                .field("evictions", evictions)
                .finish(),
            Self::ReportUntrimmed { .. }
            | Self::CreateTenant { .. }
            | Self::DrawSecret { .. }
            | Self::RemoveDefaults { .. }
            | Self::Chunk { .. }
            | Self::Commit { .. } => self.fmt_small(f),
            Self::OfferCatalog { .. }
            | Self::RecordDefault { .. }
            | Self::LeaseBlock { .. }
            | Self::InitPlacement { .. }
            | Self::ChangePlacement { .. }
            | Self::OfferChecked { .. }
            | Self::OfferRestoring { .. }
            | Self::ReportCheck { .. }
            // fmt writes these itself and never passes them here.
            | Self::SetLeader { .. } => Ok(()),
        }
    }

    /// The `Debug` of the commands of one or two fields, the secret's among them.
    fn fmt_small(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReportUntrimmed { partition, epoch } => f
                .debug_struct("ReportUntrimmed")
                .field("partition", partition)
                .field("epoch", epoch)
                .finish(),
            Self::CreateTenant { tenant } => f
                .debug_struct("CreateTenant")
                .field("tenant", tenant)
                .finish(),
            Self::DrawSecret { .. } => f
                .debug_struct("DrawSecret")
                .field("secret", &format_args!("<redacted>"))
                .finish(),
            Self::Chunk { offset, bytes } => f
                .debug_struct("Chunk")
                .field("offset", offset)
                .field("bytes", bytes)
                .finish(),
            Self::Commit { length, checksum } => f
                .debug_struct("Commit")
                .field("length", length)
                .field("checksum", checksum)
                .finish(),
            Self::RemoveDefaults { tenant } => f
                .debug_struct("RemoveDefaults")
                .field("tenant", tenant)
                .finish(),
            Self::OfferCatalog { .. }
            | Self::RecordDefault { .. }
            | Self::LeaseBlock { .. }
            | Self::InitPlacement { .. }
            | Self::ChangePlacement { .. }
            | Self::SetLeader { .. }
            | Self::CreatePrincipal { .. }
            | Self::RemovePrincipal { .. }
            | Self::RegisterSession { .. }
            | Self::LearnClose { .. }
            | Self::OfferChecked { .. }
            | Self::OfferRestoring { .. }
            // fmt_rest passes the other six here alone.
            | Self::ReportCheck { .. } => Ok(()),
        }
    }

    /// The `Debug` of a tightening's commands.
    fn fmt_check(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OfferChecked {
                tenant,
                predecessor,
                bytes,
            } => f
                .debug_struct("OfferChecked")
                .field("tenant", tenant)
                .field("predecessor", predecessor)
                .field("bytes", bytes)
                .finish(),
            Self::OfferRestoring {
                tenant,
                predecessor,
                restores,
                bytes,
            } => f
                .debug_struct("OfferRestoring")
                .field("tenant", tenant)
                .field("predecessor", predecessor)
                .field("restores", restores)
                .field("bytes", bytes)
                .finish(),
            Self::ReportCheck {
                tenant,
                version,
                partition,
                refused,
            } => f
                .debug_struct("ReportCheck")
                .field("tenant", tenant)
                .field("version", version)
                .field("partition", partition)
                .field("refused", refused)
                .finish(),
            Self::OfferCatalog { .. }
            | Self::RecordDefault { .. }
            | Self::LeaseBlock { .. }
            | Self::InitPlacement { .. }
            | Self::ChangePlacement { .. }
            | Self::SetLeader { .. }
            | Self::CreatePrincipal { .. }
            | Self::RemovePrincipal { .. }
            | Self::RegisterSession { .. }
            | Self::LearnClose { .. }
            | Self::ReportUntrimmed { .. }
            | Self::CreateTenant { .. }
            | Self::DrawSecret { .. }
            | Self::RemoveDefaults { .. }
            | Self::Chunk { .. }
            // fmt passes the other three here alone.
            | Self::Commit { .. } => Ok(()),
        }
    }
}
