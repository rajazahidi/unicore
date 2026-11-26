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
    let tier = if specs.ram_mb < 1536 || (!specs.has_sse42 && specs.ram_mb < 2048) {
        HardwareTier::LegacyUltra
    } else if specs.ram_mb < 3584 || !specs.has_vulkan {
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
            wlr_renderer: if specs.has_drm_3d { "gles2" } else { "pixman" },
            win32_backend_mode: "wined3d-opengl",
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

fn detect_drm_and_vulkan() -> (bool, bool) {
    let mut has_drm_3d = false;
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("renderD") {
                has_drm_3d = true;
                break;
            }
        }
    }

    let mut has_vulkan = false;
    if has_drm_3d {
        if let Ok(entries) = fs::read_dir("/usr/share/vulkan/icd.d") {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.ends_with(".json") && !fname.contains("lvp_icd") {
                    has_vulkan = true;
                    break;
                }
            }
        }
    }
    (has_drm_3d, has_vulkan)
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
        let (has_drm_3d, has_vulkan) = detect_drm_and_vulkan();
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
}
