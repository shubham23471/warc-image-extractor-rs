pub mod warc_parser;
pub mod extractor;
pub mod writer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    pub record_id: String,
    pub src: String,
    pub alt: String,
}

#[derive(Debug, Clone, Default)]
pub struct CountInfo {
    pub total_count: i32,
    pub response_count: i32,
    pub request_count: i32,
    pub metadata_count: i32,
    pub total_src_count: i32,
}
