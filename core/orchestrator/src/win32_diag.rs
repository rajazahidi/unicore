//! Automatic Win32 / Wine crash log analyzer & 1-click runtime repair advisor.

use std::collections::BTreeMap;

use crate::json_codec::{to_json_pretty, JsonValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Win32Diagnosis {
    pub issue_detected: bool,
    pub category: String,
    pub summary_en: String,
    pub summary_ms: String,
    pub recommended_verbs: Vec<String>,
    pub suggest_win98_ddraw: bool,
    pub matched_patterns: Vec<String>,
}

impl Win32Diagnosis {
    pub fn to_json(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("category".to_string(), JsonValue::Str(self.category.clone()));
        map.insert("issue_detected".to_string(), JsonValue::Bool(self.issue_detected));
        map.insert(
            "matched_patterns".to_string(),
            JsonValue::Array(
                self.matched_patterns
                    .iter()
                    .map(|s| JsonValue::Str(s.clone()))
                    .collect(),
            ),
        );
        map.insert(
            "recommended_verbs".to_string(),
            JsonValue::Array(
                self.recommended_verbs
                    .iter()
                    .map(|s| JsonValue::Str(s.clone()))
                    .collect(),
            ),
        );
        map.insert(
            "suggest_win98_ddraw".to_string(),
            JsonValue::Bool(self.suggest_win98_ddraw),
        );
        map.insert("summary_en".to_string(), JsonValue::Str(self.summary_en.clone()));
        map.insert("summary_ms".to_string(), JsonValue::Str(self.summary_ms.clone()));
        JsonValue::Object(map)
    }

    pub fn to_json_pretty(&self) -> String {
        to_json_pretty(&self.to_json())
    }
}

fn push_unique(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|existing| existing == item) {
        list.push(item.to_string());
    }
}

/// Analyze a Wine/Win32 launch log and suggest exact 1-click `winetricks` runtime fixes.
pub fn diagnose_win32_log(log_text: &str) -> Win32Diagnosis {
    let lower = log_text.to_ascii_lowercase();
    let mut verbs: Vec<String> = Vec::new();
    let mut patterns: Vec<String> = Vec::new();
    let mut en_notes: Vec<&str> = Vec::new();
    let mut ms_notes: Vec<&str> = Vec::new();
    let mut suggest_win98_ddraw = false;

    for needle in [
        "vcruntime140",
        "msvcp140",
        "concrt140",
        "api-ms-win-crt-",
        "ucrtbase.dll",
        "vcomp140",
    ] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "vcrun2022");
        }
    }
    if verbs.iter().any(|v| v == "vcrun2022") {
        en_notes.push("Missing Visual C++ 2015-2022 runtime (vcrun2022)");
        ms_notes.push("Runtime Visual C++ 2015-2022 hilang (vcrun2022)");
    }

    for needle in [
        "d3dx9_",
        "d3dcompiler_43",
        "d3dcompiler_47",
        "xinput1_3",
        "xaudio2_7",
        "x3daudio1_7",
    ] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "d3dx9");
            push_unique(&mut verbs, "d3dcompiler_47");
        }
    }
    if verbs.iter().any(|v| v == "d3dx9") {
        en_notes.push("Missing DirectX 9 / Shader Compiler DLLs (d3dx9, d3dcompiler_47)");
        ms_notes.push("DLL DirectX 9 / Pengkompil Shader hilang (d3dx9, d3dcompiler_47)");
    }

    for needle in ["mfc42", "msvcp60", "msvcrt40", "vb6"] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "vcrun6");
        }
    }
    if verbs.iter().any(|v| v == "vcrun6") {
        en_notes.push("Missing classic VC++ 6.0 / MFC42 runtime (vcrun6)");
        ms_notes.push("Runtime klasik VC++ 6.0 / MFC42 hilang (vcrun6)");
    }

    for needle in [
        "mscoree.dll",
        "install the .net",
        "clr error",
        "net framework",
        "dotnet",
    ] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "dotnet48");
        }
    }
    if verbs.iter().any(|v| v == "dotnet48") {
        en_notes.push("Requires .NET Framework runtime (dotnet48)");
        ms_notes.push("Memerlukan runtime .NET Framework (dotnet48)");
    }

    for needle in [
        "no fonts found",
        "could not load font",
        "tahoma",
        "arial",
        "corefonts",
    ] {
        if lower.contains(needle) && (lower.contains("font") || lower.contains("err:") || lower.contains("failed")) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "corefonts");
        }
    }
    if verbs.iter().any(|v| v == "corefonts") {
        en_notes.push("Missing Windows standard fonts (corefonts)");
        ms_notes.push("Fon standard Windows hilang (corefonts)");
    }

    for needle in ["dderr_", "directdraw", "ddraw.dll", "idirectdraw"] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            suggest_win98_ddraw = true;
        }
    }
    if suggest_win98_ddraw {
        en_notes.push("Classic DirectDraw error: switch to Classic 2D (Win98/XP cnc-ddraw)");
        ms_notes.push("Ralat DirectDraw klasik: tukar ke Mod 2D Klasik (Win98/XP cnc-ddraw)");
    }

    for needle in ["dplayx", "dplay.dll", "dpnet", "dpnhpast", "directplay"] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "directplay");
        }
    }
    if verbs.iter().any(|v| v == "directplay") {
        en_notes.push("Missing classic DirectPlay LAN/multiplayer runtime (directplay)");
        ms_notes.push("Runtime rangkaian klasik DirectPlay hilang (directplay)");
    }

    for needle in ["dsound.dll", "dmusic", "dmime", "dmloader", "idirectsound"] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "dsound");
        }
    }
    if verbs.iter().any(|v| v == "dsound") {
        en_notes.push("Missing classic DirectSound / DirectMusic audio DLLs (dsound)");
        ms_notes.push("DLL bunyi klasik DirectSound / DirectMusic hilang (dsound)");
    }

    for needle in ["quartz.dll", "amstream.dll", "indeo", "iv50", "devenum.dll"] {
        if lower.contains(needle) {
            push_unique(&mut patterns, needle);
            push_unique(&mut verbs, "quartz");
        }
    }
    if verbs.iter().any(|v| v == "quartz") {
        en_notes.push("Missing classic cutscene video codec (quartz / DirectShow)");
        ms_notes.push("Codec video intro klasik hilang (quartz / DirectShow)");
    }

    if lower.contains("glide2x") || lower.contains("glide3x") {
        push_unique(&mut patterns, "glide");
        en_notes.push("3dfx Glide renderer requested: select Direct3D/OpenGL in game setup or Classic 3D mode");
        ms_notes.push("Game perlukan 3dfx Glide: pilih Direct3D/OpenGL dalam tetapan game atau Mod 3D Klasik");
    }

    if patterns.is_empty() {
        return Win32Diagnosis {
            issue_detected: false,
            category: "healthy_or_unknown".to_string(),
            summary_en: "No known missing Win32 DLL or runtime crash found in this log.".to_string(),
            summary_ms: "Tiada ralat kehilangan DLL atau runtime Win32 yang dikenali dalam log ini."
                .to_string(),
            recommended_verbs: Vec::new(),
            suggest_win98_ddraw: false,
            matched_patterns: Vec::new(),
        };
    }

    let category = if !verbs.is_empty() {
        "missing_runtime".to_string()
    } else {
        "directdraw_era".to_string()
    };

    Win32Diagnosis {
        issue_detected: true,
        category,
        summary_en: en_notes.join("; "),
        summary_ms: ms_notes.join("; "),
        recommended_verbs: verbs,
        suggest_win98_ddraw,
        matched_patterns: patterns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_missing_vcrun_and_dx9_dlls() {
        let log = "0024:err:module:import_dll Library vcruntime140.dll not found\n0024:err:module:import_dll Library d3dx9_43.dll not found\n";
        let diag = diagnose_win32_log(log);
        assert!(diag.issue_detected);
        assert_eq!(diag.category, "missing_runtime");
        assert!(diag.recommended_verbs.contains(&"vcrun2022".to_string()));
        assert!(diag.recommended_verbs.contains(&"d3dx9".to_string()));
        assert!(diag.recommended_verbs.contains(&"d3dcompiler_47".to_string()));
        assert!(!diag.summary_ms.is_empty());
    }

    #[test]
    fn detects_directdraw_and_clean_logs() {
        let dd = diagnose_win32_log("err:ddraw:DirectDrawCreate failed with DDERR_UNSUPPORTED");
        assert!(dd.issue_detected);
        assert!(dd.suggest_win98_ddraw);

        let ok = diagnose_win32_log("=== Launching app ===\n=== Exited with code 0 ===\n");
        assert!(!ok.issue_detected);
        assert!(ok.recommended_verbs.is_empty());
    }
}
