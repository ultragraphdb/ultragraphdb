/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The HTTP/1.1 server of the control plane: `/health` and `/ready`, the config service's API and a role's admin
//! endpoint. One request per connection, bounded in its size and its time, with no chunked bodies and no keep-alive, so
//! there is no framing for two parsers to disagree on. It reads a request whole into one buffer, which the service then
//! holds, so a body reaches its log with no copy.
//!
//! [`parse_head`] decides what bytes received so far ask, purely; [`serve_http`] drives it over one stream, and
//! [`serve_listener`] over a listener, in clear text or TLS. [`BearerKey`] holds what authorises a request, a key's
//! digest, compared in constant time.

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::http_server_error::{HttpServerError, HttpServerErrorEnum};
pub use crate::traits::http_service::HttpService;
pub use crate::types::bearer_key::BearerKey;
pub use crate::types::http_limits::HttpLimits;
pub use crate::types::http_request::HttpRequest;
pub use crate::types::http_response::HttpResponse;
pub use crate::types::parsed_head::{HttpHead, ParsedHead};
pub use crate::utils::parse_head::parse_head;
pub use crate::utils::serve_http::{serve_http, serve_listener};
