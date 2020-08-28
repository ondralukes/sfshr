pub mod thread_pool {
    extern crate hex;
    extern crate rand;
    extern crate simpletcp;

    use self::rand::prelude::StdRng;
    use self::rand::{RngCore, SeedableRng};
    use crate::thread_pool::thread_pool::ThreadMessage::Accept;
    use simpletcp::simpletcp::{Error, Message, MessageError, TcpStream};
    use std::fs::{remove_file, File};
    use std::io;
    use std::io::{Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use std::string::FromUtf8Error;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::{Acquire, Release};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Arc;
    use std::thread::{spawn, JoinHandle};

    pub struct ThreadPool {
        threads: Vec<Thread>,
    }

    impl ThreadPool {
        pub fn new(size: usize) -> ThreadPool {
            let mut res = ThreadPool {
                threads: Vec::new(),
            };
            for i in 0..size {
                let (tx, rx): (Sender<ThreadMessage>, Receiver<ThreadMessage>) = channel();
                let sockets_alive = Arc::new(AtomicUsize::new(0));
                let sockets_alive_clone = sockets_alive.clone();
                let join_handle = spawn(move || {
                    thread_loop(i, rx, sockets_alive_clone);
                });
                res.threads.push(Thread {
                    join_handle,
                    sender: tx,
                    sockets_alive,
                });
            }

            res
        }

        pub fn accept(&mut self, socket: TcpStream) {
            let mut min = usize::max_value();
            let mut selected = &self.threads[0];

            for t in &self.threads {
                let sockets_alive = t.sockets_alive.load(Acquire);
                if sockets_alive < min {
                    min = sockets_alive;
                    selected = t;
                }
            }

            selected.sender.send(Accept(socket)).unwrap();
        }
    }

    impl Drop for ThreadPool {
        fn drop(&mut self) {
            println!("Terminating threads...");
            while !self.threads.is_empty() {
                let thread = self.threads.pop().unwrap();
                thread.sender.send(ThreadMessage::Terminate).unwrap();
                thread.join_handle.join().unwrap();
            }
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

    struct Thread {
        join_handle: JoinHandle<()>,
        sender: Sender<ThreadMessage>,
        sockets_alive: Arc<AtomicUsize>,
    }

    enum ThreadMessage {
        Terminate,
        Accept(TcpStream),
    }

    enum TransferError {
        InvalidMessage,
        IOError,
        NetworkError,
    }

    impl From<FromUtf8Error> for TransferError {
        fn from(_: FromUtf8Error) -> Self {
            TransferError::InvalidMessage
        }
    }

    impl From<MessageError> for TransferError {
        fn from(_: MessageError) -> Self {
            TransferError::InvalidMessage
        }
    }

    impl From<io::Error> for TransferError {
        fn from(_: io::Error) -> Self {
            TransferError::IOError
        }
    }

    impl From<Error> for TransferError {
        fn from(_: Error) -> Self {
            TransferError::NetworkError
        }
    }

    struct Client {
        socket: TcpStream,
        state: ClientState,
    }

    impl Client {
        fn new(socket: TcpStream) -> Self {
            Self {
                socket,
                state: ClientState::Idle,
            }
        }
    }

    enum ClientState {
        Idle,
        Upload(Upload),
    }

    struct Upload {
        file: File,
        id: [u8; 32],
    }

    impl Upload {
        fn begin() -> Result<Self, TransferError> {
            let mut id = [0; 32];
            StdRng::from_entropy().fill_bytes(&mut id);

            let mut path = PathBuf::from("uploads");
            path.push(hex::encode(id));
            let file = File::create(path)?;

            Ok(Self { file, id })
        }
    }

    fn thread_loop(
        thread_id: usize,
        receiver: Receiver<ThreadMessage>,
        sockets_alive: Arc<AtomicUsize>,
    ) {
        let mut clients = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(message) => match message {
                    ThreadMessage::Terminate => {
                        break;
                    }
                    ThreadMessage::Accept(socket) => {
                        clients.push(Client::new(socket));
                        sockets_alive.store(clients.len(), Release);
                    }
                },
                Err(_) => {}
            }

            let mut i = 0;
            while i < clients.len() {
                let mut remove = false;
                let client = &mut clients[i];
                match client.socket.read() {
                    Ok(msg) => match msg {
                        Some(mut msg) => match process_message(client, &mut msg) {
                            Ok(_) => {}
                            Err(_) => {
                                remove = true;
                            }
                        },
                        _ => {}
                    },
                    Err(err) => match err {
                        Error::NotReady => match client.socket.get_ready() {
                            Ok(_) => {}
                            Err(_) => {
                                remove = true;
                            }
                        },
                        _ => {
                            remove = true;
                        }
                    },
                }
                if client.socket.flush().is_err() {
                    remove = true;
                }

                if remove {
                    match &client.state {
                        ClientState::Idle => {}
                        ClientState::Upload(upload) => {
                            println!("[{}] Interrupted!", hex::encode(upload.id));
                            let mut path = PathBuf::from("uploads");
                            path.push(hex::encode(upload.id));
                            match remove_file(path) {
                                Err(io_err) => {
                                    println!(
                                        "[{}] Failed to remove file: {:?}",
                                        hex::encode(upload.id),
                                        io_err
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    clients.remove(i);
                    sockets_alive.store(clients.len(), Release);
                } else {
                    i += 1;
                }
            }
        }

        println!("[Thread #{}] Terminating.", thread_id);
    }

    fn process_message(client: &mut Client, msg: &mut Message) -> Result<(), TransferError> {
        let state = &mut client.state;
        let mut new_state = None;
        match state {
            ClientState::Idle => {
                let command = msg.read_i32()?;
                match command {
                    0 => {
                        let upload = Upload::begin()?;

                        let mut response = Message::new();
                        response.write_buffer(&upload.id);
                        client.socket.write(&response)?;
                        println!("[{}] Begun transfer.", hex::encode(upload.id));
                        new_state = Some(ClientState::Upload(upload));
                    }
                    _ => {}
                }
            }
            ClientState::Upload(upload) => {
                let cont = msg.read_u8()?;
                if cont == 0 {
                    println!("[{}] Completed", hex::encode(upload.id));
                    let mut confirm_msg = Message::new();
                    confirm_msg.write_u8(1);
                    client.socket.write(&confirm_msg)?;
                    new_state = Some(ClientState::Idle);
                } else {
                    let buffer = msg.read_buffer()?;
                    upload.file.write_all(&buffer)?;
                    let position = upload.file.seek(SeekFrom::Current(0))?;
                    println!("[{}] {}", hex::encode(upload.id), position.format_size());
                }
            }
        }

        match new_state {
            None => {}
            Some(new) => {
                client.state = new;
            }
        }
        Ok(())
    }
}
