//! Streaming output printer for non-TUI mode.
//!
//! Subscribes to Bus events and prints colored text output to the terminal.

use opencoder_core::bus::{Bus, Event};

/// Print streaming events to stdout with ANSI colors.
pub async fn print_stream(bus: &Bus, session_id: &str) {
    let mut rx = bus.subscribe();
    let session_id = session_id.to_string();

    loop {
        match rx.recv().await {
            Ok(event) => match event {
                Event::PartDelta {
                    session_id: sid,
                    field,
                    delta,
                    ..
                } => {
                    if sid.to_string() != session_id {
                        continue;
                    }
                    if field == "content" {
                        print!("{delta}");
                        use std::io::Write;
                        std::io::stdout().flush().ok();
                    }
                }
                Event::PartUpdated {
                    session_id: sid, ..
                } => {
                    if sid.to_string() != session_id {
                        continue;
                    }
                }
                Event::SessionStatus {
                    session_id: sid,
                    status,
                    ..
                } => {
                    if sid.to_string() != session_id {
                        continue;
                    }
                    if matches!(status, opencoder_core::bus::SessionStatusInfo::Idle) {
                        println!();
                        break;
                    }
                }
                Event::SessionError {
                    session_id: sid,
                    error,
                    ..
                } => {
                    if sid.to_string() != session_id {
                        continue;
                    }
                    eprintln!("\n\x1b[31mError: {error}\x1b[0m");
                    break;
                }
                _ => {}
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("\x1b[33m[warning: missed {n} events]\x1b[0m");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}
