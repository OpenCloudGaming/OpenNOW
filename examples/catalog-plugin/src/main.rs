use opennow_plugin_api::wire::{
    HelloReply, HostMessage, PluginFailure, PluginFailureCode, PluginMessage, ReplyPayload,
    RequestPayload, ShutdownReply,
};
use opennow_plugin_api::{
    CATALOG_CAPABILITY, CatalogItem, CatalogPage, Coverage, EXAMPLE_PLUGIN_ID, MAX_FRAME_BYTES,
    PluginId,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 3 && args[1] == "--package" {
        return package(Path::new(&args[2]));
    }
    if args.len() != 1 {
        return Err("Expected no arguments or --package <output.opennow-plugin>".into());
    }
    let data =
        std::env::var_os("OPENNOW_PLUGIN_DATA_DIR").ok_or("Own data directory is required")?;
    fs::write(
        Path::new(&data).join("last-process-id"),
        std::process::id().to_string(),
    )?;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut output = io::stdout().lock();
    let mut session_epoch = None;
    loop {
        let mut frame = Vec::new();
        let count = reader
            .by_ref()
            .take((MAX_FRAME_BYTES + 1) as u64)
            .read_until(b'\n', &mut frame)?;
        if count == 0 {
            return Ok(());
        }
        if count > MAX_FRAME_BYTES || frame.last() != Some(&b'\n') {
            return Err("Invalid protocol frame".into());
        }
        let message: HostMessage = serde_json::from_slice(&frame)?;
        let (epoch, id, outcome, stop) = match message {
            HostMessage::Cancel { .. } => continue,
            HostMessage::Request(request) => {
                let mut stop = false;
                let reply = match request.payload {
                    RequestPayload::Hello(hello)
                        if session_epoch.is_none()
                            && hello.plugin_id.as_str() == EXAMPLE_PLUGIN_ID =>
                    {
                        session_epoch = Some(request.epoch);
                        Ok(ReplyPayload::Hello(HelloReply {
                            plugin_id: PluginId::new(EXAMPLE_PLUGIN_ID)?,
                            version: env!("CARGO_PKG_VERSION").into(),
                            protocol_version: 1,
                            capabilities: vec![CATALOG_CAPABILITY.into()],
                        }))
                    }
                    RequestPayload::CatalogPage(query) if session_epoch == Some(request.epoch) => {
                        let start = query.cursor.as_deref().unwrap_or("0").parse::<usize>();
                        match start {
                            Ok(start) if start <= 24 => {
                                let matches: Vec<_> = (1..=24)
                                    .map(|number| CatalogItem {
                                        id: format!("example-{number}"),
                                        title: format!("Example game {number:02}"),
                                    })
                                    .filter(|item| {
                                        item.title
                                            .to_lowercase()
                                            .contains(&query.query.to_lowercase())
                                    })
                                    .collect();
                                let items: Vec<_> = matches
                                    .iter()
                                    .skip(start)
                                    .take(query.limit as usize)
                                    .cloned()
                                    .collect();
                                let end = start + items.len();
                                Ok(ReplyPayload::CatalogPage(CatalogPage {
                                    items,
                                    next_cursor: (end < matches.len()).then(|| end.to_string()),
                                    coverage: Coverage::Complete,
                                }))
                            }
                            _ => Err(PluginFailure {
                                code: PluginFailureCode::InvalidRequest,
                            }),
                        }
                    }
                    RequestPayload::Shutdown if session_epoch == Some(request.epoch) => {
                        stop = true;
                        Ok(ReplyPayload::Shutdown(ShutdownReply {}))
                    }
                    _ => Err(PluginFailure {
                        code: PluginFailureCode::UnsupportedCapability,
                    }),
                };
                (request.epoch, request.id, reply, stop)
            }
        };
        serde_json::to_writer(
            &mut output,
            &PluginMessage {
                v: 1,
                epoch,
                id,
                outcome,
            },
        )?;
        output.write_all(b"\n")?;
        output.flush()?;
        if stop {
            return Ok(());
        }
    }
}

fn package(destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let executable = fs::read(std::env::current_exe()?)?;
    let target = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else {
        return Err("This packaging target is unsupported".into());
    };
    let member = if cfg!(windows) {
        "bin/catalog.exe"
    } else {
        "bin/catalog"
    };
    let hash = format!("{:x}", Sha256::digest(&executable));
    let manifest = json!({"schemaVersion":1,"id":EXAMPLE_PLUGIN_ID,"name":"Example catalog",
        "version":env!("CARGO_PKG_VERSION"),"publisher":"OpenNOW example author",
        "description":"Illustrative read-only catalog. These entries are not playable games or a cloud service.",
        "protocolVersion":1,"capabilities":[CATALOG_CAPABILITY],"entrypoints":{target:member},
        "files":[{"path":member,"sha256":hash}]});
    let mut archive = zip::ZipWriter::new(File::create(destination)?);
    archive.start_file(
        "manifest.json",
        zip::write::SimpleFileOptions::default().unix_permissions(0o600),
    )?;
    archive.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    archive.start_file(
        member,
        zip::write::SimpleFileOptions::default().unix_permissions(0o755),
    )?;
    archive.write_all(&executable)?;
    archive.finish()?;
    Ok(())
}
