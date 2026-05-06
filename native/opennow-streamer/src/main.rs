#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod backend;
#[cfg(feature = "gstreamer")]
mod gstreamer_backend;
mod input;
mod protocol;
mod sdp;

use serde::Serialize;
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;

use backend::{create_backend, BackendReply, NativeStreamerBackend};
use protocol::{parse_command, CommandEnvelope, Event, Response, PROTOCOL_VERSION};

fn write_json<T: Serialize>(value: &T) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;
    stdout.flush()
}

fn write_response(response: &Response) -> io::Result<()> {
    write_json(response)
}

fn write_event(event: &Event) -> io::Result<()> {
    write_json(event)
}

fn write_error(id: Option<String>, code: &str, message: impl Into<String>) -> io::Result<()> {
    write_response(&Response::Error {
        id,
        code: code.to_owned(),
        message: message.into(),
    })
}

fn write_reply(reply: BackendReply) -> io::Result<bool> {
    for event in &reply.events {
        write_event(event)?;
    }
    if let Some(response) = &reply.response {
        write_response(response)?;
    }
    Ok(reply.should_continue)
}

fn handle_command(
    command: CommandEnvelope,
    backend: &mut dyn NativeStreamerBackend,
) -> io::Result<bool> {
    match command.command_type.as_str() {
        "hello" => {
            let requested = command.protocol_version.unwrap_or(0);
            if requested != PROTOCOL_VERSION {
                write_error(
                    Some(command.id),
                    "protocol-version-mismatch",
                    "Unsupported native streamer protocol version.",
                )?;
                return Ok(true);
            }

            write_response(&Response::Ready {
                id: command.id,
                capabilities: backend.capabilities(),
            })?;
        }
        "start" => {
            return write_reply(backend.start(command));
        }
        "offer" => {
            return write_reply(backend.handle_offer(command));
        }
        "remote-ice" => {
            return write_reply(backend.add_remote_ice(command));
        }
        "input" => {
            return write_reply(backend.send_input(command));
        }
        "surface" => {
            return write_reply(backend.update_render_surface(command));
        }
        "bitrate" => {
            return write_reply(backend.update_bitrate_limit(command));
        }
        "stop" => {
            return write_reply(backend.stop(command));
        }
        other => {
            write_error(
                Some(command.id),
                "unknown-command",
                format!("Unknown command: {other}"),
            )?;
        }
    }

    Ok(true)
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let (event_sender, event_receiver) = mpsc::channel::<Event>();
    let event_writer = thread::spawn(move || {
        for event in event_receiver {
            if let Err(error) = write_event(&event) {
                eprintln!("[NativeStreamer] Failed to write async event: {error}");
                break;
            }
        }
    });
    let mut backend = create_backend(Some(event_sender));

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(error) => {
                write_error(None, "invalid-json", error.to_string())?;
                continue;
            }
        };

        let command = match parse_command(value) {
            Ok(command) => command,
            Err(error) => {
                write_error(None, "invalid-command", error)?;
                continue;
            }
        };

        if !handle_command(command, backend.as_mut())? {
            break;
        }
    }

    drop(backend);
    let _ = event_writer.join();
    Ok(())
}
