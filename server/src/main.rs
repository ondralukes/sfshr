mod thread_pool;

extern crate simpletcp;

use std::fs;

use crate::thread_pool::thread_pool::ThreadPool;
use simpletcp::simpletcp::TcpServer;
use std::thread::{sleep, spawn};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::fs::File;
use std::io::Read;

fn main() {
    fs::create_dir_all("uploads/").unwrap();

    spawn(|| {
        file_checker();
    });
    let mut pool = ThreadPool::new(8);
    let server = TcpServer::new("0.0.0.0:40788").unwrap();

    loop {
        match server.accept_blocking() {
            Ok(socket) => {
                pool.accept(socket);
            }
            Err(_) => {}
        }
    }
}

fn file_checker() {
    loop {
        let dir = fs::read_dir("uploads/").unwrap();
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
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
