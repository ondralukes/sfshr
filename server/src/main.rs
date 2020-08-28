mod thread_pool;

extern crate simpletcp;

use std::fs;

use crate::thread_pool::thread_pool::ThreadPool;
use simpletcp::simpletcp::TcpServer;

fn main() {
    fs::create_dir_all("uploads/").unwrap();
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
