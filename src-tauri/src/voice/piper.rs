//! Piper, a local neural voice. Helpy downloads the Piper engine and the
//! voices the user picks, then runs the engine once per sentence.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use super::{download, dsp};

const RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";
const VOICES: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main";

/// The release archive for this computer, if Piper ships one.
fn asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("piper_linux_x86_64.tar.gz"),
        ("linux", "aarch64") => Some("piper_linux_aarch64.tar.gz"),
        // The macOS archives of this release are broken: the arm64 one holds
        // x86_64 binaries, and both leave out the dylibs piper links to.
        ("windows", "x86_64") => Some("piper_windows_amd64.zip"),
        _ => None,
    }
}

pub fn available() -> bool {
    asset().is_some()
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("piper"))
}

fn binary(app: &AppHandle) -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { "piper.exe" } else { "piper" };
    Ok(root(app)?.join("piper").join(exe))
}

fn voice_file(app: &AppHandle, key: &str) -> Result<PathBuf, String> {
    Ok(root(app)?.join("voices").join(format!("{key}.onnx")))
}

pub fn installed(app: &AppHandle, key: &str) -> bool {
    binary(app).is_ok_and(|b| b.exists()) && voice_file(app, key).is_ok_and(|v| v.exists())
}

async fn install_engine(app: &AppHandle, http: &reqwest::Client) -> Result<(), String> {
    if binary(app)?.exists() {
        return Ok(());
    }
    let asset = asset().ok_or("Piper isn't available for this computer")?;
    let dir = root(app)?;
    let archive = dir.join(asset);
    download::download(
        app,
        http,
        &format!("{RELEASE}/{asset}"),
        &archive,
        "piper:engine",
    )
    .await?;
    let dir2 = dir.clone();
    let archive2 = archive.clone();
    tokio::task::spawn_blocking(move || extract(&archive2, &dir2))
        .await
        .map_err(|e| e.to_string())??;
    let _ = std::fs::remove_file(&archive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let b = binary(app)?;
        std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract(archive: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    if archive.extension().is_some_and(|e| e == "zip") {
        zip::ZipArchive::new(file)
            .and_then(|mut z| z.extract(dest))
            .map_err(|e| format!("Couldn't unpack Piper: {e}"))
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(file))
            .unpack(dest)
            .map_err(|e| format!("Couldn't unpack Piper: {e}"))
    }
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PiperVoice {
    pub key: String,
    pub name: String,
    /// e.g. "English (United States)".
    pub language: String,
    pub quality: String,
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    pub installed: bool,
}

/// The published voice catalogue (voices.json), fetched once per run.
pub async fn catalogue(http: &reqwest::Client) -> Result<Value, String> {
    let r = http
        .get(format!("{VOICES}/voices.json"))
        .send()
        .await
        .map_err(|e| format!("Couldn't load the Piper voice list: {e}"))?;
    if !r.status().is_success() {
        return Err(format!(
            "Couldn't load the Piper voice list ({})",
            r.status()
        ));
    }
    r.json()
        .await
        .map_err(|e| format!("The Piper voice list was unreadable: {e}"))
}

pub fn list(app: &AppHandle, catalogue: &Value) -> Vec<PiperVoice> {
    let mut out: Vec<PiperVoice> = catalogue
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, v)| {
            let lang = &v["language"];
            let size = v["files"]
                .as_object()
                .and_then(|f| f.iter().find(|(p, _)| p.ends_with(".onnx")))
                .and_then(|(_, meta)| meta["size_bytes"].as_u64());
            PiperVoice {
                key: key.clone(),
                name: v["name"].as_str().unwrap_or(key).to_string(),
                language: format!(
                    "{} ({})",
                    lang["name_english"].as_str().unwrap_or("?"),
                    lang["country_english"].as_str().unwrap_or("?")
                ),
                quality: v["quality"].as_str().unwrap_or("").to_string(),
                size,
                installed: voice_file(app, key).is_ok_and(|p| p.exists()),
            }
        })
        .collect();
    out.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));
    out
}

pub async fn install_voice(
    app: &AppHandle,
    http: &reqwest::Client,
    catalogue: &Value,
    key: &str,
) -> Result<(), String> {
    install_engine(app, http).await?;
    let files = catalogue[key]["files"]
        .as_object()
        .ok_or_else(|| format!("Unknown Piper voice {key}"))?;
    let dest = voice_file(app, key)?;
    for (path, _) in files {
        let target = if path.ends_with(".onnx.json") {
            dest.with_extension("onnx.json")
        } else if path.ends_with(".onnx") {
            dest.clone()
        } else {
            continue;
        };
        download::download(
            app,
            http,
            &format!("{VOICES}/{path}"),
            &target,
            &format!("piper:{key}"),
        )
        .await?;
    }
    Ok(())
}

pub fn remove_voice(app: &AppHandle, key: &str) -> Result<(), String> {
    let v = voice_file(app, key)?;
    let _ = std::fs::remove_file(v.with_extension("onnx.json"));
    std::fs::remove_file(v).map_err(|e| e.to_string())
}

/// Speaks `text` into 16-bit samples. Returns samples and their rate.
pub fn synthesize(
    app: &AppHandle,
    key: &str,
    text: &str,
    speed: f64,
) -> Result<(Vec<f32>, u32), String> {
    if !installed(app, key) {
        return Err(format!(
            "The Piper voice {key} isn't downloaded. Download it in Settings → Voice output"
        ));
    }
    let model = voice_file(app, key)?;
    let config: Value = std::fs::read_to_string(model.with_extension("onnx.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let rate = config["audio"]["sample_rate"].as_u64().unwrap_or(22_050) as u32;
    let mut child = Command::new(binary(app)?)
        .arg("--model")
        .arg(&model)
        .arg("--output-raw")
        .arg("--length_scale")
        .arg(format!("{:.3}", 1.0 / speed.clamp(0.5, 2.0)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn't start Piper: {e}"))?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() || raw.is_empty() {
        return Err("Piper couldn't speak that".into());
    }
    Ok((dsp::pcm16_to_f32(&raw), rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_desktop_target_has_an_archive() {
        if cfg!(any(target_os = "linux", target_os = "windows"))
            && matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
        {
            assert!(available());
        }
        if cfg!(target_os = "macos") {
            assert!(!available());
        }
    }

    #[test]
    fn unpacks_tar_gz_archives() {
        let dir = std::env::temp_dir().join(format!("helpy-piper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("a.tar.gz");
        {
            let enc = flate2::write::GzEncoder::new(
                std::fs::File::create(&archive).unwrap(),
                flate2::Compression::fast(),
            );
            let mut t = tar::Builder::new(enc);
            let mut h = tar::Header::new_gnu();
            h.set_size(5);
            h.set_mode(0o755);
            h.set_cksum();
            t.append_data(&mut h, "piper/piper", &b"hello"[..]).unwrap();
            t.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.join("out");
        extract(&archive, &out).unwrap();
        assert_eq!(std::fs::read(out.join("piper/piper")).unwrap(), b"hello");
        let _ = std::fs::remove_dir_all(dir);
    }
}
