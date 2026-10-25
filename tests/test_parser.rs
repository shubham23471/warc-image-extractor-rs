use std::fs::File;
use flate2::read::MultiGzDecoder;
use warc_parser_rs::extractor::{extract_images_fast, extract_images_rcdom};
use warc_parser_rs::warc_parser::{HttpResponseView, WarcStreamReader, WarcType};

#[test]
fn test_parse_first_records() {
    let warc_path = "/Users/shubham/projects/rust_projects/warc-parser-rs/data/CC-MAIN-20241201162023-20241201192023-00000.warc.gz";
    let file = File::open(warc_path).expect("failed to open warc file");
    let gz = MultiGzDecoder::new(file);
    let mut reader = WarcStreamReader::new(gz);

    let mut payload = Vec::new();
    let mut count = 0;
    let mut response_count = 0;
    let mut request_count = 0;
    let mut metadata_count = 0;
    let mut warcinfo_count = 0;

    let mut fast_images = Vec::new();
    let mut rcdom_images = Vec::new();

    while let Some(header) = reader.read_next_header().expect("failed to read header") {
        count += 1;
        match header.warc_type {
            WarcType::Response => {
                response_count += 1;
                reader.read_payload(header.content_length, &mut payload).expect("failed to read payload");
                assert_eq!(payload.len(), header.content_length);

                // Test HTTP parser & image extraction on responses
                if let Some(http) = HttpResponseView::parse(&payload) {
                    if http.is_html {
                        let html_str = String::from_utf8_lossy(http.body);
                        extract_images_fast(http.body, &header.record_id, &mut fast_images);
                        extract_images_rcdom(&html_str, &header.record_id, &mut rcdom_images);
                    }
                }
            }
            WarcType::Request => {
                request_count += 1;
                reader.read_payload(header.content_length, &mut payload).expect("failed to read payload");
            }
            WarcType::Metadata => {
                metadata_count += 1;
                reader.read_payload(header.content_length, &mut payload).expect("failed to read payload");
            }
            WarcType::WarcInfo => {
                warcinfo_count += 1;
                reader.read_payload(header.content_length, &mut payload).expect("failed to read payload");
            }
            _ => {
                reader.read_payload(header.content_length, &mut payload).expect("failed to read payload");
            }
        }

        if count >= 300 {
            break;
        }
    }

    assert_eq!(count, 300);
    assert_eq!(warcinfo_count, 1);
    assert!(response_count > 0);
    println!("Parsed 300 records: response={}, request={}, metadata={}, warcinfo={}",
        response_count, request_count, metadata_count, warcinfo_count);
    assert!(fast_images.len() >= 200, "Fast extractor extracted {}", fast_images.len());
    assert!(rcdom_images.len() >= 200, "RcDom extractor extracted {}", rcdom_images.len());
    println!("Both extractors successfully extracted ~200 images from test sample.");
}
