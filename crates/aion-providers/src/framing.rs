use std::{mem, str};

use base64::Engine as _;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FrameKind {
    Data,
    Done,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Frame {
    pub event: Option<String>,
    pub data: String,
    pub kind: FrameKind,
}

/// Decodes a byte stream into UTF-8 text incrementally, carrying an incomplete
/// trailing multibyte sequence over to the next chunk.
///
/// `bytes_stream()` can split a multibyte UTF-8 character (e.g. a 3-byte CJK
/// char) across two network chunks. Lossy-decoding each chunk on its own would
/// turn both halves into U+FFFD and drop the character. This decoder only emits
/// complete UTF-8 sequences and retains the incomplete tail until more bytes
/// arrive.
#[derive(Default)]
pub(crate) struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    /// Decode all available bytes, replacing invalid sequences and retaining
    /// only an incomplete trailing sequence for the next call. The returned
    /// string may be empty.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> String {
        self.pending.extend_from_slice(chunk);

        let mut decoded = String::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            match str::from_utf8(&self.pending[consumed..]) {
                Ok(text) => {
                    decoded.push_str(text);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let valid_end = consumed + error.valid_up_to();
                    decoded.push_str(&String::from_utf8_lossy(&self.pending[consumed..valid_end]));
                    consumed = valid_end;
                    let Some(invalid_len) = error.error_len() else {
                        // Only an incomplete trailing sequence waits for another chunk.
                        break;
                    };
                    decoded.push('\u{fffd}');
                    consumed += invalid_len;
                }
            }
        }
        self.pending.drain(..consumed);
        decoded
    }

    /// Decode any remaining pending bytes at the true end of the stream. A
    /// genuinely-truncated trailing sequence is decoded lossily (yielding
    /// U+FFFD) since no further bytes will arrive.
    pub(crate) fn flush(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        let decoded = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        decoded
    }
}

/// Shared SSE field/event state. A blank line dispatches one event; network
/// chunks and individual data lines are not event boundaries.
#[derive(Default)]
struct SseFramer {
    line: String,
    data: String,
    event: Option<String>,
    after_cr: bool,
    started: bool,
}

#[derive(Default)]
pub(crate) struct SseEventFramer(SseFramer);

#[derive(Default)]
pub(crate) struct SseBlockFramer(SseFramer);

impl SseFramer {
    fn push_text(&mut self, text: &str, done_sentinel: Option<&str>) -> Vec<Frame> {
        let mut frames = Vec::new();
        for ch in text.chars() {
            if !self.started {
                self.started = true;
                if ch == '\u{feff}' {
                    continue;
                }
            }
            let after_cr = mem::replace(&mut self.after_cr, ch == '\r');
            if ch == '\n' && after_cr {
                continue;
            }
            if ch == '\r' || ch == '\n' {
                self.finish_line(done_sentinel, &mut frames);
            } else {
                self.line.push(ch);
            }
        }
        frames
    }

    fn finish_line(&mut self, done_sentinel: Option<&str>, frames: &mut Vec<Frame>) {
        let line = mem::take(&mut self.line);
        if line.is_empty() {
            let event = self.event.take();
            if !self.data.is_empty() {
                self.data.pop(); // Each data field appends exactly one LF.
                let data = mem::take(&mut self.data);
                let kind = if done_sentinel == Some(data.as_str()) {
                    FrameKind::Done
                } else {
                    FrameKind::Data
                };
                frames.push(Frame { event, data, kind });
            }
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = line.split_once(':').unwrap_or((&line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            "event" => self.event = Some(value.to_string()),
            _ => {}
        }
    }
}

pub(crate) fn bedrock_payload_to_frame(payload: &[u8]) -> Option<Frame> {
    let wrapper = serde_json::from_slice::<Value>(payload).ok()?;
    let b64 = wrapper.get("bytes")?.as_str()?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let inner = String::from_utf8(decoded).ok()?;
    let inner_json = serde_json::from_str::<Value>(&inner).ok()?;
    let event_type = inner_json.get("type").and_then(Value::as_str).unwrap_or("").to_string();

    Some(Frame {
        event: Some(event_type),
        data: inner,
        kind: FrameKind::Data,
    })
}

impl SseEventFramer {
    pub(crate) fn has_pending_event(&self) -> bool {
        !self.0.data.is_empty() || (!self.0.line.is_empty() && !self.0.line.starts_with(':'))
    }

    pub(crate) fn push_text(&mut self, text: &str, done_sentinel: &str) -> Vec<Frame> {
        self.0.push_text(text, Some(done_sentinel))
    }
}

impl SseBlockFramer {
    pub(crate) fn push_text(&mut self, text: &str) -> Vec<Frame> {
        self.0.push_text(text, None)
    }
}

#[cfg(test)]
#[path = "framing_test.rs"]
mod framing_test;
