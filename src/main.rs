#[warn(unused_imports)]
use warc::WarcReader;
use markup5ever_rcdom::{RcDom, Handle};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::NodeData;


#[allow(dead_code)]
#[derive(Debug)]
struct CountInfo {
    total_count: i32, 
    response_count: i32, 
    request_count: i32,
    metadata_count: i32,
    total_src_count: i32,
}

#[derive(Debug)]
#[allow(dead_code)]
struct ImageData {
    record_id: String, 
    src: String,
    alt: String,
}

#[allow(unused_variables)]
fn walk(node: &Handle, record_id: &str,
    images: &mut Vec<ImageData>) {

    match &node.data {
        NodeData::Element {name, attrs, ..} => {
            let mut src = None; 
            let mut alt = None;

            for attr in attrs.borrow().iter() {
                let attr_name = attr.name.local.to_string(); 

                if attr_name == "src" {
                    src = Some(attr.value.to_string());
                } else if attr_name == "alt"{
                    alt = Some(attr.value.to_string());
                }
            }
            
            // appending the non-null values to vector
            if let Some(src_value) = src {
                if (src_value.starts_with("http:") || src_value.starts_with("https:")) 
                && 
                (src_value.ends_with("png") || src_value.ends_with("jpeg")) { 
                    let image = ImageData {
                        record_id: record_id.to_string(),
                        src: src_value,
                        alt: alt.unwrap_or("None".to_string())
                    };
                    images.push(image)
                }
            }   
        },
        _ => {}
    }
    // recurse on children so we visit the whole tree
    for child in node.children.borrow().iter(){
        walk(child, record_id, images);
    }
}

fn main() ->  Result<(), Box<dyn std::error::Error>> {
    let warc_file_path = "/Users/shubham/projects/rust_projects/warc-parser-rs/data/CC-MAIN-20241201162023-20241201192023-00000.warc.gz";
    let warc_file = WarcReader::from_path_gzip(warc_file_path)?;
    
    let mut count = 0;
    let mut res_count = 0;
    let mut req_count = 0;
    let mut meta_count = 0;
    let mut src_count: i32 = 0;
    
    for record in warc_file.iter_records(){
        count += 1;

        match record {
            Err(e) => println!("ERROR: {}", e),
            Ok(record) => {

                if record.warc_type().to_string() == "response"{
                    res_count += 1;
                    let response_body = String::from_utf8_lossy(&record.body());
                    

                    let parser = parse_document(RcDom::default(), Default::default());
                    let dom =  parser.one::<String>(response_body.into());
                    
                    let document: Handle = dom.document;
                    let record_id = record.warc_id();
                    let mut images = Vec::new();

                    walk(&document, &record_id, &mut images);
                    for img in &images {
                        println!("here: {:?}", img);
                    }
                    
                    // logging the number of extracted URLs
                    src_count += images.len() as i32;

                } else if record.warc_type().to_string() == "request" {
                    req_count +=1;
                } else if record.warc_type().to_string() == "metadata" {
                    meta_count +=1; 
                } else {
                }
            }
        }

        if *&count > 6 {
            break
        }
    }
    
    println!("Total count: {count}");

    let stats = CountInfo{
        total_count: count, 
        response_count: res_count, 
        request_count: req_count,
        metadata_count: meta_count,
        total_src_count: src_count,
    };
    println!("-------------------------");
    println!("Parser Stats: {:?}", stats);
    Ok(())
}
