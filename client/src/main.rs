mod transfer;

extern crate base64;
extern crate openssl;

use crate::transfer::transfer::Upload;
use std::env::args;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::exit;

const BUFFER_SIZE: usize = 1024 * 1024;

fn main() {
    let mut args = args();
    args.next();

    let mut encrypt = true;
    let mut filepath = None;

    loop {
        match args.next() {
            None => {
                break;
            }
            Some(arg) => {
                if arg == "-n" || arg == "--no-encryption" {
                    encrypt = false;
                } else {
                    filepath = Some(PathBuf::from(arg));
                }
            }
        }
    }

    if filepath.is_none() {
        println!("No file specified!");
        exit(1);
    }
    let filepath = filepath.unwrap();

    let file = File::open(&filepath);
    if file.is_err() {
        println!("Failed to open file: {}", file.err().unwrap());
        exit(1);
    }

    let mut file = file.unwrap();

    let mut upload = Upload::new("localhost:40788", encrypt).unwrap();
    let mut buf = Vec::new();
    buf.resize(BUFFER_SIZE, 0);
    loop {
        let bytes_read = file.read(&mut buf).unwrap();
        if bytes_read == 0 {
            break;
        }

        upload.send(&buf[..bytes_read]).unwrap();
    }

    upload.finalize().unwrap();

    let mut download_key = Vec::new();
    download_key.extend_from_slice(&upload.id());
    if encrypt {
        download_key.extend_from_slice(upload.key().unwrap());
    }

    println!("sfshr r {}", base64::encode(&download_key));
}
