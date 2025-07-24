#[warn(unused_imports)]
use warc::WarcReader;
use warc::WarcHeader;
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
}

// &Handle: as we want to pass this without taking the ownership or cloning
#[allow(unused_variables)]
fn walk(node: &Handle) {
    match &node.data {
        NodeData::Element {name, attrs, ..} => {
            if name.local.to_string() == "img"{
                println!("---------------------");
                println!("NAME: {:?}", name.local.to_string());}

                let mut src = None; // Type: Option<String> : that why we have wrap value of src in Some()
                let mut alt = None;

                for attr in attrs.borrow().iter() {
                    let attr_name = attr.name.local.to_string(); 
                    if attr_name == "src" {
                        src = Some(attr.value.to_string());
                    } else if attr_name == "alt"{
                        alt = Some(attr.value.to_string());
                    }
                // println!("Attr Name: {}, Attr Value: {}", attr.name.local, attr.value);
                println!("src value: {:?}", src);
                println!("alt value: {:?}", alt);

                }
                
        },
        _ => {}
    }
    // recurse on children so we visit the whole tree
    for child in node.children.borrow().iter(){
        walk(child);
    }
}

fn main() ->  Result<(), Box<dyn std::error::Error>> {
    let warc_file_path = "/Users/shubham/startups/warc_parser/data/CC-MAIN-20241201162023-20241201192023-00000.warc.gz";
    let warc_file = WarcReader::from_path_gzip(warc_file_path)?;
    
    let mut count = 0;
    let mut res_count = 0;
    let mut req_count = 0;
    let mut meta_count = 0;
    
    for record in warc_file.iter_records(){
        println!("====");
        count += 1;

        // println!("{}: {}", WarcHeader::RecordID, record?.warc_id());
        // println!("{}: {}", WarcHeader::WarcType, record?.date());
     
        match record {
            Err(e) => println!("ERROR: {}", e),
            Ok(record) => {
                println!("{}: {}", WarcHeader::RecordID, record.warc_id());
                // println!("{}: {}", WarcHeader::WarcType, record.date());        
                // println!("{}: {}", WarcHeader::WarcType, record.warc_type());
                // println!("Body: {:?}", std::str::from_utf8(record.body())?);
                // println!("Body: {:?}", String::from_utf8_lossy(record.body()));
                

                if record.warc_type().to_string() == "response"{
                    res_count += 1;
                    let response_body = String::from_utf8_lossy(&record.body());
                    // println!("Body: {:?}", response_body);

                    let parser = parse_document(RcDom::default(), Default::default());
                    let dom =  parser.one::<String>(response_body.into());
                    // same as calling:  parser.one(response_body.into()); just explicit about 
                    // "I'm giving you a String and I expect it to convert into the right Tendril type."
                    
                    let document: Handle = dom.document;
                    walk(&document);
                    



                } else if record.warc_type().to_string() == "request" {
                    // println!("Body: {:?}", std::str::from_utf8((record.body()))?); 
                    req_count +=1;
                } else if record.warc_type().to_string() == "metadata" {
                    meta_count +=1; 
                } else {
                }
            }
        }
     
        if *&count > 3 {
            break
        }
    }
    
    println!("Total count: {count}");

    let stats = CountInfo{
        total_count: count, 
        response_count: res_count, 
        request_count: req_count,
        metadata_count: meta_count,
    };

    println!("Parser Stats: {:?}", stats);
    Ok(())
}
