use parquet::file::reader::{FileReader, SerializedFileReader};
use std::fs::File;

#[test]
fn test_parquet_output_validity() {
    let path = "/Users/shubham/projects/rust_projects/warc-parser-rs/data/sample.parquet";
    let file = File::open(path).expect("failed to open parquet file");
    let reader = SerializedFileReader::new(file).expect("failed to create serialized parquet reader");
    let metadata = reader.metadata();
    let num_rows = metadata.file_metadata().num_rows();
    println!("Parquet num_rows: {}", num_rows);
    assert_eq!(num_rows, 99340);
    assert_eq!(metadata.file_metadata().schema().get_fields().len(), 3);
}
