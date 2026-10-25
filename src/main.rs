use std::env;
use std::fs::File;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use flate2::read::MultiGzDecoder;
use rayon::prelude::*;
use warc::WarcReader;

use warc_parser_rs::extractor::{extract_images_fast, extract_images_rcdom};
use warc_parser_rs::warc_parser::{HttpResponseView, WarcStreamReader, WarcType};
use warc_parser_rs::writer::write_images_to_parquet;
use warc_parser_rs::ImageData;

const DEFAULT_WARC_PATH: &str =
    "/Users/shubham/projects/rust_projects/warc-parser-rs/data/CC-MAIN-20241201162023-20241201192023-00000.warc.gz";
const DEFAULT_PARQUET_PATH: &str =
    "/Users/shubham/projects/rust_projects/warc-parser-rs/data/sample.parquet";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let mut mode = "parallel"; // default to high-performance parallel pipeline
    let mut warc_path = DEFAULT_WARC_PATH.to_string();
    let mut parquet_path = DEFAULT_PARQUET_PATH.to_string();
    let mut max_records: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                if i + 1 < args.len() {
                    mode = match args[i + 1].as_str() {
                        "baseline" => "baseline",
                        "fast" => "fast",
                        "parallel" => "parallel",
                        other => {
                            eprintln!("Unknown mode '{}', defaulting to 'parallel'", other);
                            "parallel"
                        }
                    };
                    i += 1;
                }
            }
            "--file" | "-f" => {
                if i + 1 < args.len() {
                    warc_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--output" | "-o" => {
                if i + 1 < args.len() {
                    parquet_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--max-records" | "-n" => {
                if i + 1 < args.len() {
                    max_records = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            "--help" | "-h" => {
                println!("WARC Parser & Benchmarking Tool");
                println!("Usage: warc-parser-rs [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --mode <baseline|fast|parallel>  Parsing pipeline (default: parallel)");
                println!("  --file, -f <path>                Input WARC gzip file");
                println!("  --output, -o <path>              Output Parquet file");
                println!("  --max-records, -n <count>        Limit number of records parsed");
                println!("  --help, -h                       Print this help message");
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    println!("========================================================");
    println!("WARC Parser & Benchmarking Pipeline");
    println!("Mode:         {}", mode);
    println!("WARC Input:   {}", warc_path);
    println!("Output:       {}", parquet_path);
    if let Some(limit) = max_records {
        println!("Limit:        {} records", limit);
    } else {
        println!("Limit:        None (full file)");
    }
    println!("========================================================");

    match mode {
        "baseline" => run_baseline(&warc_path, &parquet_path, max_records)?,
        "fast" => run_fast_sequential(&warc_path, &parquet_path, max_records)?,
        "parallel" => run_fast_parallel(&warc_path, &parquet_path, max_records)?,
        _ => unreachable!(),
    }

    Ok(())
}

/// Baseline parser using original warc crate and html5ever RcDom DOM construction.
fn run_baseline(
    warc_path: &str,
    parquet_path: &str,
    max_records: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Running Baseline Pipeline (warc crate + html5ever RcDom) ---");
    let total_start = Instant::now();

    let warc_file = WarcReader::from_path_gzip(warc_path)?;
    let mut count = 0;
    let mut res_count = 0;
    let mut req_count = 0;
    let mut meta_count = 0;
    let mut images = Vec::new();

    let mut warc_read_time = std::time::Duration::default();
    let mut html_parse_time = std::time::Duration::default();

    let mut iter = warc_file.iter_records();

    loop {
        if let Some(limit) = max_records {
            if count >= limit {
                break;
            }
        }

        let warc_start = Instant::now();
        let record = match iter.next() {
            Some(rec) => rec,
            None => break,
        };
        warc_read_time += warc_start.elapsed();

        count += 1;

        match record {
            Err(e) => eprintln!("WARC Error: {}", e),
            Ok(rec) => {
                let wtype = rec.warc_type().to_string();
                if wtype == "response" {
                    res_count += 1;
                    let html_start = Instant::now();
                    let response_body = String::from_utf8_lossy(&rec.body());
                    let record_id = rec.warc_id().replace("<urn:uuid:", "").replace(">", "");
                    extract_images_rcdom(&response_body, &record_id, &mut images);
                    html_parse_time += html_start.elapsed();
                } else if wtype == "request" {
                    req_count += 1;
                } else if wtype == "metadata" {
                    meta_count += 1;
                }
            }
        }
    }

    let read_time_total = warc_read_time;
    let parse_time_total = html_parse_time;

    let parquet_start = Instant::now();
    write_images_to_parquet(&images, parquet_path);
    let parquet_time = parquet_start.elapsed();

    print_benchmark_report(
        "Baseline (Single-threaded)",
        count,
        res_count,
        req_count,
        meta_count,
        images.len() as i32,
        read_time_total,
        parse_time_total,
        parquet_time,
        total_start.elapsed(),
    );

    Ok(())
}

/// Custom streaming WarcStreamReader with fast image extraction (Single-threaded).
fn run_fast_sequential(
    warc_path: &str,
    parquet_path: &str,
    max_records: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Running Fast Sequential Pipeline (WarcStreamReader + Fast Extractor) ---");
    let total_start = Instant::now();

    let file = File::open(warc_path)?;
    let gz = MultiGzDecoder::new(file);
    let mut reader = WarcStreamReader::new(gz);

    let mut count = 0;
    let mut res_count = 0;
    let mut req_count = 0;
    let mut meta_count = 0;
    let mut images = Vec::new();

    let mut warc_read_time = std::time::Duration::default();
    let mut html_parse_time = std::time::Duration::default();
    let mut payload = Vec::new();

    loop {
        if let Some(limit) = max_records {
            if count >= limit {
                break;
            }
        }

        let warc_start = Instant::now();
        let header = match reader.read_next_header()? {
            Some(h) => h,
            None => break,
        };

        reader.read_payload(header.content_length, &mut payload)?;
        warc_read_time += warc_start.elapsed();

        count += 1;

        match header.warc_type {
            WarcType::Response => {
                res_count += 1;
                let html_start = Instant::now();
                if let Some(http) = HttpResponseView::parse(&payload) {
                    if http.is_html {
                        extract_images_fast(http.body, &header.record_id, &mut images);
                    }
                }
                html_parse_time += html_start.elapsed();
            }
            WarcType::Request => req_count += 1,
            WarcType::Metadata => meta_count += 1,
            _ => {}
        }
    }

    let parquet_start = Instant::now();
    write_images_to_parquet(&images, parquet_path);
    let parquet_time = parquet_start.elapsed();

    print_benchmark_report(
        "Fast Sequential",
        count,
        res_count,
        req_count,
        meta_count,
        images.len() as i32,
        warc_read_time,
        html_parse_time,
        parquet_time,
        total_start.elapsed(),
    );

    Ok(())
}

/// Batch of response payloads to be parsed by worker threads.
struct ResponseBatch {
    records: Vec<(String, Vec<u8>)>, // (record_id, payload)
}

/// High-performance multi-threaded pipeline:
/// Thread 1: Streams and decompresses WARC records from disk.
/// Thread Pool (Rayon): Parses HTTP headers and extracts image tags in parallel across all CPU cores.
/// Main Thread: Collects results and writes into Parquet.
fn run_fast_parallel(
    warc_path: &str,
    parquet_path: &str,
    max_records: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Running Multi-Threaded Parallel Pipeline ---");
    let total_start = Instant::now();

    let num_cpus = rayon::current_num_threads();
    println!("Rayon Worker Threads: {}", num_cpus);

    let (tx_batches, rx_batches) = mpsc::sync_channel::<ResponseBatch>(64);
    let (tx_images, rx_images) = mpsc::sync_channel::<Vec<ImageData>>(128);

    let warc_path_clone = warc_path.to_string();
    let total_count = std::sync::Arc::new(AtomicI32::new(0));
    let res_count = std::sync::Arc::new(AtomicI32::new(0));
    let req_count = std::sync::Arc::new(AtomicI32::new(0));
    let meta_count = std::sync::Arc::new(AtomicI32::new(0));

    let total_count_reader = total_count.clone();
    let res_count_reader = res_count.clone();
    let req_count_reader = req_count.clone();
    let meta_count_reader = meta_count.clone();

    // 1. Dedicated I/O Decompression & Reading Thread
    let io_handle = thread::spawn(move || -> std::io::Result<()> {
        let file = File::open(&warc_path_clone)?;
        let gz = MultiGzDecoder::new(file);
        let mut reader = WarcStreamReader::new(gz);

        let mut local_count = 0;
        let mut current_batch = Vec::with_capacity(128);

        while let Some(header) = reader.read_next_header()? {
            if let Some(limit) = max_records {
                if local_count >= limit {
                    break;
                }
            }

            local_count += 1;
            total_count_reader.fetch_add(1, Ordering::Relaxed);

            let mut payload = Vec::new();
            reader.read_payload(header.content_length, &mut payload)?;

            match header.warc_type {
                WarcType::Response => {
                    res_count_reader.fetch_add(1, Ordering::Relaxed);
                    current_batch.push((header.record_id, payload));
                    if current_batch.len() >= 128 {
                        let batch = std::mem::replace(&mut current_batch, Vec::with_capacity(128));
                        if tx_batches.send(ResponseBatch { records: batch }).is_err() {
                            break;
                        }
                    }
                }
                WarcType::Request => {
                    req_count_reader.fetch_add(1, Ordering::Relaxed);
                }
                WarcType::Metadata => {
                    meta_count_reader.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }

        if !current_batch.is_empty() {
            let _ = tx_batches.send(ResponseBatch { records: current_batch });
        }

        Ok(())
    });

    // 2. Parallel Processing Workers using Rayon
    let tx_images_clone = tx_images.clone();
    let worker_handle = thread::spawn(move || {
        // Pull batches from decompression thread and process in Rayon
        while let Ok(batch) = rx_batches.recv() {
            // Process the records in parallel using Rayon
            let extracted_images: Vec<ImageData> = batch
                .records
                .into_par_iter()
                .flat_map(|(record_id, payload)| {
                    let mut local_images = Vec::new();
                    if let Some(http) = HttpResponseView::parse(&payload) {
                        if http.is_html {
                            extract_images_fast(http.body, &record_id, &mut local_images);
                        }
                    }
                    local_images
                })
                .collect();

            if !extracted_images.is_empty() {
                if tx_images_clone.send(extracted_images).is_err() {
                    break;
                }
            }
        }
        drop(tx_images_clone);
    });
    drop(tx_images); // Ensure last tx_images drops when worker finishes

    // 3. Collector / Writer
    let mut all_images = Vec::new();
    while let Ok(images) = rx_images.recv() {
        all_images.extend(images);
    }

    io_handle.join().unwrap()?;
    worker_handle.join().unwrap();

    let count = total_count.load(Ordering::Relaxed);
    let res = res_count.load(Ordering::Relaxed);
    let req = req_count.load(Ordering::Relaxed);
    let meta = meta_count.load(Ordering::Relaxed);

    let parquet_start = Instant::now();
    write_images_to_parquet(&all_images, parquet_path);
    let parquet_time = parquet_start.elapsed();

    print_benchmark_report(
        "Multi-Threaded Parallel (Rayon Pipeline)",
        count as usize,
        res as usize,
        req as usize,
        meta as usize,
        all_images.len() as i32,
        std::time::Duration::from_secs(0), // I/O and parse are overlapped
        std::time::Duration::from_secs(0),
        parquet_time,
        total_start.elapsed(),
    );

    Ok(())
}

fn print_benchmark_report(
    pipeline_name: &str,
    total_count: usize,
    res_count: usize,
    req_count: usize,
    meta_count: usize,
    images_count: i32,
    read_time: std::time::Duration,
    parse_time: std::time::Duration,
    parquet_time: std::time::Duration,
    total_elapsed: std::time::Duration,
) {
    let elapsed_sec = total_elapsed.as_secs_f64();
    let records_per_sec = if elapsed_sec > 0.0 {
        total_count as f64 / elapsed_sec
    } else {
        0.0
    };

    println!("\n================== BENCHMARK REPORT ==================");
    println!("Pipeline:           {}", pipeline_name);
    println!("Total Records:      {}", total_count);
    println!("  - Responses:      {}", res_count);
    println!("  - Requests:       {}", req_count);
    println!("  - Metadata:       {}", meta_count);
    println!("Images Extracted:   {}", images_count);
    if read_time.as_secs_f64() > 0.0 {
        println!("WARC Read Time:     {:?}", read_time);
    }
    if parse_time.as_secs_f64() > 0.0 {
        println!("HTML Parse Time:    {:?}", parse_time);
    }
    println!("Parquet Write Time: {:?}", parquet_time);
    println!("------------------------------------------------------");
    println!("TOTAL ELAPSED TIME: {:?}", total_elapsed);
    println!("THROUGHPUT:         {:.1} records/sec", records_per_sec);
    println!("======================================================\n");
}
