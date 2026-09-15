use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, FontId, RichText, Stroke, TextStyle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::{
    config::{AppPaths, EditorMode, PeerConfig, Settings, ThemeMode, UiPrefs},
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
    // `editor` is the mutable working buffer that the TextEdit in
    // render_editor writes to. It mirrors the structured `blocks`
    // representation, kept in sync at boundaries (load, save, refresh).
    // Commit 3 of the block editor removes this field and switches to
    // per-block rendering.
    editor: String,
    dirty: bool,
    drafts: HashMap<PathBuf, Vec<String>>,
    blocks: Vec<String>,
    active_block: Option<usize>,
    slash_open: bool,
    loaded_modified_ms: u64,
    last_scan: Instant,
    pair_code: String,
    endpoint_short: String,
    pair_input: String,
    pair_error: Option<String>,
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
}

impl NodusApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, &theme::Palette::light());
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let initialized = (|| -> anyhow::Result<_> {
            let paths = AppPaths::discover()?;
            ensure_welcome_note(&paths.vault)?;
            let settings = Settings::load_or_create(&paths.settings)?;
            let network = NetworkService::start(
                paths.vault.clone(),
                settings.device_name.clone(),
                settings.secret_key()?,
                settings.pairing_token.clone(),
                settings.peers.clone(),
            );
            Ok((paths, settings, network))
        })();

        match initialized {
            Ok((paths, settings, network)) => {
                let notes = vault::list_notes(&paths.vault);
                let selected = notes.first().cloned();
                let (editor, blocks, loaded_modified_ms) = selected
                    .as_ref()
                    .map(|path| {
                        let content = fs::read_to_string(path).unwrap_or_default();
                        (
                            content.clone(),
                            blocks_from_content(&content),
                            vault::modified_ms(path),
                        )
                    })
                    .unwrap_or_default();
                let pairing_expanded = settings.peers.is_empty();
                Self {
                    paths,
                    settings,
                    network: Some(network),
                    notes,
                    selected,
                    editor,
                    blocks,
                    active_block: None,
                    slash_open: false,
                    dirty: false,
                    drafts: HashMap::new(),
                    loaded_modified_ms,
                    last_scan: Instant::now(),
                    pair_code: String::new(),
                    endpoint_short: "iniciando".to_owned(),
                    pair_input: String::new(),
                    pair_error: None,
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
                }
            }
            Err(error) => Self::failed(error.to_string()),
        }
    }

    fn failed(message: String) -> Self {
        let root = std::env::current_dir().unwrap_or_default();
        Self {
            paths: AppPaths {
                vault: root.join("notes"),
                settings: root.join("nodus-data/settings.json"),
            },
            settings: Settings {
                device_name: String::new(),
                secret_key: String::new(),
                pairing_token: String::new(),
                peers: vec![],
                ui: UiPrefs::default(),
            },
            network: None,
            notes: vec![],
            selected: None,
            editor: String::new(),
            blocks: Vec::new(),
            active_block: None,
            slash_open: false,
            dirty: false,
            drafts: HashMap::new(),
            loaded_modified_ms: 0,
            last_scan: Instant::now(),
            pair_code: String::new(),
            endpoint_short: String::new(),
            pair_input: String::new(),
            pair_error: None,
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
        self.stash_current_draft();
        if let Some(draft) = self.drafts.remove(&path) {
            self.blocks = draft;
            self.editor = content_from_blocks(&self.blocks);
            self.dirty = true;
        } else {
            let content = fs::read_to_string(&path).unwrap_or_default();
            self.editor = content;
            self.blocks = blocks_from_content(&self.editor);
            self.dirty = false;
        }
        self.active_block = None;
        self.slash_open = false;
        self.loaded_modified_ms = vault::modified_ms(&path);
        self.selected = Some(path);
        self.save_error = None;
    }

    fn new_note(&mut self) {
        self.stash_current_draft();
        let path = self.next_unsaved_note_path();
        self.selected = Some(path.clone());
        self.editor = "# Nova nota\n\n".to_owned();
        self.blocks = vec!["# Nova nota".to_string(), String::new()];
        self.active_block = Some(0);
        self.slash_open = false;
        self.dirty = true;
        self.loaded_modified_ms = 0;
        self.save_error = None;
        if !self.notes.contains(&path) {
            self.notes.push(path);
        }
    }

    fn next_unsaved_note_path(&self) -> PathBuf {
        for index in 1..10_000 {
            let name = if index == 1 {
                "Nova nota.md".to_owned()
            } else {
                format!("Nova nota {index}.md")
            };
            let candidate = self.paths.vault.join(name);
            if !candidate.exists() && !self.notes.contains(&candidate) {
                return candidate;
            }
        }
        vault::unique_note_path(&self.paths.vault)
    }

    fn save_current_local(&mut self) -> bool {
        self.save_error = None;
        if !self.dirty {
            return true;
        }
        let Some(path) = self.selected.clone() else {
            return true;
        };
        match fs::write(&path, self.editor.as_bytes()) {
            Ok(()) => {
                // Sync blocks from the freshly-saved editor buffer so the
                // structured form stays consistent with what's on disk.
                self.blocks = blocks_from_content(&self.editor);
                self.loaded_modified_ms = vault::modified_ms(&path);
                self.dirty = false;
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
        if self.settings.peers.is_empty() {
            self.sync_status = "Salva neste dispositivo".to_owned();
            self.sync_tone = StatusTone::Success;
        } else if let Some(network) = &self.network {
            network.sync_now();
            self.sync_status = "Salva. Iniciando sync".to_owned();
            self.sync_tone = StatusTone::Active;
        }
        self.refresh_notes();
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
        let mut notes = vault::list_notes(&self.paths.vault);
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
                self.editor = content;
                self.blocks = blocks_from_content(&self.editor);
                self.active_block = None;
                self.slash_open = false;
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
                    self.sync_status = if self.settings.peers.is_empty() {
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
        match self.settings.parse_pair_code(&self.pair_input) {
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
        let inserted = self.settings.add_peer(peer.clone());
        if inserted && let Err(error) = self.settings.save(&self.paths.settings) {
            self.settings
                .peers
                .retain(|item| item.endpoint_id != peer.endpoint_id);
            self.pair_error = Some(error.to_string());
            self.sync_status = "Não foi possível salvar o novo dispositivo".to_owned();
            self.sync_tone = StatusTone::Warning;
            if let Some(network) = &self.network {
                network.update_peers(self.settings.peers.clone());
            }
            return false;
        }
        if let Some(network) = &self.network {
            network.update_peers(self.settings.peers.clone());
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

    /// Mark that a sidebar/sync toggle just happened, so the smart repaint
    /// keeps the animation frames coming for a short window.
    fn note_panel_toggle(&mut self) {
        self.last_toggle_at = Some(Instant::now());
    }

    fn render_topbar(&mut self, root_ui: &mut egui::Ui, palette: theme::Palette) {
        let ctx = root_ui.ctx().clone();
        let sidebar_visible = self.settings.ui.sidebar_visible;

        egui::Panel::top("topbar")
            .frame(
                egui::Frame::new()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    // Left: reveal sidebar when hidden (dual toggle).
                    if !sidebar_visible {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("›").font(ui_semibold(16.0)).color(palette.muted),
                                )
                                .frame(false)
                                .fill(Color32::TRANSPARENT),
                            )
                            .on_hover_text("Mostrar sidebar (Ctrl+B)")
                            .clicked()
                        {
                            self.settings.ui.sidebar_visible = true;
                            self.note_panel_toggle();
                            self.save_ui_prefs();
                        }
                        ui.add_space(4.0);
                    }

                    // Vault name.
                    ui.label(
                        RichText::new(&self.settings.device_name)
                            .font(ui_semibold(14.0))
                            .color(palette.ink),
                    );

                    ui.add_space(20.0);

                    // Search field — moved from the sidebar.
                    let search_width = 280.0_f32.min(ui.available_width() * 0.4);
                    let search_response = ui.add_sized(
                        [search_width, 28.0],
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Buscar notas")
                            .font(ui_regular(13.0))
                            .background_color(palette.surface)
                            .margin(egui::Margin::symmetric(10, 6)),
                    );
                    if self.search_focus_request {
                        search_response.request_focus();
                        self.search_focus_request = false;
                    }

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
                            ThemeMode::System => ("🖥", "Tema: seguir sistema — clique para claro"),
                            ThemeMode::Light => ("☀", "Tema: claro — clique para escuro"),
                            ThemeMode::Dark => ("🌙", "Tema: escuro — clique para seguir sistema"),
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
                            self.save_ui_prefs();
                        }

                        ui.add_space(8.0);

                        // Save indicator pill.
                        let indicator_text = if self.dirty {
                            Some(("● Não salvo", palette.warning, palette.soft_warning))
                        } else if self
                            .save_feedback_until
                            .is_some_and(|deadline| deadline > Instant::now())
                        {
                            Some(("✓ Salvo", palette.success, palette.soft_green))
                        } else {
                            None
                        };
                        if let Some((text, fg, bg)) = indicator_text {
                            egui::Frame::new()
                                .fill(bg)
                                .corner_radius(7.0)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(text).font(ui_medium(11.0)).color(fg),
                                    );
                                });
                        }
                    });
                });
            });
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
                    ui.label(RichText::new("Nodus").font(ui_semibold(21.0)).color(palette.ink));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let chevron = egui::Button::new(
                            RichText::new("‹").font(ui_semibold(16.0)).color(palette.muted),
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
                ui.add_space(20.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(inner_width, 40.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        if ui
                            .add_sized(
                                [inner_width, 40.0],
                                egui::Button::new(
                                    RichText::new("Nova nota")
                                        .font(ui_medium(14.0))
                                        .color(palette.accent),
                                )
                                .fill(palette.surface)
                                .stroke(Stroke::new(1.0, Color32::from_rgb(171, 194, 244)))
                                .corner_radius(8.0),
                            )
                            .on_hover_text("Criar uma nota Markdown")
                            .clicked()
                        {
                            self.new_note();
                        }
                    },
                );

                ui.add_space(14.0);
                ui.label(
                    RichText::new("Suas notas")
                        .font(ui_medium(13.0))
                        .color(palette.muted),
                );
                ui.add_space(6.0);

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
                        let name = path
                            .file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or("Nota");
                        let label = if unsaved {
                            format!("{name}  (não salva)")
                        } else {
                            name.to_owned()
                        };
                        let button = egui::Button::new(
                            RichText::new(label)
                                .font(if selected {
                                    ui_medium(13.5)
                                } else {
                                    ui_regular(13.5)
                                })
                                .color(if selected { palette.ink } else { palette.muted }),
                        )
                        .fill(if selected {
                            palette.surface
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(if selected {
                            Stroke::new(1.0, palette.border)
                        } else {
                            Stroke::NONE
                        })
                        .corner_radius(7.0);
                        if ui.add_sized([inner_width, theme::layout::SIDEBAR_ROW_HEIGHT], button).clicked() {
                            self.select_note(path);
                        }
                    }
                });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(format!("{} nota(s) Markdown", self.notes.len()))
                            .font(ui_regular(11.5))
                            .color(palette.muted),
                    );
                });
            });

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
        let target_width = if sync_visible { theme::layout::SYNC_WIDTH } else { 0.0 };
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
                    ui.label(RichText::new("Sync").font(ui_semibold(20.0)).color(palette.ink));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let chevron = egui::Button::new(
                            RichText::new("›").font(ui_semibold(16.0)).color(palette.muted),
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

                if !self.settings.peers.is_empty() {
                    ui.add_space(22.0);
                    ui.label(
                        RichText::new("Dispositivos pareados")
                            .font(ui_medium(12.0))
                            .color(palette.muted),
                    );
                    ui.add_space(6.0);
                    for peer in &self.settings.peers {
                        egui::Frame::new()
                            .fill(palette.bg)
                            .corner_radius(7.0)
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_width(inner_width);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&peer.name).font(ui_medium(13.0)).color(palette.ink),
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
                } else if self.settings.peers.is_empty() {
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
                                .hint_text("NODUS2...")
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
                            ui.label(RichText::new(error).font(ui_regular(11.0)).color(palette.warning));
                        }
                    });
                }
            });
    }

    fn render_editor(&mut self, root_ui: &mut egui::Ui, palette: theme::Palette) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::symmetric(
                        theme::layout::PAPER_HORIZONTAL_MARGIN as i8,
                        theme::layout::PAPER_VERTICAL_MARGIN as i8,
                    )),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    let title = self
                        .selected
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .and_then(|value| value.to_str())
                        .unwrap_or("Nenhuma nota");
                    ui.label(RichText::new(title).font(ui_semibold(20.0)).color(palette.ink));
                    if self.dirty {
                        ui.label(
                            RichText::new("Não salva")
                                .font(ui_medium(11.5))
                                .color(palette.warning)
                                .background_color(palette.soft_warning),
                        );
                    } else if self
                        .save_feedback_until
                        .is_some_and(|deadline| deadline > Instant::now())
                    {
                        ui.label(
                            RichText::new("Salva")
                                .font(ui_medium(11.5))
                                .color(palette.success)
                                .background_color(palette.soft_green),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save = egui::Button::new(
                            RichText::new("Salvar")
                                .font(ui_medium(12.5))
                                .color(Color32::WHITE),
                        )
                        .fill(palette.accent)
                        .stroke(Stroke::NONE)
                        .corner_radius(7.0);
                        if ui
                            .add_sized(
                                [theme::layout::SAVE_BUTTON_WIDTH, 34.0],
                                save,
                            )
                            .clicked()
                        {
                            self.save_and_sync();
                        }
                        ui.add_space(8.0);
                        // Edit / Preview pill toggle. Cleaner than the
                        // previous two overlapping selectable_labels.
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let is_edit = self.settings.ui.editor_mode == EditorMode::Edit;
                            let is_preview = self.settings.ui.editor_mode == EditorMode::Preview;
                            let edit_btn = egui::Button::new(
                                RichText::new("Editar").font(ui_medium(12.5)),
                            )
                            .fill(if is_edit { palette.soft_blue } else { Color32::TRANSPARENT })
                            .stroke(Stroke::new(1.0, palette.border))
                            .corner_radius(egui::CornerRadius {
                                nw: 7,
                                ne: 0,
                                sw: 7,
                                se: 0,
                            });
                            if ui.add_sized([80.0, 30.0], edit_btn).clicked() {
                                self.settings.ui.editor_mode = EditorMode::Edit;
                            }
                            let preview_btn = egui::Button::new(
                                RichText::new("Visualizar").font(ui_medium(12.5)),
                            )
                            .fill(if is_preview { palette.soft_blue } else { Color32::TRANSPARENT })
                            .stroke(Stroke::new(1.0, palette.border))
                            .corner_radius(egui::CornerRadius {
                                nw: 0,
                                ne: 7,
                                sw: 0,
                                se: 7,
                            });
                            if ui.add_sized([96.0, 30.0], preview_btn).clicked() {
                                self.settings.ui.editor_mode = EditorMode::Preview;
                            }
                        });
                    });
                });
                if let Some(error) = &self.save_error {
                    ui.add_space(6.0);
                    ui.label(RichText::new(error).font(ui_regular(12.0)).color(palette.warning));
                }
                ui.add_space(14.0);

                let available = ui.available_size();
                let page_width = available.x.min(theme::layout::PAPER_MAX_WIDTH);
                let side_space = ((available.x - page_width) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(side_space);
                    egui::Frame::new()
                        .fill(palette.surface)
                        .stroke(Stroke::new(1.0, palette.border))
                        .corner_radius(6.0)
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 2],
                            blur: 12,
                            spread: 0,
                            color: Color32::from_black_alpha(14),
                        })
                        .inner_margin(egui::Margin::symmetric(
                            theme::layout::PAPER_INNER_PADDING as i8,
                            34,
                        ))
                        .show(ui, |ui| {
                            ui.set_width(
                                (page_width - 2.0 * theme::layout::PAPER_INNER_PADDING)
                                    .max(200.0),
                            );
                            ui.set_min_height((available.y - 4.0).max(260.0));
                            if self.settings.ui.editor_mode == EditorMode::Preview {
                                let implicit_uri = self
                                    .selected
                                    .as_ref()
                                    .and_then(|path| path.parent())
                                    .map(file_uri_prefix)
                                    .unwrap_or_else(|| "file:///".to_owned());
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    // Save prior styles so the preview mutation
                                    // doesn't leak out of this scope.
                                    let prior_body =
                                        ui.style().text_styles.get(&TextStyle::Body).cloned();
                                    let prior_heading =
                                        ui.style().text_styles.get(&TextStyle::Heading).cloned();
                                    ui.style_mut()
                                        .text_styles
                                        .insert(TextStyle::Body, serif_regular(17.0));
                                    ui.style_mut()
                                        .text_styles
                                        .insert(TextStyle::Heading, serif_semibold(27.0));
                                    let response = CommonMarkViewer::new()
                                        .indentation_spaces(2)
                                        .max_image_width(Some(ui.available_width() as usize))
                                        .default_width(Some(ui.available_width() as usize))
                                        .default_implicit_uri_scheme(implicit_uri)
                                        .show_mut(ui, &mut self.markdown_cache, &mut self.editor);
                                    if response.response.changed() {
                                        self.dirty = true;
                                    }
                                    let style = ui.style_mut();
                                    if let Some(prior) = prior_body {
                                        style.text_styles.insert(TextStyle::Body, prior);
                                    }
                                    if let Some(prior) = prior_heading {
                                        style.text_styles.insert(TextStyle::Heading, prior);
                                    }
                                });
                            } else {
                                // No frame + no auto-focus: the editor surface
                                // blends with the paper, and the only focus
                                // indicator is the blinking caret. Click to
                                // focus; Escape releases focus so the editor
                                // stops stealing keystrokes (Ctrl+B, etc.).
                                let editor_frame = egui::Frame::new()
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .inner_margin(egui::Margin::same(2));
                                let response = ui.add_sized(
                                    ui.available_size(),
                                    egui::TextEdit::multiline(&mut self.editor)
                                        .font(ui_regular(16.5))
                                        .text_color(palette.ink)
                                        .desired_width(f32::INFINITY)
                                        .frame(editor_frame)
                                        .margin(egui::Margin::same(2)),
                                );
                                if response.changed() {
                                    self.dirty = true;
                                    self.save_feedback_until = None;
                                }
                                if response.has_focus()
                                    && ui.ctx().input(|input| input.key_pressed(egui::Key::Escape))
                                {
                                    response.surrender_focus();
                                }
                            }
                        });
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
                    ui.label(RichText::new(error).font(ui_regular(12.0)).color(palette.warning));
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
                egui::Key::E,
            ))
        }) {
            self.settings.ui.editor_mode = match self.settings.ui.editor_mode {
                EditorMode::Edit => EditorMode::Preview,
                EditorMode::Preview => EditorMode::Edit,
            };
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::Slash,
            ))
        }) {
            self.search_focus_request = true;
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
                        ui.label(RichText::new(error).font(ui_regular(14.0)).color(palette.warning));
                    });
                });
            return;
        }

        let palette = theme::current_palette(&ctx, self.settings.ui.theme);
        self.render_topbar(root_ui, palette);
        self.render_sidebar(root_ui, palette);
        self.render_sync_panel(root_ui, palette);
        self.render_editor(root_ui, palette);
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

fn friendly_network_error(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "A conexão demorou demais. Salve para tentar novamente".to_owned()
    } else {
        "A rede não está disponível agora. Sua nota continua salva".to_owned()
    }
}

/// Split a Markdown document into per-block sources. A "block" is a run of
/// lines separated by blank lines (`\n\n`). Each block is trimmed; empty
/// blocks (whitespace only) are dropped.
fn blocks_from_content(content: &str) -> Vec<String> {
    content
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Join per-block sources back into a Markdown document. The output ends
/// with a single trailing newline so files always look "normal" on disk.
fn content_from_blocks(blocks: &[String]) -> String {
    if blocks.is_empty() {
        String::new()
    } else {
        blocks.join("\n\n") + "\n"
    }
}

fn ensure_welcome_note(vault: &Path) -> anyhow::Result<()> {
    let welcome = vault.join("Bem-vindo.md");
    if !welcome.exists() {
        fs::write(
            welcome,
            "# Bem-vindo ao Nodus\n\nEste arquivo é **Markdown puro** e fica na pasta `notes`.\n\n## Conectar e sincronizar\n\n1. Abra o Nodus nas duas máquinas.\n2. Cole o código do PC1 no PC2.\n3. Aceite a solicitação que aparecer no PC1.\n4. Edite esta nota e pressione `Ctrl+S`.\n\n- [x] Arquivos Markdown comuns\n- [x] Sync P2P criptografado\n- [x] Pareamento com aprovação\n- [ ] Sua próxima ideia\n\n> Depois do primeiro sync, o Nodus sincroniza novamente quando você salva.\n",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(vault: &Path) -> NodusApp {
        let mut app = NodusApp::failed(String::new());
        app.paths = AppPaths {
            vault: vault.to_owned(),
            settings: vault.join("settings.json"),
        };
        app.fatal_error = None;
        app
    }

    #[test]
    fn changing_notes_keeps_edits_as_unsaved_drafts() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("primeira.md");
        let second = directory.path().join("segunda.md");
        fs::write(&first, "# Primeira\n").unwrap();
        fs::write(&second, "# Segunda\n").unwrap();

        let mut app = test_app(directory.path());
        app.selected = Some(first.clone());
        app.editor = "# Primeira editada\n".to_owned();
        app.blocks = blocks_from_content(&app.editor);
        app.dirty = true;

        app.select_note(second.clone());
        assert_eq!(app.editor, "# Segunda\n");
        assert!(app.drafts.contains_key(&first));
        assert_eq!(fs::read_to_string(&first).unwrap(), "# Primeira\n");

        app.select_note(first);
        assert_eq!(app.editor, "# Primeira editada\n");
        assert!(app.dirty);
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
        app.editor = "nova 1\n".to_owned();
        app.blocks = blocks_from_content(&app.editor);
        app.dirty = true;
        app.drafts.insert(second.clone(), vec!["nova 2".to_string()]);

        assert_eq!(app.unsaved_count(), 2);
        assert!(app.save_all_and_sync());
        assert_eq!(fs::read_to_string(first).unwrap(), "nova 1\n");
        assert_eq!(fs::read_to_string(second).unwrap(), "nova 2\n");
        assert_eq!(app.unsaved_count(), 0);
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
        assert_eq!(app.settings.peers, vec![peer]);
        let saved: Settings =
            serde_json::from_slice(&fs::read(&app.paths.settings).unwrap()).unwrap();
        assert_eq!(saved.peers, app.settings.peers);
    }

    #[test]
    fn blocks_from_content_splits_on_blank_lines() {
        let blocks = blocks_from_content("# Title\n\nparagraph\n\n- item");
        assert_eq!(blocks, vec!["# Title", "paragraph", "- item"]);
    }

    #[test]
    fn blocks_from_content_keeps_single_newlines_within_a_block() {
        let blocks = blocks_from_content("- a\n- b\n- c\n\nnext paragraph");
        assert_eq!(blocks, vec!["- a\n- b\n- c", "next paragraph"]);
    }

    #[test]
    fn blocks_from_content_drops_blank_blocks_and_trims() {
        let blocks = blocks_from_content("  \n\n  # Title  \n\n\n\n  \n  text  ");
        assert_eq!(blocks, vec!["# Title", "text"]);
    }

    #[test]
    fn blocks_from_content_keeps_a_fenced_code_block_intact() {
        let blocks = blocks_from_content("intro\n\n```\nlet x = 1;\nlet y = 2;\n```\n\noutro");
        assert_eq!(
            blocks,
            vec!["intro", "```\nlet x = 1;\nlet y = 2;\n```", "outro"]
        );
    }

    #[test]
    fn content_from_blocks_joins_with_blank_lines() {
        let s = content_from_blocks(&["# Title".into(), "paragraph".into(), "- item".into()]);
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
}
