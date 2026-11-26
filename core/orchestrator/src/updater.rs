//! A/B system update manifest, SHA-256/signature verification, and 1024-byte GRUB `grubenv` state machine.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::json_codec::{parse_json, to_json_pretty, JsonValue};
use crate::sandbox::sha256_file;

const GRUBENV_HEADER: &str = "# GRUB Environment Block\n";
const GRUBENV_SIZE: usize = 1024;
pub const DEFAULT_SYSTEM_UPDATE_CONF: &str = "/etc/unicore/update.conf";
pub const DEFAULT_DATA_UPDATE_CONF: &str = "/data/unicore/update.conf";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    SlotA,
    SlotB,
}

/// Normalize architecture names across Linux (`uname -m`) and Rust/Android conventions.
pub fn normalize_arch(arch: &str) -> &str {
    match arch.trim() {
        "aarch64" | "arm64" => "arm64",
        "x86_64" | "amd64" => "x86_64",
        "x86" | "i386" | "i686" => "x86",
        other => other,
    }
}

impl Slot {
    pub fn label(&self) -> &'static str {
        match self {
            Slot::SlotA => "UNICORE_SYS_A",
            Slot::SlotB => "UNICORE_SYS_B",
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            Slot::SlotA => "slot_a",
            Slot::SlotB => "slot_b",
        }
    }

    pub fn other(&self) -> Slot {
        match self {
            Slot::SlotA => Slot::SlotB,
            Slot::SlotB => Slot::SlotA,
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        let norm = raw.trim().to_ascii_uppercase();
        match norm.as_str() {
            "UNICORE_SYS_A" | "SLOT_A" | "A" => Ok(Slot::SlotA),
            "UNICORE_SYS_B" | "SLOT_B" | "B" => Ok(Slot::SlotB),
            _ => Err(format!("Unknown A/B slot '{raw}'. Expected UNICORE_SYS_A or UNICORE_SYS_B.")),
        }
    }
}

fn split_version(raw: &str) -> (Vec<u64>, Option<Vec<String>>) {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match trimmed.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (trimmed, None),
    };
    let nums: Vec<u64> = core
        .split('.')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .collect();
    let pre_parts = pre.map(|p| {
        p.split('.')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    });
    (nums, pre_parts)
}

/// Compare two SemVer-like version strings (`0.1.0`, `v0.2.0-rc1`, `0.2.0`).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (nums_a, pre_a) = split_version(a);
    let (nums_b, pre_b) = split_version(b);
    let max_len = nums_a.len().max(nums_b.len());
    for idx in 0..max_len {
        let va = nums_a.get(idx).copied().unwrap_or(0);
        let vb = nums_b.get(idx).copied().unwrap_or(0);
        match va.cmp(&vb) {
            Ordering::Equal => {}
            non_eq => return non_eq,
        }
    }

    match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(pa), Some(pb)) => {
            let plen = pa.len().max(pb.len());
            for idx in 0..plen {
                match (pa.get(idx), pb.get(idx)) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(sa), Some(sb)) => {
                        let ord = match (sa.parse::<u64>(), sb.parse::<u64>()) {
                            (Ok(na), Ok(nb)) => na.cmp(&nb),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => sa.cmp(sb),
                        };
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                }
            }
            Ordering::Equal
        }
    }
}

/// Extract `VERSION_ID` from `/etc/os-release` content (defaults to `"0.0.0"`).
pub fn parse_os_version(os_release: &str) -> String {
    for line in os_release.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == "VERSION_ID" {
                let clean = v.trim().trim_matches('"').trim_matches('\'').trim();
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
        }
    }
    "0.0.0".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateConfig {
    pub update_url: String,
    pub channel: String,
    pub pubkey_path: String,
    pub allow_insecure_http: bool,
    pub require_signature: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            update_url: String::new(),
            channel: "stable".to_string(),
            pubkey_path: "/etc/unicore/update.pub".to_string(),
            allow_insecure_http: false,
            require_signature: false,
        }
    }
}

impl UpdateConfig {
    pub fn parse(raw: &str) -> Self {
        Self::default().merge(raw)
    }

    pub fn merge(mut self, override_raw: &str) -> Self {
        for line in override_raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((key, val_raw)) = trimmed.split_once('=') else {
                continue;
            };
            let val = val_raw.trim().trim_matches('"').trim_matches('\'').trim();
            match key.trim() {
                "UPDATE_URL" => self.update_url = val.to_string(),
                "UPDATE_CHANNEL" if !val.is_empty() => self.channel = val.to_string(),
                "UPDATE_PUBKEY" if !val.is_empty() => self.pubkey_path = val.to_string(),
                "ALLOW_INSECURE_HTTP" => {
                    self.allow_insecure_http =
                        val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes");
                }
                "REQUIRE_SIGNATURE" => {
                    self.require_signature =
                        val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes");
                }
                _ => {}
            }
        }
        self
    }

    pub fn render(&self) -> String {
        format!(
            "UPDATE_URL=\"{}\"\nUPDATE_CHANNEL=\"{}\"\nUPDATE_PUBKEY=\"{}\"\nALLOW_INSECURE_HTTP={}\nREQUIRE_SIGNATURE={}\n",
            self.update_url,
            self.channel,
            self.pubkey_path,
            if self.allow_insecure_http { 1 } else { 0 },
            if self.require_signature { 1 } else { 0 }
        )
    }
}

pub fn load_host_update_config() -> UpdateConfig {
    let mut cfg = fs::read_to_string(DEFAULT_SYSTEM_UPDATE_CONF)
        .map(|raw| UpdateConfig::parse(&raw))
        .unwrap_or_default();
    if let Ok(override_raw) = fs::read_to_string(DEFAULT_DATA_UPDATE_CONF) {
        cfg = cfg.merge(&override_raw);
    }
    cfg
}

pub fn validate_update_url(url: &str, allow_insecure_http: bool) -> Result<(), String> {
    let clean = url.trim();
    if clean.is_empty() {
        return Err("Update manifest URL is empty".to_string());
    }
    if clean.chars().any(|c| {
        c.is_control()
            || c.is_whitespace()
            || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&')
    }) {
        return Err("Update URL contains unsafe or invalid characters".to_string());
    }
    let rest = if let Some(r) = clean.strip_prefix("https://") {
        r
    } else if let Some(r) = clean.strip_prefix("http://") {
        if !allow_insecure_http {
            return Err("HTTPS is required for OTA update URLs (pass --allow-insecure-http only for local testing)".to_string());
        }
        r
    } else {
        return Err("Update URL must start with https:// (or http:// with --allow-insecure-http)".to_string());
    };

    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() || host.starts_with('.') {
        return Err("Update URL is missing a valid host".to_string());
    }
    Ok(())
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name != "."
        && !name.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn is_valid_sha256_hex(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactEntry {
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl ArtifactEntry {
    pub fn to_json(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("filename".to_string(), JsonValue::Str(self.filename.clone()));
        map.insert("sha256".to_string(), JsonValue::Str(self.sha256.clone()));
        map.insert("size".to_string(), JsonValue::Number(self.size as f64));
        map.insert("url".to_string(), JsonValue::Str(self.url.clone()));
        JsonValue::Object(map)
    }

    pub fn from_json(value: &JsonValue, label: &str) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| format!("Artifact '{label}' must be a JSON object"))?;
        let filename = obj
            .get("filename")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| format!("Artifact '{label}' missing filename"))?
            .to_string();
        if !is_safe_filename(&filename) {
            return Err(format!("Unsafe artifact filename in '{label}': '{filename}'"));
        }
        let sha256 = obj
            .get("sha256")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| format!("Artifact '{label}' missing sha256"))?
            .to_ascii_lowercase();
        if !is_valid_sha256_hex(&sha256) {
            return Err(format!("Invalid sha256 digest in '{label}'"));
        }
        let size = match obj.get("size") {
            Some(JsonValue::Number(n)) if *n >= 0.0 => *n as u64,
            _ => return Err(format!("Artifact '{label}' missing valid numeric size")),
        };
        let url = obj
            .get("url")
            .and_then(JsonValue::as_str)
            .unwrap_or(&filename)
            .to_string();
        Ok(Self {
            filename,
            url,
            sha256,
            size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateManifest {
    pub schema_version: u32,
    pub version: String,
    pub channel: String,
    pub arch: String,
    pub published_at: String,
    pub notes: String,
    pub vmlinuz: ArtifactEntry,
    pub initramfs: ArtifactEntry,
    pub squashfs: ArtifactEntry,
}

impl UpdateManifest {
    pub fn to_json(&self) -> JsonValue {
        let mut artifacts = BTreeMap::new();
        artifacts.insert("initramfs".to_string(), self.initramfs.to_json());
        artifacts.insert("squashfs".to_string(), self.squashfs.to_json());
        artifacts.insert("vmlinuz".to_string(), self.vmlinuz.to_json());

        let mut root = BTreeMap::new();
        root.insert("arch".to_string(), JsonValue::Str(self.arch.clone()));
        root.insert("artifacts".to_string(), JsonValue::Object(artifacts));
        root.insert("channel".to_string(), JsonValue::Str(self.channel.clone()));
        root.insert("notes".to_string(), JsonValue::Str(self.notes.clone()));
        root.insert(
            "published_at".to_string(),
            JsonValue::Str(self.published_at.clone()),
        );
        root.insert(
            "schema_version".to_string(),
            JsonValue::Number(self.schema_version as f64),
        );
        root.insert("version".to_string(), JsonValue::Str(self.version.clone()));
        JsonValue::Object(root)
    }

    pub fn from_json(value: &JsonValue) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "Manifest root must be a JSON object".to_string())?;
        let schema_version = match obj.get("schema_version") {
            Some(JsonValue::Number(n)) if (*n as u32) == 1 => 1,
            _ => return Err("Unsupported or missing manifest schema_version (expected 1)".to_string()),
        };
        let version = obj
            .get("version")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Manifest missing version".to_string())?
            .to_string();
        let channel = obj
            .get("channel")
            .and_then(JsonValue::as_str)
            .unwrap_or("stable")
            .to_string();
        let arch = obj
            .get("arch")
            .and_then(JsonValue::as_str)
            .unwrap_or("x86_64")
            .to_string();
        let published_at = obj
            .get("published_at")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .to_string();
        let notes = obj
            .get("notes")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .to_string();

        let artifacts = obj
            .get("artifacts")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| "Manifest missing artifacts object".to_string())?;
        let vmlinuz = ArtifactEntry::from_json(
            artifacts
                .get("vmlinuz")
                .ok_or_else(|| "Manifest missing artifacts.vmlinuz".to_string())?,
            "vmlinuz",
        )?;
        let initramfs = ArtifactEntry::from_json(
            artifacts
                .get("initramfs")
                .ok_or_else(|| "Manifest missing artifacts.initramfs".to_string())?,
            "initramfs",
        )?;
        let squashfs = ArtifactEntry::from_json(
            artifacts
                .get("squashfs")
                .ok_or_else(|| "Manifest missing artifacts.squashfs".to_string())?,
            "squashfs",
        )?;

        Ok(Self {
            schema_version,
            version,
            channel,
            arch,
            published_at,
            notes,
            vmlinuz,
            initramfs,
            squashfs,
        })
    }

    pub fn parse_json_str(raw: &str) -> Result<Self, String> {
        let val = parse_json(raw)?;
        Self::from_json(&val)
    }
}

pub fn resolve_artifact_url(manifest_url: &str, artifact_url: &str, filename: &str) -> String {
    let candidate = if artifact_url.trim().is_empty() {
        filename
    } else {
        artifact_url.trim()
    };
    if candidate.starts_with("https://") || candidate.starts_with("http://") {
        return candidate.to_string();
    }
    let without_query = manifest_url
        .split(['?', '#'])
        .next()
        .unwrap_or(manifest_url);
    let base = match without_query.rfind('/') {
        Some(idx) => &without_query[..=idx],
        None => "",
    };
    format!("{base}{}", candidate.trim_start_matches("./"))
}

fn build_artifact_entry(path: &Path, base_url: Option<&str>) -> Result<ArtifactEntry, String> {
    if !path.is_file() {
        return Err(format!("Artifact file not found: {}", path.display()));
    }
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid artifact filename: {}", path.display()))?
        .to_string();
    if !is_safe_filename(&filename) {
        return Err(format!("Unsafe artifact filename: '{filename}'"));
    }
    let size = fs::metadata(path)
        .map_err(|e| format!("Failed to stat {}: {e}", path.display()))?
        .len();
    let sha256 = sha256_file(path)?;
    let url = match base_url {
        Some(base) if !base.trim().is_empty() => {
            format!("{}/{}", base.trim().trim_end_matches('/'), filename)
        }
        _ => filename.clone(),
    };
    Ok(ArtifactEntry {
        filename,
        url,
        sha256,
        size,
    })
}

pub struct ManifestBuildParams<'a> {
    pub version: &'a str,
    pub channel: &'a str,
    pub arch: &'a str,
    pub notes: &'a str,
    pub vmlinuz_path: &'a Path,
    pub initramfs_path: &'a Path,
    pub squashfs_path: &'a Path,
    pub base_url: Option<&'a str>,
}

pub fn generate_manifest(params: &ManifestBuildParams<'_>) -> Result<UpdateManifest, String> {
    let clean_ver = params.version.trim();
    if clean_ver.is_empty() {
        return Err("Manifest version cannot be empty".to_string());
    }
    let published_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("unix:{}", d.as_secs()))
        .unwrap_or_else(|_| "unix:0".to_string());

    Ok(UpdateManifest {
        schema_version: 1,
        version: clean_ver.to_string(),
        channel: params.channel.trim().to_string(),
        arch: params.arch.trim().to_string(),
        published_at,
        notes: params.notes.to_string(),
        vmlinuz: build_artifact_entry(params.vmlinuz_path, params.base_url)?,
        initramfs: build_artifact_entry(params.initramfs_path, params.base_url)?,
        squashfs: build_artifact_entry(params.squashfs_path, params.base_url)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    Verified,
    UnsignedAllowed,
    NoPubkeyConfigured,
}

impl SignatureStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignatureStatus::Verified => "verified",
            SignatureStatus::UnsignedAllowed => "unsigned_allowed",
            SignatureStatus::NoPubkeyConfigured => "no_pubkey_configured",
        }
    }
}

pub fn verify_manifest_signature(
    manifest_path: &Path,
    sig_path: Option<&Path>,
    pubkey_path: &Path,
    allow_unsigned: bool,
    require_signature: bool,
) -> Result<SignatureStatus, String> {
    if allow_unsigned {
        return Ok(SignatureStatus::UnsignedAllowed);
    }
    if !pubkey_path.is_file() {
        if require_signature {
            return Err(format!(
                "Signed manifest required (REQUIRE_SIGNATURE=1), but release public key is missing at {}",
                pubkey_path.display()
            ));
        }
        return Ok(SignatureStatus::NoPubkeyConfigured);
    }
    let Some(sig) = sig_path.filter(|p| p.is_file()) else {
        return Err(format!(
            "Manifest signature (.sig) required because public key '{}' is configured",
            pubkey_path.display()
        ));
    };

    let output = Command::new("openssl")
        .args(["pkeyutl", "-verify", "-pubin", "-inkey"])
        .arg(pubkey_path)
        .arg("-rawin")
        .arg("-in")
        .arg(manifest_path)
        .arg("-sigfile")
        .arg(sig)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke openssl for signature verification: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Cryptographic signature verification failed".to_string()
        } else {
            format!("Cryptographic signature verification failed: {stderr}")
        });
    }
    Ok(SignatureStatus::Verified)
}

/// Generate an Ed25519 signing keypair using `openssl`.
pub fn generate_ed25519_keypair(privkey_path: &Path, pubkey_path: &Path) -> Result<(), String> {
    if let Some(parent) = privkey_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create key directory {}: {e}", parent.display()))?;
    }
    if let Some(parent) = pubkey_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create pubkey directory {}: {e}", parent.display()))?;
    }

    let gen_out = Command::new("openssl")
        .args(["genpkey", "-algorithm", "ED25519", "-out"])
        .arg(privkey_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke openssl genpkey: {e}"))?;
    if !gen_out.status.success() {
        let stderr = String::from_utf8_lossy(&gen_out.stderr).trim().to_string();
        return Err(format!("openssl genpkey failed: {stderr}"));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(privkey_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(privkey_path, perms);
        }
    }

    let pub_out = Command::new("openssl")
        .args(["pkey", "-in"])
        .arg(privkey_path)
        .arg("-pubout")
        .arg("-out")
        .arg(pubkey_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke openssl pkey -pubout: {e}"))?;
    if !pub_out.status.success() {
        let stderr = String::from_utf8_lossy(&pub_out.stderr).trim().to_string();
        return Err(format!("openssl pkey -pubout failed: {stderr}"));
    }
    Ok(())
}

/// Sign a manifest file using an Ed25519 private key via `openssl pkeyutl -sign -rawin`.
pub fn sign_manifest_file(
    manifest_path: &Path,
    privkey_path: &Path,
    sig_out_path: &Path,
) -> Result<(), String> {
    if !manifest_path.is_file() {
        return Err(format!(
            "Manifest not found for signing: {}",
            manifest_path.display()
        ));
    }
    if !privkey_path.is_file() {
        return Err(format!(
            "Private signing key not found: {}",
            privkey_path.display()
        ));
    }
    if let Some(parent) = sig_out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create sig directory {}: {e}", parent.display()))?;
    }

    let sign_out = Command::new("openssl")
        .args(["pkeyutl", "-sign", "-inkey"])
        .arg(privkey_path)
        .arg("-rawin")
        .arg("-in")
        .arg(manifest_path)
        .arg("-out")
        .arg(sig_out_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke openssl pkeyutl -sign: {e}"))?;
    if !sign_out.status.success() {
        let stderr = String::from_utf8_lossy(&sign_out.stderr).trim().to_string();
        return Err(format!("Manifest signing failed: {stderr}"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrubEnv {
    vars: BTreeMap<String, String>,
}

impl GrubEnv {
    pub fn new_initial(slot: Slot, version: &str) -> Self {
        let mut vars = BTreeMap::new();
        vars.insert("active_slot".to_string(), slot.label().to_string());
        vars.insert("pending_slot".to_string(), String::new());
        vars.insert("boot_tried".to_string(), "0".to_string());
        match slot {
            Slot::SlotA => {
                vars.insert("slot_a_version".to_string(), version.to_string());
                vars.insert("slot_b_version".to_string(), String::new());
            }
            Slot::SlotB => {
                vars.insert("slot_a_version".to_string(), String::new());
                vars.insert("slot_b_version".to_string(), version.to_string());
            }
        }
        Self { vars }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        if !raw.starts_with(GRUBENV_HEADER) {
            return Err("Invalid GRUB environment block header".to_string());
        }
        let mut vars = BTreeMap::new();
        for line in raw[GRUBENV_HEADER.len()..].lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            vars.insert(k.to_string(), v.to_string());
        }
        Ok(Self { vars })
    }

    pub fn render_1024(&self) -> Result<String, String> {
        let mut out = String::from(GRUBENV_HEADER);
        for (k, v) in &self.vars {
            if k.contains('\n') || v.contains('\n') || k.contains('=') {
                return Err(format!("Invalid grubenv entry '{k}={v}'"));
            }
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
        if out.len() > GRUBENV_SIZE {
            return Err(format!(
                "grubenv payload exceeds {GRUBENV_SIZE} bytes ({})",
                out.len()
            ));
        }
        while out.len() < GRUBENV_SIZE {
            out.push('#');
        }
        Ok(out)
    }

    pub fn read_file(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        Self::parse(&raw)
    }

    pub fn write_file_atomic(&self, path: &Path) -> Result<(), String> {
        let rendered = self.render_1024()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, rendered.as_bytes())
            .map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
        fs::rename(&tmp, path)
            .map_err(|e| format!("Failed to persist {}: {e}", path.display()))?;
        Ok(())
    }

    pub fn active_slot(&self) -> Slot {
        self.vars
            .get("active_slot")
            .and_then(|s| Slot::parse(s).ok())
            .unwrap_or(Slot::SlotA)
    }

    pub fn pending_slot(&self) -> Option<Slot> {
        self.vars
            .get("pending_slot")
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .and_then(|s| Slot::parse(s).ok())
    }

    pub fn boot_tried(&self) -> bool {
        self.vars.get("boot_tried").map(String::as_str) == Some("1")
    }

    pub fn slot_version(&self, slot: Slot) -> &str {
        let key = match slot {
            Slot::SlotA => "slot_a_version",
            Slot::SlotB => "slot_b_version",
        };
        self.vars.get(key).map(String::as_str).unwrap_or("")
    }

    pub fn stage_update(&mut self, target: Slot, version: &str) {
        self.vars
            .insert("pending_slot".to_string(), target.label().to_string());
        self.vars.insert("boot_tried".to_string(), "0".to_string());
        let ver_key = match target {
            Slot::SlotA => "slot_a_version",
            Slot::SlotB => "slot_b_version",
        };
        self.vars.insert(ver_key.to_string(), version.to_string());
    }

    /// Mirror GRUB's boot-once trial & auto-rollback logic for deterministic verification.
    pub fn simulate_grub_boot(&mut self) -> (Slot, bool) {
        if let Some(pending) = self.pending_slot() {
            if !self.boot_tried() {
                self.vars.insert("boot_tried".to_string(), "1".to_string());
                return (pending, false);
            }
            // Trial boot never confirmed -> clear pending_slot and roll back to active_slot.
            self.vars
                .insert("pending_slot".to_string(), String::new());
            self.vars.insert("boot_tried".to_string(), "0".to_string());
            return (self.active_slot(), true);
        }
        (self.active_slot(), false)
    }

    pub fn confirm_boot(&mut self, booted: Slot) -> bool {
        if self.pending_slot() == Some(booted) {
            self.vars
                .insert("active_slot".to_string(), booted.label().to_string());
            self.vars
                .insert("pending_slot".to_string(), String::new());
            self.vars.insert("boot_tried".to_string(), "0".to_string());
            true
        } else {
            false
        }
    }

    pub fn request_rollback(&mut self) -> Result<Slot, String> {
        let target = self.active_slot().other();
        self.vars
            .insert("active_slot".to_string(), target.label().to_string());
        self.vars
            .insert("pending_slot".to_string(), String::new());
        self.vars.insert("boot_tried".to_string(), "0".to_string());
        Ok(target)
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert(
            "active_slot".to_string(),
            JsonValue::Str(self.active_slot().label().to_string()),
        );
        map.insert("boot_tried".to_string(), JsonValue::Bool(self.boot_tried()));
        map.insert(
            "pending_slot".to_string(),
            match self.pending_slot() {
                Some(s) => JsonValue::Str(s.label().to_string()),
                None => JsonValue::Str(String::new()),
            },
        );
        map.insert(
            "slot_a_version".to_string(),
            JsonValue::Str(self.slot_version(Slot::SlotA).to_string()),
        );
        map.insert(
            "slot_b_version".to_string(),
            JsonValue::Str(self.slot_version(Slot::SlotB).to_string()),
        );
        JsonValue::Object(map)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootSlotInfo {
    pub boot_mode: String,
    pub boot_media: String,
    pub boot_slot: Option<Slot>,
}

pub fn parse_boot_env_file(raw: &str) -> BootSlotInfo {
    let mut boot_mode = "unknown".to_string();
    let mut boot_media = String::new();
    let mut boot_slot = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val_raw)) = trimmed.split_once('=') else {
            continue;
        };
        let val = val_raw.trim().trim_matches('"').trim_matches('\'');
        match key.trim() {
            "UNICORE_BOOT_MODE" if !val.is_empty() => boot_mode = val.to_string(),
            "UNICORE_BOOT_MEDIA" => boot_media = val.to_string(),
            "UNICORE_BOOT_SLOT" if !val.is_empty() => {
                boot_slot = Slot::parse(val).ok();
            }
            _ => {}
        }
    }

    BootSlotInfo {
        boot_mode,
        boot_media,
        boot_slot,
    }
}

pub fn read_boot_slot_info(path: &Path) -> BootSlotInfo {
    fs::read_to_string(path)
        .map(|s| parse_boot_env_file(&s))
        .unwrap_or(BootSlotInfo {
            boot_mode: "unknown".to_string(),
            boot_media: String::new(),
            boot_slot: None,
        })
}

pub fn build_update_status_json(
    boot: &BootSlotInfo,
    current_version: &str,
    grubenv: Option<&GrubEnv>,
    cfg: &UpdateConfig,
    standby_ready: Option<bool>,
) -> JsonValue {
    let active_slot = boot
        .boot_slot
        .or_else(|| grubenv.map(GrubEnv::active_slot))
        .unwrap_or(Slot::SlotA);
    let standby_slot = active_slot.other();
    let pending_slot = grubenv.and_then(GrubEnv::pending_slot);
    let boot_tried = grubenv.is_some_and(GrubEnv::boot_tried);
    let can_update = boot.boot_mode == "installed";
    let can_rollback = can_update
        && standby_ready.unwrap_or_else(|| {
            grubenv.is_some_and(|g| !g.slot_version(standby_slot).is_empty())
        });

    let mut map = BTreeMap::new();
    map.insert(
        "active_slot".to_string(),
        JsonValue::Str(active_slot.label().to_string()),
    );
    map.insert(
        "allow_insecure_http".to_string(),
        JsonValue::Bool(cfg.allow_insecure_http),
    );
    map.insert(
        "boot_mode".to_string(),
        JsonValue::Str(boot.boot_mode.clone()),
    );
    map.insert("boot_tried".to_string(), JsonValue::Bool(boot_tried));
    map.insert("can_rollback".to_string(), JsonValue::Bool(can_rollback));
    map.insert("can_update".to_string(), JsonValue::Bool(can_update));
    map.insert("channel".to_string(), JsonValue::Str(cfg.channel.clone()));
    map.insert(
        "current_version".to_string(),
        JsonValue::Str(current_version.to_string()),
    );
    map.insert(
        "pending_slot".to_string(),
        JsonValue::Str(
            pending_slot
                .map(|s| s.label().to_string())
                .unwrap_or_default(),
        ),
    );
    map.insert(
        "pubkey_configured".to_string(),
        JsonValue::Bool(Path::new(&cfg.pubkey_path).is_file()),
    );
    map.insert(
        "require_signature".to_string(),
        JsonValue::Bool(cfg.require_signature),
    );
    map.insert(
        "slot_a_version".to_string(),
        JsonValue::Str(
            grubenv
                .map(|g| g.slot_version(Slot::SlotA).to_string())
                .unwrap_or_default(),
        ),
    );
    map.insert(
        "slot_b_version".to_string(),
        JsonValue::Str(
            grubenv
                .map(|g| g.slot_version(Slot::SlotB).to_string())
                .unwrap_or_default(),
        ),
    );
    map.insert(
        "standby_slot".to_string(),
        JsonValue::Str(standby_slot.label().to_string()),
    );
    map.insert(
        "update_url".to_string(),
        JsonValue::Str(cfg.update_url.clone()),
    );
    JsonValue::Object(map)
}

pub fn render_manifest_pretty(manifest: &UpdateManifest) -> String {
    to_json_pretty(&manifest.to_json())
}
