/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use compio::runtime::spawn;
use ultragraphdb_protocol_transport_compio::{CompioTlsStream, Listener};
use ultragraphdb_transport_tls::TlsAcceptor;

use crate::traits::http_service::HttpService;
use crate::types::http_limits::HttpLimits;
use crate::types::http_request::HttpRequest;
use crate::types::http_response::HttpResponse;
use crate::types::parsed_head::ParsedHead;
use crate::utils::parse_head::parse_head;

/// The bytes one read asks for.
const READ_BYTES: usize = 16 * 1024;

/// Answers the one request `stream` carries by `service`, then closes. A request past its limits, or malformed, or one a
/// peer leaves unfinished, gets its refusal; a stream that fails ends with nothing, since no one is left to tell.
pub async fn serve_http<S, H>(mut stream: S, limits: &HttpLimits, service: &H)
where
    S: AsyncRead + AsyncWrite,
    H: HttpService,
{
    let read = compio::time::timeout(limits.time(), read_request(&mut stream, limits)).await;
    let response = match read {
        Ok(Ok(request)) => service.call(request).await,
        Ok(Err(status)) => HttpResponse::new(status),
        Err(_) => HttpResponse::new(408),
    };
    let written = compio::time::timeout(limits.time(), write_response(&mut stream, response)).await;
    if let Ok(Err(error)) = written {
        tracing::debug!(%error, "a response was not written");
    }
}

/// The request on `stream`, read until it is whole, or the status refusing it.
async fn read_request<S: AsyncRead>(
    stream: &mut S,
    limits: &HttpLimits,
) -> Result<HttpRequest, u16> {
    let mut received = Vec::with_capacity(READ_BYTES);
    // Each read takes at least a byte, and the limits bound the bytes a request holds, so this ends.
    for _ in 0..=limits.head() + limits.body() {
        match parse_head(&received, limits) {
            ParsedHead::Complete(head) => return Ok(HttpRequest::new(head, received)),
            ParsedHead::Refused(status) => return Err(status),
            ParsedHead::Incomplete => {}
        }
        let chunk = Vec::with_capacity(READ_BYTES);
        let BufResult(read, chunk) = stream.read(chunk).await;
        match read {
            Ok(count) if count > 0 => received.extend_from_slice(&chunk),
            Ok(_) | Err(_) => return Err(400),
        }
    }
    Err(413)
}

async fn write_response<S: AsyncWrite>(
    stream: &mut S,
    response: HttpResponse,
) -> std::io::Result<()> {
    let head = response.head().into_bytes();
    let BufResult(written, _) = stream.write_all(head).await;
    written?;
    let body = response.into_body();
    if !body.is_empty() {
        let BufResult(sent, _) = stream.write_all(body).await;
        sent?;
    }
    stream.flush().await?;
    stream.shutdown().await
}

/// Answers every connection of `listener`, each on its own task, through `tls` when it is given, and in clear text
/// when the settings asked for that. A failed accept or handshake costs that connection alone.
pub async fn serve_listener<H>(
    listener: Listener,
    tls: Option<TlsAcceptor>,
    limits: HttpLimits,
    service: H,
) where
    H: HttpService + Clone + 'static,
{
    loop {
        let carrier = match listener.accept().await {
            Ok(carrier) => carrier,
            Err(error) => {
                tracing::warn!(%error, "an HTTP connection was not accepted");
                continue;
            }
        };
        let (tls, service) = (tls.clone(), service.clone());
        spawn(async move {
            let Some(acceptor) = tls else {
                serve_http(carrier, &limits, &service).await;
                return;
            };
            let accepted =
                compio::time::timeout(limits.time(), CompioTlsStream::accept(&acceptor, carrier))
                    .await;
            match accepted {
                Ok(Ok(stream)) => serve_http(stream, &limits, &service).await,
                Ok(Err(error)) => tracing::debug!(%error, "a TLS handshake failed"),
                Err(_) => tracing::debug!("a TLS handshake did not finish in time"),
            }
        })
        .detach();
    }
}
