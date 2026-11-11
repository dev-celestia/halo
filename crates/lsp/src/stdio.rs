//! LSP Content-Length framing over byte streams.

use std::io::{self, BufRead, Write};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("missing Content-Length")]
    MissingContentLength,
    #[error("body is not valid UTF-8")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Encode a JSON body as an LSP framed message.
pub fn encode_frame(body: &str) -> Vec<u8> {
    let len = body.len();
    let header = format!("Content-Length: {len}\r\n\r\n");
    let mut out = Vec::with_capacity(header.len() + len);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(body.as_bytes());
    out
}

/// Write a framed JSON body to `writer`.
pub fn write_frame(writer: &mut impl Write, body: &str) -> Result<(), FrameError> {
    writer.write_all(&encode_frame(body))?;
    writer.flush()?;
    Ok(())
}

/// Read one framed JSON body from a buffered reader.
pub fn read_frame(reader: &mut impl BufRead) -> Result<String, FrameError> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Err(FrameError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "lsp stdout closed",
            )));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(FrameError::InvalidHeader(trimmed.to_string()));
        };
        if key.eq_ignore_ascii_case("Content-Length") {
            let len = value
                .trim()
                .parse::<usize>()
                .map_err(|e| FrameError::InvalidHeader(format!("Content-Length: {e}")))?;
            content_length = Some(len);
        }
    }
    let len = content_length.ok_or(FrameError::MissingContentLength)?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_frame() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let encoded = encode_frame(body);
        let mut cursor = Cursor::new(encoded);
        let decoded = read_frame(&mut cursor).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn rejects_missing_length() {
        let raw = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}";
        let mut cursor = Cursor::new(&raw[..]);
        assert!(matches!(
            read_frame(&mut cursor),
            Err(FrameError::MissingContentLength)
        ));
    }
}
