/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;

use crate::types::http_request::HttpRequest;
use crate::types::http_response::HttpResponse;

/// What answers a connection's one request. Static dispatch: a server names its service.
pub trait HttpService {
    /// The response to `request`.
    fn call(&self, request: HttpRequest) -> impl Future<Output = HttpResponse>;
}
