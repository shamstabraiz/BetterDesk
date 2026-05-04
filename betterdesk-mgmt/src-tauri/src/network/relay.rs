//! Relay server (hbbr) TCP client.
//!
//! Handles:
//! - TCP connection to relay server
//! - UUID-based relay registration (RequestRelay)
//! - Bidirectional encrypted stream with the peer
//! - Relay confirmation detection (skip RelayResponse from hbbr)

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use log::{debug, info, warn, error, trace};
use prost::Message;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;

use crate::crypto::{KeyExchange, SecretBoxStream};
use crate::proto::{
    message::Union as MsgUnion, rendezvous_message::Union as RdzUnion, LoginRequest,
    Message as PeerMessage, RendezvousMessage, RequestRelay,
};
use crate::protocol::{read_frame, write_frame, encode_frame, FrameCodec};

/// Active relay connection to a peer.
pub struct RelayConnection {
    reader: ReadHalf<TcpStream>,
    writer: WriteHalf<TcpStream>,
    _codec: FrameCodec,
    crypto: Option<SecretBoxStream>,
    peer_id: String,
}

/// Authentication challenge received from the peer.
pub struct AuthChallenge {
    pub salt: String,
    pub challenge: String,
}

impl RelayConnection {
    /// Connect to a relay server and establish a session with the peer.
    ///
    /// Flow:
    /// 1. TCP connect to relay_server
    /// 2. Send RequestRelay { id, uuid }
    /// 3. Read first frame — skip RelayResponse if present, otherwise treat as SignedId
    /// 4. Parse SignedId → extract peer's ephemeral Curve25519 PK
    /// 5. NaCl key exchange — seal symmetric key with peer's PK, send PublicKey
    /// 6. Enable encryption, read encrypted Hash
    /// 7. Return connection ready for authentication
    pub async fn connect(
        relay_server: &str,
        _my_id: &str,
        peer_id: &str,
        uuid: &str,
        _peer_pk: &[u8],
    ) -> Result<(Self, AuthChallenge)> {
        info!(
            "[RELAY] === Connect START === relay={} peer={} uuid={}",
            relay_server, peer_id, uuid
        );

        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            TcpStream::connect(relay_server),
        )
        .await
        .context("Relay server connection timed out (10s)")?
        .context("Failed to connect to relay server")?;

        info!("[RELAY] TCP connected to {}", relay_server);
        let (mut reader, mut writer) = tokio::io::split(stream);

        // Step 1: Send RequestRelay
        let req = RendezvousMessage {
            union: Some(RdzUnion::RequestRelay(RequestRelay {
                id: peer_id.to_string(),
                uuid: uuid.to_string(),
                socket_addr: Vec::new(),
                relay_server: String::new(),
                secure: false,
                licence_key: String::new(),
                conn_type: 0,
                token: String::new(),
                control_permissions: None,
            })),
        };
        writer.write_all(&write_frame(&req)).await?;
        info!("[RELAY] Step 1: Sent RequestRelay (peer_id={}, uuid={})", peer_id, uuid);

        // Step 2: Read first frame — could be RelayResponse (from hbbr) or SignedId (from peer).
        // Yomie Go relay does NOT send RelayResponse; original hbbr may.
        info!("[RELAY] Step 2: Waiting for first frame from relay...");
        let first_frame = read_frame(&mut reader).await
            .context("Relay channel closed before peer connected")?;
        info!("[RELAY] Step 2: Got first frame, {} bytes", first_frame.len());
        trace!("[RELAY] First frame hex (first 64 bytes): {:02x?}", &first_frame[..first_frame.len().min(64)]);

        let signed_id_frame = if let Ok(rdz) = RendezvousMessage::decode(first_frame.as_slice()) {
            if matches!(rdz.union, Some(RdzUnion::RelayResponse(_))) {
                info!("[RELAY] Step 2: Got RelayResponse from relay server, reading next frame for SignedId...");
                let frame = read_frame(&mut reader).await
                    .context("Failed to read SignedId after RelayResponse")?;
                info!("[RELAY] Step 2: Got second frame (SignedId), {} bytes", frame.len());
                frame
            } else {
                info!("[RELAY] Step 2: First frame is NOT RelayResponse (decoded as RendezvousMessage with union={:?}), treating as SignedId", rdz.union.as_ref().map(|u| std::mem::discriminant(u)));
                first_frame
            }
        } else {
            info!("[RELAY] Step 2: First frame is NOT a RendezvousMessage, treating as peer Message (SignedId)");
            first_frame
        };

        // Step 3: Parse SignedId from peer
        info!("[RELAY] Step 3: Decoding SignedId frame ({} bytes)...", signed_id_frame.len());
        let signed_id_msg = PeerMessage::decode(signed_id_frame.as_slice())
            .context("Failed to decode peer message (expected SignedId)")?;

        let signed_id_bytes = match signed_id_msg.union {
            Some(MsgUnion::SignedId(ref sid)) => {
                info!("[RELAY] Step 3: Got SignedId, id payload = {} bytes", sid.id.len());
                sid.id.clone()
            }
            other => {
                error!("[RELAY] Step 3: FAILED — expected SignedId, got: {:?}", other);
                bail!("Expected SignedId from peer, got: {:?}", other);
            }
        };

        let signed_payload = crate::crypto::verify_signed_id(&signed_id_bytes)
            .context("Failed to parse SignedId")?;

        info!(
            "[RELAY] Step 3: Parsed SignedId — peer_id='{}', peer_pk={} bytes (first 8: {:02x?})",
            signed_payload.peer_id,
            signed_payload.peer_pk.len(),
            &signed_payload.peer_pk[..8]
        );

        // Step 4: NaCl key exchange — generate symmetric key, seal with peer's PK, send PublicKey
        info!("[RELAY] Step 4: Creating NaCl key exchange...");
        let key_exchange = KeyExchange::new_initiator();
        let (asym_val, sym_val) = key_exchange
            .create_public_key_msg(&signed_payload.peer_pk)
            .context("Failed to create sealed PublicKey")?;
        info!(
            "[RELAY] Step 4: PublicKey ready — asym={} bytes, sym={} bytes",
            asym_val.len(), sym_val.len()
        );

        let pk_msg = PeerMessage {
            union: Some(MsgUnion::PublicKey(crate::proto::PublicKey {
                asymmetric_value: asym_val,
                symmetric_value: sym_val,
            })),
        };
        writer.write_all(&write_frame(&pk_msg)).await
            .context("Failed to send PublicKey to peer")?;
        info!("[RELAY] Step 4: Sent PublicKey to peer");

        // Step 5: Enable encryption with the agreed symmetric key
        let mut crypto = SecretBoxStream::new(key_exchange.symmetric_key());
        info!("[RELAY] Step 5: SecretBoxStream created — encryption ENABLED");

        // Step 6: Read Hash from peer (encrypted after key exchange)
        info!("[RELAY] Step 6: Waiting for encrypted Hash from peer...");
        let hash_frame = read_frame(&mut reader).await
            .context("Failed to read Hash after key exchange")?;
        info!("[RELAY] Step 6: Got Hash frame, {} bytes (encrypted)", hash_frame.len());

        let hash_payload = crypto.decrypt(&hash_frame)
            .map_err(|e| {
                error!("[RELAY] Step 6: DECRYPT FAILED — {:?}. Frame hex (first 48 bytes): {:02x?}", e, &hash_frame[..hash_frame.len().min(48)]);
                e
            })
            .context("Failed to decrypt Hash — key exchange mismatch")?;
        info!("[RELAY] Step 6: Decrypted Hash payload, {} bytes", hash_payload.len());

        let hash_msg = PeerMessage::decode(hash_payload.as_slice())
            .context("Failed to decode Hash message")?;

        let auth_challenge = match hash_msg.union {
            Some(MsgUnion::Hash(ref hash)) => {
                info!(
                    "[RELAY] Step 6: Hash challenge received — salt_len={}, challenge_len={}",
                    hash.salt.len(), hash.challenge.len()
                );
                AuthChallenge {
                    salt: hash.salt.clone(),
                    challenge: hash.challenge.clone(),
                }
            }
            other => {
                error!("[RELAY] Step 6: FAILED — expected Hash, got: {:?}", other);
                bail!("Expected Hash message after key exchange, got: {:?}", other);
            }
        };

        info!("[RELAY] === Connect SUCCESS === peer='{}' encrypted=true", signed_payload.peer_id);

        let conn = RelayConnection {
            reader,
            writer,
            _codec: FrameCodec::new(),
            crypto: Some(crypto),
            peer_id: signed_payload.peer_id,
        };

        Ok((conn, auth_challenge))
    }

    /// Send a LoginRequest with the hashed password.
    ///
    /// Uses encryption if active (always after key exchange).
    pub async fn send_login(
        &mut self,
        my_id: &str,
        password_hash: &[u8; 32],
    ) -> Result<()> {
        let login = PeerMessage {
            union: Some(MsgUnion::LoginRequest(LoginRequest {
                username: String::new(),
                password: password_hash.to_vec(),
                my_id: my_id.to_string(),
                my_name: whoami::devicename(),
                option: None,
                union: None,
                video_ack_required: true,
                session_id: rand::random::<u64>(),
                version: "2.0.1".into(),
                os_login: None,
                my_platform: std::env::consts::OS.into(),
                hwid: uuid::Uuid::new_v4().as_bytes().to_vec(),
            })),
        };

        self.send_message(&login).await?;
        info!("Sent LoginRequest (encrypted, my_id={})", my_id);
        Ok(())
    }

    /// Read the next peer message.
    ///
    /// Handles decryption if crypto is active.
    pub async fn read_message(&mut self) -> Result<PeerMessage> {
        loop {
            let frame = read_frame(&mut self.reader).await?;

            let payload = if let Some(ref mut crypto) = self.crypto {
                // Try decrypt, fall back to plaintext if it fails
                match crypto.try_decrypt(&frame) {
                    Some((plaintext, seq)) => {
                        crypto.commit_recv(seq);
                        plaintext
                    }
                    None => {
                        debug!("Decrypt failed, using frame as plaintext");
                        frame.to_vec()
                    }
                }
            } else {
                frame
            };

            let msg = PeerMessage::decode(payload.as_slice())
                .context("Failed to decode peer Message")?;
            return Ok(msg);
        }
    }

    /// Send a peer message.
    ///
    /// Handles encryption if crypto is active.
    pub async fn send_message(&mut self, msg: &PeerMessage) -> Result<()> {
        let payload = msg.encode_to_vec();

        let data = if let Some(ref mut crypto) = self.crypto {
            crypto.encrypt(&payload)?
        } else {
            payload
        };

        let frame = encode_frame(&data);
        self.writer.write_all(&frame).await?;
        Ok(())
    }

    /// Enable encryption with the given symmetric key.
    pub fn enable_crypto(&mut self, key: &[u8; 32]) {
        self.crypto = Some(SecretBoxStream::new(key));
        info!("Encryption enabled for peer {}", self.peer_id);
    }

    /// Get the peer ID.
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Consume the relay connection and spawn reader/writer bridge tasks.
    ///
    /// Returns `(sender, receiver)` channels suitable for `SessionManager::start()`.
    /// - Send a `PeerMessage` on `sender` → it gets framed and written to the relay.
    /// - Receive a `PeerMessage` from `receiver` ← decoded from relay frames.
    pub fn into_channels(
        self,
    ) -> (
        tokio::sync::mpsc::Sender<PeerMessage>,
        tokio::sync::mpsc::Receiver<PeerMessage>,
    ) {
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<PeerMessage>(256);
        let (in_tx, mut in_rx) = tokio::sync::mpsc::channel::<PeerMessage>(256);

        let Self {
            mut reader,
            mut writer,
            crypto,
            peer_id,
            ..
        } = self;

        // Share crypto between reader (decrypt) and writer (encrypt)
        let crypto = Arc::new(TokioMutex::new(crypto));
        let crypto_r = Arc::clone(&crypto);
        let crypto_w = Arc::clone(&crypto);

        // Reader task: relay → SessionManager
        let peer_id_r = peer_id.clone();
        tokio::spawn(async move {
            loop {
                match read_frame(&mut reader).await {
                    Ok(frame) => {
                        // Decrypt if crypto is active
                        let payload = {
                            let mut c = crypto_r.lock().await;
                            if let Some(ref mut stream) = *c {
                                match stream.decrypt(&frame) {
                                    Ok(plaintext) => plaintext,
                                    Err(_) => {
                                        debug!("Decrypt failed, trying plaintext ({})", peer_id_r);
                                        frame.to_vec()
                                    }
                                }
                            } else {
                                frame
                            }
                        };
                        match PeerMessage::decode(payload.as_slice()) {
                            Ok(msg) => {
                                if out_tx.send(msg).await.is_err() {
                                    info!("Relay reader: channel closed ({})", peer_id_r);
                                    break;
                                }
                            }
                            Err(e) => debug!("Relay decode error ({}): {}", peer_id_r, e),
                        }
                    }
                    Err(e) => {
                        warn!("Relay reader ended ({}): {}", peer_id_r, e);
                        break;
                    }
                }
            }
            // out_tx is dropped here → msg_rx.recv() returns None in SessionManager
        });

        // Writer task: SessionManager → relay
        tokio::spawn(async move {
            while let Some(msg) = in_rx.recv().await {
                let payload = msg.encode_to_vec();
                let data = {
                    let mut c = crypto_w.lock().await;
                    if let Some(ref mut stream) = *c {
                        match stream.encrypt(&payload) {
                            Ok(encrypted) => encrypted,
                            Err(e) => {
                                warn!("Encrypt failed ({}): {}", peer_id, e);
                                payload
                            }
                        }
                    } else {
                        payload
                    }
                };
                let frame = encode_frame(&data);
                if writer.write_all(&frame).await.is_err() {
                    break;
                }
            }
            info!("Relay writer ended ({})", peer_id);
        });

        (in_tx, out_rx)
    }
}
