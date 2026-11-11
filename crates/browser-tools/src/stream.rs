//! The live view: agent-browser's screencast, framed for a pane.
//!
//! The wire is a local WebSocket that emits JSON text messages. Three shapes
//! matter:
//!
//! ```text
//! {"type":"status","connected":true,"screencasting":true,"viewportWidth":1280,…}
//! {"type":"tabs","tabs":[…],"timestamp":…}
//! {"type":"frame","data":"<base64 JPEG>","metadata":{"deviceWidth":1280,…}}
//! ```
//!
//! A client that connects turns `screencasting` on, and a frame arrives roughly
//! per repaint. Frames are base64 JPEG inside JSON — not binary — so the cost is
//! ~1.33× the JPEG, which is why quality is configurable on the CLI side.
//!
//! Reading happens on its own thread and frames cross a channel: a quiet page
//! must not block the caller, and a read timeout in the middle of a frame would
//! desynchronise tungstenite.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use base64::Engine as _;
use serde::Deserialize;
use tungstenite::Message;

/// One screencast frame, still JPEG-encoded.
///
/// Decoding belongs to the renderer: this crate stays free of an image codec so
/// the CLI stays small.
#[derive(Clone)]
pub struct Frame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Event {
    Frame {
        data: String,
        #[serde(default)]
        metadata: Metadata,
    },
    Status {
        #[serde(default)]
        connected: bool,
        #[serde(default)]
        screencasting: bool,
        #[serde(default, rename = "viewportWidth")]
        viewport_width: u32,
        #[serde(default, rename = "viewportHeight")]
        viewport_height: u32,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    #[serde(default)]
    device_width: u32,
    #[serde(default)]
    device_height: u32,
}

/// What the server last said about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamStatus {
    pub connected: bool,
    pub screencasting: bool,
    pub width: u32,
    pub height: u32,
}

/// A running screencast reader.
pub struct Stream {
    frames: Receiver<Frame>,
    status: StreamStatus,
}

impl Stream {
    /// Connect to `ws://127.0.0.1:<port>` and start reading.
    ///
    /// The thread ends when the socket closes or when the receiver is dropped —
    /// which is what stops it when a pane closes.
    pub fn connect(url: &str) -> Result<Self, String> {
        let target = url.to_string();
        let reported_url = url.to_string();
        let (tx, frames) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<StreamStatus, String>>();

        thread::Builder::new()
            .name("halo-browser-stream".into())
            .spawn(move || {
                let mut socket = match tungstenite::connect(&target) {
                    Ok((socket, _response)) => socket,
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!("cannot connect to {target}: {error}")));
                        return;
                    }
                };
                let mut reported = false;
                loop {
                    let message = match socket.read() {
                        Ok(message) => message,
                        Err(_) => {
                            if !reported {
                                let _ = ready_tx.send(Err("the stream closed immediately".into()));
                            }
                            return;
                        }
                    };
                    let text = match message {
                        Message::Text(text) => text,
                        Message::Close(_) => return,
                        // Ping/Pong/Frame are handled by tungstenite or unused.
                        _ => continue,
                    };
                    let event: Event = match serde_json::from_str(text.as_str()) {
                        Ok(event) => event,
                        // A shape we do not know is not a reason to stop reading.
                        Err(_) => continue,
                    };
                    match event {
                        Event::Frame { data, metadata } => {
                            let jpeg = match base64::engine::general_purpose::STANDARD.decode(data) {
                                Ok(jpeg) => jpeg,
                                Err(_) => continue,
                            };
                            if jpeg.is_empty() {
                                continue;
                            }
                            if !reported {
                                reported = true;
                                let _ = ready_tx.send(Ok(StreamStatus {
                                    connected: true,
                                    screencasting: true,
                                    width: metadata.device_width,
                                    height: metadata.device_height,
                                }));
                            }
                            let frame = Frame {
                                jpeg,
                                width: metadata.device_width,
                                height: metadata.device_height,
                            };
                            // A closed channel means the pane is gone.
                            if tx.send(frame).is_err() {
                                return;
                            }
                        }
                        Event::Status {
                            connected,
                            screencasting,
                            viewport_width,
                            viewport_height,
                        } => {
                            if !reported {
                                reported = true;
                                let _ = ready_tx.send(Ok(StreamStatus {
                                    connected,
                                    screencasting,
                                    width: viewport_width,
                                    height: viewport_height,
                                }));
                            }
                        }
                        Event::Other => {}
                    }
                }
            })
            .map_err(|error| format!("cannot start the stream reader: {error}"))?;

        // The first message arrives immediately (status), so this only waits out
        // a connection that never opened.
        let status = ready_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| format!("no answer from {reported_url} within 10s"))??;

        Ok(Self { frames, status })
    }

    /// What the server reported on connect.
    pub fn status(&self) -> &StreamStatus {
        &self.status
    }

    /// The newest frame, discarding any that piled up while the caller was busy.
    ///
    /// A pane renders the latest state; a queue of stale frames is latency the
    /// user would feel, not smoothness.
    pub fn latest(&self) -> Result<Option<Frame>, String> {
        let mut newest = None;
        loop {
            match self.frames.try_recv() {
                Ok(frame) => newest = Some(frame),
                Err(TryRecvError::Empty) => return Ok(newest),
                Err(TryRecvError::Disconnected) => {
                    return Err("the browser stopped streaming".into());
                }
            }
        }
    }

    /// Block until the next frame, or `None` after `timeout`.
    pub fn next_within(&self, timeout: std::time::Duration) -> Result<Option<Frame>, String> {
        match self.frames.recv_timeout(timeout) {
            Ok(frame) => Ok(Some(frame)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("the browser stopped streaming".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_json_decodes_to_bytes_and_size() {
        // 1×1 white JPEG, base64.
        let jpeg_b64 = "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0a\
HBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAA\
AAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AKp//2Q==";
        let text = format!(
            r#"{{"type":"frame","data":"{jpeg_b64}","metadata":{{"deviceWidth":1280,"deviceHeight":720}}}}"#
        );
        let Event::Frame { data, metadata } = serde_json::from_str(&text).unwrap() else {
            panic!("not a frame");
        };
        assert_eq!(metadata.device_width, 1280);
        assert_eq!(metadata.device_height, 720);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        assert_eq!(&bytes[..3], &[0xff, 0xd8, 0xff]);
    }

    #[test]
    fn status_json_parses_camel_case() {
        let text = r#"{"connected":true,"engine":"chrome","recording":false,"screencasting":true,"type":"status","viewportHeight":720,"viewportWidth":1280}"#;
        let Event::Status {
            connected,
            screencasting,
            viewport_width,
            viewport_height,
        } = serde_json::from_str(text).unwrap()
        else {
            panic!("not a status");
        };
        assert!(connected && screencasting);
        assert_eq!((viewport_width, viewport_height), (1280, 720));
    }

    #[test]
    fn unknown_message_types_do_not_stop_the_reader() {
        let text = r#"{"type":"tabs","tabs":[],"timestamp":1}"#;
        assert!(matches!(
            serde_json::from_str::<Event>(text).unwrap(),
            Event::Other
        ));
    }
}
