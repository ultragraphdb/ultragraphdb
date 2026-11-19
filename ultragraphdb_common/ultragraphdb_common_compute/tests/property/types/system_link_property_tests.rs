/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An endpoint core over a system group a test plays: laws L5.3 and L5.9 of `notes/laws.md` (`PROTOCOL.md` §11.12, *Epochs*
//! and *While the system group cannot be reached*).

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_compute::{
    AuthThrottle, ClientSession, EndpointCore, EndpointLimits, EveryAuthenticated, LoginLimits,
    RelayAuthenticator, RoleDouble, RoleLinks, SystemLink, SystemPlay, TenantGate,
};
use ultragraphdb_common_role::{
    RoleClient, RoleCluster, RoleSecret, Server, serve_connection_with,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    Begin, BodyView, Command, ErrorCode, ErrorReplyView, ReadLevel, Register,
};

const CLUSTER: u64 = 7;

/// A core over a played group of one partition, holding `ann`: the group's double, the core's link and its client address.
async fn core() -> Option<(SystemPlay, RoleDouble, SystemLink, Address)> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    static FILES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let numbered = FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = root.join(format!("link-law-{}-{numbered}", std::process::id()));
    std::fs::write(&file, "a role secret").ok()?;
    let cluster = RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string()).ok()?,
        Duration::from_secs(2),
    );
    let verifier = ScramVerifier::derive("pw", b"ann's salt", 4_096).ok()?;
    let play = SystemPlay::new(CLUSTER, 1).with_principal("ann", 41, verifier);
    let script = play.clone();
    let group = RoleDouble::start(&cluster, Endpoint::System, move |frame| {
        script.answer(frame)
    })
    .await
    .ok()?;
    let links = RoleLinks::new(cluster.clone(), "compute".to_string());
    let address = Address::parse(group.address()).ok()?;
    let system = SystemLink::dial(address, cluster, links.clone(), 5_000, Readiness::default());
    let deadline =
        compio::time::interval(Duration::from_secs(1)).tick().await + Duration::from_secs(5);
    system.answering(deadline).await.ok()?;
    let (throttle, throttling) = AuthThrottle::new(LoginLimits::default()).serve();
    spawn(throttling).detach();
    let endpoint = EndpointCore::new(
        system.clone(),
        throttle,
        links,
        TenantGate::new(EveryAuthenticated),
        EndpointLimits::new(1 << 22, Duration::from_secs(30)),
    );
    let listener = Listener::bind(&Address::any_port("127.0.0.1").ok()?)
        .await
        .ok()?;
    let client = listener.local_address().ok()?;
    let server = Server::new(CLUSTER, "compute", Endpoint::Client).ok()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let relay = RelayAuthenticator::default();
            let session = ClientSession::new(endpoint.clone(), relay.clone(), None);
            let server = server.clone();
            spawn(async move { serve_connection_with(carrier, &server, relay, session).await })
                .detach();
        }
    })
    .detach();
    Some((play, group, system, client))
}

/// The command of the reply to a snapshot Begin, and its error code if it is an Error.
async fn begin(client: &mut RoleClient) -> Option<(Command, Option<ErrorCode>)> {
    client
        .send_only(&Begin::new(0, 0, true).with_level(ReadLevel::Snapshot))
        .await
        .ok()?;
    let frame = compio::time::timeout(Duration::from_secs(5), client.next_frame())
        .await
        .ok()?
        .ok()?;
    let view = frame.view().ok()?;
    let code = ErrorReplyView::from_frame(&view)
        .ok()
        .map(|error| error.code());
    Some((view.command(), code))
}

/// L5.3: any sequence of Begins naming no token between closes is answered from the view, and sends no frame to the system
/// group.
#[hegel::test(test_cases = 3)]
fn test_a_begin_asks_no_other_process(tc: TestCase) {
    let steps: Vec<bool> = (0..tc.draw(gs::integers::<usize>().min_value(1).max_value(8)))
        .map(|_| tc.draw(gs::booleans()))
        .collect();
    let ran = Runtime::new().ok().and_then(|runtime| {
        runtime.block_on(async {
            let (play, group, _system, address) = core().await?;
            let mut client = RoleClient::dial(&address, CLUSTER, "ann", "pw")
                .await
                .ok()?;
            let mut closed = play.closed();
            for close in steps {
                if close {
                    closed += 1;
                    play.close(closed, 0, &[]);
                    continue;
                }
                // The link's own task reads Members every 100 ms until the group lists every partition, which this
                // group never does: those frames are no Begin's, and one landing between the two counts is left out
                // as a close is.
                let own = [Command::EpochClose, Command::Members];
                let before = group.commands(&own).len();
                let answered = begin(&mut client).await?;
                assert_eq!(answered, (Command::Begin, None));
                let seen = group.commands(&own);
                assert_eq!(seen.len(), before, "no frame to the group: {seen:?}");
            }
            Some(())
        })
    });
    assert!(ran.is_some(), "the scenario ran");
}

/// L5.9: while the link is closed, a Register is answered code 17 and an Authenticate Error 17 on stream 0 with the connection
/// closed, while a Begin through no session goes on from the view.
#[hegel::test(test_cases = 3)]
fn test_a_closed_link_refuses_what_needs_the_group(tc: TestCase) {
    let asks: Vec<u8> = (0..tc.draw(gs::integers::<usize>().min_value(1).max_value(4)))
        .map(|_| tc.draw(gs::integers::<u8>().min_value(0).max_value(2)))
        .collect();
    let ran = Runtime::new().ok().and_then(|runtime| {
        runtime.block_on(async {
            let (_play, group, system, address) = core().await?;
            let mut client = RoleClient::dial(&address, CLUSTER, "ann", "pw")
                .await
                .ok()?;
            group.close();
            for _ in 0..5_000 {
                if !system.is_up() {
                    break;
                }
                compio::time::sleep(Duration::from_millis(1)).await;
            }
            assert!(!system.is_up(), "the link closed");
            for ask in asks {
                match ask {
                    0 => assert_eq!(begin(&mut client).await, Some((Command::Begin, None))),
                    1 => {
                        client.send_only(&Register::new([0; 16])).await.ok()?;
                        let frame = client.next_frame().await.ok()?;
                        let view = frame.view().ok()?;
                        let code = ErrorReplyView::from_frame(&view)
                            .ok()
                            .map(|error| error.code());
                        assert_eq!(code, Some(ErrorCode::Unavailable));
                    }
                    _ => {
                        let refused = RoleClient::dial(&address, CLUSTER, "ann", "pw").await.err();
                        let code = refused.and_then(|error| {
                            if let TransportErrorEnum::Refused { code, .. } = error.0 {
                                Some(code)
                            } else {
                                None
                            }
                        });
                        assert_eq!(
                            code,
                            Some(ErrorCode::Unavailable),
                            "Error 17, no failure counted"
                        );
                    }
                }
            }
            Some(())
        })
    });
    assert!(ran.is_some(), "the scenario ran");
}
