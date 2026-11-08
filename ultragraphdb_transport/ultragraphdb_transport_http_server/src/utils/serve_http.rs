/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use compio::runtime::spawn;
use ultragraphdb_protocol_transport_compio::{AcceptPace, CompioTlsStream, Listener};
use ultragraphdb_transport_tls::TlsAcceptor;

use crate::traits::http_service::HttpService;
use crate::types::connection_slots::ConnectionSlots;
use crate::types::http_limits::HttpLimits;
use crate::types::http_request::HttpRequest;
use crate::types::http_response::HttpResponse;
use crate::types::parsed_head::ParsedHead;
use crate::utils::parse_head::parse_head;

/// The bytes one read asks for.
const READ_BYTES: usize = 16 * 1024;

/// Answers the one request `stream` carries by `service`, then closes. A request past its limits, or malformed, or one a
/// peer leaves unfinished, gets its refusal, and that peer may still be sending: closing over bytes not read resets the
/// connection, which fails the peer's next write and can destroy the refusal on its way. So a refusal is followed by a
/// drain of what the peer still sends, up to the limits' time, before the close. A stream that fails ends with nothing,
/// since no one is left to tell.
///
/// The future owns `stream`: one dropped at any `.await` closes it with the response written in whole, in part or not
/// at all, and drops `service`'s call where it stood, which leaves what the service's own future leaves.
pub async fn serve_http<S, H>(mut stream: S, limits: &HttpLimits, service: &H)
where
    S: AsyncRead + AsyncWrite,
    H: HttpService,
{
    let read = compio::time::timeout(limits.time(), read_request(&mut stream, limits)).await;
    let (response, refused) = match read {
        Ok(Ok(request)) => (service.call(request).await, false),
        Ok(Err(status)) => (HttpResponse::new(status), true),
        Err(_) => (HttpResponse::new(408), true),
    };
    let written = compio::time::timeout(limits.time(), write_response(&mut stream, response)).await;
    if let Ok(Err(error)) = &written {
        tracing::debug!(%error, "a response was not written");
    }
    if refused && matches!(written, Ok(Ok(()))) {
        // The time running out ends the drain as the bytes do: the close then resets what is still arriving.
        if compio::time::timeout(limits.time(), drain(&mut stream))
            .await
            .is_err()
        {
            tracing::debug!("a refused peer was still connected when its time ran out");
        }
    }
}

/// Reads what `stream` still sends and discards it, into one buffer, until the peer ends or [`HttpLimits::DRAIN_MAX`]
/// bytes have passed. The sending side is shut by then, so the peer holds its answer and the end of it. A future
/// dropped at its read leaves nothing, since what it reads is discarded either way.
async fn drain<S: AsyncRead>(stream: &mut S) {
    let mut buffer = Vec::with_capacity(READ_BYTES);
    let mut drained = 0;
    while drained < HttpLimits::DRAIN_MAX {
        buffer.clear();
        let BufResult(read, returned) = stream.read(buffer).await;
        buffer = returned;
        match read {
            Ok(count) if count > 0 => drained += count,
            Ok(_) | Err(_) => return,
        }
    }
}

/// The request on `stream`, read until it is whole, or the status refusing it. A future dropped at a read loses the
/// request's bytes read so far, and the stream holds the rest.
async fn read_request<S: AsyncRead>(
    stream: &mut S,
    limits: &HttpLimits,
) -> Result<HttpRequest, u16> {
    let mut received = Vec::with_capacity(READ_BYTES);
    // Each read takes at least a byte, and the limits bound the bytes a request holds, so this ends: a parse for each
    // byte up to the limits, and one for the read past them, which `parse_head` refuses.
    let parses = limits
        .head()
        .saturating_add(limits.body())
        .saturating_add(1);
    for _ in 0..parses {
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

/// Writes `response` whole, flushes it and shuts the sending side. A future dropped at a write leaves the response
/// written in whole, in part or not at all, and the sending side open.
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
/// when the settings asked for that. A failed accept or handshake costs that connection alone, and failed accepts in a
/// row pause as [`AcceptPace`] says. At most `limits`' connections are served at once; the next waits in the listener's
/// backlog until one ends.
///
/// The accept loop is an event loop: it ends only when its caller drops this future, at its wait for a slot, its accept
/// or its pause, which stops accepting; the connections already accepted go on, each on its detached task within
/// `limits`' time, and give their slots back as they end.
pub async fn serve_listener<H>(
    listener: Listener,
    tls: Option<TlsAcceptor>,
    limits: HttpLimits,
    service: H,
) where
    H: HttpService + Clone + 'static,
{
    let slots = ConnectionSlots::new(limits.connections());
    let mut pace = AcceptPace::default();
    loop {
        let slot = slots.take().await;
        let carrier = pace.accept("http", || listener.accept()).await;
        let (tls, service) = (tls.clone(), service.clone());
        spawn(async move {
            // Held until the task ends, however it ends, which gives the slot back.
            let _slot = slot;
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
