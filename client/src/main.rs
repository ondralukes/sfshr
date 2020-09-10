mod transfer;

extern crate base64;
extern crate openssl;
extern crate tar;

use crate::transfer::transfer::{Download, TransferError, Upload};
use std::convert::TryInto;
use std::env::args;
use std::fs;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::exit;
use tar::{Archive, Builder};

macro_rules! printinfoln {
    ($q:expr, $($x:expr), *) => {
        if !$q {
        println!($($x,)*);
        }
    };
}

macro_rules! printinfo {
    ($q:expr, $($x:expr), *) => {
        if !$q {
        print!($($x,)*);
        }
    };
}

fn main() {
    let mut args = args();
    args.next();

    let mut encrypt = true;
    let mut receive = false;
    let mut quiet = false;
    let mut main_arg = None;
    let mut server = String::from("ondralukes.cz:40788");

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
                } else if arg == "--help" {
                    println!("Usage: sfshr [file] or sfshr -r [download key]");
                    println!(" -r [download key] - download file");
                    println!(" -n --no-encryption - do not encrypt or decrypt the file");
                    println!(" -q --quiet - do not print anything (expect download key)");
                    println!(" -s --server [hostname:port] - specify sfshr server (default: 'ondralukes.cz:40788')");
                    exit(0);
                } else if arg == "-q" || arg == "--quiet" {
                    quiet = true;
                } else if arg == "-s" || arg == "--server" {
                    match args.next() {
                        None => {
                            println!("Expected value for --server");
                            exit(1);
                        }
                        Some(val) => {
                            server = val;
                        }
                    }
                } else {
                    main_arg = Some(arg);
                }
            }
        }
    }

    if !receive {
        if main_arg.is_none() {
            printinfoln!(quiet, "No file specified!");
            exit(1);
        }

        let path = PathBuf::from(main_arg.unwrap());
        upload(server, path, encrypt, quiet);
    } else {
        if main_arg.is_none() {
            printinfoln!(quiet, "No download key specified!");
            exit(1);
        }
        let download_key = base64::decode(main_arg.unwrap().as_bytes());
        if download_key.is_err() {
            printinfoln!(quiet, "Invalid download key format!");
            exit(1);
        }
        download(server, download_key.unwrap(), encrypt, quiet);
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
        if !self.is_normal() {
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

fn upload(addr: String, mut filepath: PathBuf, encrypt: bool, _quiet: bool) {
    let mut upload = Upload::new(&addr, encrypt).unwrap_or_else(on_error);
    let mut archive = Builder::new(upload);

    filepath = filepath.canonicalize().unwrap();
    let root_path = filepath.components().last().unwrap();
    archive.append_dir_all(root_path, &filepath).unwrap();
    upload = archive.into_inner().unwrap();

    upload.finalize().unwrap_or_else(on_error);

    let mut download_key = Vec::new();
    download_key.extend_from_slice(&upload.id());
    if encrypt {
        download_key.extend_from_slice(upload.key().unwrap());
    }

    let mut server_extra = String::new();
    if &addr != "ondralukes.cz:40788" {
        server_extra = format!(" --server {}", addr);
    }
    if encrypt {
        println!("sfshr{} -r {}", server_extra, base64::encode(&download_key));
    } else {
        println!(
            "sfshr --no-encryption{} -r {}",
            server_extra,
            base64::encode(&download_key)
        );
    }
}

fn download<A: ToSocketAddrs>(addr: A, download_key: Vec<u8>, encrypt: bool, quiet: bool) {
    if (encrypt && download_key.len() != 64) || (!encrypt && download_key.len() != 32) {
        printinfoln!(quiet, "Invalid download key size!");
        exit(1);
    }

    let mut key: Option<[u8; 32]> = None;
    if encrypt {
        key = Some(download_key[32..].try_into().unwrap());
    }
    let download =
        Download::new(addr, &download_key[..32].try_into().unwrap(), key).unwrap_or_else(on_error);

    let mut archive = Archive::new(download);
    archive.unpack(".").unwrap();

    printinfoln!(quiet, "");
    printinfoln!(quiet, "\x1b[1A\x1b[0G\x1b[KSuccesfully downloaded");
}

fn on_error<T>(err: TransferError) -> T {
    let temp = Path::new(".sfshr-temp");
    if temp.exists() {
        fs::remove_file(temp).unwrap();
    }
    println!("\x1b[31mTerminating due to an error ({})\x1b[0m", err);
    exit(1);
}
