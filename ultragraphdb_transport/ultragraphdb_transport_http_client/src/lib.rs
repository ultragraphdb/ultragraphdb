/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The HTTP/1.1 client of the control plane, which a role takes its settings with. One request per connection, bounded in
//! its time and its reply's size, over TLS verified against a trust the caller names, or in clear text. An
//! `Unreachable` or `TimedOut` refusal is what waiting may mend; anything else is not.

mod errors;
mod types;
mod utils;

pub use crate::errors::http_client_error::{HttpClientError, HttpClientErrorEnum};
pub use crate::types::http_client::HttpClient;
pub use crate::types::http_reply::{HttpReply, ParsedReply};
pub use crate::types::http_url::HttpUrl;
pub use crate::utils::parse_reply::parse_reply;
pub use crate::utils::read_key::read_key;
