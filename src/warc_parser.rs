use std::io::{self, BufRead, BufReader, Read};
use memchr::memmem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarcType {
    WarcInfo,
    Response,
    Request,
    Metadata,
    Revisit,
    Resource,
    Conversion,
    Continuation,
    Unknown,
}

impl WarcType {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match bytes {
            b"warcinfo" | b"WARCINFO" => WarcType::WarcInfo,
            b"response" | b"RESPONSE" => WarcType::Response,
            b"request" | b"REQUEST" => WarcType::Request,
            b"metadata" | b"METADATA" => WarcType::Metadata,
            b"revisit" | b"REVISIT" => WarcType::Revisit,
            b"resource" | b"RESOURCE" => WarcType::Resource,
            b"conversion" | b"CONVERSION" => WarcType::Conversion,
            b"continuation" | b"CONTINUATION" => WarcType::Continuation,
            _ => WarcType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WarcType::WarcInfo => "warcinfo",
            WarcType::Response => "response",
            WarcType::Request => "request",
            WarcType::Metadata => "metadata",
            WarcType::Revisit => "revisit",
            WarcType::Resource => "resource",
            WarcType::Conversion => "conversion",
            WarcType::Continuation => "continuation",
            WarcType::Unknown => "unknown",
        }
    }
}

/// Parsed metadata for a single WARC record header block.
#[derive(Debug, Clone)]
pub struct WarcHeader {
    pub warc_type: WarcType,
    pub record_id: String,
    pub content_length: usize,
    pub date: Option<String>,
    pub target_uri: Option<String>,
    pub content_type: Option<String>,
    pub identified_payload_type: Option<String>,
}

/// Fast streaming parser for WARC files (ISO 28500).
pub struct WarcStreamReader<R: Read> {
    reader: BufReader<R>,
    header_buf: Vec<u8>,
}

impl<R: Read> WarcStreamReader<R> {
    pub fn new(reader: R) -> Self {
        Self::with_capacity(reader, 256 * 1024)
    }

    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        Self {
            reader: BufReader::with_capacity(capacity, reader),
            header_buf: Vec::with_capacity(4096),
        }
    }

    /// Skips leading whitespace/newlines and reads the WARC header block up to `\r\n\r\n` or `\n\n`.
    /// Returns `Ok(Some(header))` if a record was read, `Ok(None)` on EOF.
    pub fn read_next_header(&mut self) -> io::Result<Option<WarcHeader>> {
        self.header_buf.clear();

        // 1. Skip any leading empty lines / delimiters between records
        loop {
            let (skip_bytes, avail_len) = {
                let available = self.reader.fill_buf()?;
                if available.is_empty() {
                    return Ok(None); // Clean EOF
                }

                let mut skip = 0;
                while skip < available.len() {
                    let b = available[skip];
                    if b == b'\r' || b == b'\n' || b == b' ' || b == b'\t' {
                        skip += 1;
                    } else {
                        break;
                    }
                }
                (skip, available.len())
            };

            if skip_bytes > 0 {
                self.reader.consume(skip_bytes);
            }

            if skip_bytes < avail_len {
                // Found non-whitespace start
                break;
            }
        }

        // 2. Read until the header end sequence (\r\n\r\n or \n\n)
        let crlf_crlf = b"\r\n\r\n";
        let lf_lf = b"\n\n";

        let (header_end, delimiter_len) = loop {
            let available_len = {
                let available = self.reader.fill_buf()?;
                if available.is_empty() {
                    if self.header_buf.is_empty() {
                        return Ok(None);
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Truncated WARC header block",
                        ));
                    }
                }
                self.header_buf.extend_from_slice(available);
                available.len()
            };

            self.reader.consume(available_len);

            // Search for end delimiter in accumulated buffer
            let prev_len = self.header_buf.len() - available_len;
            let search_start = prev_len.saturating_sub(4);

            if let Some(pos) = memmem::find(&self.header_buf[search_start..], crlf_crlf) {
                break (search_start + pos, 4);
            } else if let Some(pos) = memmem::find(&self.header_buf[search_start..], lf_lf) {
                break (search_start + pos, 2);
            }
        };

        let header = parse_warc_headers(&self.header_buf[..header_end])?;

        // Any bytes after the header delimiter belong to the payload.
        // Drain header bytes so header_buf contains only the initial payload slice.
        self.header_buf.drain(..(header_end + delimiter_len));

        Ok(Some(header))
    }

    /// Reads the payload into `payload_buf`. Reuses the vector allocation.
    pub fn read_payload(&mut self, content_length: usize, payload_buf: &mut Vec<u8>) -> io::Result<()> {
        payload_buf.clear();
        payload_buf.reserve(content_length);

        // First copy any leftover bytes already buffered in self.header_buf
        let from_header_buf = self.header_buf.len().min(content_length);
        if from_header_buf > 0 {
            payload_buf.extend_from_slice(&self.header_buf[..from_header_buf]);
            self.header_buf.drain(..from_header_buf);
        }

        let remaining = content_length - payload_buf.len();
        if remaining > 0 {
            let start = payload_buf.len();
            payload_buf.resize(content_length, 0);
            self.reader.read_exact(&mut payload_buf[start..])?;
        }

        Ok(())
    }
}

/// Fast parser for WARC header fields from a raw byte slice.
fn parse_warc_headers(slice: &[u8]) -> io::Result<WarcHeader> {
    let mut warc_type = WarcType::Unknown;
    let mut record_id = String::new();
    let mut content_length: usize = 0;
    let mut date = None;
    let mut target_uri = None;
    let mut content_type = None;
    let mut identified_payload_type = None;

    let mut lines = slice.split(|&b| b == b'\n');

    // First line must be WARC/1.0 or WARC/1.1
    if let Some(first_line) = lines.next() {
        let trimmed = trim_cr(first_line);
        if !trimmed.starts_with(b"WARC/") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid WARC magic header line: {:?}", String::from_utf8_lossy(trimmed)),
            ));
        }
    }

    for line in lines {
        let line = trim_cr(line);
        if line.is_empty() {
            continue;
        }

        let colon_pos = match memchr::memchr(b':', line) {
            Some(p) => p,
            None => continue,
        };

        let field_name = trim_spaces(&line[..colon_pos]);
        let field_value = trim_spaces(&line[colon_pos + 1..]);

        if field_name.eq_ignore_ascii_case(b"WARC-Type") {
            warc_type = WarcType::from_bytes(field_value);
        } else if field_name.eq_ignore_ascii_case(b"Content-Length") {
            content_length = parse_ascii_usize(field_value).unwrap_or(0);
        } else if field_name.eq_ignore_ascii_case(b"WARC-Record-ID") {
            let id_str = String::from_utf8_lossy(field_value);
            let cleaned = id_str
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim_start_matches("urn:uuid:");
            record_id = cleaned.to_string();
        } else if field_name.eq_ignore_ascii_case(b"WARC-Target-URI") {
            target_uri = Some(String::from_utf8_lossy(field_value).into_owned());
        } else if field_name.eq_ignore_ascii_case(b"WARC-Date") {
            date = Some(String::from_utf8_lossy(field_value).into_owned());
        } else if field_name.eq_ignore_ascii_case(b"Content-Type") {
            content_type = Some(String::from_utf8_lossy(field_value).into_owned());
        } else if field_name.eq_ignore_ascii_case(b"WARC-Identified-Payload-Type") {
            identified_payload_type = Some(String::from_utf8_lossy(field_value).into_owned());
        }
    }

    Ok(WarcHeader {
        warc_type,
        record_id,
        content_length,
        date,
        target_uri,
        content_type,
        identified_payload_type,
    })
}

#[inline(always)]
fn trim_cr(s: &[u8]) -> &[u8] {
    if let Some(&b'\r') = s.last() {
        &s[..s.len() - 1]
    } else {
        s
    }
}

#[inline(always)]
fn trim_spaces(mut s: &[u8]) -> &[u8] {
    while let Some(&b) = s.first() {
        if b == b' ' || b == b'\t' {
            s = &s[1..];
        } else {
            break;
        }
    }
    while let Some(&b) = s.last() {
        if b == b' ' || b == b'\t' {
            s = &s[..s.len() - 1];
        } else {
            break;
        }
    }
    s
}

#[inline(always)]
fn parse_ascii_usize(bytes: &[u8]) -> Option<usize> {
    let mut n: usize = 0;
    for &b in bytes {
        if b.is_ascii_digit() {
            n = n.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        } else if b == b' ' || b == b'\t' {
            continue;
        } else {
            return None;
        }
    }
    Some(n)
}

/// Parsed HTTP response status and body slice from a WARC response payload.
pub struct HttpResponseView<'a> {
    pub status_code: u16,
    pub is_html: bool,
    pub body: &'a [u8],
}

impl<'a> HttpResponseView<'a> {
    /// Quickly splits raw HTTP response payload into HTTP status and body.
    pub fn parse(payload: &'a [u8]) -> Option<Self> {
        let (header_bytes, body) = if let Some(pos) = memmem::find(payload, b"\r\n\r\n") {
            (&payload[..pos], &payload[pos + 4..])
        } else if let Some(pos) = memmem::find(payload, b"\n\n") {
            (&payload[..pos], &payload[pos + 2..])
        } else {
            return None;
        };

        let mut lines = header_bytes.split(|&b| b == b'\n');
        let status_line = lines.next()?;
        let mut parts = status_line.split(|&b| b == b' ');
        let _proto = parts.next()?;
        let status_code_bytes = parts.next()?;
        let status_code = parse_ascii_usize(status_code_bytes)? as u16;

        let mut is_html = false;
        for line in lines {
            let line = trim_cr(line);
            if let Some(colon) = memchr::memchr(b':', line) {
                let name = trim_spaces(&line[..colon]);
                if name.eq_ignore_ascii_case(b"Content-Type") {
                    let val = trim_spaces(&line[colon + 1..]);
                    if val.starts_with(b"text/html") || val.starts_with(b"application/xhtml+xml") {
                        is_html = true;
                    }
                    break;
                }
            }
        }

        Some(HttpResponseView {
            status_code,
            is_html,
            body,
        })
    }
}
