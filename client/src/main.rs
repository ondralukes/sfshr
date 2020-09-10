#[macro_use]
mod transfer;

extern crate base64;
extern crate openssl;
extern crate tar;

use crate::transfer::transfer::{Download, Upload};
use std::convert::TryInto;
use std::env::args;
use std::fs::File;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::{fmt, fs, io};
use tar::{Archive, Builder};

fn main() {
    let mut args = args();
    args.next();

    let mut encrypt = true;
    let mut receive = false;
    let mut keep_tar = None;
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
                    println!(" -t --tar [tarname] - store downloaded tar as [tarname], instead of unpacking it");
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
                } else if arg == "-t" || arg == "--tar" {
                    match args.next() {
                        None => {
                            println!("Expected value for --tar");
                            exit(1);
                        }
                        Some(val) => {
                            keep_tar = Some(val);
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
        upload(server, path, encrypt, quiet, keep_tar);
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
        download(server, download_key.unwrap(), encrypt, quiet, keep_tar);
    }
}

fn upload(
    addr: String,
    mut filepath: PathBuf,
    encrypt: bool,
    quiet: bool,
    keep_tar: Option<String>,
) {
    let size = dir_size(&filepath);
    let mut upload = Upload::new(&addr, encrypt, quiet, size).unwrap_or_else(on_error);
    let mut archive = Builder::new(upload);

    match filepath.canonicalize() {
        Ok(p) => {
            filepath = p;
        }
        Err(err) => {
            printinfoln!(quiet, "Failed to open file: {}", err);
            exit(1);
        }
    }
    filepath = filepath.canonicalize().unwrap();
    let root_path = filepath.components().last().unwrap();
    if filepath.is_dir() {
        archive
            .append_dir_all(root_path, &filepath)
            .unwrap_or_else(on_error);
    } else {
        archive
            .append_file(
                root_path,
                &mut File::open(&filepath).unwrap_or_else(on_error),
            )
            .unwrap_or_else(on_error);
    }

    upload = archive.into_inner().unwrap_or_else(on_error);

    upload.finalize().unwrap_or_else(on_error);

    let mut download_key = Vec::new();
    download_key.extend_from_slice(&upload.id());
    if encrypt {
        download_key.extend_from_slice(upload.key().unwrap());
    }

    let mut extras = String::new();
    if &addr != "ondralukes.cz:40788" {
        extras = format!(" --server {}", addr);
    }

    if keep_tar.is_some() {
        extras.push_str(&format!(" --tar {}", keep_tar.unwrap()));
    }
    if encrypt {
        println!("sfshr{} -r {}", extras, base64::encode(&download_key));
    } else {
        println!(
            "sfshr --no-encryption{} -r {}",
            extras,
            base64::encode(&download_key)
        );
    }
}

fn dir_size(path: &PathBuf) -> usize {
    if path.is_file() {
        return path.metadata().unwrap().len() as usize;
    }
    let mut size = 0;
    let dir = fs::read_dir(path).unwrap();
    for entry in dir {
        let entry = entry.unwrap();
        size += dir_size(&entry.path());
    }
    size
}

fn download<A: ToSocketAddrs>(
    addr: A,
    download_key: Vec<u8>,
    encrypt: bool,
    quiet: bool,
    keep_tar: Option<String>,
) {
    if (encrypt && download_key.len() != 64) || (!encrypt && download_key.len() != 32) {
        printinfoln!(quiet, "Invalid download key size!");
        exit(1);
    }

    let mut key: Option<[u8; 32]> = None;
    if encrypt {
        key = Some(download_key[32..].try_into().unwrap());
    }
    let mut download = Download::new(addr, &download_key[..32].try_into().unwrap(), key, quiet)
        .unwrap_or_else(on_error);

    match keep_tar {
        None => {
            let mut archive = Archive::new(download);
            let mut iter = archive.entries().unwrap();
            let mut first = iter.next().unwrap().unwrap();

            let archive_root;

            let first_path = Path::new(".").join(first.path().unwrap());
            let mut fp_iter = first_path.iter();

            //Skip '.'
            fp_iter.next();
            let first_path = Path::new(fp_iter.next().unwrap());
            if first_path.exists() {
                printinfoln!(
                    quiet,
                    "Cannot write to {:?}. Destination already exists.",
                    first_path
                );
                exit(1);
            } else {
                archive_root = first_path.to_str().unwrap().to_string();
            }

            first.unpack_in(".").unwrap();

            for entry in iter {
                let mut entry = entry.unwrap();
                entry.unpack_in(".").unwrap();
            }

            printinfoln!(quiet, "");
            printinfoln!(
                quiet,
                "\x1b[1A\x1b[0G\x1b[KSuccesfully downloaded {:?}",
                archive_root
            );
        }
        Some(dest) => {
            let mut file = File::create(&dest).unwrap_or_else(on_error);
            io::copy(&mut download, &mut file).unwrap_or_else(on_error);
            printinfoln!(
                quiet,
                "\x1b[1A\x1b[0G\x1b[KSuccesfully downloaded {:?}",
                dest
            );
        }
    }
}

fn on_error<E: fmt::Display, T>(err: E) -> T {
    let temp = Path::new(".sfshr-temp");
    if temp.exists() {
        fs::remove_file(temp).unwrap();
    }
    println!("\x1b[31mTerminating due to an error ({})\x1b[0m", err);
    exit(1);
}
