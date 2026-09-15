use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use filetime::FileTime;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

const MAX_NOTE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMeta {
    pub path: String,
    pub modified_ms: u64,
    pub size: u64,
    pub hash: String,
}

pub type Manifest = BTreeMap<String, NoteMeta>;

pub fn list_notes(root: &Path) -> Vec<PathBuf> {
    let mut notes: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_markdown(entry.path()))
        .map(|entry| entry.into_path())
        .collect();
    notes.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    notes
}

pub fn build_manifest(root: &Path) -> anyhow::Result<Manifest> {
    let mut manifest = Manifest::new();
    for absolute in list_notes(root) {
        let metadata = fs::metadata(&absolute)?;
        if metadata.len() > MAX_NOTE_BYTES {
            continue;
        }
        let relative = absolute.strip_prefix(root)?;
        let path = relative_to_wire(relative)?;
        let bytes = fs::read(&absolute)?;
        let modified_ms = system_time_ms(metadata.modified().unwrap_or(UNIX_EPOCH));
        manifest.insert(
            path.clone(),
            NoteMeta {
                path,
                modified_ms,
                size: metadata.len(),
                hash: blake3::hash(&bytes).to_hex().to_string(),
            },
        );
    }
    Ok(manifest)
}

pub fn read_note_bytes(root: &Path, wire_path: &str) -> anyhow::Result<Vec<u8>> {
    let path = safe_join(root, wire_path)?;
    let metadata = fs::metadata(&path).context("a nota solicitada não existe")?;
    if metadata.len() > MAX_NOTE_BYTES {
        bail!("a nota excede o limite de 8 MB desta versão");
    }
    fs::read(path).context("não foi possível ler a nota")
}

pub fn write_remote_note(
    root: &Path,
    meta: &NoteMeta,
    content: &[u8],
    remote_label: &str,
) -> anyhow::Result<bool> {
    if content.len() as u64 > MAX_NOTE_BYTES {
        bail!("nota recebida excede 8 MB");
    }
    if blake3::hash(content).to_hex().as_str() != meta.hash {
        bail!("a nota recebida falhou na verificação de integridade");
    }
    let destination = safe_join(root, &meta.path)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut conflict_created = false;
    if destination.is_file() {
        let local = fs::read(&destination)?;
        let local_hash = blake3::hash(&local).to_hex().to_string();
        let local_ms = system_time_ms(fs::metadata(&destination)?.modified().unwrap_or(UNIX_EPOCH));
        if local_hash != meta.hash && local_ms > meta.modified_ms {
            let stem = destination
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("nota");
            let safe_remote: String = remote_label
                .chars()
                .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
                .take(16)
                .collect();
            let conflict = destination.with_file_name(format!(
                "{stem}.conflict-{}-{}.md",
                if safe_remote.is_empty() {
                    "peer"
                } else {
                    &safe_remote
                },
                meta.modified_ms
            ));
            fs::write(conflict, content)?;
            conflict_created = true;
            return Ok(conflict_created);
        }
    }

    let temp = destination.with_extension("md.nodus-tmp");
    fs::write(&temp, content)?;
    if destination.exists() {
        fs::remove_file(&destination)?;
    }
    fs::rename(temp, &destination)?;
    filetime::set_file_mtime(
        &destination,
        FileTime::from_unix_time(
            (meta.modified_ms / 1000) as i64,
            ((meta.modified_ms % 1000) * 1_000_000) as u32,
        ),
    )?;
    Ok(conflict_created)
}

pub fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map(system_time_ms)
        .unwrap_or(0)
}

pub fn unique_note_path(root: &Path) -> PathBuf {
    let first = root.join("Nova nota.md");
    if !first.exists() {
        return first;
    }
    for index in 2..10_000 {
        let candidate = root.join(format!("Nova nota {index}.md"));
        if !candidate.exists() {
            return candidate;
        }
    }
    root.join(format!("Nota {}.md", system_time_ms(SystemTime::now())))
}

fn safe_join(root: &Path, wire_path: &str) -> anyhow::Result<PathBuf> {
    if wire_path.contains('\\') {
        bail!("caminho de nota inválido");
    }
    let relative = Path::new(wire_path);
    if relative.is_absolute() || !is_markdown(relative) {
        bail!("somente caminhos Markdown relativos são aceitos");
    }
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!("caminho de nota inseguro");
        }
    }
    Ok(root.join(relative))
}

fn relative_to_wire(path: &Path) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => bail!("caminho de nota inválido"),
        }
    }
    Ok(parts.join("/"))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("md"))
}

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_traversal() {
        let root = Path::new("vault");
        assert!(safe_join(root, "../secret.md").is_err());
        assert!(safe_join(root, "ok/note.md").is_ok());
        assert!(safe_join(root, "note.txt").is_err());
    }
}
