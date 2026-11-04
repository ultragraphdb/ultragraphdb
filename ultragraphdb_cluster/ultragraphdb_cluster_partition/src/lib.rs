/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition of a storage process (partitions): its bucket ranges, its applier over its own store, its routing, and
//! P24's one owner per fact between partitions — registration of references at an entity's home, the home's referrer
//! sets, and reverse reads through them at a closed epoch.

mod errors;
mod types;

pub use crate::errors::partition_error::{PartitionError, PartitionErrorEnum};
pub use crate::types::answer::Answer;
pub use crate::types::fanout::Fanout;
pub use crate::types::home::Home;
pub use crate::types::partition::Partition;
pub use crate::types::partition_host::PartitionHost;
pub use crate::types::partition_mailbox::PartitionMailbox;
pub use crate::types::partition_message::PartitionMessage;
pub use crate::types::partition_outcome::PartitionOutcome;
pub use crate::types::reference::Reference;
pub use crate::types::reverse_query::ReverseQuery;
