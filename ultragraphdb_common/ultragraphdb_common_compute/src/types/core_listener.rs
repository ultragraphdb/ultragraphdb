/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixListener;

use ultragraphdb_common_service::bind_tcp;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Scheme};
use ultragraphdb_protocol_transport_compio::Listener;

/// Mode 0600: a role's socket opens to the user the roles run as (`PROTOCOL.md` §11.12).
const SOCKET_MODE: u32 = 0o600;

/// The socket a compute process listens at, held by every core: each core accepts on its own runtime, so a connection lands
/// on one core and its reads stay there.
#[derive(Debug)]
pub enum CoreListener {
    /// Listening at `tcp://host:port`.
    Tcp(std::net::TcpListener),
    /// Listening at `unix:///path`.
    Unix(UnixListener),
}

impl CoreListener {
    /// Listens at the role address `text`, a Unix socket a process before this one left behind replaced, a new one set to mode
    /// 0600.
    ///
    /// A future dropped at the TCP address's retries leaves nothing bound.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] for an address that does not parse or a socket that cannot be made at its path;
    /// [`RoleError::unavailable`] when a TCP address cannot be listened at, within [`bind_tcp`]'s retries.
    pub async fn bind(text: &str) -> Result<Self, RoleError> {
        let address = Address::parse(text)
            .map_err(|error| RoleError::configuration(format!("the address {text}: {error}")))?;
        match address.scheme() {
            Scheme::Tcp => bind_tcp(address.target()).await.map(Self::Tcp),
            Scheme::Unix => bind_unix(text, address.target()).map(Self::Unix),
        }
    }

    /// A handle of the same socket, for another core.
    ///
    /// # Errors
    ///
    /// [`RoleError::unavailable`] when the operating system gives none.
    pub fn another(&self) -> Result<Self, RoleError> {
        let cloned = match self {
            Self::Tcp(listener) => listener.try_clone().map(Self::Tcp),
            Self::Unix(listener) => listener.try_clone().map(Self::Unix),
        };
        cloned.map_err(|error| RoleError::unavailable(format!("the role listener: {error}")))
    }

    /// The socket as this runtime's listener, which is why it is called on the core that accepts on it.
    ///
    /// # Errors
    ///
    /// [`RoleError::unavailable`] when the runtime cannot take it.
    pub fn on_this_core(self) -> Result<Listener, RoleError> {
        let taken = match self {
            Self::Tcp(listener) => compio::net::TcpListener::from_std(listener).map(Listener::Tcp),
            Self::Unix(listener) => {
                compio::net::UnixListener::from_std(listener).map(Listener::Unix)
            }
        };
        taken.map_err(|error| RoleError::unavailable(format!("the role listener: {error}")))
    }
}

/// A Unix socket at `path`, of the address `text`.
fn bind_unix(text: &str, path: &str) -> Result<UnixListener, RoleError> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let refused = |why: &dyn std::fmt::Display| {
        RoleError::configuration(format!(
            "listening at {text}: {why}. The directory of {path} must exist and be writable by this process's user, and no file stand there"
        ))
    };
    let stale = std::fs::symlink_metadata(path).is_ok_and(|file| file.file_type().is_socket());
    if stale {
        std::fs::remove_file(path).map_err(|error| refused(&error))?;
    }
    let listener = UnixListener::bind(path).map_err(|error| refused(&error))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))
        .map_err(|error| refused(&error))?;
    Ok(listener)
}
