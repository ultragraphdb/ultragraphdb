/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Scheme};

/// The role address of `partition`, the process's partitions beginning at `first`, by the process's `[storage] role_address`
/// `base`: over TCP, the base's port plus the partition's place among them; over a Unix socket, the base's path with
/// `-p<partition>` before its extension (`storage.sock` gives `storage-p1.sock`).
///
/// # Errors
///
/// [`RoleError::configuration`] for a base that does not parse, or a port past the last.
pub fn partition_address(base: &str, first: u32, partition: u32) -> Result<String, RoleError> {
    let refused = |why: String| RoleError::configuration(format!("the address {base}: {why}"));
    let parsed = Address::parse(base).map_err(|error| refused(error.to_string()))?;
    match parsed.scheme() {
        Scheme::Tcp => {
            let target = parsed.target();
            let (host, port) = target
                .rsplit_once(':')
                .ok_or_else(|| refused("no port".to_string()))?;
            let port: u32 = port.parse().map_err(|_| refused("no port".to_string()))?;
            let port = port
                .checked_add(partition.saturating_sub(first))
                .filter(|port| *port <= u32::from(u16::MAX))
                .ok_or_else(|| refused(format!("no port for partition {partition}")))?;
            Ok(format!("tcp://{host}:{port}"))
        }
        Scheme::Unix => {
            let path = parsed.target();
            let (stem, extension) = path.rsplit_once('.').unwrap_or((path, ""));
            let dotted = if extension.is_empty() {
                String::new()
            } else {
                format!(".{extension}")
            };
            Ok(format!("unix://{stem}-p{partition}{dotted}"))
        }
    }
}
