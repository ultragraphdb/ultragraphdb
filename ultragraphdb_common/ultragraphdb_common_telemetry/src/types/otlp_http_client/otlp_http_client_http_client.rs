/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use async_trait::async_trait;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};

use super::OtlpHttpClient;
use crate::traits::dial::Dial;
use crate::utils::http1::exchange;

#[async_trait]
impl<D: Dial> HttpClient for OtlpHttpClient<D> {
    /// The exchange blocks within the future's first poll, which holds no `.await`: a future dropped before that poll
    /// leaves nothing sent, and once polled the exchange runs to its end.
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        exchange(&request, self.dial(), self.timeout(), self.tls())
    }
}
