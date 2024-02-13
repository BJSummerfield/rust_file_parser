use chrono::{LocalResult, TimeZone, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use rayon::prelude::*;
use serde_json::{self, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

fn process_file(file_path: &Path, base_output_dir: &Path) -> io::Result<()> {
    println!("Processing file: {:?}", file_path);

    let file = File::open(file_path)?;
    let gz = GzDecoder::new(file);
    let reader = BufReader::new(gz);

    let mut writers: HashMap<PathBuf, GzEncoder<BufWriter<File>>> = HashMap::new();

    for line in reader.lines().filter_map(Result::ok) {
        let mut log = match serde_json::from_str::<Value>(&line) {
            Ok(log) => log,
            Err(e) => {
                eprintln!("Failed to parse JSON from line, skipping. Error: {}", e);
                continue;
            }
        };

        if let Some(raw_str) = log["raw"].as_str() {
            if let Some((timestamp_str, _)) = raw_str.split_once(',') {
                if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                    let corrected_timestamp = match Utc.timestamp_opt(timestamp, 0) {
                        LocalResult::Single(dt) => dt.timestamp_millis(),
                        _ => {
                            eprintln!("Invalid timestamp encountered: {}", timestamp_str);
                            continue;
                        }
                    };
                    log["timestamp"] = Value::Number(serde_json::Number::from(corrected_timestamp));
                }
            }
        }

        // Determine the output path based on `cimcompliance` and create the writer
        let is_cim = log.get("cimcompliance").map_or(false, |v| v == "cim");
        let dir_name = if is_cim { "cim" } else { "non_cim" };
        let output_dir = base_output_dir.join(dir_name);
        fs::create_dir_all(&output_dir)?;

        let output_file_path = output_dir.join(
            file_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .replace(".gz", "_processed.gz"),
        );

        let writer = writers.entry(output_file_path.clone()).or_insert_with(|| {
            let file = File::create(&output_file_path).expect("Failed to create output file");
            let buf_writer = BufWriter::new(file);
            GzEncoder::new(buf_writer, Compression::default())
        });

        writeln!(writer, "{}", serde_json::to_string(&log).unwrap())
            .expect("Failed to write to file");
    }

    // Ensure all writers are flushed and finished
    for (path, writer) in writers.iter_mut() {
        writer.try_finish()?;
        println!("Finished writing to {:?}", path);
    }

    Ok(())
}

fn main() -> io::Result<()> {
    println!("Starting processing");

    let input_dir = Path::new("./dev");
    let base_output_dir = Path::new("./corrected");

    let files: Vec<_> = fs::read_dir(input_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().map_or(false, |ext| ext == "gz"))
        .collect();

    println!("Found {} files to process", files.len());

    files.par_iter().for_each(|file_path| {
        if let Err(e) = process_file(file_path, base_output_dir) {
            eprintln!("Error processing file {:?}: {}", file_path.display(), e);
        }
    });

    println!("All files processed");

    Ok(())
}
