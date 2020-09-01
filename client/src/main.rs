mod transfer;

extern crate base64;
extern crate openssl;

use crate::transfer::transfer::{Download, Upload};
use std::convert::TryInto;
use std::env::args;
use std::fs::File;
use std::io::{stdout, Read, Write};
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::exit;

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

fn upload<A: ToSocketAddrs>(addr: A, filepath: PathBuf, encrypt: bool) {
    let file = File::open(&filepath);
    if file.is_err() {
        println!("Failed to open file: {}", file.err().unwrap());
        exit(1);
    }

    let mut file = file.unwrap();

    let mut upload = Upload::new(addr, encrypt).unwrap();
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
    let mut download = Download::new(addr, &download_key[..32].try_into().unwrap(), key).unwrap();
    let mut file = File::create("output").unwrap();
    loop {
        let (buf, cont) = download.read().unwrap();
        if !cont {
            break;
        }

        file.write_all(&buf).unwrap();
    }
}
