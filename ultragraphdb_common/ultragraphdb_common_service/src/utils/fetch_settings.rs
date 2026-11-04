/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ultragraphdb_error_role::RoleError;

use crate::types::bootstrap::Bootstrap;
use crate::types::fetch_failure::FetchFailure;
use crate::types::role_identity::RoleIdentity;
use crate::types::role_start::RoleStart;

/// How long a role waits between asking an unreachable service again.
const RETRY: Duration = Duration::from_millis(200);

/// The instance's settings at the revision a load named, else the active one, from the service `bootstrap` names: `None` when
/// `stop` is set while waiting. An unreachable service is asked again until `patience` has passed. A refusal is not, since
/// waiting mends neither a key refused nor a target or instance unknown.
///
/// # Errors
///
/// [`RoleError::unavailable`] naming the URL, the target, the instance and the last failure, past `patience`;
/// [`RoleError::configuration`] for a refusal, or settings this release does not hold.
pub async fn fetch_settings(
    bootstrap: &Bootstrap,
    patience: Duration,
    stop: &AtomicBool,
) -> Result<Option<RoleStart>, RoleError> {
    let due = now() + patience;
    while !stop.load(Ordering::Acquire) {
        match fetch_once(bootstrap, bootstrap.revision()).await {
            Ok(start) => return Ok(Some(start)),
            Err(FetchFailure::Refused(error)) => return Err(error),
            Err(FetchFailure::Unreachable(why)) => {
                tracing::warn!(%why, "the config service is not answering");
                if now() >= due {
                    return Err(RoleError::unavailable(format!(
                        "{why}, for {} s",
                        patience.as_secs()
                    )));
                }
            }
        }
        compio::time::sleep(RETRY).await;
    }
    Ok(None)
}

/// One asking for the instance's settings at `revision`, the active one when it is `None`, checked as a start checks them.
pub(crate) async fn fetch_once(
    bootstrap: &Bootstrap,
    revision: Option<u64>,
) -> Result<RoleStart, FetchFailure> {
    let (target, instance) = (bootstrap.target(), bootstrap.instance());
    let url = bootstrap.client().url().as_str();
    let path = match revision {
        Some(number) => format!("/v1/targets/{target}/revisions/{number}/instances/{instance}"),
        None => format!("/v1/targets/{target}/instances/{instance}"),
    };
    let asked = format!("target {target}, instance {instance}, from the config service at {url}");
    let refused = |why: String| FetchFailure::Refused(RoleError::configuration(why));
    match bootstrap.client().request("GET", &path, b"").await {
        Ok(reply) if reply.status() == 200 => {
            let number = reply
                .header("X-Revision")
                .and_then(|number| number.parse().ok())
                .ok_or_else(|| refused(format!("the settings of {asked} name no revision")))?;
            let identity = RoleIdentity::new(target, instance, number);
            RoleStart::from_settings(reply.body(), identity)
                .map(|start| start.with_bootstrap(bootstrap.clone()))
                .map_err(FetchFailure::Refused)
        }
        Ok(reply) => Err(refused(format!(
            "the settings of {asked} were refused with status {}: {}",
            reply.status(),
            String::from_utf8_lossy(reply.body()).trim_end()
        ))),
        Err(error) if error.is_unreachable() => Err(FetchFailure::Unreachable(format!(
            "the settings of {asked} could not be read: {error}"
        ))),
        Err(error) => Err(refused(format!(
            "the settings of {asked} could not be read: {error}"
        ))),
    }
}

/// The monotonic clock, for a deadline that leaves no trace in what is decided.
#[expect(
    clippy::disallowed_methods,
    reason = "a role's start waits on real time, which no simulation drives"
)]
fn now() -> Instant {
    Instant::now()
}
