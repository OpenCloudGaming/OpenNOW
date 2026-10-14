use opennow_plugin_api::MAX_FRAME_BYTES;
use opennow_plugin_api::provider::*;
use opennow_sdk_demo::DemoProvider;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

fn main() {
    if run().is_err() {
        eprintln!("OpenNOW SDK demo could not complete the requested operation");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 4 && args[1] == "--package" {
        return opennow_sdk_demo::package::create(
            &std::env::current_exe()?,
            Path::new(&args[2]),
            Path::new(&args[3]),
        )
        .map_err(Into::into);
    }
    if args.len() != 1 {
        return Err("Expected no arguments or --package <media-worker> <archive>".into());
    }
    let directory =
        std::env::var_os("OPENNOW_PLUGIN_DATA_DIR").ok_or("Own data directory is required")?;
    let mut provider = DemoProvider::open(Path::new(&directory))?;
    let mut reader = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut epoch = None;
    loop {
        let mut bytes = Vec::new();
        let count = reader
            .by_ref()
            .take((MAX_FRAME_BYTES + 1) as u64)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            return Ok(());
        }
        if count > MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
            return Err("Invalid control frame".into());
        }
        let message = HostMessageV2::decode(&bytes)?;
        let HostMessageV2::Request(request) = message else {
            continue;
        };
        let hello = matches!(request.request, ProviderRequest::Hello(_));
        if (epoch.is_none() && !hello)
            || (epoch.is_some() && (epoch != Some(request.epoch) || hello))
        {
            return Err("Invalid control incarnation".into());
        }
        let stop = matches!(request.request, ProviderRequest::Shutdown(_));
        let response = provider.handle(&request);
        response.validate_for(&request)?;
        if hello && matches!(response.outcome, ProviderOutcome::Success { .. }) {
            epoch = Some(request.epoch);
        }
        let mut bytes = serde_json::to_vec(&PluginMessageV2::Response(response))?;
        if bytes.len() >= MAX_FRAME_BYTES {
            return Err("Oversized control response".into());
        }
        bytes.push(b'\n');
        output.write_all(&bytes)?;
        output.flush()?;
        if stop {
            return Ok(());
        }
    }
}
