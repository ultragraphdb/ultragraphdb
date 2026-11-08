/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_transport_compio::Listener;

/// Mode 0600: a role's socket opens to the user the roles run as (`PROTOCOL.md` §11.12).
const SOCKET_MODE: u32 = 0o600;

/// How long a role waits between two asks for a TCP port in use.
const BIND_RETRY: Duration = Duration::from_millis(50);

/// How many times a role asks again for a TCP port in use: 5 s of `BIND_RETRY`. A killed process holds its ports until the kernel
/// tears down its io_uring ring, measured at no more than 15 ms past its reaping (`notes/group-11.md`).
pub const BIND_RETRIES: u32 = 100;

/// Binds the TCP address `address`, `host:port`. A port in use is asked for again every 50 ms for up to 5 s, each retry logged:
/// a process killed moments ago holds its ports until the kernel has torn down its io_uring ring. Every role binds its TCP addresses
/// here.
///
/// A future dropped at its pause between retries leaves nothing bound.
///
/// # Errors
///
/// [`RoleError::unavailable`] naming the address, at once for anything but a port in use, and for that past the last retry.
pub async fn bind_tcp(address: &str) -> Result<TcpListener, RoleError> {
    let mut retries = 0;
    loop {
        match TcpListener::bind(address) {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() != ErrorKind::AddrInUse => {
                return Err(RoleError::unavailable(format!(
                    "listening at {address}: {error}"
                )));
            }
            Err(error) if retries == BIND_RETRIES => {
                return Err(RoleError::unavailable(format!(
                    "listening at {address}: {error}, through {BIND_RETRIES} retries, one every 50 ms. Another process holds the port: stop it, or name another address in the setting"
                )));
            }
            Err(_) => {
                retries += 1;
                tracing::warn!(%address, retry = retries, of = BIND_RETRIES, "the port is in use, as a process killed moments ago holds it until the kernel has torn down its io_uring ring: asking again in 50 ms");
                compio::time::sleep(BIND_RETRY).await;
            }
        }
    }
}

/// Listens at the role address `text`. A Unix socket a process before this one left behind is replaced, and the new one
/// is set to mode 0600; a TCP address takes [`bind_tcp`]'s retries.
///
/// A future dropped at its bind leaves nothing listening, and at most a socket file at the path, which the next `listen`
/// at that path replaces.
///
/// # Errors
///
/// [`RoleError::configuration`] for an address that does not parse or a Unix socket that cannot be made at its path, which says
/// what to do; [`RoleError::unavailable`] when a TCP address cannot be listened at.
pub async fn listen(text: &str) -> Result<Listener, RoleError> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let address = Address::parse(text)
        .map_err(|error| RoleError::configuration(format!("the address {text}: {error}")))?;
    let Some(path) = address.path() else {
        let listener = compio::net::TcpListener::from_std(bind_tcp(address.target()).await?)
            .map_err(|error| RoleError::unavailable(format!("listening at {text}: {error}")))?;
        return Ok(Listener::Tcp(listener));
    };
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let unavailable = |why: &dyn std::fmt::Display| cannot_listen(text, path, why);
    let stale = std::fs::symlink_metadata(path).is_ok_and(|file| file.file_type().is_socket());
    if stale {
        std::fs::remove_file(path).map_err(|error| unavailable(&error))?;
    }
    let listener = Listener::bind(&address)
        .await
        .map_err(|error| unavailable(&error))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))
        .map_err(|error| unavailable(&error))?;
    Ok(listener)
}

/// Why `text`, a Unix socket at `path`, cannot be listened at: its path is the deployment's to lay out, so this is a configuration
/// error saying how.
fn cannot_listen(text: &str, path: &Path, why: &(impl std::fmt::Display + ?Sized)) -> RoleError {
    use std::os::unix::fs::FileTypeExt;

    let stands = std::fs::symlink_metadata(path).is_ok_and(|file| !file.file_type().is_socket());
    if stands {
        return RoleError::configuration(format!(
            "listening at {text}: {why}. {} is a file and no socket: remove it, or name another address in the setting",
            path.display()
        ));
    }
    let directory = path.parent().unwrap_or(path);
    RoleError::configuration(format!(
        "listening at {text}: {why}. The directory {} must exist and be writable by this process's user: create it, or name another address in the setting",
        directory.display()
    ))
}
