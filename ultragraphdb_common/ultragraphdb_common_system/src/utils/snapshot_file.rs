/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::Path;

use ultragraphdb_error_role::RoleError;

/// The snapshot at `path`, when one was stored.
pub(crate) fn load(path: &Path) -> Result<Option<Vec<u8>>, RoleError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(path, &error)),
    }
}

/// Stores `bytes` at `path` whole or not at all, durable before it returns: written beside it, synced, renamed over it,
/// and its directory synced, so the log it lets a caller trim is never trimmed past a snapshot a crash loses.
pub(crate) fn store(path: &Path, bytes: &[u8]) -> Result<(), RoleError> {
    use std::io::Write;

    let failed_at = |error: std::io::Error| failed(path, &error);
    let beside = path.with_extension("next");
    let mut file = std::fs::File::create(&beside).map_err(failed_at)?;
    file.write_all(bytes).map_err(failed_at)?;
    file.sync_all().map_err(failed_at)?;
    std::fs::rename(&beside, path).map_err(failed_at)?;
    if let Some(directory) = path.parent() {
        std::fs::File::open(directory)
            .and_then(|handle| handle.sync_all())
            .map_err(failed_at)?;
    }
    Ok(())
}

fn failed(path: &Path, error: &std::io::Error) -> RoleError {
    RoleError::io(format!("the snapshot {}: {error}", path.display()))
}
