/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_http_server::{HttpRequest, HttpResponse, HttpService};

use crate::types::readiness::Readiness;

/// `/health` and `/ready`: the first answers once the role runs, the second once the role is ready.
#[derive(Debug, Clone)]
pub struct HealthService {
    readiness: Readiness,
}

impl HealthService {
    /// The answers for a role of `readiness`.
    #[must_use]
    pub fn new(readiness: Readiness) -> Self {
        Self { readiness }
    }

    fn answer(&self, method: &str, path: &str) -> HttpResponse {
        if method != "GET" {
            return HttpResponse::new(405).with_header("Allow", "GET");
        }
        match path {
            "/health" => HttpResponse::new(200).with_body("ok\n"),
            "/ready" if self.readiness.is_ready() => HttpResponse::new(200).with_body("ok\n"),
            "/ready" => HttpResponse::new(503).with_body("not ready\n"),
            _ => HttpResponse::new(404),
        }
    }
}

impl HttpService for HealthService {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        self.answer(request.method(), request.path())
    }
}
