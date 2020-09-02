mod transfer;

extern crate base64;
extern crate openssl;

use crate::transfer::transfer::{Download, Upload, TransferError};
use std::convert::TryInto;
use std::env::args;
use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom, stdin, stdout};
use std::net::ToSocketAddrs;
use std::path::{PathBuf, Path};
use std::process::exit;
use std::time::Instant;
use std::fs;

const BUFFER_SIZE: usize = 1024 * 1024;

fn main() {
    let mut args = args();
    args.next();

    let mut encrypt = true;
    let mut receive = false;
    let mut main_arg = None;

    loop {
        match args.next() {
            None => {
                break;
            }
            Some(arg) => {
                if arg == "-n" || arg == "--no-encryption" {
                    encrypt = false;
                } else if arg == "-r" {
                    receive = true;
                } else {
                    main_arg = Some(arg);
                }
            }
        }
    }

    if !receive {
        if main_arg.is_none() {
            println!("No file specified!");
            exit(1);
        }

        let path = PathBuf::from(main_arg.unwrap());
        upload("localhost:40788", path, encrypt);
    } else {
        if main_arg.is_none() {
            println!("No download key specified!");
            exit(1);
        }
        let download_key = base64::decode(main_arg.unwrap().as_bytes());
        if download_key.is_err() {
            println!("Invalid download key format!");
            exit(1);
        }
        download("localhost:40788", download_key.unwrap(), encrypt);
    }
}

trait FormatSize {
    fn format_size(self) -> String;
}

impl FormatSize for u64 {
    fn format_size(self) -> String {
        const PREFIXES: [&str; 5] = ["", "Ki", "Mi", "Gi", "Ti"];

        let mut i: f64 = 1.0;
        let mut index = 0;
        loop {
            if (self as f64 / i) < 500.0 {
                break;
            }

            i *= 1024.0;
            index += 1;
        }

        format!("{:.3} {}B", self as f64 / i, PREFIXES[index])
    }
}

impl FormatSize for f64 {
    fn format_size(self) -> String {
        const PREFIXES: [&str; 5] = ["", "Ki", "Mi", "Gi", "Ti"];

        let mut i: f64 = 1.0;
        let mut index = 0;
        if !self.is_normal(){
            return String::from("NaN");
        }
        loop {
            if (self / i) < 500.0 {
                break;
            }

            i *= 1024.0;
            index += 1;
        }

        format!("{:.3} {}B", self as f64 / i, PREFIXES[index])
    }
}

fn upload<A: ToSocketAddrs>(addr: A, filepath: PathBuf, encrypt: bool) {
    let file = File::open(&filepath);
    if file.is_err() {
        println!("Failed to open file: {}", file.err().unwrap());
        exit(1);
    }

    let mut file = file.unwrap();

    let mut upload = Upload::new(addr, encrypt).unwrap_or_else(on_error);

    upload.write_filename(filepath.file_name().unwrap().to_str().unwrap()).unwrap_or_else(on_error);
    let mut buf = Vec::new();
    buf.resize(BUFFER_SIZE, 0);

    let time = Instant::now();
    loop {
        let bytes_read = file.read(&mut buf).unwrap();
        if bytes_read == 0 {
            break;
        }

        upload.send(&buf[..bytes_read]).unwrap_or_else(on_error);
        let time = time.elapsed().as_micros() as f64;
        let size = file.seek(SeekFrom::Current(0)).unwrap();
        let speed = size as f64/time*1000000.0;

        println!("Uploaded {:^12} @ {:^12}  \x1b[1A\x1b[0G", size.format_size(), format!("{}/s", speed.format_size()));
    }

    upload.finalize().unwrap_or_else(on_error);
    let time = time.elapsed().as_micros() as f64;
    let size = file.seek(SeekFrom::Current(0)).unwrap();
    let speed = size as f64/time*1000000.0;

    println!("Uploaded {:^12} @ {:^12}  \x1b[1A\x1b[0G", size.format_size(), format!("{}/s",  speed.format_size()));



    let mut download_key = Vec::new();
    download_key.extend_from_slice(&upload.id());
    if encrypt {
        download_key.extend_from_slice(upload.key().unwrap());
    }

    if encrypt {
        println!("sfshr -r {}", base64::encode(&download_key));
    } else {
        println!("sfshr --no-encryption -r {}", base64::encode(&download_key));
    }
}

fn download<A: ToSocketAddrs>(addr: A, download_key: Vec<u8>, encrypt: bool) {
    if (encrypt && download_key.len() != 64) || (!encrypt && download_key.len() != 32) {
        println!("Invalid download key size!");
        exit(1);
    }

    let mut key: Option<[u8; 32]> = None;
    if encrypt {
        key = Some(download_key[32..].try_into().unwrap());
    }
    let mut download = Download::new(addr, &download_key[..32].try_into().unwrap(), key).unwrap_or_else(on_error);
    let mut file = File::create(".sfshr-temp").unwrap();
    let time = Instant::now();
    loop {
        let (buf, cont) = download.read().unwrap_or_else(on_error);
        if !cont {
            break;
        }

        file.write_all(&buf).unwrap();
        let time = time.elapsed().as_micros() as f64;
        let size = file.seek(SeekFrom::Current(0)).unwrap();
        let speed = size as f64/time*1000000.0;

        println!("Downloaded {:^12} @ {:^12}\x1b[1A\x1b[0G", size.format_size(), format!("{}/s", speed.format_size()));
    }
    println!();

    let filename = download.filename();
    if Path::new(&filename).exists() {
        loop {
            print!("\x1b[1A\x1b[0G\x1b[KFile {} already exists. Replace? [yes/no]", filename);
            stdout().flush().unwrap();
            let stdin = stdin();
            let mut line = String::new();
            stdin.read_line(&mut line).unwrap();
            line.retain(|c| {
                c != '\n' && c != '\r'
            });
            if line == "yes" {
                break;
            }
            if line == "no" {
                fs::remove_file(".sfshr-temp").unwrap();
                println!("\x1b[1A\x1b[0G\x1b[KAborted");
                exit(1);
            }
        }
    }
    fs::rename(".sfshr-temp", &filename).unwrap();
    println!("\x1b[1A\x1b[0G\x1b[KSuccesfully downloaded {}", &filename);
}

fn on_error<T>(_: TransferError) -> T {
    println!("\x1b[31mTerminating due to an error\x1b[0m");
    exit(1);
}