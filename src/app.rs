use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context;
use eframe::egui::{self, Color32, FontId, RichText, Stroke};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::{
    config::{AppPaths, PeerConfig, Settings, ThemeMode, UiPrefs, ViewMode, decode_pair_code},
    network::{NetworkEvent, NetworkService},
    theme::{
        self, install_fonts, serif_regular, serif_semibold, ui_medium, ui_regular, ui_semibold,
    },
    vault,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusTone {
    Neutral,
    Active,
    Success,
    Warning,
}

/// Compute the available width inside a left/right panel given its total
/// width and side margin. Clamped to a sensible minimum so buttons never
/// collapse to zero pixels.
fn inner_width_for(panel_width: f32, margin: f32) -> f32 {
    (panel_width - 2.0 * margin - 8.0).max(theme::layout::SIDEBAR_BUTTON_WIDTH_MIN)
}

pub struct NodusApp {
    paths: AppPaths,
    settings: Settings,
    network: Option<NetworkService>,
    notes: Vec<PathBuf>,
    selected: Option<PathBuf>,
    dirty: bool,
    last_edit_at: Option<Instant>,
    drafts: HashMap<PathBuf, Vec<Block>>,
    blocks: Vec<Block>,
    active_block: Option<usize>,
    pending_focus: Option<usize>,
    slash_open: bool,
    slash_selected_index: usize,
    loaded_modified_ms: u64,
    last_scan: Instant,
    pair_code: String,
    endpoint_short: String,
    pair_input: String,
    pair_error: Option<String>,
    vault_error: Option<String>,
    incoming_pair_requests: VecDeque<(String, PeerConfig)>,
    outgoing_pair_pending: Option<String>,
    sync_status: String,
    sync_tone: StatusTone,
    search: String,
    search_focus_request: bool,
    last_toggle_at: Option<Instant>,
    pairing_expanded: bool,
    save_feedback_until: Option<Instant>,
    save_error: Option<String>,
    close_dialog: bool,
    allow_close: bool,
    markdown_cache: CommonMarkCache,
    fatal_error: Option<String>,
    full_editor_text: String,
    applied_theme_is_dark: Option<bool>,
}

impl NodusApp {
    fn active_vault_path(&self) -> Option<&Path> {
        self.settings
            .active_vault()
            .map(|vault| vault.path.as_path())
    }

    fn active_peers(&self) -> &[PeerConfig] {
        self.settings
            .active_vault()
            .map(|vault| vault.peers.as_slice())
            .unwrap_or(&[])
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_edit_at = Some(Instant::now());
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let initialized = (|| -> anyhow::Result<_> {
            let paths = AppPaths::discover()?;
            let settings =
                Settings::load_or_create(&paths.settings, paths.legacy_vault.as_deref())?;
            let network = if let Some(vault) = settings.active_vault() {
                Some(NetworkService::start(
                    vault.path.clone(),
                    settings.device_name.clone(),
                    vault.secret_key()?,
                    vault.pairing_token.clone(),
                    vault.id.clone(),
                    vault.name.clone(),
                    vault.peers.clone(),
                ))
            } else {
                None
            };
            Ok((paths, settings, network))
        })();

        let (paths, settings, network) = match initialized {
            Ok(tuple) => tuple,
            Err(error) => return Self::failed(&cc.egui_ctx, error.to_string()),
        };

        let initial_palette = theme::current_palette(&cc.egui_ctx, settings.ui.theme);
        theme::apply(&cc.egui_ctx, &initial_palette);

        let notes = settings
            .active_vault()
            .map(|v| vault::list_notes(&v.path))
            .unwrap_or_default();
        let selected = notes.first().cloned();
        let (blocks, loaded_modified_ms, full_editor_text) = selected
            .as_ref()
            .map(|path| {
                let content = fs::read_to_string(path).unwrap_or_default();
                (
                    blocks_from_content(&content),
                    vault::modified_ms(path),
                    content,
                )
            })
            .unwrap_or_default();
        let pairing_expanded = settings.active_vault().is_some_and(|v| v.peers.is_empty());
        Self {
            paths,
            settings,
            network,
            notes,
            selected,
            blocks,
            active_block: None,
            pending_focus: None,
            slash_open: false,
            slash_selected_index: 0,
            dirty: false,
            last_edit_at: None,
            drafts: HashMap::new(),
            loaded_modified_ms,
            last_scan: Instant::now(),
            pair_code: String::new(),
            endpoint_short: "iniciando".to_owned(),
            pair_input: String::new(),
            pair_error: None,
            vault_error: None,
            incoming_pair_requests: VecDeque::new(),
            outgoing_pair_pending: None,
            sync_status: "Preparando conexão".to_owned(),
            sync_tone: StatusTone::Neutral,
            search: String::new(),
            search_focus_request: false,
            last_toggle_at: None,
            pairing_expanded,
            save_feedback_until: None,
            save_error: None,
            close_dialog: false,
            allow_close: false,
            markdown_cache: CommonMarkCache::default(),
            fatal_error: None,
            full_editor_text,
            applied_theme_is_dark: None,
        }
    }

    fn failed(ctx: &egui::Context, message: String) -> Self {
        let root = std::env::current_dir().unwrap_or_default();
        let palette = theme::current_palette(ctx, ThemeMode::System);
        theme::apply(ctx, &palette);
        Self {
            paths: AppPaths {
                settings: root.join("nodus-data/settings.json"),
                legacy_vault: None,
            },
            settings: Settings {
                device_name: String::new(),
                vaults: vec![],
                active_vault_id: None,
                ui: UiPrefs::default(),
            },
            network: None,
            notes: vec![],
            selected: None,
            blocks: Vec::new(),
            active_block: None,
            pending_focus: None,
            slash_open: false,
            slash_selected_index: 0,
            dirty: false,
            last_edit_at: None,
            drafts: HashMap::new(),
            loaded_modified_ms: 0,
            last_scan: Instant::now(),
            pair_code: String::new(),
            endpoint_short: String::new(),
            pair_input: String::new(),
            pair_error: None,
            vault_error: None,
            incoming_pair_requests: VecDeque::new(),
            outgoing_pair_pending: None,
            sync_status: String::new(),
            sync_tone: StatusTone::Warning,
            search: String::new(),
            search_focus_request: false,
            last_toggle_at: None,
            pairing_expanded: false,
            save_feedback_until: None,
            save_error: None,
            close_dialog: false,
            allow_close: false,
            markdown_cache: CommonMarkCache::default(),
            fatal_error: Some(message),
            full_editor_text: String::new(),
            applied_theme_is_dark: Some(palette.dark),
        }
    }

    fn stash_current_draft(&mut self) {
        if self.dirty
            && let Some(path) = &self.selected
        {
            self.drafts.insert(path.clone(), self.blocks.clone());
        }
    }

    fn select_note(&mut self, path: PathBuf) {
        if self.selected.as_ref() == Some(&path) {
            return;
        }
        if self.dirty {
            if !self.save_current_local() {
                return;
            }
            if let Some(network) = &self.network {
                network.sync_now();
            }
        }
        if let Some(draft) = self.drafts.remove(&path) {
            self.blocks = draft;
            self.full_editor_text = content_from_blocks(&self.blocks);
            self.mark_dirty();
        } else {
            let content = fs::read_to_string(&path).unwrap_or_default();
            self.blocks = blocks_from_content(&content);
            self.full_editor_text = content;
            self.dirty = false;
        }
        self.active_block = None;
        self.pending_focus = None;
        self.slash_open = false;
        self.slash_selected_index = 0;
        self.loaded_modified_ms = vault::modified_ms(&path);
        self.selected = Some(path);
        self.save_error = None;
    }

    fn new_note(&mut self) {
        self.stash_current_draft();
        let path = self.next_unsaved_note_path();
        self.selected = Some(path.clone());
        self.blocks = vec![Block::h1("Nova nota"), Block::paragraph("")];
        self.full_editor_text = content_from_blocks(&self.blocks);
        self.active_block = Some(1);
        self.pending_focus = Some(1);
        self.slash_open = false;
        self.slash_selected_index = 0;
        self.mark_dirty();
        self.loaded_modified_ms = 0;
        self.save_error = None;
        if !self.notes.contains(&path) {
            self.notes.push(path);
        }
    }

    fn delete_note(&mut self, path: &Path) {
        let _ = fs::remove_file(path);
        self.drafts.remove(path);
        self.refresh_notes();
        if self.selected.as_deref() == Some(path) {
            self.selected = self.notes.first().cloned();
            if let Some(sel) = &self.selected {
                let content = fs::read_to_string(sel).unwrap_or_default();
                self.blocks = blocks_from_content(&content);
                self.full_editor_text = content;
                self.loaded_modified_ms = vault::modified_ms(sel);
            } else {
                self.blocks = Vec::new();
                self.full_editor_text = String::new();
                self.loaded_modified_ms = 0;
            }
            self.dirty = false;
            self.active_block = None;
        }
    }

    fn next_unsaved_note_path(&self) -> PathBuf {
        let Some(vault_path) = self.active_vault_path() else {
            return PathBuf::from("Nova nota.md");
        };
        for index in 1..10_000 {
            let name = if index == 1 {
                "Nova nota.md".to_owned()
            } else {
                format!("Nova nota {index}.md")
            };
            let candidate = vault_path.join(name);
            if !candidate.exists() && !self.notes.contains(&candidate) {
                return candidate;
            }
        }
        vault::unique_note_path(vault_path)
    }

    fn save_current_local(&mut self) -> bool {
        self.save_error = None;
        if !self.dirty {
            return true;
        }
        let Some(path) = self.selected.clone() else {
            return true;
        };
        let content = if self.settings.ui.view_mode == ViewMode::Split {
            self.blocks = blocks_from_content(&self.full_editor_text);
            self.full_editor_text.clone()
        } else {
            let c = content_from_blocks(&self.blocks);
            self.full_editor_text = c.clone();
            c
        };
        match fs::write(&path, content.as_bytes()) {
            Ok(()) => {
                self.loaded_modified_ms = vault::modified_ms(&path);
                self.dirty = false;
                self.last_edit_at = None;
                self.drafts.remove(&path);
                self.save_feedback_until = Some(Instant::now() + Duration::from_secs(2));
                true
            }
            Err(error) => {
                self.save_error = Some(format!("Não foi possível salvar: {error}"));
                false
            }
        }
    }

    fn save_and_sync(&mut self) {
        if !self.save_current_local() {
            return;
        }
        if self.active_peers().is_empty() {
            self.sync_status = "Salva neste dispositivo".to_owned();
            self.sync_tone = StatusTone::Success;
        } else if let Some(network) = &self.network {
            network.sync_now();
            self.sync_status = "Salva. Iniciando sync".to_owned();
            self.sync_tone = StatusTone::Active;
        }
        self.refresh_notes();
    }

    fn maybe_autosave(&mut self) {
        if self.dirty
            && self
                .last_edit_at
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(700))
        {
            self.save_and_sync();
        }
    }

    fn save_all_and_sync(&mut self) -> bool {
        if !self.save_current_local() {
            return false;
        }
        let pending: Vec<_> = self
            .drafts
            .iter()
            .map(|(path, blocks)| (path.clone(), blocks.clone()))
            .collect();
        for (path, blocks) in pending {
            let content = content_from_blocks(&blocks);
            if let Err(error) = fs::write(&path, content.as_bytes()) {
                self.save_error = Some(format!(
                    "Não foi possível salvar {}: {error}",
                    path.display()
                ));
                return false;
            }
            self.drafts.remove(&path);
        }
        if let Some(network) = &self.network {
            network.sync_now();
        }
        true
    }

    fn unsaved_count(&self) -> usize {
        self.drafts.len() + usize::from(self.dirty)
    }

    fn note_is_unsaved(&self, path: &Path) -> bool {
        (self.dirty && self.selected.as_deref() == Some(path)) || self.drafts.contains_key(path)
    }

    fn refresh_notes(&mut self) {
        let Some(vault_path) = self.active_vault_path().map(Path::to_path_buf) else {
            self.notes.clear();
            return;
        };
        let mut notes = vault::list_notes(&vault_path);
        for path in self.drafts.keys() {
            if !notes.contains(path) {
                notes.push(path.clone());
            }
        }
        if self.dirty
            && let Some(path) = &self.selected
            && !notes.contains(path)
        {
            notes.push(path.clone());
        }
        notes.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
        self.notes = notes;

        if !self.dirty
            && let Some(selected) = &self.selected
        {
            let modified = vault::modified_ms(selected);
            if modified != 0 && modified != self.loaded_modified_ms {
                let content = fs::read_to_string(selected).unwrap_or_default();
                self.blocks = blocks_from_content(&content);
                self.active_block = None;
                self.pending_focus = None;
                self.slash_open = false;
                self.slash_selected_index = 0;
                self.loaded_modified_ms = modified;
            }
        }
    }

    fn poll_network(&mut self) {
        let events: Vec<_> = self
            .network
            .as_ref()
            .map(|network| network.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                NetworkEvent::Ready {
                    pair_code,
                    endpoint_id,
                } => {
                    self.pair_code = pair_code;
                    self.endpoint_short = endpoint_id.chars().take(10).collect();
                    self.sync_status = if self.active_peers().is_empty() {
                        "Pronto para conectar".to_owned()
                    } else {
                        "Pronto. Salve para sincronizar".to_owned()
                    };
                    self.sync_tone = StatusTone::Neutral;
                }
                NetworkEvent::Syncing { peer } => {
                    self.sync_status = format!("Enviando para {peer}");
                    self.sync_tone = StatusTone::Active;
                }
                NetworkEvent::Synced {
                    peer,
                    changed,
                    direct,
                } => {
                    let route = match direct {
                        Some(true) => "conexão direta",
                        Some(false) => "relay criptografado",
                        None => "conexão segura",
                    };
                    self.sync_status = if changed == 0 {
                        format!("{peer} está em dia")
                    } else {
                        format!("{changed} alteração(ões) com {peer}")
                    };
                    self.sync_status.push_str(&format!(" via {route}"));
                    self.sync_tone = StatusTone::Success;
                    self.refresh_notes();
                }
                NetworkEvent::PairRequested { request_id, peer } => {
                    if !self
                        .incoming_pair_requests
                        .iter()
                        .any(|(known_id, _)| known_id == &request_id)
                    {
                        self.incoming_pair_requests
                            .push_back((request_id, peer.clone()));
                    }
                    self.sync_status = format!("{} quer se conectar", peer.name);
                    self.sync_tone = StatusTone::Active;
                }
                NetworkEvent::PairApproved {
                    peer,
                    initiate_sync,
                } => {
                    self.outgoing_pair_pending = None;
                    if self.persist_peer(peer.clone()) {
                        self.pair_input.clear();
                        self.pairing_expanded = false;
                        self.sync_status = format!("{} conectado com segurança", peer.name);
                        self.sync_tone = StatusTone::Success;
                        if initiate_sync && let Some(network) = &self.network {
                            network.sync_now();
                        }
                    }
                }
                NetworkEvent::PairRejected { peer } => {
                    self.outgoing_pair_pending = None;
                    self.sync_status = format!("{peer} recusou a conexão");
                    self.sync_tone = StatusTone::Warning;
                }
                NetworkEvent::PairRequestFinished { request_id } => {
                    let previous_count = self.incoming_pair_requests.len();
                    self.incoming_pair_requests
                        .retain(|(known_id, _)| known_id != &request_id);
                    if self.incoming_pair_requests.len() != previous_count {
                        self.sync_status = "A solicitação de conexão expirou".to_owned();
                        self.sync_tone = StatusTone::Warning;
                    }
                }
                NetworkEvent::Error { peer, message } => {
                    let pairing_error = message.contains("pareamento:");
                    if pairing_error {
                        self.outgoing_pair_pending = None;
                    }
                    if let Some(peer) = peer {
                        self.sync_status = if pairing_error {
                            format!("Não foi possível conectar a {peer}. Tente adicionar novamente")
                        } else {
                            format!("{peer} não respondeu. Salve para tentar de novo")
                        };
                    } else {
                        self.sync_status = friendly_network_error(&message);
                    }
                    self.sync_tone = StatusTone::Warning;
                }
            }
        }
    }

    fn add_peer(&mut self) {
        self.pair_error = None;
        let invite = (|| -> anyhow::Result<_> {
            let invite = decode_pair_code(&self.pair_input)?;
            let vault = self
                .settings
                .active_vault()
                .context("escolha um vault antes de conectar um dispositivo")?;
            if invite.peer.endpoint_id == vault.secret_key()?.public().to_string() {
                anyhow::bail!("esse é o código deste próprio dispositivo");
            }
            if invite.vault_id != vault.id {
                if !vault.peers.is_empty() {
                    anyhow::bail!("esse código pertence a outro vault");
                }
                let old_id = vault.id.clone();
                if let Some(vault) = self.settings.vaults.iter_mut().find(|v| v.id == old_id) {
                    vault.id = invite.vault_id.clone();
                }
                self.settings.active_vault_id = Some(invite.vault_id.clone());
                self.settings.save(&self.paths.settings)?;
                self.start_active_network()?;
            }
            Ok(invite)
        })();
        match invite {
            Ok(invite) => {
                let peer_name = invite.peer.name.clone();
                if let Some(network) = &self.network {
                    network.request_pair(invite);
                }
                self.outgoing_pair_pending = Some(peer_name.clone());
                self.sync_status =
                    format!("Pedido enviado a {peer_name}. Confirme no outro dispositivo");
                self.sync_tone = StatusTone::Active;
            }
            Err(error) => self.pair_error = Some(error.to_string()),
        }
    }

    fn persist_peer(&mut self, peer: PeerConfig) -> bool {
        let inserted = self
            .settings
            .active_vault_mut()
            .is_some_and(|vault| vault.add_peer(peer.clone()));
        if inserted && let Err(error) = self.settings.save(&self.paths.settings) {
            if let Some(vault) = self.settings.active_vault_mut() {
                vault
                    .peers
                    .retain(|item| item.endpoint_id != peer.endpoint_id);
            }
            self.pair_error = Some(error.to_string());
            self.sync_status = "Não foi possível salvar o novo dispositivo".to_owned();
            self.sync_tone = StatusTone::Warning;
            if let Some(network) = &self.network {
                network.update_peers(self.active_peers().to_vec());
            }
            return false;
        }
        if let Some(network) = &self.network {
            network.update_peers(self.active_peers().to_vec());
        }
        true
    }

    /// Best-effort save of UI prefs to `settings.json`. Logs but does not
    /// surface failures (matching the silent-fail pattern used by pairing).
    fn save_ui_prefs(&mut self) {
        if let Err(error) = self.settings.save(&self.paths.settings) {
            eprintln!("não foi possível salvar preferências de UI: {error}");
        }
    }

    fn start_active_network(&mut self) -> anyhow::Result<()> {
        self.network = if let Some(vault) = self.settings.active_vault() {
            Some(NetworkService::start(
                vault.path.clone(),
                self.settings.device_name.clone(),
                vault.secret_key()?,
                vault.pairing_token.clone(),
                vault.id.clone(),
                vault.name.clone(),
                vault.peers.clone(),
            ))
        } else {
            None
        };
        self.pair_code.clear();
        self.endpoint_short = "iniciando".to_owned();
        Ok(())
    }

    fn load_active_vault(&mut self) {
        self.notes = self
            .active_vault_path()
            .map(vault::list_notes)
            .unwrap_or_default();
        self.selected = self.notes.first().cloned();
        let content = self
            .selected
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();
        self.blocks = blocks_from_content(&content);
        self.full_editor_text = content;
        self.loaded_modified_ms = self.selected.as_ref().map_or(0, |p| vault::modified_ms(p));
        self.dirty = false;
        self.last_edit_at = None;
        self.drafts.clear();
        self.active_block = None;
        self.search.clear();
        self.pair_input.clear();
        self.incoming_pair_requests.clear();
        self.pairing_expanded = self.active_peers().is_empty();
    }

    fn switch_vault(&mut self, id: &str) {
        if self.settings.active_vault_id.as_deref() == Some(id) {
            return;
        }
        if !self.save_all_and_sync() {
            return;
        }
        if self.settings.activate_vault(id) {
            self.load_active_vault();
            if let Err(error) = self
                .settings
                .save(&self.paths.settings)
                .and_then(|_| self.start_active_network())
            {
                self.vault_error = Some(error.to_string());
            } else {
                self.vault_error = None;
                self.sync_status = "Preparando conexão".to_owned();
            }
        }
    }

    fn choose_vault(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Escolha a pasta do vault")
            .pick_folder()
        else {
            return;
        };
        match self.settings.add_vault(&folder) {
            Ok(_) => {
                self.load_active_vault();
                if let Err(error) = self
                    .settings
                    .save(&self.paths.settings)
                    .and_then(|_| self.start_active_network())
                {
                    self.vault_error = Some(error.to_string());
                } else {
                    self.vault_error = None;
                    self.sync_status = "Vault pronto para conectar".to_owned();
                }
            }
            Err(error) => self.vault_error = Some(error.to_string()),
        }
    }

    /// Mark that a sidebar/sync toggle just happened, so the smart repaint
    /// keeps the animation frames coming for a short window.
    fn note_panel_toggle(&mut self) {
        self.last_toggle_at = Some(Instant::now());
    }

    fn render_topbar(&mut self, root_ui: &mut egui::Ui, palette: theme::Palette) {
        let ctx = root_ui.ctx().clone();
        let sidebar_visible = self.settings.ui.sidebar_visible;
        let vaults: Vec<_> = self
            .settings
            .vaults
            .iter()
            .map(|v| (v.id.clone(), v.name.clone()))
            .collect();
        let active_vault = self
            .settings
            .active_vault()
            .map(|v| v.name.clone())
            .unwrap_or_else(|| "Escolher vault".to_owned());
        let mut switch_to = None;
        let mut choose_vault = false;

        egui::Panel::top("topbar")
            .frame(
                egui::Frame::new()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::symmetric(14, 9)),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    // Left: toggle sidebar button.
                    let sidebar_icon = if sidebar_visible { "Painel" } else { "Notas" };
                    let sidebar_tip = if sidebar_visible {
                        "Ocultar sidebar (Ctrl+B)"
                    } else {
                        "Mostrar sidebar (Ctrl+B)"
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(sidebar_icon)
                                    .font(ui_semibold(15.0))
                                    .color(palette.muted),
                            )
                            .frame(false)
                            .fill(Color32::TRANSPARENT),
                        )
                        .on_hover_text(sidebar_tip)
                        .clicked()
                    {
                        self.settings.ui.sidebar_visible = !self.settings.ui.sidebar_visible;
                        self.note_panel_toggle();
                        self.save_ui_prefs();
                    }
                    ui.add_space(4.0);

                    ui.menu_button(
                        RichText::new(active_vault)
                            .font(ui_semibold(13.0))
                            .color(palette.ink),
                        |ui| {
                            ui.set_min_width(210.0);
                            for (id, name) in &vaults {
                                let selected =
                                    self.settings.active_vault_id.as_deref() == Some(id.as_str());
                                if ui.selectable_label(selected, name).clicked() {
                                    switch_to = Some(id.clone());
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Adicionar vault…").clicked() {
                                choose_vault = true;
                                ui.close();
                            }
                        },
                    );

                    if let Some(selected_path) = &self.selected {
                        ui.label(
                            RichText::new("/")
                                .font(ui_regular(12.0))
                                .color(palette.border),
                        );
                        let file_name = selected_path
                            .file_name()
                            .and_then(|v| v.to_str())
                            .unwrap_or("Nota");
                        let clean_title = file_name.strip_suffix(".md").unwrap_or(file_name);
                        ui.label(
                            RichText::new(clean_title)
                                .font(ui_semibold(13.5))
                                .color(palette.ink),
                        );
                    }

                    ui.add_space(14.0);

                    // Search field with clear button.
                    let search_width = 220.0_f32.min(ui.available_width() * 0.32);
                    let search_response = ui.add_sized(
                        [search_width, 28.0],
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Buscar notas (Ctrl+/)")
                            .font(ui_regular(12.5))
                            .background_color(palette.surface)
                            .margin(egui::Margin::symmetric(8, 5)),
                    );
                    if self.search_focus_request {
                        search_response.request_focus();
                        self.search_focus_request = false;
                    }
                    if !self.search.is_empty()
                        && ui
                            .add(
                                egui::Button::new(
                                    RichText::new("×")
                                        .font(ui_semibold(12.0))
                                        .color(palette.muted),
                                )
                                .frame(false)
                                .fill(Color32::TRANSPARENT),
                            )
                            .on_hover_text("Limpar busca")
                            .clicked()
                    {
                        self.search.clear();
                    }

                    ui.add_space(10.0);

                    // View mode segmented control (Notion / Split / Preview).
                    let current_mode = self.settings.ui.view_mode;
                    egui::Frame::new()
                        .fill(palette.surface)
                        .stroke(Stroke::new(1.0, palette.border))
                        .corner_radius(7.0)
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let modes = [
                                (ViewMode::Notion, "Escrever", "Editor em blocos (Ctrl+E)"),
                                (
                                    ViewMode::Split,
                                    "Dividir",
                                    "Markdown e leitura lado a lado (Ctrl+E)",
                                ),
                                (ViewMode::Preview, "Ler", "Leitura do Markdown (Ctrl+E)"),
                            ];
                            for (mode, label, tip) in modes {
                                let active = current_mode == mode;
                                let (bg, fg) = if active {
                                    (palette.soft_blue, palette.accent)
                                } else {
                                    (Color32::TRANSPARENT, palette.muted)
                                };
                                let btn = egui::Button::new(
                                    RichText::new(label)
                                        .font(if active {
                                            ui_medium(11.5)
                                        } else {
                                            ui_regular(11.5)
                                        })
                                        .color(fg),
                                )
                                .fill(bg)
                                .stroke(Stroke::NONE)
                                .corner_radius(5.0);
                                if ui.add(btn).on_hover_text(tip).clicked() && !active {
                                    if mode == ViewMode::Split {
                                        self.full_editor_text = content_from_blocks(&self.blocks);
                                    } else if current_mode == ViewMode::Split {
                                        self.blocks = blocks_from_content(&self.full_editor_text);
                                    }
                                    self.settings.ui.view_mode = mode;
                                    self.save_ui_prefs();
                                }
                            }
                        });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Sync panel toggle.
                        let sync_label = if self.settings.ui.sync_panel_visible {
                            "Sync ✓"
                        } else {
                            "Sync"
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(sync_label)
                                        .font(ui_medium(12.0))
                                        .color(palette.ink),
                                )
                                .frame(false)
                                .fill(Color32::TRANSPARENT),
                            )
                            .on_hover_text("Alternar painel de sincronização (Ctrl+Shift+P)")
                            .clicked()
                        {
                            self.settings.ui.sync_panel_visible =
                                !self.settings.ui.sync_panel_visible;
                            self.note_panel_toggle();
                            self.save_ui_prefs();
                        }

                        ui.add_space(8.0);

                        // Theme cycle button: Light → Dark → System → Light.
                        let (theme_glyph, theme_tooltip) = match self.settings.ui.theme {
                            ThemeMode::System => {
                                ("Sistema", "Tema: seguir sistema — clique para claro")
                            }
                            ThemeMode::Light => ("Claro", "Tema: claro — clique para escuro"),
                            ThemeMode::Dark => {
                                ("Escuro", "Tema: escuro — clique para seguir sistema")
                            }
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(theme_glyph)
                                        .font(ui_regular(15.0))
                                        .color(palette.ink),
                                )
                                .frame(false)
                                .fill(Color32::TRANSPARENT),
                            )
                            .on_hover_text(theme_tooltip)
                            .clicked()
                        {
                            let next = match self.settings.ui.theme {
                                ThemeMode::Light => ThemeMode::Dark,
                                ThemeMode::Dark => ThemeMode::System,
                                ThemeMode::System => ThemeMode::Light,
                            };
                            self.settings.ui.theme = next;
                            let new_palette = theme::current_palette(&ctx, next);
                            theme::apply(&ctx, &new_palette);
                            self.applied_theme_is_dark = Some(new_palette.dark);
                            self.save_ui_prefs();
                        }

                        ui.add_space(8.0);

                        // Autosave state. Ctrl+S remains available as "save now".
                        if self.dirty {
                            ui.label(
                                RichText::new("Salvando…")
                                    .font(ui_medium(11.5))
                                    .color(palette.muted),
                            );
                        } else if self
                            .save_feedback_until
                            .is_some_and(|deadline| deadline > Instant::now())
                        {
                            egui::Frame::new()
                                .fill(palette.soft_green)
                                .corner_radius(6.0)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("Salvo")
                                            .font(ui_medium(11.0))
                                            .color(palette.success),
                                    );
                                });
                        }
                    });
                });
            });
        if let Some(id) = switch_to {
            self.switch_vault(&id);
        }
        if choose_vault {
            self.choose_vault();
        }
    }

    fn render_sidebar(&mut self, root_ui: &mut egui::Ui, palette: theme::Palette) {
        let ctx = root_ui.ctx().clone();
        let sidebar_visible = self.settings.ui.sidebar_visible;
        let target_width = if sidebar_visible {
            self.settings.ui.sidebar_width
        } else {
            0.0
        };
        let width = ctx.animate_value_with_time(
            egui::Id::new("sidebar-width"),
            target_width,
            theme::layout::SIDEBAR_ANIMATION_TIME,
        );

        let mut note_to_select: Option<PathBuf> = None;
        let mut note_to_delete: Option<PathBuf> = None;
        let active_path = self.active_vault_path().map(display_path);

        let response = egui::Panel::left("notes")
            .resizable(sidebar_visible)
            .default_size(width)
            .size_range(if sidebar_visible {
                theme::layout::SIDEBAR_MIN_WIDTH..=theme::layout::SIDEBAR_MAX_WIDTH
            } else {
                0.0..=0.0
            })
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::same(theme::layout::SIDEBAR_MARGIN as i8)),
            )
            .show(root_ui, |ui| {
                let inner_width = inner_width_for(width, theme::layout::SIDEBAR_MARGIN);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Nodus")
                            .font(ui_semibold(21.0))
                            .color(palette.ink),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let chevron = egui::Button::new(
                            RichText::new("‹")
                                .font(ui_semibold(16.0))
                                .color(palette.muted),
                        )
                        .frame(false)
                        .fill(Color32::TRANSPARENT);
                        if ui
                            .add(chevron)
                            .on_hover_text("Ocultar sidebar (Ctrl+B)")
                            .clicked()
                        {
                            self.settings.ui.sidebar_visible = false;
                            self.note_panel_toggle();
                            self.save_ui_prefs();
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("local-first")
                                .font(ui_regular(11.0))
                                .color(palette.muted),
                        );
                    });
                });
                if let Some(path) = &active_path {
                    ui.label(
                        RichText::new(path)
                            .font(ui_regular(10.5))
                            .color(palette.muted),
                    );
                }
                ui.add_space(12.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(inner_width, 36.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        let new_note_btn = egui::Button::new(
                            RichText::new("+  Nova nota")
                                .font(ui_medium(13.0))
                                .color(palette.accent),
                        )
                        .fill(if palette.dark {
                            Color32::from_rgb(33, 43, 62)
                        } else {
                            palette.soft_blue
                        })
                        .stroke(Stroke::new(1.0, palette.accent.gamma_multiply(0.35)))
                        .corner_radius(6.0);
                        if ui
                            .add_enabled_ui(active_path.is_some(), |ui| {
                                ui.add_sized([inner_width, 34.0], new_note_btn)
                            })
                            .inner
                            .on_hover_text("Criar uma nota Markdown")
                            .clicked()
                        {
                            self.new_note();
                        }
                    },
                );

                ui.add_space(14.0);
                ui.label(
                    RichText::new("Notas")
                        .font(ui_semibold(12.0))
                        .color(palette.muted),
                );
                ui.add_space(4.0);

                let query = self.search.trim().to_lowercase();
                let notes: Vec<_> = self
                    .notes
                    .iter()
                    .filter(|path| {
                        query.is_empty()
                            || path
                                .file_name()
                                .and_then(|value| value.to_str())
                                .is_some_and(|name| name.to_lowercase().contains(&query))
                    })
                    .cloned()
                    .collect();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for path in notes {
                        let selected = self.selected.as_ref() == Some(&path);
                        let unsaved = self.note_is_unsaved(&path);
                        let file_name = path
                            .file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or("Nota");
                        let clean_title = file_name.strip_suffix(".md").unwrap_or(file_name);

                        let bg_color = if selected {
                            if palette.dark {
                                Color32::from_rgb(36, 46, 68)
                            } else {
                                palette.soft_blue
                            }
                        } else {
                            Color32::TRANSPARENT
                        };

                        let item_rect_approx = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(inner_width - 4.0, 32.0),
                        );
                        let is_hovered = ui.rect_contains_pointer(item_rect_approx);

                        let mut delete_clicked = false;
                        let item_frame = egui::Frame::new()
                            .fill(bg_color)
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(8, 6));

                        let row_resp = item_frame.show(ui, |ui| {
                            ui.set_width(inner_width - 8.0);
                            ui.set_min_height(28.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(clean_title)
                                        .font(if selected {
                                            ui_semibold(13.0)
                                        } else {
                                            ui_regular(13.0)
                                        })
                                        .color(if selected {
                                            palette.ink
                                        } else {
                                            palette.ink.gamma_multiply(0.85)
                                        }),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if is_hovered
                                            && ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new("×")
                                                            .font(ui_regular(11.0))
                                                            .color(palette.muted),
                                                    )
                                                    .frame(false)
                                                    .fill(Color32::TRANSPARENT),
                                                )
                                                .on_hover_text("Excluir nota")
                                                .clicked()
                                        {
                                            delete_clicked = true;
                                            note_to_delete = Some(path.clone());
                                        }

                                        if unsaved {
                                            ui.label(
                                                RichText::new("●")
                                                    .font(ui_regular(8.0))
                                                    .color(palette.warning),
                                            );
                                        }
                                    },
                                );
                            });
                        });
                        let row_interact = ui.interact(
                            row_resp.response.rect,
                            ui.id().with(("note_row", &path)),
                            egui::Sense::click(),
                        );
                        if row_interact.clicked() && !delete_clicked {
                            note_to_select = Some(path.clone());
                        }
                        ui.add_space(2.0);
                    }
                });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui.button("Adicionar vault…").clicked() {
                        self.choose_vault();
                    }
                    ui.label(
                        RichText::new(format!("{} nota(s)", self.notes.len()))
                            .font(ui_regular(11.0))
                            .color(palette.muted),
                    );
                });
            });

        if let Some(target) = note_to_select {
            self.select_note(target);
        }

        if let Some(target) = note_to_delete {
            self.delete_note(&target);
        }

        // Persist any user-driven width change (only when fully expanded and
        // not in the middle of a toggle animation).
        if sidebar_visible && (width - target_width).abs() < 1.0 {
            let actual = response.response.rect.width();
            if (actual - self.settings.ui.sidebar_width).abs() > 1.0 {
                self.settings.ui.sidebar_width = actual;
                self.save_ui_prefs();
            }
        }
    }

    fn render_sync_panel(&mut self, root_ui: &mut egui::Ui, palette: theme::Palette) {
        let ctx = root_ui.ctx().clone();
        let sync_visible = self.settings.ui.sync_panel_visible;
        let target_width = if sync_visible {
            theme::layout::SYNC_WIDTH
        } else {
            0.0
        };
        let width = ctx.animate_value_with_time(
            egui::Id::new("sync-width"),
            target_width,
            theme::layout::SYNC_ANIMATION_TIME,
        );

        // Skip rendering entirely while collapsed (saves layout work and avoids
        // a 1-pixel sliver).
        if width < 1.0 {
            return;
        }

        let inner_width = inner_width_for(width, theme::layout::SYNC_MARGIN);
        let peers = self.active_peers().to_vec();

        egui::Panel::right("sync")
            .exact_size(width)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0, palette.border))
                    .inner_margin(egui::Margin::same(theme::layout::SYNC_MARGIN as i8)),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Sync")
                            .font(ui_semibold(20.0))
                            .color(palette.ink),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let chevron = egui::Button::new(
                            RichText::new("›")
                                .font(ui_semibold(16.0))
                                .color(palette.muted),
                        )
                        .frame(false)
                        .fill(Color32::TRANSPARENT);
                        if ui
                            .add(chevron)
                            .on_hover_text("Ocultar painel de sync (Ctrl+Shift+P)")
                            .clicked()
                        {
                            self.settings.ui.sync_panel_visible = false;
                            self.note_panel_toggle();
                            self.save_ui_prefs();
                        }
                    });
                });
                ui.add_space(10.0);
                let (status_bg, status_color) = match self.sync_tone {
                    StatusTone::Neutral => (palette.bg, palette.muted),
                    StatusTone::Active => (palette.soft_blue, palette.accent),
                    StatusTone::Success => (palette.soft_green, palette.success),
                    StatusTone::Warning => (palette.soft_warning, palette.warning),
                };
                egui::Frame::new()
                    .fill(status_bg)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(inner_width);
                        ui.label(
                            RichText::new(&self.sync_status)
                                .font(ui_medium(12.5))
                                .color(status_color),
                        );
                    });

                ui.add_space(22.0);
                ui.label(
                    RichText::new("Este dispositivo")
                        .font(ui_medium(12.0))
                        .color(palette.muted),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&self.settings.device_name)
                        .font(ui_semibold(15.0))
                        .color(palette.ink),
                );
                ui.label(
                    RichText::new(format!("ID {}", self.endpoint_short))
                        .monospace()
                        .size(10.5)
                        .color(palette.muted),
                );

                if !peers.is_empty() {
                    ui.add_space(22.0);
                    ui.label(
                        RichText::new("Dispositivos pareados")
                            .font(ui_medium(12.0))
                            .color(palette.muted),
                    );
                    ui.add_space(6.0);
                    for peer in &peers {
                        egui::Frame::new()
                            .fill(palette.bg)
                            .corner_radius(7.0)
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_width(inner_width);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&peer.name)
                                            .font(ui_medium(13.0))
                                            .color(palette.ink),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new("Pareado")
                                                    .font(ui_regular(10.5))
                                                    .color(palette.success),
                                            );
                                        },
                                    );
                                });
                            });
                        ui.add_space(6.0);
                    }
                }

                ui.add_space(18.0);
                let pairing_label = if self.pairing_expanded {
                    "Ocultar pareamento"
                } else if peers.is_empty() {
                    "Conectar primeiro dispositivo"
                } else {
                    "Conectar outro dispositivo"
                };
                if ui
                    .add_sized(
                        [inner_width, 36.0],
                        egui::Button::new(RichText::new(pairing_label).font(ui_medium(12.5)))
                            .fill(if self.pairing_expanded {
                                palette.bg
                            } else {
                                palette.soft_blue
                            })
                            .stroke(Stroke::new(1.0, palette.border))
                            .corner_radius(7.0),
                    )
                    .clicked()
                {
                    self.pairing_expanded = !self.pairing_expanded;
                }

                if self.pairing_expanded {
                    ui.add_space(14.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(
                            RichText::new("Compartilhe seu código")
                                .font(ui_semibold(13.0))
                                .color(palette.ink),
                        );
                        ui.label(
                            RichText::new("Envie-o por um canal em que você confia.")
                                .font(ui_regular(11.5))
                                .color(palette.muted),
                        );
                        ui.add_space(7.0);
                        let mut shown_code = self.pair_code.clone();
                        ui.add_sized(
                            [inner_width, 54.0],
                            egui::TextEdit::multiline(&mut shown_code)
                                .font(FontId::monospace(9.5))
                                .interactive(false)
                                .background_color(palette.surface)
                                .margin(egui::Margin::same(7)),
                        );
                        if ui
                            .add_sized(
                                [inner_width, 32.0],
                                egui::Button::new(
                                    RichText::new("Copiar código").font(ui_medium(12.0)),
                                ),
                            )
                            .clicked()
                            && !self.pair_code.is_empty()
                        {
                            ctx.copy_text(self.pair_code.clone());
                        }

                        ui.add_space(14.0);
                        ui.label(
                            RichText::new("Cole o código da outra máquina")
                                .font(ui_semibold(13.0))
                                .color(palette.ink),
                        );
                        ui.add_space(7.0);
                        ui.add_sized(
                            [inner_width, 54.0],
                            egui::TextEdit::multiline(&mut self.pair_input)
                                .font(FontId::monospace(9.5))
                                .hint_text("NODUS3...")
                                .background_color(palette.surface)
                                .margin(egui::Margin::same(7)),
                        );
                        let enabled = !self.pair_input.trim().is_empty()
                            && self.outgoing_pair_pending.is_none();
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(
                                    RichText::new("Adicionar dispositivo")
                                        .font(ui_medium(12.5))
                                        .color(Color32::WHITE),
                                )
                                .fill(palette.accent)
                                .stroke(Stroke::NONE)
                                .corner_radius(7.0)
                                .min_size([inner_width, 36.0].into()),
                            )
                            .clicked()
                        {
                            self.add_peer();
                        }
                        if let Some(peer) = &self.outgoing_pair_pending {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("Aguardando confirmação em {peer}…"))
                                    .font(ui_regular(11.5))
                                    .color(palette.accent),
                            );
                        }
                        if let Some(error) = &self.pair_error {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(error)
                                    .font(ui_regular(11.0))
                                    .color(palette.warning),
                            );
                        }
                    });
                }
            });
    }

    fn render_editor(&mut self, root_ui: &mut egui::Ui, palette: theme::Palette) {
        if self.selected.is_none() {
            let has_vault = self.settings.active_vault().is_some();
            let vault_error = self.vault_error.clone();
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(palette.bg))
                .show(root_ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() * 0.28);
                        ui.label(RichText::new(if has_vault { "Seu vault está vazio" } else { "Suas notas, na sua pasta" }).font(serif_semibold(30.0)).color(palette.ink));
                        ui.add_space(8.0);
                        ui.label(RichText::new(if has_vault { "Crie a primeira nota Markdown para começar." } else { "Escolha uma pasta existente. O Nodus lembrará dela entre atualizações." }).font(ui_regular(14.0)).color(palette.muted));
                        ui.add_space(20.0);
                        if has_vault {
                            if ui.add(egui::Button::new("Criar primeira nota").fill(palette.accent)).clicked() { self.new_note(); }
                        } else if ui.add(egui::Button::new("Escolher pasta do vault").fill(palette.accent)).clicked() { self.choose_vault(); }
                        if let Some(error) = &vault_error { ui.add_space(10.0); ui.label(RichText::new(error).color(palette.warning)); }
                    });
                });
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.bg))
            .show(root_ui, |ui| {
                if let Some(error) = &self.save_error {
                    ui.label(
                        RichText::new(error)
                            .font(ui_regular(12.0))
                            .color(palette.warning),
                    );
                    ui.add_space(8.0);
                }

                if self.settings.ui.view_mode == ViewMode::Split {
                    self.render_split_editor(ui, palette);
                } else {
                    let available = ui.available_size();
                    let page_width = available.x.min(760.0);
                    let side_space = ((available.x - page_width) / 2.0).max(18.0);

                    ui.horizontal_top(|ui| {
                        ui.add_space(side_space);
                        ui.vertical(|ui| {
                            ui.set_width(page_width);
                            ui.add_space(14.0);
                            match self.settings.ui.view_mode {
                                ViewMode::Notion => self.render_notion_blocks(ui, palette),
                                ViewMode::Preview => self.render_preview_mode(ui, palette),
                                ViewMode::Split => unreachable!(),
                            }
                        });
                    });
                }
            });
    }

    /// Notion-like block editor: visual typography per block, interactive checkboxes,
    /// auto-split on Enter, and floating slash menu.
    fn render_notion_blocks(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let implicit_uri = self
            .selected
            .as_ref()
            .and_then(|path| path.parent())
            .map(file_uri_prefix)
            .unwrap_or_else(|| "file:///".to_owned());

        let total = self.blocks.len();
        let mut new_active = self.active_block;
        let mut pending_focus_block = self.pending_focus;
        let mut block_to_insert: Option<(usize, Block)> = None;
        let mut block_to_remove: Option<usize> = None;
        let mut text_changed = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for idx in 0..total {
                    let is_active = self.active_block == Some(idx);
                    let block = &mut self.blocks[idx];

                    if is_active {
                        let (font, min_height) = match &block.kind {
                            BlockKind::Heading1 => (serif_semibold(28.0), 38.0),
                            BlockKind::Heading2 => (serif_semibold(22.0), 32.0),
                            BlockKind::Heading3 => (serif_semibold(18.0), 28.0),
                            BlockKind::Code { .. } => (FontId::monospace(13.0), 30.0),
                            _ => (serif_regular(16.5), 26.0),
                        };

                        let frame_text = egui::Frame::new()
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .inner_margin(egui::Margin::symmetric(2, 2));

                        let (response, submitted) = match &mut block.kind {
                            BlockKind::Checklist(checked) => {
                                ui.horizontal(|ui| {
                                    let mut chk = *checked;
                                    if ui.checkbox(&mut chk, "").changed() {
                                        *checked = chk;
                                        text_changed = true;
                                    }
                                    let resp = ui.add_sized(
                                        egui::vec2(ui.available_width(), min_height),
                                        egui::TextEdit::singleline(&mut block.text)
                                            .font(font)
                                            .text_color(palette.ink)
                                            .frame(frame_text)
                                            .hint_text("Item de checklist..."),
                                    );
                                    let sub = resp.lost_focus()
                                        && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                                    (resp, sub)
                                })
                                .inner
                            }
                            BlockKind::Bullet => {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("•")
                                            .font(ui_semibold(17.0))
                                            .color(palette.accent),
                                    );
                                    let resp = ui.add_sized(
                                        egui::vec2(ui.available_width(), min_height),
                                        egui::TextEdit::singleline(&mut block.text)
                                            .font(font)
                                            .text_color(palette.ink)
                                            .frame(frame_text)
                                            .hint_text("Item da lista..."),
                                    );
                                    let sub = resp.lost_focus()
                                        && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                                    (resp, sub)
                                })
                                .inner
                            }
                            BlockKind::Numbered(n) => {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{n}."))
                                            .font(ui_semibold(14.5))
                                            .color(palette.muted),
                                    );
                                    let resp = ui.add_sized(
                                        egui::vec2(ui.available_width(), min_height),
                                        egui::TextEdit::singleline(&mut block.text)
                                            .font(font)
                                            .text_color(palette.ink)
                                            .frame(frame_text)
                                            .hint_text("Item numerado..."),
                                    );
                                    let sub = resp.lost_focus()
                                        && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                                    (resp, sub)
                                })
                                .inner
                            }
                            BlockKind::Heading1 => {
                                let resp = ui.add_sized(
                                    egui::vec2(ui.available_width(), min_height),
                                    egui::TextEdit::singleline(&mut block.text)
                                        .font(font)
                                        .text_color(palette.ink)
                                        .frame(frame_text)
                                        .hint_text("Título 1..."),
                                );
                                let sub = resp.lost_focus()
                                    && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                                (resp, sub)
                            }
                            BlockKind::Heading2 => {
                                let resp = ui.add_sized(
                                    egui::vec2(ui.available_width(), min_height),
                                    egui::TextEdit::singleline(&mut block.text)
                                        .font(font)
                                        .text_color(palette.ink)
                                        .frame(frame_text)
                                        .hint_text("Título 2..."),
                                );
                                let sub = resp.lost_focus()
                                    && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                                (resp, sub)
                            }
                            BlockKind::Heading3 => {
                                let resp = ui.add_sized(
                                    egui::vec2(ui.available_width(), min_height),
                                    egui::TextEdit::singleline(&mut block.text)
                                        .font(font)
                                        .text_color(palette.ink)
                                        .frame(frame_text)
                                        .hint_text("Título 3..."),
                                );
                                let sub = resp.lost_focus()
                                    && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                                (resp, sub)
                            }
                            BlockKind::Quote => {
                                let quote_frame = egui::Frame::new()
                                    .fill(if palette.dark {
                                        Color32::from_rgb(30, 34, 43)
                                    } else {
                                        palette.soft_blue
                                    })
                                    .stroke(Stroke::new(3.0, palette.accent))
                                    .corner_radius(egui::CornerRadius {
                                        nw: 4,
                                        sw: 4,
                                        ne: 0,
                                        se: 0,
                                    })
                                    .inner_margin(egui::Margin {
                                        left: 14,
                                        right: 10,
                                        top: 7,
                                        bottom: 7,
                                    });
                                quote_frame
                                    .show(ui, |ui| {
                                        let resp = ui.add_sized(
                                            egui::vec2(ui.available_width(), min_height),
                                            egui::TextEdit::multiline(&mut block.text)
                                                .font(font)
                                                .text_color(palette.ink)
                                                .frame(frame_text)
                                                .hint_text("Citação..."),
                                        );
                                        (resp, false)
                                    })
                                    .inner
                            }
                            BlockKind::Code { lang } => {
                                let code_frame = egui::Frame::new()
                                    .fill(if palette.dark {
                                        Color32::from_rgb(20, 23, 30)
                                    } else {
                                        Color32::from_rgb(240, 243, 248)
                                    })
                                    .stroke(Stroke::new(1.0, palette.border))
                                    .corner_radius(6.0)
                                    .inner_margin(egui::Margin::symmetric(12, 10));
                                code_frame
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new("linguagem:")
                                                    .font(ui_regular(10.5))
                                                    .color(palette.muted),
                                            );
                                            ui.add_sized(
                                                [80.0, 20.0],
                                                egui::TextEdit::singleline(lang)
                                                    .font(ui_medium(11.0))
                                                    .frame(egui::Frame::NONE),
                                            );
                                        });
                                        ui.add_space(4.0);
                                        let resp = ui.add_sized(
                                            egui::vec2(ui.available_width(), min_height),
                                            egui::TextEdit::multiline(&mut block.text)
                                                .font(font)
                                                .text_color(palette.ink)
                                                .frame(frame_text)
                                                .hint_text("Código..."),
                                        );
                                        (resp, false)
                                    })
                                    .inner
                            }
                            BlockKind::Divider => {
                                ui.separator();
                                let resp = ui.label(
                                    RichText::new("Divisor (pressione Backspace para remover)")
                                        .font(ui_regular(11.0))
                                        .color(palette.muted),
                                );
                                (resp, false)
                            }
                            BlockKind::Paragraph => {
                                let resp = ui.add_sized(
                                    egui::vec2(ui.available_width(), min_height),
                                    egui::TextEdit::multiline(&mut block.text)
                                        .font(font)
                                        .text_color(palette.ink)
                                        .frame(frame_text)
                                        .hint_text(
                                            "Digite '/' para comandos ou comece a escrever...",
                                        ),
                                );
                                (resp, false)
                            }
                        };

                        if response.changed() {
                            text_changed = true;

                            if block.kind == BlockKind::Paragraph {
                                if let Some(rest) = block.text.strip_prefix("### ") {
                                    block.kind = BlockKind::Heading3;
                                    block.text = rest.to_string();
                                } else if let Some(rest) = block.text.strip_prefix("## ") {
                                    block.kind = BlockKind::Heading2;
                                    block.text = rest.to_string();
                                } else if let Some(rest) = block.text.strip_prefix("# ") {
                                    block.kind = BlockKind::Heading1;
                                    block.text = rest.to_string();
                                } else if let Some(rest) = block
                                    .text
                                    .strip_prefix("- [ ] ")
                                    .or_else(|| block.text.strip_prefix("[] "))
                                {
                                    block.kind = BlockKind::Checklist(false);
                                    block.text = rest.to_string();
                                } else if let Some(rest) = block
                                    .text
                                    .strip_prefix("- ")
                                    .or_else(|| block.text.strip_prefix("* "))
                                {
                                    block.kind = BlockKind::Bullet;
                                    block.text = rest.to_string();
                                } else if let Some(rest) = block.text.strip_prefix("1. ") {
                                    block.kind = BlockKind::Numbered(1);
                                    block.text = rest.to_string();
                                } else if let Some(rest) = block.text.strip_prefix("> ") {
                                    block.kind = BlockKind::Quote;
                                    block.text = rest.to_string();
                                } else if block.text.trim() == "---" {
                                    block.kind = BlockKind::Divider;
                                    block.text.clear();
                                } else if block.text.starts_with("```") {
                                    let lang =
                                        block.text.trim_start_matches("```").trim().to_string();
                                    block.kind = BlockKind::Code { lang };
                                    block.text.clear();
                                }
                            }

                            if block.text.starts_with('/') {
                                self.slash_open = true;
                            }

                            if block.kind == BlockKind::Paragraph && block.text.ends_with('\n') {
                                let is_shift = ui.ctx().input(|i| i.modifiers.shift);
                                if !is_shift {
                                    block.text.pop();
                                    block_to_insert = Some((idx + 1, Block::paragraph("")));
                                }
                            }
                        }

                        if submitted {
                            match &block.kind {
                                BlockKind::Checklist(_) => {
                                    if block.text.trim().is_empty() {
                                        block.kind = BlockKind::Paragraph;
                                    } else {
                                        block_to_insert =
                                            Some((idx + 1, Block::checklist(false, "")));
                                    }
                                }
                                BlockKind::Bullet => {
                                    if block.text.trim().is_empty() {
                                        block.kind = BlockKind::Paragraph;
                                    } else {
                                        block_to_insert = Some((idx + 1, Block::bullet("")));
                                    }
                                }
                                BlockKind::Numbered(n) => {
                                    if block.text.trim().is_empty() {
                                        block.kind = BlockKind::Paragraph;
                                    } else {
                                        block_to_insert =
                                            Some((idx + 1, Block::numbered(n + 1, "")));
                                    }
                                }
                                BlockKind::Heading1 | BlockKind::Heading2 | BlockKind::Heading3 => {
                                    block_to_insert = Some((idx + 1, Block::paragraph("")));
                                }
                                _ => {}
                            }
                        }

                        if pending_focus_block == Some(idx) {
                            response.request_focus();
                            pending_focus_block = None;
                        }

                        if response.has_focus() {
                            let (esc_pressed, backspace_pressed) = ui.ctx().input(|input| {
                                (
                                    input.key_pressed(egui::Key::Escape),
                                    input.key_pressed(egui::Key::Backspace),
                                )
                            });

                            if esc_pressed {
                                if self.slash_open {
                                    block.text = strip_slash_trigger(&block.text);
                                    self.slash_open = false;
                                    text_changed = true;
                                } else {
                                    response.surrender_focus();
                                    new_active = None;
                                }
                            }

                            if backspace_pressed && block.text.is_empty() {
                                if block.kind != BlockKind::Paragraph {
                                    block.kind = BlockKind::Paragraph;
                                    text_changed = true;
                                } else if total > 1 && idx > 0 {
                                    block_to_remove = Some(idx);
                                }
                            }
                        } else if response.lost_focus()
                            && pending_focus_block.is_none()
                            && !self.slash_open
                        {
                            new_active = None;
                        }
                    } else {
                        // Visual rendered block
                        match &mut block.kind {
                            BlockKind::Heading1 => {
                                ui.add_space(8.0);
                                let resp = ui.add(
                                    egui::Label::new(
                                        RichText::new(&block.text)
                                            .font(serif_semibold(28.0))
                                            .color(palette.ink),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                let mut row_rect = resp.rect;
                                row_rect.min.x = ui.max_rect().left();
                                row_rect.max.x = ui.max_rect().right();
                                let click_resp = ui.interact(
                                    row_rect,
                                    ui.id().with(("h1", idx)),
                                    egui::Sense::click(),
                                );
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Heading2 => {
                                ui.add_space(6.0);
                                let resp = ui.add(
                                    egui::Label::new(
                                        RichText::new(&block.text)
                                            .font(serif_semibold(22.0))
                                            .color(palette.ink),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                let mut row_rect = resp.rect;
                                row_rect.min.x = ui.max_rect().left();
                                row_rect.max.x = ui.max_rect().right();
                                let click_resp = ui.interact(
                                    row_rect,
                                    ui.id().with(("h2", idx)),
                                    egui::Sense::click(),
                                );
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Heading3 => {
                                ui.add_space(4.0);
                                let resp = ui.add(
                                    egui::Label::new(
                                        RichText::new(&block.text)
                                            .font(serif_semibold(18.0))
                                            .color(palette.ink),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                let mut row_rect = resp.rect;
                                row_rect.min.x = ui.max_rect().left();
                                row_rect.max.x = ui.max_rect().right();
                                let click_resp = ui.interact(
                                    row_rect,
                                    ui.id().with(("h3", idx)),
                                    egui::Sense::click(),
                                );
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Checklist(checked) => {
                                let mut is_checked = *checked;
                                let mut checked_changed = false;
                                let row_id = ui.id().with(("chk_row", idx));
                                let row_resp = ui.horizontal(|ui| {
                                    let cb = ui.checkbox(&mut is_checked, "");
                                    if cb.changed() {
                                        checked_changed = true;
                                    }
                                    let txt = if is_checked {
                                        RichText::new(&block.text)
                                            .font(serif_regular(16.5))
                                            .color(palette.muted)
                                            .strikethrough()
                                    } else {
                                        RichText::new(&block.text)
                                            .font(serif_regular(16.5))
                                            .color(palette.ink)
                                    };
                                    ui.add(egui::Label::new(txt));
                                });
                                if checked_changed {
                                    *checked = is_checked;
                                    text_changed = true;
                                }
                                let mut row_rect = row_resp.response.rect;
                                row_rect.min.x = ui.max_rect().left();
                                row_rect.max.x = ui.max_rect().right();
                                let mut text_click_rect = row_rect;
                                text_click_rect.min.x = row_resp.response.rect.min.x + 24.0;
                                let click_resp =
                                    ui.interact(text_click_rect, row_id, egui::Sense::click());
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() && !checked_changed {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Bullet => {
                                let row_id = ui.id().with(("bullet_row", idx));
                                let row_resp = ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("•")
                                            .font(ui_semibold(17.0))
                                            .color(palette.accent),
                                    );
                                    ui.add(egui::Label::new(
                                        RichText::new(&block.text)
                                            .font(serif_regular(16.5))
                                            .color(palette.ink),
                                    ));
                                });
                                let mut row_rect = row_resp.response.rect;
                                row_rect.min.x = ui.max_rect().left();
                                row_rect.max.x = ui.max_rect().right();
                                let click_resp =
                                    ui.interact(row_rect, row_id, egui::Sense::click());
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Numbered(n) => {
                                let row_id = ui.id().with(("num_row", idx));
                                let row_resp = ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{n}."))
                                            .font(ui_semibold(14.5))
                                            .color(palette.muted),
                                    );
                                    ui.add(egui::Label::new(
                                        RichText::new(&block.text)
                                            .font(serif_regular(16.5))
                                            .color(palette.ink),
                                    ));
                                });
                                let mut row_rect = row_resp.response.rect;
                                row_rect.min.x = ui.max_rect().left();
                                row_rect.max.x = ui.max_rect().right();
                                let click_resp =
                                    ui.interact(row_rect, row_id, egui::Sense::click());
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Quote => {
                                let quote_frame = egui::Frame::new()
                                    .fill(if palette.dark {
                                        Color32::from_rgb(30, 34, 43)
                                    } else {
                                        palette.soft_blue
                                    })
                                    .stroke(Stroke::new(3.0, palette.accent))
                                    .corner_radius(egui::CornerRadius {
                                        nw: 4,
                                        sw: 4,
                                        ne: 0,
                                        se: 0,
                                    })
                                    .inner_margin(egui::Margin {
                                        left: 14,
                                        right: 10,
                                        top: 7,
                                        bottom: 7,
                                    });
                                let quote_resp = quote_frame.show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add(egui::Label::new(
                                        RichText::new(&block.text)
                                            .font(serif_regular(16.5))
                                            .italics()
                                            .color(palette.ink),
                                    ));
                                });
                                let click_resp = ui.interact(
                                    quote_resp.response.rect,
                                    ui.id().with(("quote_row", idx)),
                                    egui::Sense::click(),
                                );
                                if click_resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if click_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Code { lang } => {
                                let code_frame = egui::Frame::new()
                                    .fill(if palette.dark {
                                        Color32::from_rgb(20, 23, 30)
                                    } else {
                                        Color32::from_rgb(240, 243, 248)
                                    })
                                    .stroke(Stroke::new(1.0, palette.border))
                                    .corner_radius(6.0)
                                    .inner_margin(egui::Margin::symmetric(12, 10));
                                code_frame.show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if !lang.is_empty() {
                                        ui.label(
                                            RichText::new(lang.as_str())
                                                .font(ui_medium(11.0))
                                                .color(palette.muted),
                                        );
                                        ui.add_space(2.0);
                                    }
                                    let label_resp = ui.add(
                                        egui::Label::new(
                                            RichText::new(&block.text)
                                                .font(FontId::monospace(13.0))
                                                .color(palette.ink),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if label_resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                    }
                                    if label_resp.clicked() {
                                        new_active = Some(idx);
                                        pending_focus_block = Some(idx);
                                    }
                                });
                            }
                            BlockKind::Divider => {
                                let sep = ui.add(egui::Separator::default().spacing(16.0));
                                let sep_resp = ui.interact(
                                    sep.rect,
                                    ui.id().with(("sep", idx)),
                                    egui::Sense::click(),
                                );
                                if sep_resp.clicked() {
                                    new_active = Some(idx);
                                    pending_focus_block = Some(idx);
                                }
                            }
                            BlockKind::Paragraph => {
                                let block_id = ui.id().with(("block_p", idx));
                                let is_placeholder = block.text.trim().is_empty();
                                let display_text = if is_placeholder {
                                    "Comece a escrever ou digite '/' para comandos..."
                                } else {
                                    &block.text
                                };
                                let text_color = if is_placeholder {
                                    palette.muted
                                } else {
                                    palette.ink
                                };

                                if !is_placeholder
                                    && (block.text.contains('*')
                                        || block.text.contains('`')
                                        || block.text.contains('['))
                                {
                                    let inner_resp = ui.scope(|ui| {
                                        // egui maps `strong()` to the active-widget color.
                                        // Keep Markdown bold text readable in both themes.
                                        ui.visuals_mut().widgets.active.fg_stroke.color =
                                            palette.ink;
                                        CommonMarkViewer::new()
                                            .indentation_spaces(2)
                                            .max_image_width(Some(ui.available_width() as usize))
                                            .default_width(Some(ui.available_width() as usize))
                                            .default_implicit_uri_scheme(implicit_uri.clone())
                                            .show(ui, &mut self.markdown_cache, &block.text)
                                    });
                                    let mut block_rect = inner_resp.response.rect;
                                    if block_rect.height() < 24.0 {
                                        block_rect.max.y = block_rect.min.y + 24.0;
                                    }
                                    block_rect.min.x = ui.max_rect().left();
                                    block_rect.max.x = ui.max_rect().right();
                                    let click_resp =
                                        ui.interact(block_rect, block_id, egui::Sense::click());
                                    if click_resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                    }
                                    if click_resp.clicked() {
                                        new_active = Some(idx);
                                        pending_focus_block = Some(idx);
                                    }
                                } else {
                                    let resp = ui.add(
                                        egui::Label::new(
                                            RichText::new(display_text)
                                                .font(serif_regular(16.5))
                                                .color(text_color),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                    }
                                    if resp.clicked() {
                                        new_active = Some(idx);
                                        pending_focus_block = Some(idx);
                                    }
                                }
                            }
                        }
                    }

                    ui.add_space(4.0);

                    if is_active
                        && self.slash_open
                        && let Some(chosen_kind) = Self::render_slash_menu(
                            ui,
                            palette,
                            &block.text,
                            &mut self.slash_selected_index,
                        )
                    {
                        block.kind = chosen_kind;
                        block.text = strip_slash_trigger(&block.text);
                        self.slash_open = false;
                        self.slash_selected_index = 0;
                        text_changed = true;
                        pending_focus_block = Some(idx);
                    }
                }

                let available = ui.available_size();
                if available.y > 40.0 {
                    let (_, empty_resp) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), available.y.max(120.0)),
                        egui::Sense::click(),
                    );
                    if empty_resp.clicked() {
                        if self
                            .blocks
                            .last()
                            .map(|b| b.text.trim().is_empty() && b.kind == BlockKind::Paragraph)
                            .unwrap_or(false)
                        {
                            let last_idx = self.blocks.len().saturating_sub(1);
                            new_active = Some(last_idx);
                            pending_focus_block = Some(last_idx);
                        } else {
                            let next_idx = self.blocks.len();
                            block_to_insert = Some((next_idx, Block::paragraph("")));
                        }
                    }
                }
            });

        if let Some(rem_idx) = block_to_remove {
            if self.blocks.len() > 1 {
                self.blocks.remove(rem_idx);
                new_active = Some(rem_idx.saturating_sub(1));
                pending_focus_block = new_active;
                text_changed = true;
            } else if let Some(first) = self.blocks.first_mut() {
                first.kind = BlockKind::Paragraph;
                first.text.clear();
                new_active = Some(0);
                pending_focus_block = Some(0);
                text_changed = true;
            }
        }

        if let Some((ins_idx, ins_block)) = block_to_insert {
            let actual_idx = ins_idx.min(self.blocks.len());
            self.blocks.insert(actual_idx, ins_block);
            new_active = Some(actual_idx);
            pending_focus_block = Some(actual_idx);
            text_changed = true;
        }

        if text_changed {
            self.mark_dirty();
            self.save_feedback_until = None;
            self.full_editor_text = content_from_blocks(&self.blocks);
        }

        if self.slash_open && self.active_block.is_none() {
            self.slash_open = false;
            self.slash_selected_index = 0;
        }

        self.active_block = new_active;
        self.pending_focus = pending_focus_block;
    }

    /// Split View: Markdown editor on the left, live CommonMark preview on the right.
    fn render_split_editor(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let implicit_uri = self
            .selected
            .as_ref()
            .and_then(|path| path.parent())
            .map(file_uri_prefix)
            .unwrap_or_else(|| "file:///".to_owned());

        let total_width = ui.available_width();
        let col_width = ((total_width - 24.0) / 2.0).max(120.0);

        ui.horizontal_top(|ui| {
            egui::Frame::new()
                .fill(palette.surface)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(col_width);
                    ui.set_min_height((ui.available_height() - 8.0).max(400.0));
                    let resp = ui.add_sized(
                        egui::vec2(col_width - 16.0, (ui.available_height() - 20.0).max(380.0)),
                        egui::TextEdit::multiline(&mut self.full_editor_text)
                            .font(FontId::monospace(13.0))
                            .text_color(palette.ink)
                            .frame(egui::Frame::NONE),
                    );
                    if resp.changed() {
                        self.mark_dirty();
                        self.save_feedback_until = None;
                        self.blocks = blocks_from_content(&self.full_editor_text);
                    }
                });

            ui.add_space(8.0);

            egui::Frame::new()
                .fill(palette.surface)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(col_width);
                    ui.set_min_height((ui.available_height() - 8.0).max(400.0));
                    egui::ScrollArea::vertical()
                        .id_salt("split_preview_scroll")
                        .show(ui, |ui| {
                            ui.visuals_mut().widgets.active.fg_stroke.color = palette.ink;
                            CommonMarkViewer::new()
                                .indentation_spaces(2)
                                .max_image_width(Some(ui.available_width() as usize))
                                .default_width(Some(ui.available_width() as usize))
                                .default_implicit_uri_scheme(implicit_uri)
                                .show_mut(ui, &mut self.markdown_cache, &mut self.full_editor_text);
                        });
                });
        });
    }

    /// Pure reading / preview mode with full rendered Markdown.
    fn render_preview_mode(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        let implicit_uri = self
            .selected
            .as_ref()
            .and_then(|path| path.parent())
            .map(file_uri_prefix)
            .unwrap_or_else(|| "file:///".to_owned());

        let original_text = content_from_blocks(&self.blocks);
        let mut full_text = original_text.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.visuals_mut().widgets.active.fg_stroke.color = palette.ink;
            CommonMarkViewer::new()
                .indentation_spaces(2)
                .max_image_width(Some(ui.available_width() as usize))
                .default_width(Some(ui.available_width() as usize))
                .default_implicit_uri_scheme(implicit_uri)
                .show_mut(ui, &mut self.markdown_cache, &mut full_text);
        });
        if full_text != original_text {
            self.blocks = blocks_from_content(&full_text);
            self.full_editor_text = full_text;
            self.mark_dirty();
        }
    }

    /// Render the slash command menu as a floating dropdown under the active block.
    fn render_slash_menu(
        ui: &mut egui::Ui,
        palette: theme::Palette,
        block_text: &str,
        slash_selected_index: &mut usize,
    ) -> Option<BlockKind> {
        let raw_query = block_text.strip_prefix('/').unwrap_or("");
        let query = raw_query.trim().to_lowercase();
        let matching: Vec<&SlashOption> = SLASH_COMMANDS
            .iter()
            .filter(|cmd| {
                query.is_empty()
                    || cmd.label.to_lowercase().contains(&query)
                    || cmd.keywords.contains(&query)
            })
            .collect();

        if matching.is_empty() {
            return None;
        }

        let (up, down, enter) = ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
            )
        });

        if down {
            *slash_selected_index =
                (*slash_selected_index + 1).min(matching.len().saturating_sub(1));
        }
        if up {
            *slash_selected_index = slash_selected_index.saturating_sub(1);
        }
        if enter && let Some(cmd) = matching.get(*slash_selected_index) {
            return Some(cmd.kind.clone());
        }

        let mut chosen = None;

        egui::Frame::new()
            .fill(palette.surface)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(8.0)
            .shadow(egui::epaint::Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(if palette.dark { 50 } else { 20 }),
            })
            .inner_margin(egui::Margin::symmetric(6, 6))
            .show(ui, |ui| {
                ui.set_width(280.0);
                ui.label(
                    RichText::new("COMANDOS BÁSICOS")
                        .font(ui_semibold(10.0))
                        .color(palette.muted),
                );
                ui.add_space(4.0);

                for (i, cmd) in matching.iter().take(8).enumerate() {
                    let is_selected = i == *slash_selected_index;
                    let (bg, fg) = if is_selected {
                        (palette.soft_blue, palette.accent)
                    } else {
                        (Color32::TRANSPARENT, palette.ink)
                    };

                    let item_btn = egui::Button::new(
                        RichText::new(format!("{}   {}", cmd.icon, cmd.label))
                            .font(if is_selected {
                                ui_medium(12.5)
                            } else {
                                ui_regular(12.5)
                            })
                            .color(fg),
                    )
                    .fill(bg)
                    .stroke(Stroke::NONE)
                    .corner_radius(5.0);

                    let resp = ui.add_sized([268.0, 26.0], item_btn);
                    if resp.on_hover_text(cmd.desc).clicked() {
                        chosen = Some(cmd.kind.clone());
                    }
                }
            });

        chosen
    }

    fn render_close_dialog(&mut self, ctx: &egui::Context, palette: theme::Palette) {
        if !self.close_dialog {
            return;
        }
        let count = self.unsaved_count();
        let frame = egui::Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(12.0)
            .inner_margin(egui::Margin::same(24))
            .shadow(egui::epaint::Shadow {
                offset: [0, 10],
                blur: 36,
                spread: 0,
                color: Color32::from_black_alpha(45),
            });
        egui::Modal::new(egui::Id::new("unsaved-close"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(90))
            .show(ctx, |ui| {
                ui.set_width(430.0);
                ui.label(
                    RichText::new("Salvar antes de fechar?")
                        .font(ui_semibold(20.0))
                        .color(palette.ink),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "Você tem {count} {} não {}. Salve para manter suas alterações.",
                        if count == 1 { "nota" } else { "notas" },
                        if count == 1 { "salva" } else { "salvas" }
                    ))
                    .font(ui_regular(14.0))
                    .color(palette.muted),
                );
                if let Some(error) = &self.save_error {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(error)
                            .font(ui_regular(12.0))
                            .color(palette.warning),
                    );
                }
                ui.add_space(20.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Salvar e fechar")
                                    .font(ui_medium(13.0))
                                    .color(Color32::WHITE),
                            )
                            .fill(palette.accent)
                            .stroke(Stroke::NONE)
                            .corner_radius(7.0),
                        )
                        .clicked()
                        && self.save_all_and_sync()
                    {
                        self.allow_close = true;
                        self.close_dialog = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui
                        .button(RichText::new("Cancelar").font(ui_medium(13.0)))
                        .clicked()
                    {
                        self.close_dialog = false;
                    }
                    if ui
                        .button(
                            RichText::new("Descartar alterações")
                                .font(ui_medium(13.0))
                                .color(palette.warning),
                        )
                        .clicked()
                    {
                        self.drafts.clear();
                        self.dirty = false;
                        self.allow_close = true;
                        self.close_dialog = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }

    fn render_pair_request_dialog(&mut self, ctx: &egui::Context, palette: theme::Palette) {
        let Some((request_id, peer)) = self.incoming_pair_requests.front().cloned() else {
            return;
        };
        let frame = egui::Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(12.0)
            .inner_margin(egui::Margin::same(24))
            .shadow(egui::epaint::Shadow {
                offset: [0, 10],
                blur: 36,
                spread: 0,
                color: Color32::from_black_alpha(45),
            });
        egui::Modal::new(egui::Id::new("incoming-pair-request"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(90))
            .show(ctx, |ui| {
                ui.set_width(430.0);
                ui.label(
                    RichText::new("Novo dispositivo quer se conectar")
                        .font(ui_semibold(20.0))
                        .color(palette.ink),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "{} solicitou acesso às notas deste dispositivo.",
                        peer.name
                    ))
                    .font(ui_regular(14.0))
                    .color(palette.muted),
                );
                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(palette.bg)
                    .corner_radius(7.0)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "ID {}",
                                peer.endpoint_id.chars().take(12).collect::<String>()
                            ))
                            .monospace()
                            .size(11.0)
                            .color(palette.muted),
                        );
                    });
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "Aceite apenas se você iniciou este pareamento no outro computador.",
                    )
                    .font(ui_regular(12.0))
                    .color(palette.muted),
                );
                ui.add_space(20.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Aceitar conexão")
                                    .font(ui_medium(13.0))
                                    .color(Color32::WHITE),
                            )
                            .fill(palette.accent)
                            .stroke(Stroke::NONE)
                            .corner_radius(7.0),
                        )
                        .clicked()
                    {
                        if let Some(network) = &self.network {
                            network.answer_pair(request_id.clone(), true);
                        }
                        self.incoming_pair_requests.pop_front();
                        self.sync_status = format!("Aceitando {}…", peer.name);
                        self.sync_tone = StatusTone::Active;
                    }
                    if ui
                        .button(RichText::new("Recusar").font(ui_medium(13.0)))
                        .clicked()
                    {
                        if let Some(network) = &self.network {
                            network.answer_pair(request_id.clone(), false);
                        }
                        self.incoming_pair_requests.pop_front();
                        self.sync_status = format!("Conexão de {} recusada", peer.name);
                        self.sync_tone = StatusTone::Neutral;
                    }
                });
            });
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !self.allow_close
            && ctx.input(|input| input.viewport().close_requested())
            && self.unsaved_count() > 0
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
    }
}

impl eframe::App for NodusApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        self.poll_network();
        self.handle_close_request(&ctx);
        if self.last_scan.elapsed() >= Duration::from_millis(900) {
            self.refresh_notes();
            self.last_scan = Instant::now();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::S,
            ))
        }) {
            self.save_and_sync();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::B,
            ))
        }) {
            self.settings.ui.sidebar_visible = !self.settings.ui.sidebar_visible;
            self.note_panel_toggle();
            self.save_ui_prefs();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                egui::Key::P,
            ))
        }) {
            self.settings.ui.sync_panel_visible = !self.settings.ui.sync_panel_visible;
            self.note_panel_toggle();
            self.save_ui_prefs();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::Slash,
            ))
        }) {
            self.search_focus_request = true;
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::E,
            ))
        }) {
            let next = match self.settings.ui.view_mode {
                ViewMode::Notion => ViewMode::Split,
                ViewMode::Split => ViewMode::Preview,
                ViewMode::Preview => ViewMode::Notion,
            };
            if next == ViewMode::Split {
                self.full_editor_text = content_from_blocks(&self.blocks);
            } else if self.settings.ui.view_mode == ViewMode::Split {
                self.blocks = blocks_from_content(&self.full_editor_text);
            }
            self.settings.ui.view_mode = next;
            self.save_ui_prefs();
        }

        if let Some(error) = &self.fatal_error {
            let palette = theme::current_palette(&ctx, self.settings.ui.theme);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(palette.bg))
                .show(root_ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.label(
                            RichText::new("Nodus não conseguiu iniciar")
                                .font(ui_semibold(22.0))
                                .color(palette.ink),
                        );
                        ui.label(
                            RichText::new(error)
                                .font(ui_regular(14.0))
                                .color(palette.warning),
                        );
                    });
                });
            return;
        }

        let palette = theme::current_palette(&ctx, self.settings.ui.theme);
        if self.applied_theme_is_dark != Some(palette.dark) {
            theme::apply(&ctx, &palette);
            self.applied_theme_is_dark = Some(palette.dark);
        }
        self.render_topbar(root_ui, palette);
        self.render_sidebar(root_ui, palette);
        self.render_sync_panel(root_ui, palette);
        self.render_editor(root_ui, palette);
        self.maybe_autosave();
        self.render_pair_request_dialog(&ctx, palette);
        self.render_close_dialog(&ctx, palette);

        // Smart repaint: drive the event loop from actual activity instead of
        // an unconditional 4 fps tick. Idle keeps a slow 1s heartbeat so the
        // network layer still wakes for incoming sync events.
        let animation_window = Duration::from_millis(300);
        let recently_toggled = self
            .last_toggle_at
            .is_some_and(|at| at.elapsed() < animation_window);
        let needs_continuous = recently_toggled
            || self.dirty
            || self
                .save_feedback_until
                .is_some_and(|deadline| deadline > Instant::now())
            || self.sync_tone == StatusTone::Active;
        if needs_continuous {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}

fn file_uri_prefix(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    format!("file:///{normalized}/")
}

fn display_path(path: &Path) -> String {
    let shown = path.display().to_string();
    shown.strip_prefix(r"\\?\").unwrap_or(&shown).to_owned()
}

fn friendly_network_error(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "A conexão demorou demais. Salve para tentar novamente".to_owned()
    } else {
        "A rede não está disponível agora. Sua nota continua salva".to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Heading1,
    Heading2,
    Heading3,
    Checklist(bool),
    Bullet,
    Numbered(usize),
    Quote,
    Code { lang: String },
    Divider,
    Paragraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
}

impl Block {
    pub fn new(kind: BlockKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Paragraph, text)
    }

    pub fn h1(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Heading1, text)
    }

    pub fn h2(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Heading2, text)
    }

    pub fn h3(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Heading3, text)
    }

    pub fn checklist(checked: bool, text: impl Into<String>) -> Self {
        Self::new(BlockKind::Checklist(checked), text)
    }

    pub fn bullet(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Bullet, text)
    }

    pub fn numbered(num: usize, text: impl Into<String>) -> Self {
        Self::new(BlockKind::Numbered(num), text)
    }

    pub fn quote(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Quote, text)
    }

    pub fn code(lang: impl Into<String>, code: impl Into<String>) -> Self {
        Self::new(BlockKind::Code { lang: lang.into() }, code)
    }

    pub fn divider() -> Self {
        Self::new(BlockKind::Divider, "")
    }

    pub fn from_markdown_chunk(chunk: &str) -> Self {
        let trimmed = chunk.trim();
        if trimmed == "---" || trimmed == "***" {
            return Self::divider();
        }
        if trimmed.starts_with("```") {
            let lines: Vec<&str> = chunk.lines().collect();
            let first_line = lines.first().copied().unwrap_or("");
            let lang = first_line.trim_start_matches("```").trim().to_string();
            let body = if lines.len() > 1 {
                let end = if lines.last().is_some_and(|l| l.trim().starts_with("```")) {
                    lines.len().saturating_sub(1)
                } else {
                    lines.len()
                };
                lines[1..end].join("\n")
            } else {
                String::new()
            };
            return Self::code(lang, body);
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            return Self::h3(rest);
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            return Self::h2(rest);
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            return Self::h1(rest);
        }
        if let Some(rest) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
            .or_else(|| trimmed.strip_prefix("* [X] "))
        {
            return Self::checklist(true, rest);
        }
        if let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
        {
            return Self::checklist(false, rest);
        }
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            return Self::bullet(rest);
        }
        if let Some(rest) = trimmed.strip_prefix("> ") {
            return Self::quote(rest);
        }
        if let Some(dot_pos) = trimmed.find(". ")
            && let Ok(num) = trimmed[..dot_pos].parse::<usize>()
        {
            return Self::numbered(num, &trimmed[dot_pos + 2..]);
        }
        Self::paragraph(trimmed)
    }

    pub fn to_markdown(&self) -> String {
        match &self.kind {
            BlockKind::Heading1 => format!("# {}", self.text),
            BlockKind::Heading2 => format!("## {}", self.text),
            BlockKind::Heading3 => format!("### {}", self.text),
            BlockKind::Checklist(true) => format!("- [x] {}", self.text),
            BlockKind::Checklist(false) => format!("- [ ] {}", self.text),
            BlockKind::Bullet => format!("- {}", self.text),
            BlockKind::Numbered(n) => format!("{n}. {}", self.text),
            BlockKind::Quote => format!("> {}", self.text),
            BlockKind::Code { lang } => {
                if lang.is_empty() {
                    format!("```\n{}\n```", self.text)
                } else {
                    format!("```{lang}\n{}\n```", self.text)
                }
            }
            BlockKind::Divider => "---".to_string(),
            BlockKind::Paragraph => self.text.clone(),
        }
    }

    pub fn is_list_item(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::Checklist(_) | BlockKind::Bullet | BlockKind::Numbered(_)
        )
    }
}

impl From<&str> for Block {
    fn from(s: &str) -> Self {
        Self::from_markdown_chunk(s)
    }
}

impl From<String> for Block {
    fn from(s: String) -> Self {
        Self::from_markdown_chunk(&s)
    }
}

pub fn blocks_from_content(content: &str) -> Vec<Block> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();
    let mut in_code = false;

    let flush = |lines: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !lines.is_empty() {
            let chunk = lines.join("\n");
            let trimmed = chunk.trim();
            if !trimmed.is_empty() {
                blocks.push(Block::from_markdown_chunk(&chunk));
            }
            lines.clear();
        }
    };

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code {
                current_lines.push(line);
                flush(&mut current_lines, &mut blocks);
                in_code = false;
                continue;
            } else {
                flush(&mut current_lines, &mut blocks);
                in_code = true;
                current_lines.push(line);
                continue;
            }
        }

        if in_code {
            current_lines.push(line);
            continue;
        }

        if trimmed.is_empty() {
            flush(&mut current_lines, &mut blocks);
            continue;
        }

        let is_heading =
            trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ");
        let is_divider = trimmed == "---" || trimmed == "***";
        let is_quote = trimmed.starts_with("> ");
        let is_checklist = trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
            || trimmed.starts_with("* [ ] ")
            || trimmed.starts_with("* [x] ");
        let is_bullet = !is_checklist && (trimmed.starts_with("- ") || trimmed.starts_with("* "));
        let is_numbered = trimmed
            .find(". ")
            .is_some_and(|pos| trimmed[..pos].parse::<usize>().is_ok());

        let is_single_line_block =
            is_heading || is_divider || is_quote || is_checklist || is_bullet || is_numbered;

        if is_single_line_block {
            flush(&mut current_lines, &mut blocks);
            blocks.push(Block::from_markdown_chunk(line));
        } else {
            current_lines.push(line);
        }
    }

    flush(&mut current_lines, &mut blocks);

    if blocks.is_empty() {
        blocks.push(Block::paragraph(""));
    }

    blocks
}

pub fn content_from_blocks(blocks: &[Block]) -> String {
    if blocks.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (i, block) in blocks.iter().enumerate() {
        let md = block.to_markdown();
        out.push_str(&md);
        if i + 1 < blocks.len() {
            let next = &blocks[i + 1];
            if block.is_list_item() && next.is_list_item() {
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }
    }
    out.push('\n');
    out
}

struct SlashOption {
    label: &'static str,
    kind: BlockKind,
    icon: &'static str,
    desc: &'static str,
    keywords: &'static str,
}

const SLASH_COMMANDS: &[SlashOption] = &[
    SlashOption {
        label: "Texto",
        kind: BlockKind::Paragraph,
        icon: "¶",
        desc: "Parágrafo normal de texto",
        keywords: "texto paragraph normal",
    },
    SlashOption {
        label: "Título 1",
        kind: BlockKind::Heading1,
        icon: "H1",
        desc: "Grande cabeçalho de seção",
        keywords: "titulo title heading h1 grande",
    },
    SlashOption {
        label: "Título 2",
        kind: BlockKind::Heading2,
        icon: "H2",
        desc: "Médio cabeçalho de subseção",
        keywords: "titulo title heading h2 medio",
    },
    SlashOption {
        label: "Título 3",
        kind: BlockKind::Heading3,
        icon: "H3",
        desc: "Pequeno subtítulo",
        keywords: "titulo title heading h3 pequeno",
    },
    SlashOption {
        label: "Checklist",
        kind: BlockKind::Checklist(false),
        icon: "☑",
        desc: "Tarefa com caixa de seleção",
        keywords: "checklist task tarefa todo check",
    },
    SlashOption {
        label: "Lista com marcadores",
        kind: BlockKind::Bullet,
        icon: "•",
        desc: "Lista de tópicos simples",
        keywords: "lista bullet list marcadores pontos",
    },
    SlashOption {
        label: "Lista numerada",
        kind: BlockKind::Numbered(1),
        icon: "1.",
        desc: "Lista com contagem sequencial",
        keywords: "lista numerada number ordered",
    },
    SlashOption {
        label: "Citação",
        kind: BlockKind::Quote,
        icon: "❝",
        desc: "Destacar uma citação ou nota",
        keywords: "citacao quote destaque bloco",
    },
    SlashOption {
        label: "Código",
        kind: BlockKind::Code {
            lang: String::new(),
        },
        icon: "</>",
        desc: "Bloco de código com sintaxe",
        keywords: "codigo code snippet bloco",
    },
    SlashOption {
        label: "Divisor",
        kind: BlockKind::Divider,
        icon: "—",
        desc: "Linha horizontal de separação",
        keywords: "divisor separador divider linha",
    },
];

fn strip_slash_trigger(block: &str) -> String {
    let Some(after_slash) = block.strip_prefix('/') else {
        return block.to_string();
    };
    let cut = after_slash
        .find(char::is_whitespace)
        .unwrap_or(after_slash.len());
    after_slash[cut..].trim_start().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(vault: &Path) -> NodusApp {
        let mut app = NodusApp::failed(&egui::Context::default(), String::new());
        app.paths = AppPaths {
            settings: vault.join("settings.json"),
            legacy_vault: None,
        };
        app.settings.add_vault(vault).unwrap();
        app.fatal_error = None;
        app
    }

    #[test]
    fn changing_notes_autosaves_the_previous_note() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("primeira.md");
        let second = directory.path().join("segunda.md");
        fs::write(&first, "# Primeira\n").unwrap();
        fs::write(&second, "# Segunda\n").unwrap();

        let mut app = test_app(directory.path());
        app.selected = Some(first.clone());
        app.blocks = vec![Block::h1("Primeira editada")];
        app.dirty = true;

        app.select_note(second.clone());
        assert_eq!(app.blocks, vec![Block::h1("Segunda")]);
        assert!(!app.drafts.contains_key(&first));
        assert_eq!(fs::read_to_string(&first).unwrap(), "# Primeira editada\n");

        app.select_note(first);
        assert_eq!(app.blocks, vec![Block::h1("Primeira editada")]);
        assert!(!app.dirty);
    }

    #[test]
    fn save_all_persists_current_and_inactive_drafts() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("primeira.md");
        let second = directory.path().join("segunda.md");
        fs::write(&first, "antiga 1\n").unwrap();
        fs::write(&second, "antiga 2\n").unwrap();

        let mut app = test_app(directory.path());
        app.selected = Some(first.clone());
        app.blocks = vec![Block::paragraph("nova 1")];
        app.dirty = true;
        app.drafts
            .insert(second.clone(), vec![Block::paragraph("nova 2")]);

        assert_eq!(app.unsaved_count(), 2);
        assert!(app.save_all_and_sync());
        assert_eq!(fs::read_to_string(first).unwrap(), "nova 1\n");
        assert_eq!(fs::read_to_string(second).unwrap(), "nova 2\n");
        assert_eq!(app.unsaved_count(), 0);
    }

    #[test]
    fn autosave_persists_after_the_debounce_window() {
        let directory = tempfile::tempdir().unwrap();
        let note = directory.path().join("auto.md");
        fs::write(&note, "antes\n").unwrap();
        let mut app = test_app(directory.path());
        app.selected = Some(note.clone());
        app.blocks = vec![Block::paragraph("depois")];
        app.dirty = true;
        app.last_edit_at = Some(Instant::now() - Duration::from_millis(701));

        app.maybe_autosave();

        assert_eq!(fs::read_to_string(note).unwrap(), "depois\n");
        assert!(!app.dirty);
    }

    #[test]
    fn configured_editor_theme_uses_a_light_text_field() {
        let context = egui::Context::default();
        theme::apply(&context, &theme::Palette::light());

        assert_eq!(context.theme(), egui::Theme::Light);
        assert_eq!(
            context.global_style().visuals.text_edit_bg_color(),
            theme::Palette::light().surface
        );
    }

    #[test]
    fn approved_peer_is_persisted_for_future_syncs() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(directory.path());
        let peer = PeerConfig {
            name: "Notebook".to_owned(),
            endpoint_id: "endpoint-test".to_owned(),
            ticket: "ticket-test".to_owned(),
        };

        assert!(app.persist_peer(peer.clone()));
        assert_eq!(app.settings.active_vault().unwrap().peers, vec![peer]);
        let saved: Settings =
            serde_json::from_slice(&fs::read(&app.paths.settings).unwrap()).unwrap();
        assert_eq!(
            saved.active_vault().unwrap().peers,
            app.settings.active_vault().unwrap().peers
        );
    }

    #[test]
    fn blocks_from_content_splits_on_blank_lines() {
        let blocks = blocks_from_content("# Title\n\nparagraph\n\n- item");
        assert_eq!(
            blocks,
            vec![
                Block::h1("Title"),
                Block::paragraph("paragraph"),
                Block::bullet("item")
            ]
        );
    }

    #[test]
    fn blocks_from_content_keeps_single_newlines_within_a_block() {
        let blocks = blocks_from_content("- a\n- b\n- c\n\nnext paragraph");
        assert_eq!(
            blocks,
            vec![
                Block::bullet("a"),
                Block::bullet("b"),
                Block::bullet("c"),
                Block::paragraph("next paragraph")
            ]
        );
    }

    #[test]
    fn blocks_from_content_drops_blank_blocks_and_trims() {
        let blocks = blocks_from_content("  \n\n  # Title  \n\n\n\n  \n  text  ");
        assert_eq!(blocks, vec![Block::h1("Title"), Block::paragraph("text")]);
    }

    #[test]
    fn blocks_from_content_keeps_a_fenced_code_block_intact() {
        let blocks = blocks_from_content("intro\n\n```\nlet x = 1;\nlet y = 2;\n```\n\noutro");
        assert_eq!(
            blocks,
            vec![
                Block::paragraph("intro"),
                Block::code("", "let x = 1;\nlet y = 2;"),
                Block::paragraph("outro")
            ]
        );
    }

    #[test]
    fn content_from_blocks_joins_with_blank_lines() {
        let s = content_from_blocks(&[
            Block::h1("Title"),
            Block::paragraph("paragraph"),
            Block::bullet("item"),
        ]);
        assert_eq!(s, "# Title\n\nparagraph\n\n- item\n");
    }

    #[test]
    fn content_from_blocks_empty_yields_empty_string() {
        assert_eq!(content_from_blocks(&[]), "");
    }

    #[test]
    fn round_trip_preserves_normal_documents() {
        let original = "# Title\n\nparagraph\n\n- a\n- b\n- c\n\nmore text";
        let blocks = blocks_from_content(original);
        let restored = content_from_blocks(&blocks);
        assert_eq!(restored.trim(), original.trim());
    }

    #[test]
    fn round_trip_preserves_code_fence_blocks() {
        let original = "intro\n\n```\nlet x = 1;\nlet y = 2;\n```\n\noutro";
        let blocks = blocks_from_content(original);
        let restored = content_from_blocks(&blocks);
        assert_eq!(restored.trim(), original.trim());
    }

    #[test]
    fn blocks_from_content_handles_crlf_newlines() {
        let content = "# Titulo Windows\r\n\r\nParagrafo 1\r\n\r\n- [ ] Tarefa CRLF";
        let blocks = blocks_from_content(content);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], Block::h1("Titulo Windows"));
        assert_eq!(blocks[1], Block::paragraph("Paragrafo 1"));
        assert_eq!(blocks[2], Block::checklist(false, "Tarefa CRLF"));
    }

    #[test]
    fn strip_slash_trigger_removes_only_the_trigger() {
        assert_eq!(strip_slash_trigger("/"), "");
        assert_eq!(strip_slash_trigger("/he"), "");
        assert_eq!(strip_slash_trigger("/he rest"), "rest");
        assert_eq!(strip_slash_trigger("hello"), "hello");
    }

    #[test]
    fn initial_view_mode_defaults_to_notion() {
        let directory = tempfile::tempdir().unwrap();
        let app = test_app(directory.path());
        assert_eq!(app.settings.ui.view_mode, ViewMode::Notion);
    }

    #[test]
    fn delete_note_removes_file_and_draft() {
        let directory = tempfile::tempdir().unwrap();
        let note_path = directory.path().join("to_delete.md");
        fs::write(&note_path, "# Para deletar\n").unwrap();
        let note_path = note_path.canonicalize().unwrap();

        let mut app = test_app(directory.path());
        app.refresh_notes();
        assert!(app.notes.contains(&note_path));

        app.drafts
            .insert(note_path.clone(), vec![Block::paragraph("rascunho")]);
        app.delete_note(&note_path);

        assert!(!note_path.exists());
        assert!(!app.drafts.contains_key(&note_path));
        assert!(!app.notes.contains(&note_path));
    }

    #[test]
    fn configured_editor_theme_uses_dark_palette_when_dark() {
        let context = egui::Context::default();
        theme::apply(&context, &theme::Palette::dark());

        assert_eq!(context.theme(), egui::Theme::Dark);
        assert_eq!(
            context.global_style().visuals.text_edit_bg_color(),
            theme::Palette::dark().surface
        );
        assert_eq!(
            context.global_style().visuals.override_text_color,
            Some(theme::DARK_INK)
        );
    }

    #[test]
    fn new_note_initializes_with_notion_ready_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(directory.path());
        app.new_note();

        assert_eq!(app.blocks.len(), 2);
        assert_eq!(app.blocks[0], Block::h1("Nova nota"));
        assert_eq!(app.blocks[1], Block::paragraph(""));
        assert_eq!(app.active_block, Some(1));
        assert_eq!(app.pending_focus, Some(1));
        assert!(app.dirty);
    }

    #[test]
    fn select_note_loads_blocks_and_resets_active_state() {
        let directory = tempfile::tempdir().unwrap();
        let note_path = directory.path().join("nota_teste.md");
        fs::write(
            &note_path,
            "# Minha Nota\n\n- [ ] Tarefa 1\n- [x] Tarefa 2\n",
        )
        .unwrap();

        let mut app = test_app(directory.path());
        app.active_block = Some(5);
        app.pending_focus = Some(5);
        app.select_note(note_path.clone());

        assert_eq!(app.selected, Some(note_path));
        assert_eq!(app.blocks.len(), 3);
        assert_eq!(app.blocks[0], Block::h1("Minha Nota"));
        assert_eq!(app.blocks[1], Block::checklist(false, "Tarefa 1"));
        assert_eq!(app.blocks[2], Block::checklist(true, "Tarefa 2"));
        assert_eq!(app.active_block, None);
        assert_eq!(app.pending_focus, None);
        assert!(!app.dirty);
    }
}
