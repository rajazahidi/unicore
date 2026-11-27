//! Built-in 1-click curated App Catalog with HTTPS & SHA-256 verification.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::inspector::{AppPlatform, WinEra};
use crate::json_codec::{parse_json, to_json_pretty, JsonValue};
use crate::sandbox::{sha256_file, AppSandbox, SandboxManager};
use crate::updater::{load_host_update_config, validate_update_url, verify_manifest_signature, SignatureStatus};

pub const DEFAULT_SYSTEM_CATALOG: &str = "/usr/share/unicore/catalog.json";
pub const DEFAULT_DATA_CATALOG: &str = "/data/unicore/catalog.json";
pub const OFFICIAL_CATALOG_URL: &str =
    "https://github.com/rajazahidi/unicore/releases/latest/download/unicore-catalog.json";
pub const EMBEDDED_CATALOG_JSON: &str =
    include_str!("../../../os-builder/overlay/usr/share/unicore/catalog.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size_label: String,
    pub category: String,
    pub platform: AppPlatform,
    pub win_era: Option<WinEra>,
    pub download_url: String,
    pub sha256: String,
    pub filename: String,
    pub archive_entry: String,
    pub bundled_path: String,
    pub flatpak_id: String,
    pub arches: Vec<String>,
    pub summary_en: String,
    pub summary_ms: String,
}

impl CatalogApp {
    pub fn is_offline_bundled(&self) -> bool {
        !self.bundled_path.is_empty() && self.download_url.is_empty() && self.flatpak_id.is_empty()
    }

    pub fn is_flatpak(&self) -> bool {
        self.platform == AppPlatform::LinuxFlatpak && !self.flatpak_id.is_empty()
    }

    pub fn matches_arch(&self, host_arch: &str) -> bool {
        if self.arches.is_empty() || host_arch.trim().is_empty() {
            return true;
        }
        let lowered = host_arch.trim().to_ascii_lowercase();
        let norm = match lowered.as_str() {
            "amd64" | "x86_64" => "x86_64",
            "arm64" | "aarch64" => "aarch64",
            other => other,
        };
        self.arches.iter().any(|a| a.eq_ignore_ascii_case(norm))
    }

    pub fn summary(&self, is_ms: bool) -> &str {
        if is_ms && !self.summary_ms.is_empty() {
            &self.summary_ms
        } else {
            &self.summary_en
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        if !self.arches.is_empty() {
            map.insert(
                "arches".to_string(),
                JsonValue::Array(self.arches.iter().cloned().map(JsonValue::Str).collect()),
            );
        }
        map.insert("archive_entry".to_string(), JsonValue::Str(self.archive_entry.clone()));
        map.insert("bundled_path".to_string(), JsonValue::Str(self.bundled_path.clone()));
        map.insert("category".to_string(), JsonValue::Str(self.category.clone()));
        map.insert("download_url".to_string(), JsonValue::Str(self.download_url.clone()));
        map.insert("filename".to_string(), JsonValue::Str(self.filename.clone()));
        if !self.flatpak_id.is_empty() {
            map.insert("flatpak_id".to_string(), JsonValue::Str(self.flatpak_id.clone()));
        }
        map.insert("id".to_string(), JsonValue::Str(self.id.clone()));
        map.insert("name".to_string(), JsonValue::Str(self.name.clone()));
        map.insert(
            "platform".to_string(),
            JsonValue::Str(self.platform.as_str().to_string()),
        );
        map.insert("sha256".to_string(), JsonValue::Str(self.sha256.clone()));
        map.insert("size_label".to_string(), JsonValue::Str(self.size_label.clone()));
        map.insert("summary_en".to_string(), JsonValue::Str(self.summary_en.clone()));
        map.insert("summary_ms".to_string(), JsonValue::Str(self.summary_ms.clone()));
        map.insert("version".to_string(), JsonValue::Str(self.version.clone()));
        map.insert(
            "win_era".to_string(),
            match self.win_era {
                Some(era) => JsonValue::Str(era.as_str().to_string()),
                None => JsonValue::Null,
            },
        );
        JsonValue::Object(map)
    }

    fn from_json(value: &JsonValue) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "Catalog entry must be a JSON object".to_string())?;
        let req_str = |k: &str| -> Result<String, String> {
            obj.get(k)
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("Catalog entry missing '{k}'"))
        };
        let opt_str = |k: &str| -> String {
            obj.get(k)
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .unwrap_or("")
                .to_string()
        };

        let id = req_str("id")?;
        validate_catalog_id(&id)?;
        let platform = AppPlatform::parse(&req_str("platform")?)?;
        let win_era = match obj.get("win_era").and_then(JsonValue::as_str) {
            Some(raw) if !raw.trim().is_empty() => Some(WinEra::parse(raw.trim())?),
            _ => None,
        };
        let flatpak_id = opt_str("flatpak_id");
        let filename = if platform == AppPlatform::LinuxFlatpak {
            let f = opt_str("filename");
            if f.is_empty() {
                if flatpak_id.is_empty() {
                    return Err(format!("Flatpak catalog entry '{id}' missing 'flatpak_id'"));
                }
                flatpak_id.clone()
            } else {
                f
            }
        } else {
            req_str("filename")?
        };
        let arches = obj
            .get("arches")
            .and_then(JsonValue::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(JsonValue::as_str)
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            id,
            name: req_str("name")?,
            version: opt_str("version"),
            size_label: opt_str("size_label"),
            category: req_str("category")?.to_ascii_lowercase(),
            platform,
            win_era,
            download_url: opt_str("download_url"),
            sha256: opt_str("sha256").to_ascii_lowercase(),
            filename,
            archive_entry: opt_str("archive_entry"),
            bundled_path: opt_str("bundled_path"),
            flatpak_id,
            arches,
            summary_en: opt_str("summary_en"),
            summary_ms: opt_str("summary_ms"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCatalog {
    pub version: u32,
    pub apps: Vec<CatalogApp>,
}

/// Validate catalog app identifier (`[a-z0-9_-]`, 1..=64 chars).
pub fn validate_catalog_id(id: &str) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("Catalog ID must be 1..=64 characters".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
    {
        return Err(format!(
            "Invalid catalog ID '{id}': only lowercase ASCII letters, digits, '-' and '_' allowed"
        ));
    }
    Ok(())
}

/// Reject empty, absolute, parent-traversal (`..`), or shell-metacharacter relative paths.
pub fn validate_safe_rel_path(rel: &str) -> Result<(), String> {
    let clean = rel.trim();
    if clean.is_empty() {
        return Err("Relative path cannot be empty".to_string());
    }
    if clean.starts_with('/') || clean.starts_with('\\') {
        return Err(format!("Path must be relative: '{clean}'"));
    }
    if clean
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&'))
    {
        return Err(format!("Path contains disallowed characters: '{clean}'"));
    }
    if clean.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..") {
        return Err(format!("Path contains unsafe traversal segment: '{clean}'"));
    }
    Ok(())
}

fn validate_single_filename(filename: &str) -> Result<(), String> {
    validate_safe_rel_path(filename)?;
    if filename.contains('/') {
        return Err(format!("Filename cannot contain directory separators: '{filename}'"));
    }
    Ok(())
}

/// Normalize and validate a 64-character lowercase SHA-256 hex digest.
pub fn validate_sha256_hex(hex: &str) -> Result<String, String> {
    let clean = hex.trim().to_ascii_lowercase();
    if clean.len() != 64 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Invalid SHA-256 digest '{hex}': expected 64 hex chars"));
    }
    Ok(clean)
}

/// Verify that `path` matches `expected_hex` using streaming SHA-256.
pub fn verify_file_sha256(path: &Path, expected_hex: &str) -> Result<String, String> {
    let expected = validate_sha256_hex(expected_hex)?;
    let actual = sha256_file(path)?;
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}",
            path.display()
        ));
    }
    Ok(actual)
}

/// Parse an `AppCatalog` JSON document.
pub fn parse_catalog_json(raw: &str) -> Result<AppCatalog, String> {
    let parsed = parse_json(raw)?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| "Catalog JSON root must be an object".to_string())?;
    let version = match obj.get("version") {
        Some(JsonValue::Number(n)) if *n >= 1.0 => *n as u32,
        _ => 1,
    };
    let apps_arr = obj
        .get("apps")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "Catalog JSON missing 'apps' array".to_string())?;
    let mut apps = Vec::with_capacity(apps_arr.len());
    for entry in apps_arr {
        apps.push(CatalogApp::from_json(entry)?);
    }
    Ok(AppCatalog { version, apps })
}

/// Load active App Catalog (override -> `/data/unicore/catalog.json` -> `/usr/share/unicore/catalog.json` -> embedded).
pub fn load_catalog(override_path: Option<&Path>) -> Result<AppCatalog, String> {
    if let Some(path) = override_path {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read catalog {}: {e}", path.display()))?;
        return parse_catalog_json(&raw);
    }
    for path in [Path::new(DEFAULT_DATA_CATALOG), Path::new(DEFAULT_SYSTEM_CATALOG)] {
        if path.is_file() {
            let raw = fs::read_to_string(path)
                .map_err(|e| format!("Failed to read catalog {}: {e}", path.display()))?;
            return parse_catalog_json(&raw);
        }
    }
    parse_catalog_json(EMBEDDED_CATALOG_JSON)
}

/// Filter catalog entries by category, search query, and optional host architecture.
pub fn filter_catalog_apps_for_arch(
    apps: &[CatalogApp],
    category: &str,
    query: &str,
    host_arch: &str,
) -> Vec<CatalogApp> {
    let cat = category.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();

    apps.iter()
        .filter(|app| {
            if !app.matches_arch(host_arch) {
                return false;
            }
            let cat_ok = match cat.as_str() {
                "" | "all" => true,
                "windows" => matches!(
                    app.platform,
                    AppPlatform::WindowsPe
                        | AppPlatform::WindowsNe16
                        | AppPlatform::WindowsMsi
                        | AppPlatform::Dos16
                ),
                "linux" | "flatpak" => matches!(
                    app.platform,
                    AppPlatform::NativeElf | AppPlatform::LinuxFlatpak
                ),
                other => app.category == other,
            };
            if !cat_ok {
                return false;
            }
            if q.is_empty() {
                return true;
            }
            app.id.to_ascii_lowercase().contains(&q)
                || app.name.to_ascii_lowercase().contains(&q)
                || app.summary_en.to_ascii_lowercase().contains(&q)
                || app.summary_ms.to_ascii_lowercase().contains(&q)
                || app.category.to_ascii_lowercase().contains(&q)
                || app.flatpak_id.to_ascii_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// Filter catalog entries by category and text search query (all architectures).
pub fn filter_catalog_apps(apps: &[CatalogApp], category: &str, query: &str) -> Vec<CatalogApp> {
    filter_catalog_apps_for_arch(apps, category, query, "")
}

pub fn validate_flatpak_id(id: &str) -> Result<(), String> {
    let clean = id.trim();
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() < 3
        || parts.iter().any(|p| {
            p.is_empty()
                || !p
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
    {
        return Err(format!("Invalid Flatpak application ID '{id}'"));
    }
    Ok(())
}

pub fn find_catalog_app<'a>(catalog: &'a AppCatalog, id: &str) -> Result<&'a CatalogApp, String> {
    let needle = id.trim();
    catalog
        .apps
        .iter()
        .find(|a| a.id == needle)
        .ok_or_else(|| format!("Catalog application '{needle}' not found"))
}

fn validate_remote_entry(app: &CatalogApp, allow_insecure_http: bool) -> Result<(), String> {
    validate_catalog_id(&app.id)?;
    if app.platform == AppPlatform::LinuxFlatpak {
        validate_flatpak_id(&app.flatpak_id)?;
        return Ok(());
    }
    validate_single_filename(&app.filename)?;
    if !app.archive_entry.is_empty() {
        validate_safe_rel_path(&app.archive_entry)?;
    }
    if !app.download_url.is_empty() {
        validate_update_url(&app.download_url, allow_insecure_http)?;
        validate_sha256_hex(&app.sha256)?;
    } else if app.bundled_path.is_empty() {
        return Err(format!(
            "Catalog entry '{}' has neither download_url nor bundled_path",
            app.id
        ));
    }
    Ok(())
}

fn download_verified_payload(
    app: &CatalogApp,
    app_store_dir: &Path,
    allow_insecure_http: bool,
) -> Result<PathBuf, String> {
    if app.download_url.trim().is_empty() {
        return Err(format!(
            "Offline app '{}' is not bundled on this system ({})",
            app.id, app.bundled_path
        ));
    }
    validate_update_url(&app.download_url, allow_insecure_http)?;
    let expected_sha = validate_sha256_hex(&app.sha256)?;
    validate_single_filename(&app.filename)?;

    fs::create_dir_all(app_store_dir)
        .map_err(|e| format!("Failed to create store cache {}: {e}", app_store_dir.display()))?;

    let payload_path = app_store_dir.join(&app.filename);
    if payload_path.is_file()
        && sha256_file(&payload_path).ok().as_deref() == Some(expected_sha.as_str())
    {
        return Ok(payload_path);
    }

    let tmp_path = app_store_dir.join(format!("{}.download", app.filename));
    let status = Command::new("curl")
        .args(["-fsSL", "--connect-timeout", "15", "--max-time", "600", "-o"])
        .arg(&tmp_path)
        .arg(app.download_url.trim())
        .status()
        .map_err(|e| format!("Failed to launch curl for {}: {e}", app.id))?;
    if !status.success() {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "Failed to download '{}' from {} (exit {status})",
            app.name, app.download_url
        ));
    }

    if let Err(err) = verify_file_sha256(&tmp_path, &expected_sha) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    fs::rename(&tmp_path, &payload_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed to finalize payload {}: {e}", payload_path.display())
    })?;
    Ok(payload_path)
}

fn extract_zip_entry(zip_path: &Path, extract_dir: &Path, rel_entry: &str) -> Result<PathBuf, String> {
    validate_safe_rel_path(rel_entry)?;
    if extract_dir.exists() {
        let _ = fs::remove_dir_all(extract_dir);
    }
    fs::create_dir_all(extract_dir)
        .map_err(|e| format!("Failed to create extract dir {}: {e}", extract_dir.display()))?;

    let status = Command::new("unzip")
        .args(["-q", "-o"])
        .arg(zip_path)
        .arg("-d")
        .arg(extract_dir)
        .status()
        .map_err(|e| format!("Failed to invoke unzip for {}: {e}", zip_path.display()))?;
    if !status.success() {
        return Err(format!("Failed to extract zip archive {}", zip_path.display()));
    }

    let canon_root = fs::canonicalize(extract_dir)
        .map_err(|e| format!("Failed to resolve extract dir: {e}"))?;
    let entry_path = extract_dir.join(rel_entry);
    let canon_entry = fs::canonicalize(&entry_path)
        .map_err(|e| format!("Archive missing expected binary '{}': {e}", rel_entry))?;
    if !canon_entry.starts_with(&canon_root) || !canon_entry.is_file() {
        return Err(format!(
            "Archive entry '{}' escapes extraction directory",
            rel_entry
        ));
    }
    Ok(canon_entry)
}

/// Download (if needed), SHA-256 verify, extract (if portable zip), and register `app` in `SandboxManager`.
pub fn install_catalog_app(
    manager: &mut SandboxManager,
    app: &CatalogApp,
    store_root: &Path,
    allow_insecure_http: bool,
) -> Result<AppSandbox, String> {
    validate_catalog_id(&app.id)?;

    let install_source = if !app.bundled_path.is_empty() && Path::new(&app.bundled_path).is_file() {
        let bundled = PathBuf::from(&app.bundled_path);
        if !app.sha256.is_empty() {
            verify_file_sha256(&bundled, &app.sha256)?;
        }
        bundled
    } else {
        let app_store_dir = store_root.join(&app.id);
        let payload = download_verified_payload(app, &app_store_dir, allow_insecure_http)?;
        if app.archive_entry.is_empty() {
            payload
        } else {
            extract_zip_entry(&payload, &app_store_dir.join("app"), &app.archive_entry)?
        }
    };

    let mut sandbox = manager.install(&install_source, Some(&app.name))?;
    if let Some(era) = app.win_era {
        if matches!(
            sandbox.platform,
            AppPlatform::WindowsPe | AppPlatform::WindowsNe16 | AppPlatform::WindowsMsi
        ) {
            sandbox = manager.set_win_era(&sandbox.app_id, Some(era))?;
        }
    }
    Ok(sandbox)
}

/// Fetch, cryptographically verify (when `/etc/unicore/update.pub` or `REQUIRE_SIGNATURE=1` is active),
/// validate, and atomically store a remote HTTPS `catalog.json`.
pub fn sync_catalog_verified(
    url: &str,
    dest_path: &Path,
    allow_insecure_http: bool,
    allow_unsigned: bool,
) -> Result<(AppCatalog, SignatureStatus), String> {
    let effective_url = if url.trim().is_empty() {
        OFFICIAL_CATALOG_URL
    } else {
        url.trim()
    };
    validate_update_url(effective_url, allow_insecure_http)?;
    let parent = dest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| "Catalog output path requires a parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;

    let tmp_path = dest_path.with_extension("sync.tmp");
    let sig_tmp = dest_path.with_extension("sync.tmp.sig");
    let status = Command::new("curl")
        .args(["-fsSL", "--connect-timeout", "15", "--max-time", "60", "-o"])
        .arg(&tmp_path)
        .arg(effective_url)
        .status()
        .map_err(|e| format!("Failed to run curl for catalog sync: {e}"))?;
    if !status.success() {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Failed to download catalog from {effective_url} (exit {status})"));
    }

    let sig_url = format!("{effective_url}.sig");
    let _ = Command::new("curl")
        .args(["-fsSL", "--connect-timeout", "10", "--max-time", "20", "-o"])
        .arg(&sig_tmp)
        .arg(&sig_url)
        .status();

    let upd_cfg = load_host_update_config();
    let sig_status = match verify_manifest_signature(
        &tmp_path,
        sig_tmp.is_file().then_some(sig_tmp.as_path()),
        Path::new(&upd_cfg.pubkey_path),
        allow_unsigned,
        upd_cfg.require_signature,
    ) {
        Ok(st) => st,
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&sig_tmp);
            return Err(format!("Catalog signature verification failed: {e}"));
        }
    };
    let _ = fs::remove_file(&sig_tmp);

    let raw = fs::read_to_string(&tmp_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed to read downloaded catalog: {e}")
    })?;
    let _ = fs::remove_file(&tmp_path);

    let catalog = parse_catalog_json(&raw)?;
    for entry in &catalog.apps {
        validate_remote_entry(entry, allow_insecure_http)?;
    }

    let serialized = serialize_catalog_pretty(&catalog);
    fs::write(&tmp_path, serialized)
        .map_err(|e| format!("Failed to stage catalog file: {e}"))?;
    fs::rename(&tmp_path, dest_path)
        .map_err(|e| format!("Failed to save catalog {}: {e}", dest_path.display()))?;
    Ok((catalog, sig_status))
}

/// Fetch, parse, validate, and atomically store a remote HTTPS `catalog.json`.
pub fn sync_catalog_from_url(
    url: &str,
    dest_path: &Path,
    allow_insecure_http: bool,
) -> Result<AppCatalog, String> {
    let (catalog, _) = sync_catalog_verified(url, dest_path, allow_insecure_http, false)?;
    Ok(catalog)
}

pub fn serialize_catalog_pretty(catalog: &AppCatalog) -> String {
    let mut root = BTreeMap::new();
    root.insert("version".to_string(), JsonValue::Number(catalog.version as f64));
    root.insert(
        "apps".to_string(),
        JsonValue::Array(catalog.apps.iter().map(CatalogApp::to_json).collect()),
    );
    format!("{}\n", to_json_pretty(&JsonValue::Object(root)))
}
