/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The role address of `partition`, the process's partitions beginning at `first`, by the process's `[storage] role_address`
/// `base`: over TCP, the base's port plus the partition's place among them; over a Unix socket, the base's path with
/// `-p<partition>` before its extension (`storage.sock` gives `storage-p1.sock`).
///
/// # Errors
///
/// What is wrong with the base, for one that is neither form, or a port past the last.
pub fn partition_address(base: &str, first: u32, partition: u32) -> Result<String, String> {
    if let Some(target) = base.strip_prefix("tcp://") {
        let (host, port) = target
            .rsplit_once(':')
            .ok_or_else(|| format!("{base} names no port"))?;
        let port: u32 = port.parse().map_err(|_| format!("{base} names no port"))?;
        let port = port
            .checked_add(partition.saturating_sub(first))
            .filter(|port| *port <= u32::from(u16::MAX))
            .ok_or_else(|| format!("{base} leaves no port for partition {partition}"))?;
        return Ok(format!("tcp://{host}:{port}"));
    }
    let path = base
        .strip_prefix("unix://")
        .ok_or_else(|| format!("{base} is neither tcp://host:port nor unix:///path"))?;
    let (stem, extension) = path.rsplit_once('.').unwrap_or((path, ""));
    let dotted = if extension.is_empty() {
        String::new()
    } else {
        format!(".{extension}")
    };
    Ok(format!("unix://{stem}-p{partition}{dotted}"))
}
