use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock, mpsc},
    thread,
    time::Duration,
};

use anyhow::{Context, bail};
use iroh::{Endpoint, EndpointId, SecretKey, endpoint::presets};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    config::{PairInvite, PeerConfig, encode_pair_code},
    vault::{self, Manifest, NoteMeta},
};

const ALPN: &[u8] = b"nodus/sync/2";
const MAX_PACKET_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Ready {
        pair_code: String,
        endpoint_id: String,
    },
    Syncing {
        peer: String,
    },
    Synced {
        peer: String,
        changed: usize,
        direct: Option<bool>,
    },
    PairRequested {
        request_id: String,
        peer: PeerConfig,
    },
    PairApproved {
        peer: PeerConfig,
        initiate_sync: bool,
    },
    PairRejected {
        peer: String,
    },
    PairRequestFinished {
        request_id: String,
    },
    Error {
        peer: Option<String>,
        message: String,
    },
}

enum NetworkCommand {
    SyncNow,
    RequestPair(PairInvite),
    AnswerPair { request_id: String, accept: bool },
}

struct NetworkIdentity {
    device_name: String,
    secret_key: SecretKey,
    pairing_token: String,
    vault_id: String,
    vault_name: String,
}

pub struct NetworkService {
    peers: Arc<RwLock<Vec<PeerConfig>>>,
    commands: tokio::sync::mpsc::UnboundedSender<NetworkCommand>,
    pub events: mpsc::Receiver<NetworkEvent>,
}

impl NetworkService {
    pub fn start(
        vault: PathBuf,
        device_name: String,
        secret_key: SecretKey,
        pairing_token: String,
        vault_id: String,
        vault_name: String,
        initial_peers: Vec<PeerConfig>,
    ) -> Self {
        let peers = Arc::new(RwLock::new(initial_peers));
        let worker_peers = peers.clone();
        let (event_tx, event_rx) = mpsc::channel();
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        thread::Builder::new()
            .name("nodus-network".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(2)
                    .thread_name("nodus-io")
                    .build();
                match runtime {
                    Ok(runtime) => {
                        let identity = NetworkIdentity {
                            device_name,
                            secret_key,
                            pairing_token,
                            vault_id,
                            vault_name,
                        };
                        if let Err(error) = runtime.block_on(run_network(
                            vault,
                            identity,
                            worker_peers,
                            command_rx,
                            event_tx.clone(),
                        )) {
                            let _ = event_tx.send(NetworkEvent::Error {
                                peer: None,
                                message: format!("rede indisponível: {error:#}"),
                            });
                        }
                    }
                    Err(error) => {
                        let _ = event_tx.send(NetworkEvent::Error {
                            peer: None,
                            message: format!("falha ao iniciar rede: {error}"),
                        });
                    }
                }
            })
            .expect("network thread can be created");
        Self {
            peers,
            commands: command_tx,
            events: event_rx,
        }
    }

    pub fn update_peers(&self, peers: Vec<PeerConfig>) {
        if let Ok(mut current) = self.peers.write() {
            *current = peers;
        }
    }

    pub fn sync_now(&self) {
        let _ = self.commands.send(NetworkCommand::SyncNow);
    }

    pub fn request_pair(&self, invite: PairInvite) {
        let _ = self.commands.send(NetworkCommand::RequestPair(invite));
    }

    pub fn answer_pair(&self, request_id: String, accept: bool) {
        let _ = self
            .commands
            .send(NetworkCommand::AnswerPair { request_id, accept });
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Packet {
    PairRequest {
        token: String,
        requester: PeerConfig,
    },
    PairDecision {
        accepted: bool,
    },
    Manifest(Manifest),
    Put {
        meta: NoteMeta,
        content: Vec<u8>,
    },
    Request {
        path: String,
    },
    Done,
}

async fn run_network(
    vault: PathBuf,
    identity: NetworkIdentity,
    peers: Arc<RwLock<Vec<PeerConfig>>>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<NetworkCommand>,
    events: mpsc::Sender<NetworkEvent>,
) -> anyhow::Result<()> {
    let NetworkIdentity {
        device_name,
        secret_key,
        pairing_token,
        vault_id,
        vault_name,
    } = identity;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;

    publish_pair_code(
        &endpoint,
        &device_name,
        &pairing_token,
        &vault_id,
        &vault_name,
        &events,
    );
    let online_endpoint = endpoint.clone();
    let online_events = events.clone();
    let online_name = device_name.clone();
    let online_pairing_token = pairing_token.clone();
    let online_vault_id = vault_id.clone();
    let online_vault_name = vault_name.clone();
    tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(12), online_endpoint.online()).await;
        publish_pair_code(
            &online_endpoint,
            &online_name,
            &online_pairing_token,
            &online_vault_id,
            &online_vault_name,
            &online_events,
        );
    });

    let accept_endpoint = endpoint.clone();
    let accept_peers = peers.clone();
    let accept_events = events.clone();
    let accept_vault = vault.clone();
    let accept_pairing_token = pairing_token.clone();
    let pending_answers = Arc::new(Mutex::new(std::collections::HashMap::<
        String,
        tokio::sync::oneshot::Sender<bool>,
    >::new()));
    let accept_pending_answers = pending_answers.clone();
    tokio::spawn(async move {
        while let Some(incoming) = accept_endpoint.accept().await {
            let peers = accept_peers.clone();
            let events = accept_events.clone();
            let vault = accept_vault.clone();
            let pairing_token = accept_pairing_token.clone();
            let pending_answers = accept_pending_answers.clone();
            tokio::spawn(async move {
                let result = async {
                    let connection = incoming.await?;
                    let remote_id = connection.remote_id();
                    let (mut send, mut recv) = connection.accept_bi().await?;
                    let first: Packet = recv_packet(&mut recv).await?;
                    match first {
                        Packet::Manifest(remote_manifest) => {
                            let peer = find_peer(&peers, remote_id)
                                .context("conexão recusada: dispositivo não pareado")?;
                            let changed = serve_sync_from_manifest(
                                &mut send,
                                &mut recv,
                                &vault,
                                &peer,
                                remote_manifest,
                            )
                            .await?;
                            connection.close(0u32.into(), b"sync complete");
                            Ok::<_, anyhow::Error>(Some((peer, changed)))
                        }
                        Packet::PairRequest { token, requester } => {
                            let requester = validate_pair_request(remote_id, requester)?;
                            let already_known = find_peer(&peers, remote_id).is_some();
                            let (accepted, request_id) = if already_known {
                                (true, None)
                            } else if token != pairing_token {
                                (false, None)
                            } else {
                                let request_id = remote_id.to_string();
                                let (answer_tx, answer_rx) = tokio::sync::oneshot::channel();
                                if let Ok(mut pending) = pending_answers.lock()
                                    && let Some(previous) =
                                        pending.insert(request_id.clone(), answer_tx)
                                {
                                    let _ = previous.send(false);
                                }
                                let _ = events.send(NetworkEvent::PairRequested {
                                    request_id: request_id.clone(),
                                    peer: requester.clone(),
                                });
                                let answer =
                                    tokio::time::timeout(Duration::from_secs(120), answer_rx)
                                        .await
                                        .ok()
                                        .and_then(Result::ok)
                                        .unwrap_or(false);
                                if let Ok(mut pending) = pending_answers.lock() {
                                    pending.remove(&request_id);
                                }
                                (answer, Some(request_id))
                            };
                            send_packet(&mut send, &Packet::PairDecision { accepted }).await?;
                            send.finish()?;
                            let _ =
                                tokio::time::timeout(Duration::from_secs(5), send.stopped()).await;
                            if let Some(request_id) = request_id {
                                let _ =
                                    events.send(NetworkEvent::PairRequestFinished { request_id });
                            }
                            if accepted && !already_known {
                                add_peer_shared(&peers, requester.clone());
                                let _ = events.send(NetworkEvent::PairApproved {
                                    peer: requester,
                                    initiate_sync: false,
                                });
                            }
                            connection.close(0u32.into(), b"pair complete");
                            Ok(None)
                        }
                        _ => bail!("início de conexão inválido"),
                    }
                }
                .await;
                match result {
                    Ok(Some((peer, changed))) => {
                        let _ = events.send(NetworkEvent::Synced {
                            peer: peer.name,
                            changed,
                            direct: None,
                        });
                    }
                    Ok(None) => {}
                    Err(_error) => {}
                }
            });
        }
    });

    let sync_in_flight = Arc::new(Mutex::new(HashSet::<String>::new()));
    let pair_in_flight = Arc::new(Mutex::new(HashSet::<String>::new()));
    while let Some(command) = commands.recv().await {
        match command {
            NetworkCommand::SyncNow => {
                let known = peers.read().map(|value| value.clone()).unwrap_or_default();
                for peer in known {
                    spawn_sync(
                        endpoint.clone(),
                        vault.clone(),
                        peer,
                        events.clone(),
                        sync_in_flight.clone(),
                    );
                }
            }
            NetworkCommand::RequestPair(invite) => {
                let target_id = invite.peer.endpoint_id.clone();
                let inserted = pair_in_flight
                    .lock()
                    .map(|mut active| active.insert(target_id.clone()))
                    .unwrap_or(false);
                if !inserted {
                    continue;
                }
                let endpoint = endpoint.clone();
                let device_name = device_name.clone();
                let events = events.clone();
                let pair_in_flight = pair_in_flight.clone();
                tokio::spawn(async move {
                    let peer_name = invite.peer.name.clone();
                    match request_pair(&endpoint, &device_name, &invite).await {
                        Ok(true) => {
                            let _ = events.send(NetworkEvent::PairApproved {
                                peer: invite.peer,
                                initiate_sync: true,
                            });
                        }
                        Ok(false) => {
                            let _ = events.send(NetworkEvent::PairRejected { peer: peer_name });
                        }
                        Err(error) => {
                            let _ = events.send(NetworkEvent::Error {
                                peer: Some(peer_name),
                                message: format!("pareamento: {error:#}"),
                            });
                        }
                    }
                    if let Ok(mut active) = pair_in_flight.lock() {
                        active.remove(&target_id);
                    }
                });
            }
            NetworkCommand::AnswerPair { request_id, accept } => {
                if let Ok(mut pending) = pending_answers.lock()
                    && let Some(answer) = pending.remove(&request_id)
                {
                    let _ = answer.send(accept);
                }
            }
        }
    }
    Ok(())
}

fn publish_pair_code(
    endpoint: &Endpoint,
    device_name: &str,
    pairing_token: &str,
    vault_id: &str,
    vault_name: &str,
    events: &mpsc::Sender<NetworkEvent>,
) {
    let _ = events.send(NetworkEvent::Ready {
        pair_code: encode_pair_code(
            device_name,
            endpoint.addr(),
            pairing_token,
            vault_id,
            vault_name,
        ),
        endpoint_id: endpoint.id().to_string(),
    });
}

fn find_peer(peers: &Arc<RwLock<Vec<PeerConfig>>>, id: EndpointId) -> Option<PeerConfig> {
    peers
        .read()
        .ok()?
        .iter()
        .find(|peer| peer.endpoint_id == id.to_string())
        .cloned()
}

fn add_peer_shared(peers: &Arc<RwLock<Vec<PeerConfig>>>, peer: PeerConfig) {
    if let Ok(mut known) = peers.write()
        && !known
            .iter()
            .any(|item| item.endpoint_id == peer.endpoint_id)
    {
        known.push(peer);
    }
}

fn validate_pair_request(
    remote_id: EndpointId,
    requester: PeerConfig,
) -> anyhow::Result<PeerConfig> {
    let ticket: iroh_tickets::endpoint::EndpointTicket = requester
        .ticket
        .parse()
        .context("endereço de retorno do pareamento inválido")?;
    if requester.endpoint_id != remote_id.to_string() || ticket.endpoint_addr().id != remote_id {
        bail!("a identidade do pedido de pareamento não corresponde à conexão");
    }
    let mut requester = requester;
    requester.name = requester.name.trim().chars().take(48).collect();
    if requester.name.is_empty() {
        requester.name = format!("Dispositivo {}", &requester.endpoint_id[..8]);
    }
    Ok(requester)
}

async fn request_pair(
    endpoint: &Endpoint,
    device_name: &str,
    invite: &PairInvite,
) -> anyhow::Result<bool> {
    let addr = invite.peer.endpoint_addr()?;
    let connection = tokio::time::timeout(Duration::from_secs(12), endpoint.connect(addr, ALPN))
        .await
        .context("tempo de conexão esgotado")??;
    if connection.remote_id() != invite.peer.id()? {
        bail!("a identidade remota não corresponde ao convite");
    }
    let requester = PeerConfig {
        name: device_name.trim().chars().take(48).collect(),
        endpoint_id: endpoint.id().to_string(),
        ticket: iroh_tickets::endpoint::EndpointTicket::new(endpoint.addr()).to_string(),
    };
    let (mut send, mut recv) = connection.open_bi().await?;
    send_packet(
        &mut send,
        &Packet::PairRequest {
            token: invite.token.clone(),
            requester,
        },
    )
    .await?;
    send.finish()?;
    let accepted =
        match tokio::time::timeout(Duration::from_secs(130), recv_packet::<Packet>(&mut recv))
            .await
            .context("a confirmação de pareamento expirou")??
        {
            Packet::PairDecision { accepted } => accepted,
            _ => bail!("resposta de pareamento inesperada"),
        };
    let _ = recv.read_to_end(1).await;
    connection.close(0u32.into(), b"pair complete");
    Ok(accepted)
}

fn spawn_sync(
    endpoint: Endpoint,
    vault: PathBuf,
    peer: PeerConfig,
    events: mpsc::Sender<NetworkEvent>,
    in_flight: Arc<Mutex<HashSet<String>>>,
) {
    let inserted = in_flight
        .lock()
        .map(|mut active| active.insert(peer.endpoint_id.clone()))
        .unwrap_or(false);
    if !inserted {
        return;
    }
    tokio::spawn(async move {
        let _ = events.send(NetworkEvent::Syncing {
            peer: peer.name.clone(),
        });
        match sync_with_peer(&endpoint, &vault, &peer).await {
            Ok((changed, direct)) => {
                let _ = events.send(NetworkEvent::Synced {
                    peer: peer.name.clone(),
                    changed,
                    direct,
                });
            }
            Err(error) => {
                let _ = events.send(NetworkEvent::Error {
                    peer: Some(peer.name.clone()),
                    message: format!("{error:#}"),
                });
            }
        }
        if let Ok(mut active) = in_flight.lock() {
            active.remove(&peer.endpoint_id);
        }
    });
}

async fn sync_with_peer(
    endpoint: &Endpoint,
    vault: &Path,
    peer: &PeerConfig,
) -> anyhow::Result<(usize, Option<bool>)> {
    let addr = peer.endpoint_addr()?;
    let connection = tokio::time::timeout(Duration::from_secs(12), endpoint.connect(addr, ALPN))
        .await
        .context("tempo de conexão esgotado")??;
    if connection.remote_id() != peer.id()? {
        bail!("a identidade remota não corresponde ao código pareado");
    }
    let (mut send, mut recv) = connection.open_bi().await?;
    let local = vault::build_manifest(vault)?;
    send_packet(&mut send, &Packet::Manifest(local.clone())).await?;
    let remote = match recv_packet(&mut recv).await? {
        Packet::Manifest(manifest) => manifest,
        _ => bail!("resposta de sincronização inesperada"),
    };

    let mut changed = 0;
    let all_paths: BTreeSet<_> = local.keys().chain(remote.keys()).cloned().collect();
    for path in all_paths {
        match (local.get(&path), remote.get(&path)) {
            (Some(ours), None) => {
                let content = vault::read_note_bytes(vault, &path)?;
                send_packet(
                    &mut send,
                    &Packet::Put {
                        meta: ours.clone(),
                        content,
                    },
                )
                .await?;
                changed += 1;
            }
            (None, Some(_)) => {
                send_packet(&mut send, &Packet::Request { path: path.clone() }).await?;
                let packet = recv_packet(&mut recv).await?;
                apply_put(packet, vault, &peer.name)?;
                changed += 1;
            }
            (Some(ours), Some(theirs)) if ours.hash != theirs.hash => {
                let ours_wins = ours.modified_ms > theirs.modified_ms
                    || (ours.modified_ms == theirs.modified_ms
                        && endpoint.id().to_string() < peer.endpoint_id);
                if ours_wins {
                    let content = vault::read_note_bytes(vault, &path)?;
                    send_packet(
                        &mut send,
                        &Packet::Put {
                            meta: ours.clone(),
                            content,
                        },
                    )
                    .await?;
                } else {
                    send_packet(&mut send, &Packet::Request { path: path.clone() }).await?;
                    let packet = recv_packet(&mut recv).await?;
                    apply_put(packet, vault, &peer.name)?;
                }
                changed += 1;
            }
            _ => {}
        }
    }
    send_packet(&mut send, &Packet::Done).await?;
    send.finish()?;
    let _ = recv.read_to_end(1).await;
    let direct = connection_is_direct(&connection);
    connection.close(0u32.into(), b"sync complete");
    Ok((changed, direct))
}

#[cfg(test)]
async fn serve_sync(
    mut send: iroh::endpoint::SendStream,
    mut recv: iroh::endpoint::RecvStream,
    vault: &Path,
    peer: &PeerConfig,
) -> anyhow::Result<usize> {
    let remote = match recv_packet(&mut recv).await? {
        Packet::Manifest(manifest) => manifest,
        _ => bail!("início de sincronização inválido"),
    };
    serve_sync_from_manifest(&mut send, &mut recv, vault, peer, remote).await
}

async fn serve_sync_from_manifest(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    vault: &Path,
    peer: &PeerConfig,
    _remote: Manifest,
) -> anyhow::Result<usize> {
    let local = vault::build_manifest(vault)?;
    send_packet(send, &Packet::Manifest(local)).await?;
    let mut changed = 0;
    loop {
        match recv_packet(recv).await? {
            Packet::Put { meta, content } => {
                vault::write_remote_note(vault, &meta, &content, &peer.name)?;
                changed += 1;
            }
            Packet::Request { path } => {
                let manifest = vault::build_manifest(vault)?;
                let meta = manifest
                    .get(&path)
                    .context("nota solicitada não existe")?
                    .clone();
                let content = vault::read_note_bytes(vault, &path)?;
                send_packet(send, &Packet::Put { meta, content }).await?;
            }
            Packet::Done => break,
            Packet::Manifest(_) | Packet::PairRequest { .. } | Packet::PairDecision { .. } => {
                bail!("pacote inesperado durante a sincronização")
            }
        }
    }
    send.finish()?;
    Ok(changed)
}

fn apply_put(packet: Packet, vault: &Path, peer: &str) -> anyhow::Result<()> {
    match packet {
        Packet::Put { meta, content } => {
            vault::write_remote_note(vault, &meta, &content, peer)?;
            Ok(())
        }
        _ => bail!("o dispositivo não enviou a nota solicitada"),
    }
}

fn connection_is_direct(connection: &iroh::endpoint::Connection) -> Option<bool> {
    let paths = connection.paths();
    let selected = paths.iter().find(|path| path.is_selected())?;
    Some(selected.is_ip())
}

async fn send_packet(
    stream: &mut iroh::endpoint::SendStream,
    packet: &Packet,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(packet)?;
    if bytes.len() > MAX_PACKET_BYTES {
        bail!("pacote de sincronização excede o limite desta versão");
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    Ok(())
}

async fn recv_packet<T>(stream: &mut iroh::endpoint::RecvStream) -> anyhow::Result<T>
where
    T: DeserializeOwned,
{
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_PACKET_BYTES {
        bail!("o dispositivo enviou um pacote grande demais");
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).context("pacote de sincronização inválido")
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh_tickets::endpoint::EndpointTicket;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn one_way_invite_approves_and_then_syncs() {
        let host = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&[31; 32]))
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let guest = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&[32; 32]))
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let invite = PairInvite {
            peer: PeerConfig {
                name: "PC 1".to_owned(),
                endpoint_id: host.id().to_string(),
                ticket: EndpointTicket::new(host.addr()).to_string(),
            },
            token: "token-de-convite-com-tamanho-suficiente".to_owned(),
            vault_id: "vault-test".to_owned(),
            vault_name: "Teste".to_owned(),
        };

        let host_clone = host.clone();
        let server = tokio::spawn(async move {
            let connection = host_clone.accept().await.unwrap().await.unwrap();
            let remote_id = connection.remote_id();
            let (mut send, mut recv) = connection.accept_bi().await.unwrap();
            let request: Packet = recv_packet(&mut recv).await.unwrap();
            let Packet::PairRequest { token, requester } = request else {
                panic!("expected a pairing request");
            };
            assert_eq!(token, "token-de-convite-com-tamanho-suficiente");
            let requester = validate_pair_request(remote_id, requester).unwrap();
            send_packet(&mut send, &Packet::PairDecision { accepted: true })
                .await
                .unwrap();
            send.finish().unwrap();
            let _ = tokio::time::timeout(Duration::from_secs(5), send.stopped()).await;
            requester
        });

        assert!(request_pair(&guest, "PC 2", &invite).await.unwrap());
        let requester = server.await.unwrap();
        assert_eq!(requester.name, "PC 2");
        assert_eq!(requester.endpoint_id, guest.id().to_string());

        let host_vault = tempfile::tempdir().unwrap();
        let guest_vault = tempfile::tempdir().unwrap();
        std::fs::write(
            guest_vault.path().join("pareamento.md"),
            "# Pareado com um código\n",
        )
        .unwrap();
        let host_clone = host.clone();
        let host_path = host_vault.path().to_owned();
        let sync_server = tokio::spawn(async move {
            let connection = host_clone.accept().await.unwrap().await.unwrap();
            assert_eq!(connection.remote_id(), requester.id().unwrap());
            let (send, recv) = connection.accept_bi().await.unwrap();
            serve_sync(send, recv, &host_path, &requester)
                .await
                .unwrap()
        });
        let (changed, _) = sync_with_peer(&guest, guest_vault.path(), &invite.peer)
            .await
            .unwrap();
        assert_eq!(changed, 1);
        assert_eq!(sync_server.await.unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(host_vault.path().join("pareamento.md")).unwrap(),
            "# Pareado com um código\n"
        );
        host.close().await;
        guest.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn syncs_a_markdown_file_between_two_endpoints() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let content = b"# P2P\n\nEsta nota atravessou uma conexao iroh.\n";
        std::fs::write(left_dir.path().join("teste.md"), content).unwrap();

        let left_secret = SecretKey::from_bytes(&[11; 32]);
        let right_secret = SecretKey::from_bytes(&[22; 32]);
        let left = Endpoint::builder(presets::Minimal)
            .secret_key(left_secret)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let right = Endpoint::builder(presets::Minimal)
            .secret_key(right_secret)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();

        let left_peer = PeerConfig {
            name: "Esquerda".to_owned(),
            endpoint_id: left.id().to_string(),
            ticket: EndpointTicket::new(left.addr()).to_string(),
        };
        let right_peer = PeerConfig {
            name: "Direita".to_owned(),
            endpoint_id: right.id().to_string(),
            ticket: EndpointTicket::new(right.addr()).to_string(),
        };

        let right_clone = right.clone();
        let right_path = right_dir.path().to_owned();
        let server = tokio::spawn(async move {
            let connection = right_clone.accept().await.unwrap().await.unwrap();
            assert_eq!(connection.remote_id(), left_peer.id().unwrap());
            let (send, recv) = connection.accept_bi().await.unwrap();
            serve_sync(send, recv, &right_path, &left_peer)
                .await
                .unwrap()
        });

        let (changed, direct) = sync_with_peer(&left, left_dir.path(), &right_peer)
            .await
            .unwrap();
        assert_eq!(changed, 1);
        assert_ne!(direct, Some(false));
        assert_eq!(server.await.unwrap(), 1);
        assert_eq!(
            std::fs::read(right_dir.path().join("teste.md")).unwrap(),
            content
        );

        let return_content = b"# Volta\n\nCriada no segundo dispositivo.\n";
        std::fs::write(right_dir.path().join("volta.md"), return_content).unwrap();
        let right_clone = right.clone();
        let right_path = right_dir.path().to_owned();
        let left_peer_again = PeerConfig {
            name: "Esquerda".to_owned(),
            endpoint_id: left.id().to_string(),
            ticket: EndpointTicket::new(left.addr()).to_string(),
        };
        let server = tokio::spawn(async move {
            let connection = right_clone.accept().await.unwrap().await.unwrap();
            assert_eq!(connection.remote_id(), left_peer_again.id().unwrap());
            let (send, recv) = connection.accept_bi().await.unwrap();
            serve_sync(send, recv, &right_path, &left_peer_again)
                .await
                .unwrap()
        });
        let (changed, _) = sync_with_peer(&left, left_dir.path(), &right_peer)
            .await
            .unwrap();
        assert_eq!(changed, 1);
        assert_eq!(server.await.unwrap(), 0);
        assert_eq!(
            std::fs::read(left_dir.path().join("volta.md")).unwrap(),
            return_content
        );
        left.close().await;
        right.close().await;
    }
}
