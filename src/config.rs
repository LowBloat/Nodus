use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{EndpointAddr, EndpointId, SecretKey};
use iroh_tickets::endpoint::EndpointTicket;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub vault: PathBuf,
    pub settings: PathBuf,
}

impl AppPaths {
    pub fn discover() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("não foi possível descobrir a pasta atual")?;
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| cwd.clone());
        let root = if cfg!(debug_assertions) && cwd.join("Cargo.toml").is_file() {
            cwd
        } else {
            exe_dir
        };
        let paths = Self {
            vault: root.join("notes"),
            settings: root.join("nodus-data").join("settings.json"),
        };
        fs::create_dir_all(&paths.vault).context("não foi possível criar a pasta notes")?;
        fs::create_dir_all(root.join("nodus-data"))
            .context("não foi possível criar a pasta nodus-data")?;
        Ok(paths)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerConfig {
    pub name: String,
    pub endpoint_id: String,
    pub ticket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairInvite {
    pub peer: PeerConfig,
    pub token: String,
}

impl PeerConfig {
    pub fn endpoint_addr(&self) -> anyhow::Result<EndpointAddr> {
        let ticket: EndpointTicket = self
            .ticket
            .parse()
            .context("o endereço do dispositivo salvo é inválido")?;
        Ok(ticket.endpoint_addr().clone())
    }

    pub fn id(&self) -> anyhow::Result<EndpointId> {
        self.endpoint_id
            .parse()
            .context("a identidade do dispositivo salvo é inválida")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub device_name: String,
    pub secret_key: String,
    #[serde(default)]
    pub pairing_token: String,
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
}

impl Settings {
    pub fn load_or_create(path: &Path) -> anyhow::Result<Self> {
        if path.is_file() {
            let bytes = fs::read(path).context("não foi possível ler as configurações")?;
            let mut settings: Self =
                serde_json::from_slice(&bytes).context("configurações inválidas")?;
            let mut changed = false;
            if settings.device_name == "Meu dispositivo" {
                settings.device_name = system_device_name();
                changed = true;
            }
            if settings.pairing_token.is_empty() {
                settings.pairing_token = new_pairing_token();
                changed = true;
            }
            if changed {
                settings.save(path)?;
            }
            return Ok(settings);
        }

        let secret = rand::rng().random::<[u8; 32]>();
        let settings = Self {
            device_name: system_device_name(),
            secret_key: URL_SAFE_NO_PAD.encode(secret),
            pairing_token: new_pairing_token(),
            peers: Vec::new(),
        };
        settings.save(path)?;
        Ok(settings)
    }

    pub fn secret_key(&self) -> anyhow::Result<SecretKey> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.secret_key)
            .context("a chave local está corrompida")?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("a chave local tem tamanho inválido"))?;
        Ok(SecretKey::from_bytes(&bytes))
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).context("não foi possível gravar as configurações")?;
        if path.exists() {
            fs::remove_file(path).context("não foi possível atualizar as configurações")?;
        }
        fs::rename(temp, path).context("não foi possível finalizar as configurações")
    }

    pub fn parse_pair_code(&self, code: &str) -> anyhow::Result<PairInvite> {
        let invite = decode_pair_code(code.trim())?;
        let peer = &invite.peer;
        let own_id = self.secret_key()?.public().to_string();
        if peer.endpoint_id == own_id {
            bail!("esse é o código deste próprio dispositivo");
        }
        Ok(invite)
    }

    pub fn add_peer(&mut self, peer: PeerConfig) -> bool {
        if self
            .peers
            .iter()
            .any(|known| known.endpoint_id == peer.endpoint_id)
        {
            return false;
        }
        self.peers.push(peer);
        true
    }
}

fn new_pairing_token() -> String {
    URL_SAFE_NO_PAD.encode(rand::rng().random::<[u8; 24]>())
}

fn system_device_name() -> String {
    hostname::get()
        .ok()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Meu dispositivo".to_owned())
}

#[derive(Debug, Serialize, Deserialize)]
struct PairCode {
    version: u8,
    name: String,
    ticket: String,
    token: String,
}

pub fn encode_pair_code(name: &str, addr: EndpointAddr, token: &str) -> String {
    let payload = PairCode {
        version: 2,
        name: name.trim().chars().take(48).collect(),
        ticket: EndpointTicket::new(addr).to_string(),
        token: token.to_owned(),
    };
    let json = serde_json::to_vec(&payload).expect("pair code is serializable");
    format!("NODUS2.{}", URL_SAFE_NO_PAD.encode(json))
}

pub fn decode_pair_code(code: &str) -> anyhow::Result<PairInvite> {
    if code.starts_with("NODUS1.") {
        bail!("esse código é de uma versão antiga; atualize o Nodus no outro dispositivo");
    }
    let encoded = code
        .strip_prefix("NODUS2.")
        .context("o código deve começar com NODUS2")?;
    let json = URL_SAFE_NO_PAD
        .decode(encoded)
        .context("o código está incompleto ou inválido")?;
    let pair: PairCode = serde_json::from_slice(&json).context("o código não é reconhecido")?;
    if pair.version != 2 || pair.token.len() < 24 {
        bail!("essa versão do código ainda não é suportada");
    }
    let ticket: EndpointTicket = pair
        .ticket
        .parse()
        .context("o endereço do código é inválido")?;
    let id = ticket.endpoint_addr().id.to_string();
    Ok(PairInvite {
        peer: PeerConfig {
            name: if pair.name.trim().is_empty() {
                format!("Dispositivo {}", &id[..8])
            } else {
                pair.name
            },
            endpoint_id: id,
            ticket: pair.ticket,
        },
        token: pair.token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_code_round_trip() {
        let secret = SecretKey::from_bytes(&[7; 32]);
        let addr = EndpointAddr::new(secret.public());
        let encoded =
            encode_pair_code("Notebook", addr, "convite-seguro-com-mais-de-24-caracteres");
        let decoded = decode_pair_code(&encoded).unwrap();
        assert_eq!(decoded.peer.name, "Notebook");
        assert_eq!(decoded.peer.endpoint_id, secret.public().to_string());
        assert_eq!(decoded.token, "convite-seguro-com-mais-de-24-caracteres");
    }

    #[test]
    fn rejects_old_pairing_codes_with_an_upgrade_message() {
        let error = decode_pair_code("NODUS1.antigo").unwrap_err().to_string();
        assert!(error.contains("versão antiga"));
    }

    #[test]
    fn existing_settings_receive_a_pairing_token_on_upgrade() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let secret = URL_SAFE_NO_PAD.encode([9_u8; 32]);
        fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "device_name": "PC antigo",
                "secret_key": secret,
                "peers": []
            }))
            .unwrap(),
        )
        .unwrap();

        let settings = Settings::load_or_create(&path).unwrap();
        assert!(settings.pairing_token.len() >= 24);
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted["pairing_token"], settings.pairing_token);
    }

    #[test]
    fn an_existing_peer_can_reapply_a_new_invite_to_repair_pairing() {
        let own_secret = SecretKey::from_bytes(&[41; 32]);
        let peer_secret = SecretKey::from_bytes(&[42; 32]);
        let code = encode_pair_code(
            "PC 1",
            EndpointAddr::new(peer_secret.public()),
            "convite-novo-para-reparar-o-pareamento",
        );
        let known_peer = decode_pair_code(&code).unwrap().peer;
        let settings = Settings {
            device_name: "PC 2".to_owned(),
            secret_key: URL_SAFE_NO_PAD.encode(own_secret.to_bytes()),
            pairing_token: new_pairing_token(),
            peers: vec![known_peer],
        };

        let invite = settings.parse_pair_code(&code).unwrap();
        assert_eq!(invite.peer.endpoint_id, peer_secret.public().to_string());
    }
}
