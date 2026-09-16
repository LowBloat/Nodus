use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use directories::ProjectDirs;
use iroh::{EndpointAddr, EndpointId, SecretKey};
use iroh_tickets::endpoint::EndpointTicket;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub settings: PathBuf,
    pub legacy_vault: Option<PathBuf>,
}

impl AppPaths {
    pub fn discover() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("não foi possível descobrir a pasta atual")?;
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| cwd.clone());
        let old_root = if cfg!(debug_assertions) && cwd.join("Cargo.toml").is_file() {
            cwd
        } else {
            exe_dir
        };
        let config_dir = ProjectDirs::from("dev", "Nodus", "Nodus")
            .context("não foi possível localizar o AppData do usuário")?
            .config_dir()
            .to_path_buf();
        fs::create_dir_all(&config_dir)
            .context("não foi possível criar a pasta de configurações no AppData")?;
        let settings = config_dir.join("settings.json");
        let legacy_settings = old_root.join("nodus-data").join("settings.json");
        if !settings.exists() && legacy_settings.is_file() {
            fs::copy(&legacy_settings, &settings)
                .context("não foi possível migrar as configurações antigas")?;
        }
        let old_vault = old_root.join("notes");
        Ok(Self {
            settings,
            legacy_vault: old_vault.is_dir().then_some(old_vault),
        })
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
    pub vault_id: String,
    pub vault_name: String,
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
pub struct VaultConfig {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub secret_key: String,
    pub pairing_token: String,
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
}

impl VaultConfig {
    fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("Vault")
            .to_owned();
        Self {
            id: URL_SAFE_NO_PAD.encode(rand::rng().random::<[u8; 18]>()),
            name,
            path,
            secret_key: URL_SAFE_NO_PAD.encode(rand::rng().random::<[u8; 32]>()),
            pairing_token: new_pairing_token(),
            peers: Vec::new(),
        }
    }
    pub fn secret_key(&self) -> anyhow::Result<SecretKey> {
        decode_secret(&self.secret_key)
    }
    pub fn add_peer(&mut self, peer: PeerConfig) -> bool {
        if self.peers.iter().any(|p| p.endpoint_id == peer.endpoint_id) {
            false
        } else {
            self.peers.push(peer);
            true
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    #[default]
    Notion,
    Split,
    Preview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub theme: ThemeMode,
    pub view_mode: ViewMode,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    pub sync_panel_visible: bool,
}
impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            view_mode: ViewMode::Notion,
            sidebar_visible: true,
            sidebar_width: 248.0,
            sync_panel_visible: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub device_name: String,
    #[serde(default)]
    pub vaults: Vec<VaultConfig>,
    #[serde(default)]
    pub active_vault_id: Option<String>,
    #[serde(default)]
    pub ui: UiPrefs,
}

#[derive(Debug, Deserialize)]
struct LegacySettings {
    device_name: String,
    secret_key: String,
    #[serde(default)]
    pairing_token: String,
    #[serde(default)]
    peers: Vec<PeerConfig>,
    #[serde(default)]
    ui: UiPrefs,
}

impl Settings {
    pub fn load_or_create(path: &Path, legacy_vault: Option<&Path>) -> anyhow::Result<Self> {
        if path.is_file() {
            let bytes = fs::read(path).context("não foi possível ler as configurações")?;
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).context("configurações inválidas")?;
            if value.get("vaults").is_some() {
                let mut settings: Self =
                    serde_json::from_value(value).context("configurações inválidas")?;
                let mut changed = false;
                if settings.device_name == "Meu dispositivo" {
                    settings.device_name = system_device_name();
                    changed = true;
                }
                if settings
                    .active_vault_id
                    .as_ref()
                    .is_some_and(|id| !settings.vaults.iter().any(|v| &v.id == id))
                {
                    settings.active_vault_id = settings.vaults.first().map(|v| v.id.clone());
                    changed = true;
                }
                if changed {
                    settings.save(path)?;
                }
                return Ok(settings);
            }
            let old: LegacySettings =
                serde_json::from_slice(&bytes).context("configurações inválidas")?;
            let mut vaults = Vec::new();
            if let Some(folder) = legacy_vault {
                let mut vault = VaultConfig::new(normalize_existing_dir(folder)?);
                if let Some(shared_id) = legacy_shared_vault_id(&old.secret_key, &old.peers) {
                    vault.id = shared_id;
                }
                vault.secret_key = old.secret_key;
                vault.pairing_token = if old.pairing_token.is_empty() {
                    new_pairing_token()
                } else {
                    old.pairing_token
                };
                vault.peers = old.peers;
                vaults.push(vault);
            }
            let settings = Self {
                device_name: if old.device_name == "Meu dispositivo" {
                    system_device_name()
                } else {
                    old.device_name
                },
                active_vault_id: vaults.first().map(|v| v.id.clone()),
                vaults,
                ui: old.ui,
            };
            settings.save(path)?;
            return Ok(settings);
        }
        let settings = Self {
            device_name: system_device_name(),
            vaults: Vec::new(),
            active_vault_id: None,
            ui: UiPrefs::default(),
        };
        settings.save(path)?;
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("não foi possível criar a pasta de configurações")?;
        }
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(self)?)
            .context("não foi possível gravar as configurações")?;
        if path.exists() {
            fs::remove_file(path).context("não foi possível atualizar as configurações")?;
        }
        fs::rename(temp, path).context("não foi possível finalizar as configurações")
    }
    pub fn active_vault(&self) -> Option<&VaultConfig> {
        self.active_vault_id
            .as_ref()
            .and_then(|id| self.vaults.iter().find(|v| &v.id == id))
    }
    pub fn active_vault_mut(&mut self) -> Option<&mut VaultConfig> {
        let id = self.active_vault_id.clone()?;
        self.vaults.iter_mut().find(|v| v.id == id)
    }
    pub fn activate_vault(&mut self, id: &str) -> bool {
        if self.vaults.iter().any(|v| v.id == id) {
            self.active_vault_id = Some(id.to_owned());
            true
        } else {
            false
        }
    }
    pub fn add_vault(&mut self, path: &Path) -> anyhow::Result<String> {
        let path = normalize_existing_dir(path)?;
        for existing in &self.vaults {
            if path == existing.path {
                bail!("essa pasta já é um vault");
            }
            if path.starts_with(&existing.path) {
                bail!("um vault não pode existir dentro de outro vault");
            }
            if existing.path.starts_with(&path) {
                bail!("essa pasta contém um vault já cadastrado");
            }
        }
        let vault = VaultConfig::new(path);
        let id = vault.id.clone();
        self.vaults.push(vault);
        self.active_vault_id = Some(id.clone());
        Ok(id)
    }
}

fn normalize_existing_dir(path: &Path) -> anyhow::Result<PathBuf> {
    if !path.is_dir() {
        bail!("escolha uma pasta existente");
    }
    path.canonicalize()
        .context("não foi possível abrir a pasta escolhida")
}
fn decode_secret(value: &str) -> anyhow::Result<SecretKey> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .context("a chave local está corrompida")?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("a chave local tem tamanho inválido"))?;
    Ok(SecretKey::from_bytes(&bytes))
}
fn legacy_shared_vault_id(secret: &str, peers: &[PeerConfig]) -> Option<String> {
    let mut members = peers
        .iter()
        .map(|peer| peer.endpoint_id.clone())
        .collect::<Vec<_>>();
    members.push(decode_secret(secret).ok()?.public().to_string());
    members.sort();
    members.dedup();
    if members.len() < 2 {
        return None;
    }
    let digest = blake3::hash(members.join("\n").as_bytes());
    Some(URL_SAFE_NO_PAD.encode(&digest.as_bytes()[..18]))
}
fn new_pairing_token() -> String {
    URL_SAFE_NO_PAD.encode(rand::rng().random::<[u8; 24]>())
}
fn system_device_name() -> String {
    hostname::get()
        .ok()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Meu dispositivo".to_owned())
}

#[derive(Debug, Serialize, Deserialize)]
struct PairCode {
    version: u8,
    name: String,
    ticket: String,
    token: String,
    vault_id: String,
    vault_name: String,
}

pub fn encode_pair_code(
    name: &str,
    addr: EndpointAddr,
    token: &str,
    vault_id: &str,
    vault_name: &str,
) -> String {
    let payload = PairCode {
        version: 3,
        name: name.trim().chars().take(48).collect(),
        ticket: EndpointTicket::new(addr).to_string(),
        token: token.to_owned(),
        vault_id: vault_id.to_owned(),
        vault_name: vault_name.chars().take(80).collect(),
    };
    format!(
        "NODUS3.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("pair code is serializable"))
    )
}

pub fn decode_pair_code(code: &str) -> anyhow::Result<PairInvite> {
    if code.starts_with("NODUS1.") || code.starts_with("NODUS2.") {
        bail!("esse código é de uma versão antiga; gere um novo código no outro dispositivo");
    }
    let encoded = code
        .strip_prefix("NODUS3.")
        .context("o código deve começar com NODUS3")?;
    let json = URL_SAFE_NO_PAD
        .decode(encoded)
        .context("o código está incompleto ou inválido")?;
    let pair: PairCode = serde_json::from_slice(&json).context("o código não é reconhecido")?;
    if pair.version != 3 || pair.token.len() < 24 || pair.vault_id.is_empty() {
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
        vault_id: pair.vault_id,
        vault_name: pair.vault_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pair_code_round_trip_is_scoped_to_a_vault() {
        let secret = SecretKey::from_bytes(&[7; 32]);
        let decoded = decode_pair_code(&encode_pair_code(
            "Notebook",
            EndpointAddr::new(secret.public()),
            "convite-seguro-com-mais-de-24-caracteres",
            "vault-1",
            "Trabalho",
        ))
        .unwrap();
        assert_eq!(decoded.peer.name, "Notebook");
        assert_eq!(decoded.vault_id, "vault-1");
        assert_eq!(decoded.vault_name, "Trabalho");
    }
    #[test]
    fn rejects_nested_vaults_in_both_directions() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        let mut s = Settings {
            device_name: "PC".into(),
            vaults: vec![],
            active_vault_id: None,
            ui: UiPrefs::default(),
        };
        s.add_vault(root.path()).unwrap();
        assert!(s.add_vault(&child).is_err());
        let mut s = Settings {
            device_name: "PC".into(),
            vaults: vec![],
            active_vault_id: None,
            ui: UiPrefs::default(),
        };
        s.add_vault(&child).unwrap();
        assert!(s.add_vault(root.path()).is_err());
    }
    #[test]
    fn new_install_waits_for_a_chosen_vault() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load_or_create(&dir.path().join("settings.json"), None).unwrap();
        assert!(s.vaults.is_empty());
        assert!(s.active_vault().is_none());
    }
    #[test]
    fn migrates_the_portable_settings_and_vault() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes");
        fs::create_dir(&notes).unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, serde_json::to_vec(&serde_json::json!({"device_name":"PC antigo","secret_key":URL_SAFE_NO_PAD.encode([9_u8;32]),"pairing_token":"token-antigo-com-mais-de-24-caracteres","peers":[]})).unwrap()).unwrap();
        let s = Settings::load_or_create(&path, Some(&notes)).unwrap();
        assert_eq!(
            s.active_vault().unwrap().path,
            notes.canonicalize().unwrap()
        );
        assert_eq!(
            s.active_vault().unwrap().pairing_token,
            "token-antigo-com-mais-de-24-caracteres"
        );
    }

    #[test]
    fn paired_legacy_devices_derive_the_same_vault_id() {
        let first = SecretKey::from_bytes(&[21; 32]);
        let second = SecretKey::from_bytes(&[22; 32]);
        let first_peer = PeerConfig {
            name: "B".into(),
            endpoint_id: second.public().to_string(),
            ticket: "unused".into(),
        };
        let second_peer = PeerConfig {
            name: "A".into(),
            endpoint_id: first.public().to_string(),
            ticket: "unused".into(),
        };
        let first_id =
            legacy_shared_vault_id(&URL_SAFE_NO_PAD.encode(first.to_bytes()), &[first_peer])
                .unwrap();
        let second_id =
            legacy_shared_vault_id(&URL_SAFE_NO_PAD.encode(second.to_bytes()), &[second_peer])
                .unwrap();
        assert_eq!(first_id, second_id);
    }
}
