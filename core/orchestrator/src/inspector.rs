//! Binary format inspector for Windows PE/MSI, Android APK, and native ELF files.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Supported application binary platforms recognized by Universal OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPlatform {
    WindowsPe,
    WindowsNe16,
    WindowsMsi,
    Dos16,
    AndroidApk,
    NativeElf,
    Unknown,
}

impl AppPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppPlatform::WindowsPe => "windows_pe",
            AppPlatform::WindowsNe16 => "windows_ne16",
            AppPlatform::WindowsMsi => "windows_msi",
            AppPlatform::Dos16 => "dos16",
            AppPlatform::AndroidApk => "android_apk",
            AppPlatform::NativeElf => "native_elf",
            AppPlatform::Unknown => "unknown",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "windows_pe" => Ok(AppPlatform::WindowsPe),
            "windows_ne16" => Ok(AppPlatform::WindowsNe16),
            "windows_msi" => Ok(AppPlatform::WindowsMsi),
            "dos16" => Ok(AppPlatform::Dos16),
            "android_apk" => Ok(AppPlatform::AndroidApk),
            "native_elf" => Ok(AppPlatform::NativeElf),
            "unknown" => Ok(AppPlatform::Unknown),
            other => Err(format!("Unknown platform '{other}'")),
        }
    }
}

/// Target Windows compatibility era inferred from binary headers or chosen by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinEra {
    Win98,
    WinXp,
    Win7,
    Win10,
}

impl WinEra {
    pub fn as_str(&self) -> &'static str {
        match self {
            WinEra::Win98 => "win98",
            WinEra::WinXp => "winxp",
            WinEra::Win7 => "win7",
            WinEra::Win10 => "win10",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "win98" => Ok(WinEra::Win98),
            "winxp" => Ok(WinEra::WinXp),
            "win7" => Ok(WinEra::Win7),
            "win10" => Ok(WinEra::Win10),
            other => Err(format!(
                "Unknown Windows era '{other}'. Valid options: win98, winxp, win7, win10"
            )),
        }
    }
}

impl fmt::Display for AppPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured metadata extracted from a binary file or package on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInfo {
    pub platform: AppPlatform,
    pub arch: String,
    pub sub_abis: Vec<String>,
    pub is_gui: bool,
    pub is_library: bool,
    pub has_dex: bool,
    pub win_era: Option<WinEra>,
}

impl BinaryInfo {
    /// Return `true` when executing this binary on `host_arch` requires CPU translation.
    pub fn requires_cpu_translation(&self, host_arch: &str) -> bool {
        match self.platform {
            AppPlatform::AndroidApk => {
                if self.sub_abis.is_empty() {
                    // Pure DEX bytecode compiles ahead-of-time via ART for the host CPU.
                    return false;
                }
                let compatible = compatible_android_abis(host_arch);
                !self
                    .sub_abis
                    .iter()
                    .any(|abi| compatible.contains(&abi.as_str()))
            }
            AppPlatform::WindowsPe | AppPlatform::WindowsNe16 | AppPlatform::NativeElf => {
                if self.arch == "unknown" {
                    return false;
                }
                let compatible = compatible_native_arches(host_arch);
                !compatible.contains(&self.arch.as_str())
            }
            AppPlatform::Dos16 | AppPlatform::WindowsMsi | AppPlatform::Unknown => false,
        }
    }
}

const OLE_MAGIC: &[u8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
const ELF_MAGIC: &[u8] = b"\x7fELF";
const ZIP_LOCAL_MAGIC: &[u8] = b"PK\x03\x04";
const ZIP_EMPTY_EOCD_MAGIC: &[u8] = b"PK\x05\x06";
const PE_DLL_CHARACTERISTIC: u16 = 0x2000;

fn compatible_native_arches(host_arch: &str) -> &'static [&'static str] {
    match host_arch {
        "x86_64" => &["x86_64", "x86", "x86_16"],
        "arm64" => &["arm64", "arm"],
        "x86" => &["x86", "x86_16"],
        _ => &[],
    }
}

fn compatible_android_abis(host_arch: &str) -> &'static [&'static str] {
    match host_arch {
        "x86_64" => &["x86_64", "x86"],
        "arm64" => &["arm64-v8a", "armeabi-v7a", "armeabi"],
        "x86" => &["x86"],
        _ => &[],
    }
}

fn map_pe_machine(machine: u16) -> &'static str {
    match machine {
        0x014C => "x86",
        0x8664 => "x86_64",
        0xAA64 => "arm64",
        0x01C0 | 0x01C4 => "arm",
        _ => "unknown",
    }
}

fn map_elf_machine(machine: u16) -> &'static str {
    match machine {
        0x03 => "x86",
        0x3E => "x86_64",
        0xB7 => "arm64",
        0x28 => "arm",
        0xF3 => "riscv64",
        _ => "unknown",
    }
}

fn android_abi_to_arch(abi: &str) -> Option<&'static str> {
    match abi {
        "x86_64" => Some("x86_64"),
        "arm64-v8a" => Some("arm64"),
        "armeabi-v7a" | "armeabi" => Some("arm"),
        "x86" => Some("x86"),
        _ => None,
    }
}

fn read_u16_le(buf: &[u8], offset: usize) -> Option<u16> {
    let bytes: [u8; 2] = buf.get(offset..offset + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn read_u32_le(buf: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = buf.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn classify_pe_era(
    sub_major: u16,
    sub_minor: u16,
    os_major: u16,
    os_minor: u16,
    arch: &str,
) -> WinEra {
    let (major, minor) = if sub_major > 0 {
        (sub_major, sub_minor)
    } else {
        (os_major, os_minor)
    };
    match major {
        1..=4 => WinEra::Win98,
        5 => WinEra::WinXp,
        6 if minor == 1 || (minor == 0 && arch == "x86") => WinEra::Win7,
        _ => WinEra::Win10,
    }
}

fn parse_pe(data: &[u8]) -> Option<BinaryInfo> {
    if data.len() < 64 || !data.starts_with(b"MZ") {
        return None;
    }

    let e_lfanew = read_u32_le(data, 0x3C)? as usize;
    if e_lfanew < 64 {
        return None;
    }

    // Detect 16-bit Win16 / Win98 NE executables (`NE` header at e_lfanew).
    if data.get(e_lfanew..e_lfanew + 2) == Some(b"NE") {
        return Some(BinaryInfo {
            platform: AppPlatform::WindowsNe16,
            arch: "x86_16".to_string(),
            sub_abis: Vec::new(),
            is_gui: true,
            is_library: false,
            has_dex: false,
            win_era: Some(WinEra::Win98),
        });
    }

    let coff_offset = e_lfanew.checked_add(4)?;
    if coff_offset.checked_add(20)? > data.len() {
        return None;
    }
    if data.get(e_lfanew..e_lfanew + 4)? != b"PE\x00\x00" {
        return None;
    }

    let machine = read_u16_le(data, coff_offset)?;
    let opt_size = read_u16_le(data, coff_offset + 16)? as usize;
    let characteristics = read_u16_le(data, coff_offset + 18)?;

    let arch = map_pe_machine(machine).to_string();
    let is_library = (characteristics & PE_DLL_CHARACTERISTIC) != 0;

    let opt_offset = coff_offset + 20;
    let mut is_gui = true;
    let mut win_era = WinEra::Win10;
    // OS version (+40) and SubsystemVersion (+48) share identical offsets in PE32 and PE32+.
    if opt_size >= 52 && opt_offset + 52 <= data.len() {
        let os_major = read_u16_le(data, opt_offset + 40).unwrap_or(0);
        let os_minor = read_u16_le(data, opt_offset + 42).unwrap_or(0);
        let sub_major = read_u16_le(data, opt_offset + 48).unwrap_or(0);
        let sub_minor = read_u16_le(data, opt_offset + 50).unwrap_or(0);
        win_era = classify_pe_era(sub_major, sub_minor, os_major, os_minor, &arch);
    }
    // Subsystem resides at offset 68 in both PE32 and PE32+ optional headers.
    if opt_size >= 70 && opt_offset + 70 <= data.len() {
        if let Some(subsystem) = read_u16_le(data, opt_offset + 68) {
            is_gui = matches!(subsystem, 2 | 9);
        }
    }

    Some(BinaryInfo {
        platform: AppPlatform::WindowsPe,
        arch,
        sub_abis: Vec::new(),
        is_gui,
        is_library,
        has_dex: false,
        win_era: Some(win_era),
    })
}

/// Detect real 16-bit MS-DOS MZ executables (requiring non-zero page/header sizes so corrupt MZ stubs stay Unknown).
fn parse_dos_mz(data: &[u8]) -> Option<BinaryInfo> {
    if data.len() < 28 || (!data.starts_with(b"MZ") && !data.starts_with(b"ZM")) {
        return None;
    }
    let e_cp = read_u16_le(data, 4)?;
    let e_cparhdr = read_u16_le(data, 8)?;
    let hdr_bytes = (e_cparhdr as usize).checked_mul(16)?;
    if e_cp < 1 || e_cparhdr < 2 || hdr_bytes > data.len() {
        return None;
    }
    Some(BinaryInfo {
        platform: AppPlatform::Dos16,
        arch: "x86_16".to_string(),
        sub_abis: Vec::new(),
        is_gui: true,
        is_library: false,
        has_dex: false,
        win_era: None,
    })
}

fn parse_elf(data: &[u8]) -> Option<BinaryInfo> {
    if data.len() < 20 || !data.starts_with(ELF_MAGIC) {
        return None;
    }
    let e_type_bytes: [u8; 2] = data.get(16..18)?.try_into().ok()?;
    let e_mach_bytes: [u8; 2] = data.get(18..20)?.try_into().ok()?;
    let (e_type, e_machine) = match data[5] {
        1 => (
            u16::from_le_bytes(e_type_bytes),
            u16::from_le_bytes(e_mach_bytes),
        ),
        2 => (
            u16::from_be_bytes(e_type_bytes),
            u16::from_be_bytes(e_mach_bytes),
        ),
        _ => return None,
    };

    let arch = map_elf_machine(e_machine).to_string();
    let is_library = e_type == 3 && arch == "unknown";
    Some(BinaryInfo {
        platform: AppPlatform::NativeElf,
        arch,
        sub_abis: Vec::new(),
        is_gui: false,
        is_library,
        has_dex: false,
        win_era: None,
    })
}

/// Read ZIP Central Directory filenames without decompressing entry payloads.
fn read_zip_entry_names(file: &mut File, file_len: u64) -> Option<Vec<String>> {
    if file_len < 22 {
        return None;
    }
    let tail_len = file_len.min(65_557) as usize;
    file.seek(SeekFrom::Start(file_len - tail_len as u64)).ok()?;
    let mut tail = vec![0u8; tail_len];
    file.read_exact(&mut tail).ok()?;

    // Locate End of Central Directory (PK\x05\x06) scanning backwards.
    let mut eocd_idx = None;
    let mut idx = tail.len().saturating_sub(22);
    loop {
        if tail.get(idx..idx + 4) == Some(ZIP_EMPTY_EOCD_MAGIC) {
            let comment_len = read_u16_le(&tail, idx + 20)? as usize;
            if idx + 22 + comment_len == tail.len() {
                eocd_idx = Some(idx);
                break;
            }
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    let eocd = eocd_idx?;
    let total_entries = read_u16_le(&tail, eocd + 10)? as usize;
    let cd_size = read_u32_le(&tail, eocd + 12)? as usize;
    let cd_offset = read_u32_le(&tail, eocd + 16)? as u64;

    if cd_offset.checked_add(cd_size as u64)? > file_len {
        return None;
    }

    file.seek(SeekFrom::Start(cd_offset)).ok()?;
    let mut cd = vec![0u8; cd_size];
    file.read_exact(&mut cd).ok()?;

    let mut names = Vec::with_capacity(total_entries);
    let mut pos = 0usize;
    for _ in 0..total_entries {
        if cd.get(pos..pos + 4) != Some(b"PK\x01\x02") {
            return None;
        }
        let name_len = read_u16_le(&cd, pos + 28)? as usize;
        let extra_len = read_u16_le(&cd, pos + 30)? as usize;
        let comment_len = read_u16_le(&cd, pos + 32)? as usize;
        let name_start = pos + 46;
        let name_end = name_start.checked_add(name_len)?;
        let name_slice = cd.get(name_start..name_end)?;
        names.push(String::from_utf8_lossy(name_slice).into_owned());
        pos = name_end.checked_add(extra_len)?.checked_add(comment_len)?;
    }
    Some(names)
}

fn parse_apk(file: &mut File, file_len: u64, head: &[u8]) -> Option<BinaryInfo> {
    if !head.starts_with(ZIP_LOCAL_MAGIC) && !head.starts_with(ZIP_EMPTY_EOCD_MAGIC) {
        return None;
    }
    let names = read_zip_entry_names(file, file_len)?;
    if !names.iter().any(|name| name == "AndroidManifest.xml") {
        return None;
    }

    let mut abis = BTreeSet::new();
    let mut has_dex = false;
    for entry in &names {
        if entry.starts_with("classes") && entry.ends_with(".dex") {
            has_dex = true;
        } else if entry.starts_with("lib/") && entry.ends_with(".so") {
            let mut parts = entry.split('/');
            let _lib = parts.next();
            if let Some(abi) = parts.next() {
                if !abi.is_empty() {
                    abis.insert(abi.to_string());
                }
            }
        }
    }

    let priority = ["x86_64", "arm64-v8a", "armeabi-v7a", "armeabi", "x86"];
    let mut arch = "noarch".to_string();
    for preferred in priority {
        if abis.contains(preferred) {
            if let Some(mapped) = android_abi_to_arch(preferred) {
                arch = mapped.to_string();
                break;
            }
        }
    }

    Some(BinaryInfo {
        platform: AppPlatform::AndroidApk,
        arch,
        sub_abis: abis.into_iter().collect(),
        is_gui: true,
        is_library: false,
        has_dex,
        win_era: None,
    })
}

/// Inspect a binary file or package on disk and classify its target platform.
pub fn inspect_binary(file_path: impl AsRef<Path>) -> Result<BinaryInfo, String> {
    let path = file_path.as_ref();
    if !path.is_file() {
        return Err(format!("Binary not found: {}", path.display()));
    }

    let mut file = File::open(path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("Failed to stat {}: {e}", path.display()))?
        .len();

    let head_len = file_len.min(4096) as usize;
    let mut head = vec![0u8; head_len];
    file.read_exact(&mut head)
        .map_err(|e| format!("Failed to read header of {}: {e}", path.display()))?;

    if head.starts_with(b"MZ") || head.starts_with(b"ZM") {
        if head.starts_with(b"MZ") {
            if let Some(pe) = parse_pe(&head) {
                return Ok(pe);
            }
            // Handle unusually large DOS stubs beyond 4 KiB.
            if file_len <= 1024 * 1024 {
                file.seek(SeekFrom::Start(0))
                    .map_err(|e| format!("Seek failed: {e}"))?;
                let mut full = vec![0u8; file_len as usize];
                file.read_exact(&mut full)
                    .map_err(|e| format!("Read failed: {e}"))?;
                if let Some(pe) = parse_pe(&full) {
                    return Ok(pe);
                }
            }
        }
        if let Some(dos) = parse_dos_mz(&head) {
            return Ok(dos);
        }
    }

    if head.starts_with(ELF_MAGIC) {
        if let Some(elf) = parse_elf(&head) {
            return Ok(elf);
        }
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if head.starts_with(OLE_MAGIC) && ext == "msi" {
        return Ok(BinaryInfo {
            platform: AppPlatform::WindowsMsi,
            arch: "x86_64".to_string(),
            sub_abis: Vec::new(),
            is_gui: true,
            is_library: false,
            has_dex: false,
            win_era: Some(WinEra::Win10),
        });
    }

    if let Some(apk) = parse_apk(&mut file, file_len, &head) {
        return Ok(apk);
    }

    if (ext == "com" && file_len > 0 && file_len <= 65_280) || (ext == "bat" && file_len > 0) {
        return Ok(BinaryInfo {
            platform: AppPlatform::Dos16,
            arch: "x86_16".to_string(),
            sub_abis: Vec::new(),
            is_gui: true,
            is_library: false,
            has_dex: false,
            win_era: None,
        });
    }

    Ok(BinaryInfo {
        platform: AppPlatform::Unknown,
        arch: "unknown".to_string(),
        sub_abis: Vec::new(),
        is_gui: false,
        is_library: false,
        has_dex: false,
        win_era: None,
    })
}
