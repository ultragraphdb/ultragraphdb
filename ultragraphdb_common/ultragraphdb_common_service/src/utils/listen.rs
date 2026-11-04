/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_transport_compio::Listener;

/// Mode 0600: a role's socket opens to the user the roles run as (`PROTOCOL.md` §11.12).
const SOCKET_MODE: u32 = 0o600;

/// Listens at the role address `text`. A Unix socket a process before this one left behind is replaced, and the new one
/// is set to mode 0600.
///
/// # Errors
///
/// [`RoleError::configuration`] for an address that does not parse, and [`RoleError::unavailable`] when it cannot be
/// listened at.
pub async fn listen(text: &str) -> Result<Listener, RoleError> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let address = Address::parse(text)
        .map_err(|error| RoleError::configuration(format!("the address {text}: {error}")))?;
    let unavailable =
        |why: &dyn std::fmt::Display| RoleError::unavailable(format!("listening at {text}: {why}"));
    let path = address.path();
    if let Some(path) = path {
        let stale = std::fs::symlink_metadata(path).is_ok_and(|file| file.file_type().is_socket());
        if stale {
            std::fs::remove_file(path).map_err(|error| unavailable(&error))?;
        }
    }
    let listener = Listener::bind(&address)
        .await
        .map_err(|error| unavailable(&error))?;
    if let Some(path) = path {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))
            .map_err(|error| unavailable(&error))?;
    }
    Ok(listener)
}
