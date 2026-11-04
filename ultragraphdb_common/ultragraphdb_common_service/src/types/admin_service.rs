/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;

use ultragraphdb_transport_http_server::{BearerKey, HttpRequest, HttpResponse, HttpService};

use crate::types::bootstrap::Bootstrap;
use crate::types::fetch_failure::FetchFailure;
use crate::utils::fetch_settings::fetch_once;

/// `POST /admin/load-config`: fetches and checks the revision the body names, and only then asks the role to stop and execute
/// itself again at it. A clone shares all of it: the runtime is one thread, so `Rc` holds them.
#[derive(Debug, Clone)]
pub struct AdminService {
    key: Rc<BearerKey>,
    bootstrap: Rc<Bootstrap>,
    load: Rc<Cell<Option<u64>>>,
}

impl AdminService {
    /// The service authorising `key`, fetching through `bootstrap`, and setting `load` to the revision a request takes.
    #[must_use]
    pub fn new(key: BearerKey, bootstrap: Bootstrap, load: Rc<Cell<Option<u64>>>) -> Self {
        Self {
            key: Rc::new(key),
            bootstrap: Rc::new(bootstrap),
            load,
        }
    }
}

impl HttpService for AdminService {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        if (request.method(), request.path()) != ("POST", "/admin/load-config") {
            return text(
                404,
                "the one request here is POST /admin/load-config\n".to_string(),
            );
        }
        if request.bearer().is_none() {
            return text(401, "the admin key is required\n".to_string());
        }
        if !self.key.allows(request.bearer()) {
            return text(403, "this request requires the admin key\n".to_string());
        }
        let revision = std::str::from_utf8(request.body())
            .ok()
            .and_then(|number| number.trim().parse().ok());
        let Some(revision) = revision else {
            return text(
                400,
                "the body must hold a revision's number in decimal\n".to_string(),
            );
        };
        if let Some(loading) = self.load.get() {
            return text(
                409,
                format!("this role is already loading revision {loading}\n"),
            );
        }
        match fetch_once(&self.bootstrap, Some(revision)).await {
            Ok(_) => {
                self.load.set(Some(revision));
                text(202, format!("loading revision {revision}\n"))
            }
            Err(FetchFailure::Unreachable(why)) => text(
                503,
                format!("revision {revision} cannot be loaded, and this role runs on: {why}\n"),
            ),
            Err(FetchFailure::Refused(why)) => text(
                422,
                format!("revision {revision} cannot be loaded, and this role runs on: {why}\n"),
            ),
        }
    }
}

fn text(status: u16, body: String) -> HttpResponse {
    HttpResponse::new(status)
        .with_header("Content-Type", "text/plain")
        .with_body(body)
}
