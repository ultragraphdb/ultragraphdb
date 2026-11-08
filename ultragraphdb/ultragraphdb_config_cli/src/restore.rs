/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::Path;

use compio::runtime::Runtime;
use ultragraphdb_common_config_check::{TargetFiles, check_central, write_central};
use ultragraphdb_transport_http_client::{HttpClient, HttpReply, HttpUrl};
use ultragraphdb_transport_tls::TlsTrust;

/// `sysexits`: a service is unavailable.
const EX_UNAVAILABLE: u8 = 69;
/// `sysexits`: an I/O error.
const EX_IOERR: u8 = 74;
/// `sysexits`: a configuration error.
const EX_CONFIG: u8 = 78;

/// Why a restore stopped: the code to exit with, and what to tell the administrator.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) code: u8,
    pub(crate) message: String,
}

fn failure(code: u8, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}

/// What a restore asks for.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Restore<'a> {
    pub(crate) url: &'a str,
    pub(crate) token: &'a str,
    pub(crate) folder: &'a Path,
    pub(crate) ca_file: Option<&'a str>,
}

/// A listed target: its name, and its latest revision and what it says is active, unless it is retired.
type Listed<'a> = (&'a str, Option<(u64, &'a str)>);

/// One line of `GET /v1/targets`: the target's name, and the latest revision and active revision it holds, or nothing for a
/// retired target, whose settings no one runs, so a folder restored from them would begin it again.
fn listed(line: &str) -> Result<Listed<'_>, String> {
    let fields: Vec<&str> = line.split(' ').collect();
    match fields.as_slice() {
        [name, "latest", latest, "active", active, rest @ ..] => {
            if !rest.is_empty() {
                return Ok((name, None));
            }
            let number = latest
                .parse()
                .map_err(|_| format!("the listing names {latest} as a revision"))?;
            Ok((name, Some((number, *active))))
        }
        _ => Err(format!("the listing holds the line {line}")),
    }
}

/// A reply's body as text, for an error.
fn said(reply: &HttpReply) -> String {
    String::from_utf8_lossy(reply.body()).trim_end().to_string()
}

fn client(restore: Restore<'_>) -> Result<HttpClient, Failure> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let refused = |what: &dyn std::fmt::Display| failure(EX_CONFIG, what.to_string());
    let url = HttpUrl::parse(restore.url).map_err(|error| refused(&error))?;
    let trust = match (url.is_secure(), restore.ca_file) {
        (false, _) => None,
        (true, Some(file)) => Some(TlsTrust::from_pem_file(file).map_err(|error| refused(&error))?),
        (true, None) => Some(TlsTrust::system().map_err(|error| refused(&error))?),
    };
    HttpClient::new(url, trust, restore.token.to_string()).map_err(|error| refused(&error))
}

/// Asks `method` of `path`: the reply, 200 alone being one.
///
/// A future dropped at the request drops the connection with it; a GET changes nothing on the service.
async fn asked(client: &HttpClient, path: &str, what: &str) -> Result<HttpReply, Failure> {
    let url = client.url().as_str();
    let reply = client.request("GET", path, b"").await.map_err(|error| {
        let code = if error.is_unreachable() {
            EX_UNAVAILABLE
        } else {
            EX_CONFIG
        };
        failure(code, format!("reading {what}: {error}"))
    })?;
    match reply.status() {
        200 => Ok(reply),
        401 | 403 => Err(failure(
            EX_CONFIG,
            format!("{url} refused the token reading {what}: {}", said(&reply)),
        )),
        status => Err(failure(
            EX_CONFIG,
            format!(
                "{url} refused reading {what} with status {status}: {}",
                said(&reply)
            ),
        )),
    }
}

/// Reads the latest revision of every target not retired, then writes them: what to tell the administrator.
pub(crate) fn run_restore(restore: Restore<'_>) -> Result<Vec<String>, Failure> {
    let client = client(restore)?;
    let runtime =
        Runtime::new().map_err(|error| failure(EX_IOERR, format!("the runtime: {error}")))?;
    let mut lines = Vec::new();
    let targets = runtime.block_on(async {
        let listing = asked(&client, "/v1/targets", "the targets").await?;
        let text = String::from_utf8_lossy(listing.body()).into_owned();
        let mut targets = Vec::new();
        for line in text.lines() {
            let (name, held) = listed(line)
                .map_err(|why| failure(EX_CONFIG, format!("{}: {why}", restore.url)))?;
            let Some((latest, active)) = held else {
                lines.push(format!("target {name} is retired, and is left out"));
                continue;
            };
            let reply = asked(
                &client,
                &format!("/v1/targets/{name}/revisions/{latest}"),
                &format!("target {name}"),
            )
            .await?;
            let files = TargetFiles::from_bundle(name, reply.body())
                .map_err(|defect| failure(EX_CONFIG, defect.to_string()))?;
            lines.push(if active == latest.to_string() {
                format!("target {name}: revision {latest}, the latest and the active")
            } else {
                format!(
                    "target {name}: revision {latest}, the latest; the service's active revision is {active}, and a service beginning from this folder makes the latest active"
                )
            });
            targets.push(files);
        }
        Ok::<Vec<TargetFiles>, Failure>(targets)
    })?;
    if targets.is_empty() {
        return Err(failure(
            EX_CONFIG,
            format!(
                "{} holds no target but retired ones, so nothing was written",
                restore.url
            ),
        ));
    }
    let central = write_central(restore.folder, &targets)
        .map_err(|defect| failure(EX_IOERR, defect.to_string()))?;
    let report = check_central(&central);
    if !report.passes() {
        let defects: Vec<String> = report.defects().iter().map(ToString::to_string).collect();
        return Err(failure(
            EX_CONFIG,
            format!(
                "{} was written, and this release refuses it:\n{}",
                central.display(),
                defects.join("\n")
            ),
        ));
    }
    lines.push(format!(
        "{} is restored, and a config service can begin from it",
        central.display()
    ));
    Ok(lines)
}
