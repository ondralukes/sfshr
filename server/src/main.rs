mod config;
mod thread_pool;

extern crate simpletcp;

use std::fs;

use crate::config::config::Config;
use crate::thread_pool::thread_pool::ThreadPool;
use simpletcp::simpletcp::TcpServer;
use std::env::args;
use std::fs::File;
use std::io::Read;
use std::process::exit;
use std::thread::{sleep, spawn};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
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
    spawn(move || {
        file_checker(cfg_clone);
    });
    let mut pool = ThreadPool::new(&cfg);
    let server = TcpServer::new("0.0.0.0:40788").unwrap();
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

fn file_checker(config: Config) {
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
                    fs::remove_file(entry.path()).unwrap();
                    println!("{:?} expired", entry.path());
                }
            }
        }
        sleep(Duration::from_secs(5));
    }
}
