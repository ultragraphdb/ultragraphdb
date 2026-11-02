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
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        exchange(&request, self.dial(), self.timeout(), self.tls())
    }
}
