//! Hardware capability detection and adaptive runtime profiling for 2005-present PCs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::json_codec::{to_json_pretty, JsonValue};

/// Broad hardware generation tiers spanning 2005-era to modern PCs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareTier {
    /// 2005-2008 class: <= 1.5 GiB RAM or pre-SSE4.2 / non-3D legacy GPU.
    LegacyUltra,
    /// 2009-2014 class: 1.5 - 3.5 GiB RAM or OpenGL-only GPU without Vulkan.
    LegacyMid,
    /// 2015-present class: >= 4 GiB RAM with hardware Vulkan support.
    Modern,
}

impl HardwareTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            HardwareTier::LegacyUltra => "legacy_ultra",
            HardwareTier::LegacyMid => "legacy_mid",
            HardwareTier::Modern => "modern",
        }
    }
}

/// Raw hardware attributes inspected from the running system or test fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareSpecs {
    pub ram_mb: u64,
    pub cpu_cores: usize,
    pub cpu_model: String,
    pub has_sse42: bool,
    pub has_avx: bool,
    pub has_vulkan: bool,
    pub has_drm_3d: bool,
    pub firmware: String,
}

/// Tuned OS runtime parameters derived from `HardwareSpecs`.
#[derive(Debug, Clone, PartialEq)]
pub struct HardwareProfile {
    pub tier: HardwareTier,
    pub specs: HardwareSpecs,
    pub zram_mb: u64,
    pub swappiness: u32,
    pub idle_hibernate_sec: f64,
    pub wlr_renderer: &'static str,
    pub win32_backend_mode: &'static str,
    pub android_graphics: &'static str,
    pub lightweight_shell: bool,
}

/// Classify raw hardware specifications into an adaptive UniCore runtime profile.
pub fn classify_hardware(specs: &HardwareSpecs) -> HardwareProfile {
    let tier = if specs.ram_mb < 1536 {
        HardwareTier::LegacyUltra
    } else if specs.ram_mb < 3584 || !specs.has_vulkan || !specs.has_sse42 {
        HardwareTier::LegacyMid
    } else {
        HardwareTier::Modern
    };

    match tier {
        HardwareTier::LegacyUltra => HardwareProfile {
            tier,
            specs: specs.clone(),
            zram_mb: (specs.ram_mb * 3 / 4).max(256),
            swappiness: 100,
            idle_hibernate_sec: 5.0,
            wlr_renderer: "pixman",
            win32_backend_mode: if specs.has_drm_3d {
                "wined3d-opengl"
            } else {
                "gdi-legacy"
            },
            android_graphics: "swiftshader",
            lightweight_shell: true,
        },
        HardwareTier::LegacyMid => HardwareProfile {
            tier,
            specs: specs.clone(),
            zram_mb: (specs.ram_mb / 2).max(512),
            swappiness: 80,
            idle_hibernate_sec: 15.0,
            wlr_renderer: if specs.has_drm_3d && specs.has_sse42 {
                "gles2"
            } else {
                "pixman"
            },
            win32_backend_mode: if specs.has_drm_3d {
                "wined3d-opengl"
            } else {
                "gdi-legacy"
            },
            android_graphics: if specs.has_vulkan {
                "vulkan-dma-buf"
            } else {
                "swiftshader"
            },
            lightweight_shell: false,
        },
        HardwareTier::Modern => HardwareProfile {
            tier,
            specs: specs.clone(),
            zram_mb: (specs.ram_mb / 4).clamp(512, 4096),
            swappiness: 60,
            idle_hibernate_sec: 30.0,
            wlr_renderer: "auto",
            win32_backend_mode: "dxvk+vkd3d",
            android_graphics: "vulkan-dma-buf",
            lightweight_shell: false,
        },
    }
}

fn parse_meminfo_mb(raw: &str) -> Option<u64> {
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

fn parse_cpuinfo(raw: &str) -> (usize, String, bool, bool) {
    let mut cores = 0usize;
    let mut model = String::new();
    let mut has_sse42 = false;
    let mut has_avx = false;

    for line in raw.lines() {
        if line.starts_with("processor") {
            cores += 1;
        } else if model.is_empty() && line.starts_with("model name") {
            if let Some((_, val)) = line.split_once(':') {
                model = val.trim().to_string();
            }
        } else if line.starts_with("flags") {
            let flags: Vec<&str> = line.split_whitespace().collect();
            if flags.contains(&"sse4_2") {
                has_sse42 = true;
            }
            if flags.contains(&"avx") {
                has_avx = true;
            }
        }
    }
    if cores == 0 {
        cores = 1;
    }
    if model.is_empty() {
        model = "unknown".to_string();
    }
    (cores, model, has_sse42, has_avx)
}

/// Classify a single DRM GPU device into `(has_safe_gles2_3d, has_hardware_vulkan)`.
/// Prevents pre-GLES2/legacy GPUs (such as NVIDIA GeForce 6150SE / NV44 `0x10de:0x03d0`
/// on `nouveau` or pre-SSE4.2 Intel/ATI IGPs) from triggering broken Wayland GLES2 or
/// false-positive Vulkan profiles when Mesa ICD JSON files are pre-installed on disk.
pub fn classify_drm_gpu(
    vendor: &str,
    device: &str,
    driver: &str,
    has_sse42: bool,
    has_intel_icd: bool,
    has_amd_icd: bool,
) -> (bool, bool) {
    let vendor_lc = vendor.trim().to_ascii_lowercase();
    let driver_lc = driver.trim().to_ascii_lowercase();
    let dev_id = u16::from_str_radix(
        device
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X"),
        16,
    )
    .ok();

    match driver_lc.as_str() {
        "bochs-drm" | "cirrus" | "simpledrm" | "vesadrm" | "efidrm" | "qxl" | "mga" | "ast"
        | "udl" | "vkms" => return (false, false),
        _ => {}
    }

    match vendor_lc.as_str() {
        // NVIDIA (GeForce / nForce / Quadro)
        "0x10de" => {
            if driver_lc == "nvidia" {
                return (true, has_sse42);
            }
            // Legacy NV3x/NV4x/MCP61/C51/C61/C67/C68/C73/C77/C79 (e.g. GeForce 6150SE 0x03d0)
            // use Mesa nv30/legacy nouveau where wlroots gles2 frequently black-screens.
            let is_legacy_nv = !has_sse42
                || matches!(
                    dev_id,
                    Some(0x0000..=0x03ff) | Some(0x0530..=0x053f) | Some(0x07e0..=0x07ef)
                );
            if is_legacy_nv {
                (false, false)
            } else {
                (true, false)
            }
        }
        // Intel integrated/discrete GPUs
        "0x8086" => {
            if driver_lc == "i915" || driver_lc == "xe" {
                (has_sse42, has_sse42 && has_intel_icd)
            } else {
                (false, false)
            }
        }
        // AMD / ATI Radeon
        "0x1002" => {
            if driver_lc == "amdgpu" {
                (true, has_amd_icd)
            } else if driver_lc == "radeon" {
                (has_sse42, false)
            } else {
                (false, false)
            }
        }
        _ => (has_sse42, false),
    }
}

fn detect_drm_and_vulkan(has_sse42: bool) -> (bool, bool) {
    let mut has_render_node = false;
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("renderD") {
                has_render_node = true;
                break;
            }
        }
    }
    if !has_render_node {
        return (false, false);
    }

    let mut has_intel_icd = false;
    let mut has_amd_icd = false;
    if let Ok(entries) = fs::read_dir("/usr/share/vulkan/icd.d") {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if fname.ends_with(".json") && !fname.contains("lvp_icd") {
                if fname.contains("intel") {
                    has_intel_icd = true;
                }
                if fname.contains("radeon") || fname.contains("amd") {
                    has_amd_icd = true;
                }
            }
        }
    }

    let mut any_card = false;
    let mut best_3d = false;
    let mut best_vk = false;
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let dev_dir = entry.path().join("device");
            let vendor = fs::read_to_string(dev_dir.join("vendor")).unwrap_or_default();
            let device = fs::read_to_string(dev_dir.join("device")).unwrap_or_default();
            let driver = fs::read_link(dev_dir.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_default();
            if vendor.trim().is_empty() && driver.is_empty() {
                continue;
            }
            any_card = true;
            let (d3d, vk) = classify_drm_gpu(
                &vendor,
                &device,
                &driver,
                has_sse42,
                has_intel_icd,
                has_amd_icd,
            );
            best_3d |= d3d;
            best_vk |= vk;
        }
    }

    if any_card {
        (best_3d, best_vk)
    } else {
        (has_render_node && has_sse42, false)
    }
}

/// Detect the current host hardware capabilities.
pub fn detect_host_hardware() -> HardwareSpecs {
    if cfg!(target_os = "linux") {
        let ram_mb = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| parse_meminfo_mb(&s))
            .unwrap_or(2048);
        let (cpu_cores, cpu_model, has_sse42, has_avx) = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .map(|s| parse_cpuinfo(&s))
            .unwrap_or((1, "unknown".to_string(), false, false));
        let (has_drm_3d, has_vulkan) = detect_drm_and_vulkan(has_sse42);
        let firmware = if Path::new("/sys/firmware/efi").exists() {
            "uefi"
        } else {
            "bios"
        }
        .to_string();
        return HardwareSpecs {
            ram_mb,
            cpu_cores,
            cpu_model,
            has_sse42,
            has_avx,
            has_vulkan,
            has_drm_3d,
            firmware,
        };
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    HardwareSpecs {
        ram_mb: 4096,
        cpu_cores: cores,
        cpu_model: std::env::consts::ARCH.to_string(),
        has_sse42: true,
        has_avx: false,
        has_vulkan: false,
        has_drm_3d: true,
        firmware: "uefi".to_string(),
    }
}

impl HardwareProfile {
    /// Format profile as POSIX shell variable assignments for boot-time scripts.
    pub fn to_shell_env(&self) -> String {
        format!(
            "UNICORE_HW_TIER={}\n\
             UNICORE_RAM_MB={}\n\
             UNICORE_ZRAM_MB={}\n\
             UNICORE_SWAPPINESS={}\n\
             UNILOAD_IDLE_TIMEOUT={}\n\
             WLR_RENDERER={}\n\
             UNILOAD_HW_WIN32_MODE={}\n\
             UNILOAD_HW_ANDROID_GRAPHICS={}\n\
             UNICORE_LIGHTWEIGHT_SHELL={}\n\
             UNICORE_FIRMWARE={}\n",
            self.tier.as_str(),
            self.specs.ram_mb,
            self.zram_mb,
            self.swappiness,
            self.idle_hibernate_sec as u64,
            self.wlr_renderer,
            self.win32_backend_mode,
            self.android_graphics,
            if self.lightweight_shell { 1 } else { 0 },
            self.specs.firmware,
        )
    }

    pub fn to_json_string(&self) -> String {
        let mut specs_map = BTreeMap::new();
        specs_map.insert(
            "cpu_cores".to_string(),
            JsonValue::Number(self.specs.cpu_cores as f64),
        );
        specs_map.insert(
            "cpu_model".to_string(),
            JsonValue::Str(self.specs.cpu_model.clone()),
        );
        specs_map.insert(
            "firmware".to_string(),
            JsonValue::Str(self.specs.firmware.clone()),
        );
        specs_map.insert("has_avx".to_string(), JsonValue::Bool(self.specs.has_avx));
        specs_map.insert(
            "has_drm_3d".to_string(),
            JsonValue::Bool(self.specs.has_drm_3d),
        );
        specs_map.insert(
            "has_sse42".to_string(),
            JsonValue::Bool(self.specs.has_sse42),
        );
        specs_map.insert(
            "has_vulkan".to_string(),
            JsonValue::Bool(self.specs.has_vulkan),
        );
        specs_map.insert(
            "ram_mb".to_string(),
            JsonValue::Number(self.specs.ram_mb as f64),
        );

        let mut root = BTreeMap::new();
        root.insert(
            "android_graphics".to_string(),
            JsonValue::Str(self.android_graphics.to_string()),
        );
        root.insert(
            "idle_hibernate_sec".to_string(),
            JsonValue::Number(self.idle_hibernate_sec),
        );
        root.insert(
            "lightweight_shell".to_string(),
            JsonValue::Bool(self.lightweight_shell),
        );
        root.insert("specs".to_string(), JsonValue::Object(specs_map));
        root.insert(
            "swappiness".to_string(),
            JsonValue::Number(self.swappiness as f64),
        );
        root.insert(
            "tier".to_string(),
            JsonValue::Str(self.tier.as_str().to_string()),
        );
        root.insert(
            "win32_backend_mode".to_string(),
            JsonValue::Str(self.win32_backend_mode.to_string()),
        );
        root.insert(
            "wlr_renderer".to_string(),
            JsonValue::Str(self.wlr_renderer.to_string()),
        );
        root.insert("zram_mb".to_string(), JsonValue::Number(self.zram_mb as f64));
        to_json_pretty(&JsonValue::Object(root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_2005_2010_and_modern_hardware_accurately() {
        let pc_2005 = HardwareSpecs {
            ram_mb: 512,
            cpu_cores: 1,
            cpu_model: "Intel Pentium 4 3.00GHz".to_string(),
            has_sse42: false,
            has_avx: false,
            has_vulkan: false,
            has_drm_3d: false,
            firmware: "bios".to_string(),
        };
        let prof_2005 = classify_hardware(&pc_2005);
        assert_eq!(prof_2005.tier, HardwareTier::LegacyUltra);
        assert_eq!(prof_2005.wlr_renderer, "pixman");
        assert_eq!(prof_2005.win32_backend_mode, "gdi-legacy");
        assert_eq!(prof_2005.zram_mb, 384);
        assert!(prof_2005.lightweight_shell);

        let laptop_2010 = HardwareSpecs {
            ram_mb: 2048,
            cpu_cores: 2,
            cpu_model: "Intel Core i3 M 350".to_string(),
            has_sse42: true,
            has_avx: false,
            has_vulkan: false,
            has_drm_3d: true,
            firmware: "bios".to_string(),
        };
        let prof_2010 = classify_hardware(&laptop_2010);
        assert_eq!(prof_2010.tier, HardwareTier::LegacyMid);
        assert_eq!(prof_2010.wlr_renderer, "gles2");
        assert_eq!(prof_2010.win32_backend_mode, "wined3d-opengl");
        assert_eq!(prof_2010.zram_mb, 1024);
        assert!(!prof_2010.lightweight_shell);

        let pc_modern = HardwareSpecs {
            ram_mb: 16384,
            cpu_cores: 8,
            cpu_model: "AMD Ryzen 7".to_string(),
            has_sse42: true,
            has_avx: true,
            has_vulkan: true,
            has_drm_3d: true,
            firmware: "uefi".to_string(),
        };
        let prof_modern = classify_hardware(&pc_modern);
        assert_eq!(prof_modern.tier, HardwareTier::Modern);
        assert_eq!(prof_modern.wlr_renderer, "auto");
        assert_eq!(prof_modern.win32_backend_mode, "dxvk+vkd3d");
        assert_eq!(prof_modern.android_graphics, "vulkan-dma-buf");
    }

    #[test]
    fn classifies_hp_dx2355_geforce_6150se_2gb_safely() {
        // HP Compaq dx2355: 2 GB DDR2 (1856 MB usable after UMA VRAM), AMD Athlon 64 X2
        // (no SSE4.2), NVIDIA GeForce 6150SE (0x10de:0x03d0, nouveau), even with Intel/AMD ICDs installed.
        let (drm_3d, vk) = classify_drm_gpu("0x10de", "0x03d0", "nouveau", false, true, true);
        assert!(!drm_3d);
        assert!(!vk);

        let dx2355 = HardwareSpecs {
            ram_mb: 1856,
            cpu_cores: 2,
            cpu_model: "AMD Athlon(tm) 64 X2 Dual Core Processor 5200+".to_string(),
            has_sse42: false,
            has_avx: false,
            has_vulkan: vk,
            has_drm_3d: drm_3d,
            firmware: "bios".to_string(),
        };
        let prof = classify_hardware(&dx2355);
        assert_eq!(prof.tier, HardwareTier::LegacyMid);
        assert!(!prof.lightweight_shell);
        assert_eq!(prof.wlr_renderer, "pixman");
        assert_eq!(prof.win32_backend_mode, "gdi-legacy");
        assert_eq!(prof.zram_mb, 928);

        // Modern AMD amdgpu + Intel i915 still get Vulkan when ICD is present.
        assert_eq!(
            classify_drm_gpu("0x1002", "0x73df", "amdgpu", true, true, true),
            (true, true)
        );
        assert_eq!(
            classify_drm_gpu("0x8086", "0x9a49", "i915", true, true, true),
            (true, true)
        );
    }
}
