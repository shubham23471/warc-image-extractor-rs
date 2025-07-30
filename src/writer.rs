
use std::{fs::File, path::Path, sync::Arc};
use std::io::Write;
use parquet::{
    data_type::{ByteArray, ByteArrayType},
    file::{properties::WriterProperties, writer::SerializedFileWriter, 
        writer::SerializedRowGroupWriter},
    schema::parser::parse_message_type,

};

use crate::ImageData;

fn write_rows_to_parquet<W: Write>(row_group_writer: &mut SerializedRowGroupWriter<W>, 
                        images: &[ImageData]) 
    where W: Send{
    
    for field in ["record_id", "src", "alt"] {
        if let Some(mut serialized_column) = row_group_writer.next_column().unwrap() {
        // Use the typed API for clarity and future-proofing
        let typed_writer = serialized_column.typed::<ByteArrayType>();

        // Map each ImageData.record_id (String) into a ByteArray
        let values: Vec<ByteArray> = images
            .iter()
            .map(|img| 
                match field {
                    "record_id" => ByteArray::from(img.record_id.as_str()),
                    "src" => ByteArray::from(img.src.as_str()),
                    "alt" => ByteArray::from(img.alt.as_str()),
                    _ => unreachable!(),
            }).collect();

        // No definition levels (REQUIRED field)
        typed_writer.write_batch(&values, None, None).unwrap();

        // Close this column before moving on
        serialized_column.close().unwrap();
    }
    }
    
}


/// Write the `record_id` field of each image into a Parquet file at `path`.
pub fn write_images_to_parquet(images: &[ImageData], path: &str) {
    // 1. Interpret `path`
    let path = Path::new(path);

    // 2. Define a simple Parquet schema: one required BYTE_ARRAY column
    let message_type = r#"
      message schema {
        REQUIRED BYTE_ARRAY record_id;
        REQUIRED BYTE_ARRAY src;
        REQUIRED BYTE_ARRAY alt;
      }
    "#;
    let schema = Arc::new(parse_message_type(message_type).unwrap());

    // 3. Create the file and writer
    let file = File::create(path).unwrap();
    let props = Arc::new(WriterProperties::builder().build());
    let mut writer = SerializedFileWriter::new(file, schema, props).unwrap();

    // 4. Start a single row group
    let mut row_group_writer = writer.next_row_group().unwrap();

    write_rows_to_parquet(&mut row_group_writer, images);

    // 6. Finalize the row group and writer
    row_group_writer.close().unwrap();
    writer.close().unwrap();
}



pub fn print_hello() {
    println!("[DEBUG] | Message from writer.rs");
}