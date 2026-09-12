use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{self, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const LIST_LIMIT: usize = 100;

pub struct MediaService {
    root: PathBuf,
}

impl MediaService {
    pub fn new() -> io::Result<Self> {
        let root = pictures_directory().join("OpenNOW");
        fs::create_dir_all(root.join("Screenshots"))?;
        fs::create_dir_all(root.join("Recordings"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> Value {
        json!({ "path": self.root.to_string_lossy() })
    }

    pub fn recording_target(&self, params: &Value) -> Result<Value, String> {
        let title = params["gameTitle"]
            .as_str()
            .map(sanitized_title)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "opennow".to_owned());
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let directory = self.directory_for_kind("recording")?;
        for suffix in 0_u16..=999 {
            let suffix = if suffix == 0 {
                String::new()
            } else {
                format!("-{suffix}")
            };
            let stem = format!("OpenNOW-{title}-{timestamp}{suffix}");
            let path = directory.join(format!("{stem}.mkv"));
            let part = directory.join(format!(".{stem}.mkv.part"));
            if !path.exists() && !part.exists() {
                return Ok(json!({
                    "path":path.to_string_lossy(),
                    "thumbnailPath":directory.join(format!("{stem}-thumb.jpg")).to_string_lossy(),
                }));
            }
        }
        Err("Could not allocate a unique recording file name".to_owned())
    }

    pub fn validate_recording_target(&self, params: &Value) -> Result<Value, String> {
        let raw = params["outputPath"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Native recording requires an outputPath".to_owned())?;
        let path = PathBuf::from(raw);
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| value.starts_with("OpenNOW-") && !value.contains(".."))
            .ok_or_else(|| "Recording output file name is invalid".to_owned())?;
        if path.extension().and_then(|value| value.to_str()) != Some("mkv") || path.exists() {
            return Err("Recording output must be a new .mkv file".to_owned());
        }
        let recordings = self.directory_for_kind("recording")?;
        let expected_parent = recordings
            .canonicalize()
            .map_err(|error| format!("Could not validate recordings directory: {error}"))?;
        let actual_parent = path
            .parent()
            .ok_or_else(|| "Recording output has no parent directory".to_owned())?
            .canonicalize()
            .map_err(|error| format!("Could not validate recording output directory: {error}"))?;
        if actual_parent != expected_parent {
            return Err("Recording output is outside the OpenNOW recordings directory".to_owned());
        }
        let part = recordings.join(format!(".{file_name}.part"));
        if part.exists() {
            return Err("Recording output is already being written".to_owned());
        }
        Ok(json!({"outputPath":path.to_string_lossy()}))
    }

    pub fn list(&self, params: &Value) -> Result<Value, String> {
        let filter = params["gameTitle"]
            .as_str()
            .map(sanitized_title)
            .filter(|value| !value.is_empty());
        let mut entries = Vec::new();
        self.collect("screenshot", "Screenshots", &mut entries)?;
        self.collect("recording", "Recordings", &mut entries)?;
        if let Some(filter) = filter {
            entries.retain(|entry| {
                entry["fileName"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase().contains(&filter))
            });
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(entry["createdAtMs"].as_u64().unwrap_or(0)));
        entries.truncate(LIST_LIMIT);
        let screenshot_count = entries
            .iter()
            .filter(|entry| entry["kind"] == "screenshot")
            .count();
        let recording_count = entries.len() - screenshot_count;
        Ok(json!({
            "items": entries,
            "screenshots": screenshot_count,
            "recordings": recording_count,
            "rootPath": self.root.to_string_lossy()
        }))
    }

    pub fn acceptance_evidence(&self, since_ms: Option<u128>) -> Result<Value, String> {
        let listed = self.list(&json!({}))?;
        let items = listed["items"]
            .as_array()
            .ok_or_else(|| "Media listing did not contain items".to_owned())?;
        let after_start = |item: &&Value| {
            since_ms.is_none_or(|minimum| {
                u128::from(item["createdAtMs"].as_u64().unwrap_or_default()) >= minimum
            })
        };
        let screenshot = items
            .iter()
            .filter(|item| item["kind"] == "screenshot")
            .find(after_start)
            .map(|item| self.evidence_for_item(item))
            .transpose()?;
        let recording = items
            .iter()
            .filter(|item| item["kind"] == "recording")
            .find(after_start)
            .map(|item| self.evidence_for_item(item))
            .transpose()?;
        let recording_has_thumbnail = recording
            .as_ref()
            .is_some_and(|item| item["thumbnail"].is_object());
        Ok(json!({
            "schemaVersion": 1,
            "sinceMs": since_ms.map(|value| value.to_string()),
            "screenshot": screenshot,
            "recording": recording,
            "complete": screenshot.is_some() && recording.is_some() && recording_has_thumbnail
        }))
    }

    pub fn delete(&self, params: &Value) -> Result<Value, String> {
        if params["confirmed"].as_bool() != Some(true) {
            return Err("Media deletion requires explicit confirmation".to_owned());
        }
        let kind = params["kind"]
            .as_str()
            .ok_or_else(|| "media.delete requires kind".to_owned())?;
        let id = safe_id(&params["id"])?;
        let directory = self.directory_for_kind(kind)?;
        let path = directory.join(id);
        if !path.is_file() {
            return Err("Media item was not found".to_owned());
        }
        fs::remove_file(&path).map_err(|error| format!("Could not delete media: {error}"))?;
        if kind == "recording" {
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let thumbnail = directory.join(format!("{stem}-thumb.jpg"));
            let _ = fs::remove_file(thumbnail);
        }
        Ok(json!({ "deleted": true, "id": id, "kind": kind }))
    }

    fn collect(&self, kind: &str, folder: &str, output: &mut Vec<Value>) -> Result<(), String> {
        let directory = self.root.join(folder);
        let items = fs::read_dir(&directory)
            .map_err(|error| format!("Could not read media directory: {error}"))?;
        for item in items.flatten() {
            let path = item.path();
            if !path.is_file() || !allowed_extension(kind, &path) || is_thumbnail(&path) {
                continue;
            }
            let Ok(metadata) = item.metadata() else {
                continue;
            };
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let created = metadata
                .created()
                .or_else(|_| metadata.modified())
                .unwrap_or(UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let media_url = Url::from_file_path(&path)
                .ok()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let thumbnail_url = if kind == "recording" {
                let stem = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                let thumbnail = directory.join(format!("{stem}-thumb.jpg"));
                thumbnail
                    .is_file()
                    .then(|| {
                        Url::from_file_path(thumbnail)
                            .ok()
                            .map(|value| value.to_string())
                    })
                    .flatten()
                    .unwrap_or_default()
            } else {
                media_url.clone()
            };
            output.push(json!({
                "id": file_name,
                "kind": kind,
                "fileName": file_name,
                "filePath": path.to_string_lossy(),
                "mediaUrl": media_url,
                "thumbnailUrl": thumbnail_url,
                "createdAtMs": created,
                "sizeBytes": metadata.len()
            }));
        }
        Ok(())
    }

    fn evidence_for_item(&self, item: &Value) -> Result<Value, String> {
        let path = PathBuf::from(
            item["filePath"]
                .as_str()
                .ok_or_else(|| "Media evidence path is unavailable".to_owned())?,
        );
        let (size, digest) = digest_regular_file(&path)?;
        let thumbnail = if item["kind"] == "recording" {
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let path = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{stem}-thumb.jpg"));
            if path.exists() {
                let (thumbnail_size, thumbnail_digest) = digest_regular_file(&path)?;
                Some(json!({
                    "fileName": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
                    "sizeBytes": thumbnail_size,
                    "sha256": thumbnail_digest
                }))
            } else {
                None
            }
        } else {
            None
        };
        Ok(json!({
            "kind": item["kind"],
            "fileName": item["fileName"],
            "createdAtMs": item["createdAtMs"],
            "sizeBytes": size,
            "sha256": digest,
            "thumbnail": thumbnail
        }))
    }

    fn directory_for_kind(&self, kind: &str) -> Result<PathBuf, String> {
        match kind {
            "screenshot" => Ok(self.root.join("Screenshots")),
            "recording" => Ok(self.root.join("Recordings")),
            _ => Err("Unsupported media kind".to_owned()),
        }
    }
}

fn digest_regular_file(path: &Path) -> Result<(u64, String), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect media evidence: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("Media evidence must be a regular, non-symlink file".to_owned());
    }
    let file =
        fs::File::open(path).map_err(|error| format!("Could not open media evidence: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash media evidence: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok((metadata.len(), format!("{:x}", hasher.finalize())))
}

fn pictures_directory() -> PathBuf {
    if let Some(path) = env::var_os("OPENNOW_PICTURES_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = env::var_os("USERPROFILE") {
        return PathBuf::from(path).join("Pictures");
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Pictures")
}

fn safe_id(value: &Value) -> Result<&str, String> {
    let id = value
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "media.delete requires an id".to_owned())?;
    let path = Path::new(id);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || id.contains("..")
    {
        return Err("Invalid media id".to_owned());
    }
    Ok(id)
}

fn allowed_extension(kind: &str, path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match kind {
        "screenshot" => matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp"),
        "recording" => matches!(extension.as_str(), "mp4" | "webm" | "mkv" | "mov" | "avi"),
        _ => false,
    }
}

fn is_thumbnail(path: &Path) -> bool {
    path.file_stem()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.ends_with("-thumb"))
}

fn sanitized_title(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn media_listing_is_bounded_and_deletion_is_scoped() {
        let directory = env::temp_dir().join(format!(
            "opennow-media-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        unsafe { env::set_var("OPENNOW_PICTURES_DIR", &directory) };
        let service = MediaService::new().unwrap();
        let target = service
            .recording_target(&json!({"gameTitle":"Test / Game"}))
            .unwrap();
        let validated = service
            .validate_recording_target(&json!({"outputPath":target["path"]}))
            .unwrap();
        assert_eq!(validated["outputPath"], target["path"]);
        assert!(
            service
                .validate_recording_target(&json!({"outputPath":directory.join("escape.mkv")}))
                .is_err()
        );
        fs::write(service.root.join("Screenshots/shot.png"), b"png").unwrap();
        fs::write(service.root.join("Recordings/OpenNOW-test.mkv"), b"mkv").unwrap();
        fs::write(
            service.root.join("Recordings/OpenNOW-test-thumb.jpg"),
            b"jpg",
        )
        .unwrap();
        fs::write(service.root.join("Screenshots/nope.txt"), b"no").unwrap();
        let listed = service.list(&json!({})).unwrap();
        assert_eq!(listed["items"].as_array().unwrap().len(), 2);
        let evidence = service.acceptance_evidence(Some(0)).unwrap();
        assert_eq!(evidence["complete"], true);
        assert_eq!(evidence["screenshot"]["sizeBytes"], 3);
        assert_eq!(
            evidence["screenshot"]["sha256"],
            "8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c"
        );
        assert_eq!(evidence["recording"]["thumbnail"]["sizeBytes"], 3);
        let rendered = evidence.to_string();
        assert!(!rendered.contains(directory.to_string_lossy().as_ref()));
        assert!(
            service
                .delete(&json!({"kind":"screenshot","id":"../shot.png","confirmed":true}))
                .is_err()
        );
        assert!(
            service
                .delete(&json!({"kind":"screenshot","id":"shot.png"}))
                .is_err()
        );
        service
            .delete(&json!({"kind":"screenshot","id":"shot.png","confirmed":true}))
            .unwrap();
        assert!(!service.root.join("Screenshots/shot.png").exists());
        let _ = fs::remove_dir_all(directory);
        unsafe { env::remove_var("OPENNOW_PICTURES_DIR") };
    }
}
