use crate::ImageData;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use memchr::memchr;

/// Extracts images using the user's original html5ever full RcDom tree parsing.
pub fn extract_images_rcdom(html: &str, record_id: &str, images: &mut Vec<ImageData>) {
    let parser = parse_document(RcDom::default(), Default::default());
    let dom = parser.one::<String>(html.to_string());
    walk_rcdom(&dom.document, record_id, images);
}

fn walk_rcdom(node: &Handle, record_id: &str, images: &mut Vec<ImageData>) {
    if let NodeData::Element { attrs, .. } = &node.data {
        let mut src = None;
        let mut alt = None;

        for attr in attrs.borrow().iter() {
            let attr_name = &attr.name.local;
            if attr_name.as_ref() == "src" {
                src = Some(attr.value.to_string());
            } else if attr_name.as_ref() == "alt" {
                alt = Some(attr.value.to_string());
            }
        }

        if let Some(src_value) = src {
            if (src_value.starts_with("http:") || src_value.starts_with("https:"))
                && (src_value.ends_with("png") || src_value.ends_with("jpeg"))
            {
                images.push(ImageData {
                    record_id: record_id.to_string(),
                    src: src_value,
                    alt: alt.unwrap_or_else(|| "None".to_string()),
                });
            }
        }
    }

    for child in node.children.borrow().iter() {
        walk_rcdom(child, record_id, images);
    }
}

/// High-performance zero-allocation streaming HTML image scanner.
/// Scans directly over raw HTML bytes without creating DOM trees or allocating for non-image tags.
pub fn extract_images_fast(html_bytes: &[u8], record_id: &str, images: &mut Vec<ImageData>) {
    let mut cursor = 0;
    let len = html_bytes.len();

    while cursor < len {
        // Search for next '<'
        let next_lt = match memchr(b'<', &html_bytes[cursor..]) {
            Some(pos) => cursor + pos,
            None => break,
        };
        cursor = next_lt + 1;

        // Check if tag starts with "img" or "IMG"
        if cursor + 3 <= len {
            let tag_prefix = &html_bytes[cursor..cursor + 3];
            if tag_prefix.eq_ignore_ascii_case(b"img") {
                cursor += 3;
                // Next char must be whitespace or '>' or '/'
                if cursor < len {
                    let next_b = html_bytes[cursor];
                    if next_b == b' ' || next_b == b'\t' || next_b == b'\n' || next_b == b'\r' || next_b == b'/' || next_b == b'>' {
                        // Scan attributes until '>'
                        parse_img_tag_attributes(&html_bytes, &mut cursor, record_id, images);
                    }
                }
            }
        }
    }
}

fn parse_img_tag_attributes(
    bytes: &[u8],
    cursor: &mut usize,
    record_id: &str,
    images: &mut Vec<ImageData>,
) {
    let len = bytes.len();
    let mut src: Option<String> = None;
    let mut alt: Option<String> = None;

    while *cursor < len {
        // Skip whitespace
        while *cursor < len && (bytes[*cursor] == b' ' || bytes[*cursor] == b'\t' || bytes[*cursor] == b'\r' || bytes[*cursor] == b'\n') {
            *cursor += 1;
        }

        if *cursor >= len {
            break;
        }

        let b = bytes[*cursor];
        if b == b'>' {
            *cursor += 1;
            break;
        }
        if b == b'/' {
            *cursor += 1;
            continue;
        }

        // Read attribute name
        let name_start = *cursor;
        while *cursor < len {
            let ch = bytes[*cursor];
            if ch == b'=' || ch == b'>' || ch == b'/' || ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                break;
            }
            *cursor += 1;
        }
        let attr_name = &bytes[name_start..*cursor];

        // Skip whitespace up to '='
        while *cursor < len && (bytes[*cursor] == b' ' || bytes[*cursor] == b'\t' || bytes[*cursor] == b'\r' || bytes[*cursor] == b'\n') {
            *cursor += 1;
        }

        let mut attr_value: Option<&[u8]> = None;

        if *cursor < len && bytes[*cursor] == b'=' {
            *cursor += 1; // Consume '='

            // Skip whitespace after '='
            while *cursor < len && (bytes[*cursor] == b' ' || bytes[*cursor] == b'\t' || bytes[*cursor] == b'\r' || bytes[*cursor] == b'\n') {
                *cursor += 1;
            }

            if *cursor < len {
                let quote = bytes[*cursor];
                if quote == b'"' || quote == b'\'' {
                    *cursor += 1; // Consume quote
                    let val_start = *cursor;
                    while *cursor < len && bytes[*cursor] != quote {
                        *cursor += 1;
                    }
                    attr_value = Some(&bytes[val_start..*cursor]);
                    if *cursor < len && bytes[*cursor] == quote {
                        *cursor += 1; // Consume closing quote
                    }
                } else {
                    // Unquoted attribute value
                    let val_start = *cursor;
                    while *cursor < len {
                        let ch = bytes[*cursor];
                        if ch == b'>' || ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                            break;
                        }
                        *cursor += 1;
                    }
                    attr_value = Some(&bytes[val_start..*cursor]);
                }
            }
        }

        if attr_name.eq_ignore_ascii_case(b"src") {
            if let Some(val) = attr_value {
                src = Some(String::from_utf8_lossy(val).into_owned());
            }
        } else if attr_name.eq_ignore_ascii_case(b"alt") {
            if let Some(val) = attr_value {
                alt = Some(String::from_utf8_lossy(val).into_owned());
            }
        }
    }

    if let Some(src_val) = src {
        if (src_val.starts_with("http:") || src_val.starts_with("https:"))
            && (src_val.ends_with("png") || src_val.ends_with("jpeg"))
        {
            images.push(ImageData {
                record_id: record_id.to_string(),
                src: src_val,
                alt: alt.unwrap_or_else(|| "None".to_string()),
            });
        }
    }
}
