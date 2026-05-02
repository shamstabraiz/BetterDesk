//! Signal server (hbbs) TCP client.
//!
//! Handles:
//! - RegisterPeer (heartbeat / registration)
//! - PunchHoleRequest → PunchHoleResponse / RelayResponse
//! - Framing via varint-prefixed protobuf

use anyhow::{bail, Context, Result};
use log::{info, warn};
use prost::Message;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::proto::{
    rendezvous_message::Union as RdzUnion, ConnType, NatType, PunchHoleRequest,
    RendezvousMessage,
};
use crate::protocol::{read_frame, write_frame};

/// Result of a punch hole / relay negotiation.
pub enum PunchResult {
    /// Direct connection via UDP hole punch (not implemented yet).
    Direct {
        peer_pk: Vec<u8>,
        socket_addr: Vec<u8>,
    },
    /// Relay via hbbr.
    Relay {
        relay_server: String,
        peer_pk: Vec<u8>,
        uuid: String,
    },
    /// Peer is offline or doesn't exist.
    Failure(String),
}

/// Connect to the signal server and request a connection to a peer.
pub async fn punch_hole(
    server_addr: &str,
    my_id: &str,
    peer_id: &str,
    force_relay: bool,
) -> Result<PunchResult> {
    info!(
        "[SIGNAL] === PunchHole START === server={} my_id={} peer_id={} force_relay={}",
        server_addr, my_id, peer_id, force_relay
    );

    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        TcpStream::connect(server_addr),
    )
    .await
    .context("Signal server connection timed out (10s)")?
    .context("Failed to connect to signal server")?;
    info!("[SIGNAL] TCP connected to {}", server_addr);

    // Send PunchHoleRequest directly as the FIRST message.
    //
    // Why not read server's KeyExchange first?
    //   - If DualModeListener (TLS auto-detect) is active, the server peeks the
    //     first byte from the client to detect TLS vs plain. If we try to read
    //     first, both sides block waiting for each other → deadlock.
    //
    // Flow:
    // 1. Client sends PunchHoleRequest
    // 2. DualModeListener peeks first byte → not 0x16 → plain TCP
    // 3. NegotiateSecureTCP sends KeyExchange (buffered in TCP)
    // 4. NegotiateSecureTCP reads our PunchHoleRequest → not KeyExchange → plain mode
    // 5. Server sets FirstMsg = PunchHoleRequest → handlePunchHoleRequestTCP
    // 6. Server responds with PunchHoleResponse/RelayResponse
    // 7. Client reads: KeyExchange (skipped by wait loop), then the actual response
    let phr = RendezvousMessage {
        union: Some(RdzUnion::PunchHoleRequest(PunchHoleRequest {
            id: peer_id.to_string(),
            nat_type: NatType::Symmetric.into(),
            licence_key: String::new(),
            conn_type: ConnType::DefaultConn.into(),
            token: String::new(),
            version: "2.0.1".into(),
            udp_port: 0,
            force_relay,
            upnp_port: 0,
            socket_addr_v6: Vec::new(),
        })),
    };
    stream.write_all(&write_frame(&phr)).await?;
    info!("[SIGNAL] Step 1: Sent PunchHoleRequest (peer_id={}, force_relay={})", peer_id, force_relay);
    info!("[SIGNAL] Step 2: Waiting for PunchHoleResponse/RelayResponse...");

    // Step 3: Wait for response (PunchHoleResponse or RelayResponse)
    let result = wait_for_punch_response(&mut stream).await?;

    // Close signal connection
    let _ = stream.shutdown().await;

    Ok(result)
}

/// Wait for and parse the response to our PunchHoleRequest.
///
/// The server may send intermediate messages (KeyExchange, HealthCheck) before
/// the actual response.  We skip those.
async fn wait_for_punch_response(stream: &mut TcpStream) -> Result<PunchResult> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(15);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("Timeout waiting for punch hole response");
        }

        let frame = tokio::time::timeout(remaining, read_frame(stream))
            .await
            .context("Timeout reading frame")?
            .context("Failed to read frame")?;

        let msg = RendezvousMessage::decode(frame.as_slice())
            .context("Failed to decode RendezvousMessage")?;

        match msg.union {
            Some(RdzUnion::PunchHoleResponse(phr)) => {
                info!(
                    "[SIGNAL] Got PunchHoleResponse: failure={}, relay_server='{}', pk_len={}, socket_addr_len={}, other_failure='{}'",
                    phr.failure, phr.relay_server, phr.pk.len(), phr.socket_addr.len(), phr.other_failure
                );
                // Check for other_failure text first
                if !phr.other_failure.is_empty() {
                    return Ok(PunchResult::Failure(phr.other_failure));
                }

                // Check for failure enum (0 = ID_NOT_EXIST, but also the
                // default value — so we check relay_server and pk to
                // distinguish "no failure" from "ID not found").
                let failure_code = phr.failure;
                if failure_code != 0 {
                    let failure_msg = match failure_code {
                        // 0 = IdNotExist (handled as default below)
                        2 => "Peer is offline",
                        3 => "License mismatch",
                        4 => "License overuse",
                        _ => "Peer ID does not exist",
                    };
                    return Ok(PunchResult::Failure(failure_msg.to_string()));
                }

                // Check if relay is provided
                if !phr.relay_server.is_empty() {
                    let uuid = uuid::Uuid::new_v4().to_string();
                    info!(
                        "[SIGNAL] === PunchHole RELAY === relay_server='{}', pk_len={}, generated uuid='{}'",
                        phr.relay_server, phr.pk.len(), uuid
                    );
                    return Ok(PunchResult::Relay {
                        relay_server: phr.relay_server,
                        peer_pk: phr.pk,
                        uuid,
                    });
                }

                // If we have a socket_addr, it's a direct connection
                if !phr.socket_addr.is_empty() {
                    return Ok(PunchResult::Direct {
                        peer_pk: phr.pk,
                        socket_addr: phr.socket_addr,
                    });
                }

                // No relay and no socket — treat as ID not found
                return Ok(PunchResult::Failure(
                    "Peer ID does not exist".to_string(),
                ));
            }
            Some(RdzUnion::RelayResponse(rr)) => {
                info!(
                    "[SIGNAL] Got RelayResponse: relay_server='{}', uuid='{}', has_pk={}",
                    rr.relay_server, rr.uuid, rr.union.is_some()
                );
                let peer_pk = match rr.union {
                    Some(crate::proto::relay_response::Union::Pk(pk)) => pk,
                    _ => Vec::new(),
                };
                return Ok(PunchResult::Relay {
                    relay_server: rr.relay_server,
                    peer_pk,
                    uuid: rr.uuid,
                });
            }
            Some(RdzUnion::KeyExchange(_)) | Some(RdzUnion::Hc(_)) => {
                info!("[SIGNAL] Skipping intermediate message (KeyExchange/HealthCheck)");
                continue;
            }
            other => {
                warn!("[SIGNAL] Unexpected rendezvous message: {:?}", other);
                continue;
            }
        }
    }
}
