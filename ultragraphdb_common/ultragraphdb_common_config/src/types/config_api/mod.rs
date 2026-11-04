/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use ultragraphdb_transport_http_server::{BearerKey, HttpRequest, HttpResponse, HttpService};

use crate::errors::config_error::{ConfigError, ConfigErrorEnum};
use crate::types::config_command::ConfigCommand;
use crate::types::config_handle::ConfigHandle;

/// What a presented key may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Nothing,
    Read,
    Admin,
}

/// The config service's HTTP API. A clone shares the keys: the runtime is one thread, so `Rc` holds them.
#[derive(Debug, Clone)]
pub struct ConfigApi {
    store: ConfigHandle,
    read_key: Rc<BearerKey>,
    admin_key: Rc<BearerKey>,
}

impl ConfigApi {
    /// The API over `store`, authorising `read_key` to read and `admin_key` to read and change.
    #[must_use]
    pub fn new(store: ConfigHandle, read_key: BearerKey, admin_key: BearerKey) -> Self {
        Self {
            store,
            read_key: Rc::new(read_key),
            admin_key: Rc::new(admin_key),
        }
    }

    /// Both keys are compared whatever the first says, so the time taken names neither.
    fn scope(&self, presented: Option<&str>) -> Scope {
        let admin = self.admin_key.allows(presented);
        let read = self.read_key.allows(presented);
        if admin {
            Scope::Admin
        } else if read {
            Scope::Read
        } else {
            Scope::Nothing
        }
    }

    async fn targets(&self) -> HttpResponse {
        let Some(targets) = self.store.ask(ConfigCommand::Targets).await else {
            return stopped();
        };
        let lines: String = targets
            .iter()
            .map(|target| {
                let active = target
                    .active()
                    .map_or("none".to_string(), |number| number.to_string());
                let retired = if target.retired() { " retired" } else { "" };
                format!(
                    "{} latest {} active {active}{retired}\n",
                    target.name(),
                    target.latest()
                )
            })
            .collect();
        text(200, lines)
    }

    async fn instance(&self, target: &str, revision: Option<u64>, instance: &str) -> HttpResponse {
        let answer = self
            .store
            .ask(|reply| ConfigCommand::Instance {
                target: target.to_string(),
                revision,
                instance: instance.to_string(),
                reply,
            })
            .await;
        match answer {
            None => stopped(),
            Some(Err(error)) => refusal(&error),
            Some(Ok((number, rendered))) => HttpResponse::new(200)
                .with_header("Content-Type", "application/toml")
                .with_header("X-Revision", number.to_string())
                .with_body(rendered),
        }
    }

    async fn files(&self, target: &str, revision: u64) -> HttpResponse {
        let answer = self
            .store
            .ask(|reply| ConfigCommand::Files {
                target: target.to_string(),
                revision,
                reply,
            })
            .await;
        match answer {
            None => stopped(),
            Some(Err(error)) => refusal(&error),
            Some(Ok(files)) => HttpResponse::new(200)
                .with_header("X-Revision", revision.to_string())
                .with_body(files),
        }
    }

    /// The active revision's files as a file to save, which `add` takes back as they are.
    async fn download(&self, target: &str) -> HttpResponse {
        let answer = self
            .store
            .ask(|reply| ConfigCommand::Download {
                target: target.to_string(),
                reply,
            })
            .await;
        match answer {
            None => stopped(),
            Some(Err(error)) => refusal(&error),
            Some(Ok((number, files))) => HttpResponse::new(200)
                .with_header("Content-Type", "application/octet-stream")
                .with_header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{target}-revision-{number}.bundle\""),
                )
                .with_header("X-Revision", number.to_string())
                .with_body(files),
        }
    }

    async fn add(&self, target: &str, request: HttpRequest) -> HttpResponse {
        let Some(base) = request.query("base").and_then(|base| base.parse().ok()) else {
            return text(
                400,
                "base must name, in decimal, the latest revision this was edited from; 0 begins a target\n",
            );
        };
        let target = target.to_string();
        let body = request.into_body();
        let answer = self
            .store
            .ask(|reply| ConfigCommand::Add {
                target,
                base,
                body,
                reply,
            })
            .await;
        match answer {
            None => stopped(),
            Some(Err(error)) => refusal(&error),
            Some(Ok(number)) => HttpResponse::new(201)
                .with_header("X-Revision", number.to_string())
                .with_body(format!("revision {number}\n")),
        }
    }

    async fn activate(&self, target: &str, body: &[u8]) -> HttpResponse {
        let revision = std::str::from_utf8(body)
            .ok()
            .and_then(|number| number.trim().parse().ok());
        let Some(revision) = revision else {
            return text(400, "the body must hold a revision's number in decimal\n");
        };
        let answer = self
            .store
            .ask(|reply| ConfigCommand::Activate {
                target: target.to_string(),
                revision,
                reply,
            })
            .await;
        match answer {
            None => stopped(),
            Some(Err(error)) => refusal(&error),
            Some(Ok(())) => text(200, format!("revision {revision} is active\n")),
        }
    }

    async fn retire(&self, target: &str) -> HttpResponse {
        let answer = self
            .store
            .ask(|reply| ConfigCommand::Retire {
                target: target.to_string(),
                reply,
            })
            .await;
        match answer {
            None => stopped(),
            Some(Err(error)) => refusal(&error),
            Some(Ok(())) => text(200, format!("target {target} is retired\n")),
        }
    }

    async fn route(&self, request: HttpRequest) -> HttpResponse {
        let scope = self.scope(request.bearer());
        if scope == Scope::Nothing {
            return HttpResponse::new(401)
                .with_header("WWW-Authenticate", "Bearer")
                .with_body("a key is required\n");
        }
        let method = request.method().to_string();
        let path = request.path().to_string();
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let needs_admin = !matches!(
            (method.as_str(), parts.as_slice()),
            ("GET", ["v1", "targets"])
                | ("GET", ["v1", "targets", _, "instances", _])
                | ("GET", ["v1", "targets", _, "revisions", _, "instances", _])
        );
        if needs_admin && scope != Scope::Admin {
            return text(403, "this request requires the admin key\n");
        }
        match (method.as_str(), parts.as_slice()) {
            ("GET", ["v1", "targets"]) => self.targets().await,
            ("GET", ["v1", "targets", target, "instances", instance]) => {
                self.instance(target, None, instance).await
            }
            (
                "GET",
                [
                    "v1",
                    "targets",
                    target,
                    "revisions",
                    number,
                    "instances",
                    instance,
                ],
            ) => match number.parse() {
                Ok(number) => self.instance(target, Some(number), instance).await,
                Err(_) => not_a_number(number),
            },
            ("GET", ["v1", "targets", target, "revisions", number]) => match number.parse() {
                Ok(number) => self.files(target, number).await,
                Err(_) => not_a_number(number),
            },
            ("GET", ["v1", "targets", target, "download"]) => self.download(target).await,
            ("POST", ["v1", "targets", target, "revisions"]) => self.add(target, request).await,
            ("PUT", ["v1", "targets", target, "active"]) => {
                self.activate(target, request.body()).await
            }
            ("DELETE", ["v1", "targets", target]) => self.retire(target).await,
            _ => text(404, format!("no such request: {method} {path}\n")),
        }
    }
}

impl HttpService for ConfigApi {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        match (request.method(), request.path()) {
            ("GET", "/health" | "/ready") => HttpResponse::new(200).with_body("ok\n"),
            _ => self.route(request).await,
        }
    }
}

fn text(status: u16, body: impl Into<Vec<u8>>) -> HttpResponse {
    HttpResponse::new(status)
        .with_header("Content-Type", "text/plain")
        .with_body(body)
}

fn not_a_number(number: &str) -> HttpResponse {
    text(400, format!("{number} is not a revision's number\n"))
}

/// The owner ended, which only a failed log does while the process lives.
fn stopped() -> HttpResponse {
    text(503, "the store has stopped\n")
}

/// `error` as a status and its one clear text.
fn refusal(error: &ConfigError) -> HttpResponse {
    let status = match &error.0 {
        ConfigErrorEnum::UnknownTarget { .. }
        | ConfigErrorEnum::Retired { .. }
        | ConfigErrorEnum::NoActiveRevision { .. }
        | ConfigErrorEnum::UnknownRevision { .. }
        | ConfigErrorEnum::UnknownInstance { .. } => 404,
        ConfigErrorEnum::Stale { .. } => 409,
        ConfigErrorEnum::Refused { .. } => 422,
        ConfigErrorEnum::MalformedRecord { .. }
        | ConfigErrorEnum::ReplayDiverged { .. }
        | ConfigErrorEnum::Log(_) => 503,
    };
    text(status, format!("{error}\n"))
}
