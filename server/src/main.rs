mod config;
mod thread_pool;

extern crate simpletcp;

use std::fs;

use crate::config::config::Config;
use crate::thread_pool::thread_pool::{FormatSize, ThreadPool};
use simpletcp::simpletcp::TcpServer;
use std::env::args;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
    let total_size = Arc::new(Mutex::new(0));
    let mut args = args().into_iter();
    let mut config_file = String::from("config");
    loop {
        let arg = args.next();
        if arg.is_none() {
            break;
        }
        let arg = arg.unwrap();

        if arg == "--config" || arg == "-c" {
            let value = args.next();
            if value.is_none() {
                println!("Expected value for option {}!", arg);
                exit(1);
            }
            config_file = value.unwrap();
        }
    }
    let cfg = Config::new(config_file);
    fs::create_dir_all(cfg.uploads()).unwrap();

    let cfg_clone = cfg.clone();
    let total_size_clone = total_size.clone();
    spawn(move || {
        file_checker(cfg_clone, total_size_clone);
    });
    let mut pool = ThreadPool::new(&cfg, &total_size);

    let key;
    let key_file = Path::new(cfg.key_file());
    if key_file.exists() {
        let mut key_file = File::open(key_file).unwrap();
        let mut buf = Vec::new();
        key_file.read_to_end(&mut buf).unwrap();
        key = Some(buf);
    } else {
        key = None;
    }
    let server = TcpServer::new_with_key("0.0.0.0:40788", key.as_deref()).unwrap();
    if key.is_none() {
        let mut key_file = File::create(key_file).unwrap();
        key_file.write_all(&server.key()).unwrap();
    }
    println!("ready");
    loop {
        match server.accept_blocking() {
            Ok(socket) => {
                pool.accept(socket);
            }
            Err(_) => {}
        }
    }
}

fn file_checker(config: Config, total_size: Arc<Mutex<u64>>) {
    let mut prev_total_size = 0;
    loop {
        let dir = fs::read_dir(config.uploads()).unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        for entry in dir {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                let mut file = File::open(entry.path()).unwrap();
                let mut bytes = [0; 8];
                file.read_exact(&mut bytes).unwrap();

                let expiration = u64::from_le_bytes(bytes);

                if expiration < timestamp {
                    let mut total_size = total_size.lock().unwrap();
                    *total_size -= file.metadata().unwrap().len();
                    fs::remove_file(entry.path()).unwrap();
                    println!("{:?} expired", entry.path());
                }
            }
        }

        {
            let total_size = total_size.lock().unwrap();
            if *total_size != prev_total_size {
                prev_total_size = *total_size;
                let percentage = (*total_size) as f64 / config.max_total_size() as f64 * 100.0;
                println!(
                    "Space used {} of {} ({:.2}%).",
                    (*total_size).format_size(),
                    config.max_total_size().format_size(),
                    percentage
                );
            }
        }

        sleep(Duration::from_secs(5));
    }
}
