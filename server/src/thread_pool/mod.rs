pub mod thread_pool {
    extern crate hex;
    extern crate rand;
    extern crate simpletcp;

    use self::rand::prelude::StdRng;
    use self::rand::{RngCore, SeedableRng};
    use crate::thread_pool::thread_pool::ThreadMessage::Accept;
    use simpletcp::simpletcp::{Error, Message, MessageError, TcpStream};
    use std::fs::{remove_file, File};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use std::string::FromUtf8Error;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::{Acquire, Release};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Arc;
    use std::thread::{spawn, JoinHandle};
    use std::{fmt, io};

    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;

    use crate::config::config::Config;
    use simpletcp::utils::{EV_POLLIN, EV_POLLOUT};
    use std::fmt::{Display, Formatter};
    #[cfg(windows)]
    use std::os::unix::io::AsRawSocket;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub struct ThreadPool<'a> {
        threads: Vec<Thread>,
        _config: &'a Config,
    }

    impl<'a> ThreadPool<'a> {
        pub fn new(config: &'a Config) -> ThreadPool {
            let mut res = ThreadPool {
                threads: Vec::new(),
                _config: config,
            };
            for i in 0..config.thread_count() {
                let (tx, rx): (Sender<ThreadMessage>, Receiver<ThreadMessage>) = channel();
                let sockets_alive = Arc::new(AtomicUsize::new(0));
                let sockets_alive_clone = sockets_alive.clone();
                let config_clone = config.clone();
                let join_handle = spawn(move || {
                    thread_loop(i, config_clone, rx, sockets_alive_clone);
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

    impl Drop for ThreadPool<'_> {
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
        SizeLimitExceeded,
    }

    impl Display for TransferError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match self {
                TransferError::InvalidMessage => {
                    f.write_str("TransferError::InvalidMessage: Received an invalid message")
                },
                TransferError::IOError => {
                    f.write_str("TransferError::IOError: An error occurred during a file operation. Maybe your file expired?")
                },
                TransferError::NetworkError => {
                    f.write_str("TransferError::NetworkError")
                },
                TransferError::SizeLimitExceeded => {
                    f.write_str("TransferError::SizeLimitExceeded")
                }
            }
        }
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

    struct Client<'a> {
        socket: TcpStream,
        state: ClientState,
        config: &'a Config,
    }

    impl<'a> Client<'a> {
        fn new(socket: TcpStream, config: &'a Config) -> Self {
            Self {
                socket,
                state: ClientState::Idle,
                config,
            }
        }

        fn read_and_process(&mut self) -> Result<(), TransferError> {
            match self.socket.read() {
                Ok(msg) => match msg {
                    Some(mut msg) => match self.process_message(&mut msg) {
                        Ok(_) => {}
                        Err(err) => {
                            return Err(err);
                        }
                    },
                    _ => {}
                },
                Err(err) => match err {
                    Error::NotReady => match self.socket.get_ready() {
                        Ok(_) => {}
                        Err(_) => {
                            return Err(TransferError::from(err));
                        }
                    },
                    _ => {
                        return Err(TransferError::from(err));
                    }
                },
            }
            Ok(())
        }

        fn process_message(&mut self, msg: &mut Message) -> Result<(), TransferError> {
            let mut new_state = None;
            match &mut self.state {
                ClientState::Idle => {
                    let command = msg.read_i32()?;
                    match command {
                        0 => {
                            let upload = Upload::begin(self.config)?;

                            let mut response = Message::new();
                            response.write_buffer(&upload.id);
                            response.write_u64(self.config.max_size());
                            self.socket.write(&response)?;
                            println!("[{}] Begin upload.", hex::encode(upload.id));
                            new_state = Some(ClientState::Upload(upload));
                        }
                        1 => {
                            let id = msg.read_buffer()?;
                            let hex_id = hex::encode(&id);
                            let download = Download::begin(self.config, id)?;
                            println!("[{}] Begin download.", hex_id);
                            new_state = Some(ClientState::Download(download));
                        }
                        _ => {}
                    }
                }
                ClientState::Upload(upload) => {
                    let cont = msg.read_u8()?;
                    if cont == 0 {
                        println!("[{}] Completed", hex::encode(upload.id));
                        let mut confirm_msg = Message::new();
                        confirm_msg.write_i8(1);
                        self.socket.write(&confirm_msg)?;
                        new_state = Some(ClientState::Idle);
                    } else {
                        let buffer = msg.read_buffer()?;
                        upload.write(&buffer)?;
                        let position = upload.position()?;
                        if position > self.config.max_size() {
                            return Err(TransferError::SizeLimitExceeded);
                        }
                        println!(
                            "[{}] Uploaded {}",
                            hex::encode(upload.id),
                            position.format_size()
                        );
                    }
                }
                _ => {}
            }

            match new_state {
                None => {}
                Some(new) => {
                    self.state = new;
                }
            }
            Ok(())
        }

        fn flush_and_process(&mut self, buffer: &mut Vec<u8>) -> Result<(), TransferError> {
            let flushed = self.socket.flush()?;

            if flushed {
                let mut new_state = None;

                match &mut self.state {
                    ClientState::Download(download) => {
                        let mut message = Message::new();

                        let bytes_read = download.read(buffer)?;
                        if bytes_read != 0 {
                            message.write_i8(1);
                            message.write_buffer(&buffer[..bytes_read]);
                            println!(
                                "[{}] Downloaded {}",
                                hex::encode(&download.id),
                                download.position()?.format_size()
                            );
                        } else {
                            message.write_i8(0);
                            new_state = Some(ClientState::Idle);
                            println!("[{}] Completed", hex::encode(&download.id));
                        }

                        self.socket.write(&message)?;
                    }
                    _ => {}
                }

                match new_state {
                    None => {}
                    Some(new) => {
                        self.state = new;
                    }
                }
            }

            Ok(())
        }

        #[allow(unused_must_use)]
        fn send_error(&mut self, description: String) -> () {
            let mut message = Message::new();
            message.write_i8(-1);
            message.write_buffer(description.as_bytes());

            self.socket.write(&message);
        }

        fn break_operation(&mut self) -> () {
            match &self.state {
                ClientState::Upload(upload) => {
                    println!("[{}] Interrupted!", hex::encode(upload.id));
                    let mut path = PathBuf::from(self.config.uploads());
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
                _ => {}
            }
            self.state = ClientState::Idle;
        }
    }

    #[cfg(unix)]
    impl AsRawFd for Client<'_> {
        fn as_raw_fd(&self) -> i32 {
            self.socket.as_raw_fd()
        }
    }

    #[cfg(windows)]
    impl AsRawSocket for Client<'_> {
        fn as_raw_socket(&self) -> i32 {
            self.socket.as_raw_socket()
        }
    }

    impl Drop for Client<'_> {
        fn drop(&mut self) {
            self.break_operation();
        }
    }

    enum ClientState {
        Idle,
        Upload(Upload),
        Download(Download),
    }

    struct Upload {
        file: File,
        id: [u8; 32],
    }

    impl Upload {
        fn begin(config: &Config) -> Result<Self, TransferError> {
            let mut id = [0; 32];
            StdRng::from_entropy().fill_bytes(&mut id);

            let mut path = PathBuf::from(config.uploads());
            path.push(hex::encode(id));
            let file = File::create(path)?;

            let mut upload = Self { file, id };
            upload.write_expiration(config)?;

            Ok(upload)
        }

        fn write_expiration(&mut self, config: &Config) -> Result<(), TransferError> {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + config.expiration();
            self.file.write_all(&timestamp.to_le_bytes())?;
            Ok(())
        }

        fn write(&mut self, buffer: &[u8]) -> Result<(), TransferError> {
            self.file.write_all(buffer)?;
            Ok(())
        }

        fn position(&mut self) -> Result<u64, TransferError> {
            Ok(self.file.seek(SeekFrom::Current(0))?)
        }
    }

    struct Download {
        file: File,
        id: Vec<u8>,
    }

    impl Download {
        fn begin(config: &Config, id: Vec<u8>) -> Result<Self, TransferError> {
            let mut path = PathBuf::from(config.uploads());
            path.push(hex::encode(&id));
            let mut file = File::open(path)?;
            file.seek(SeekFrom::Start(8))?;
            Ok(Self { file, id })
        }

        fn read(&mut self, buffer: &mut [u8]) -> Result<usize, TransferError> {
            Ok(self.file.read(buffer)?)
        }

        fn position(&mut self) -> Result<u64, TransferError> {
            Ok(self.file.seek(SeekFrom::Current(0))?)
        }
    }

    fn thread_loop(
        thread_id: u64,
        config: Config,
        receiver: Receiver<ThreadMessage>,
        sockets_alive: Arc<AtomicUsize>,
    ) {
        let mut thread_buffer = Vec::new();
        thread_buffer.resize(32 * 1024 * 1024, 0);
        let mut clients = Vec::new();
        let mut fds = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(message) => match message {
                    ThreadMessage::Terminate => {
                        break;
                    }
                    ThreadMessage::Accept(mut socket) => {
                        if socket.get_ready().is_ok() {
                            clients.push(Client::new(socket, &config));
                            fds = simpletcp::utils::get_fd_array(&clients);
                            sockets_alive.store(clients.len(), Release);
                        }
                    }
                },
                Err(_) => {}
            }

            let index = simpletcp::utils::poll_set_timeout(&mut fds, EV_POLLIN | EV_POLLOUT, 50);

            match index {
                None => {}
                Some(index) => {
                    let mut remove = false;
                    let client = &mut clients[index as usize];

                    match client.read_and_process() {
                        Err(error) => {
                            client.send_error(format!("{}", error));
                            match error {
                                TransferError::NetworkError => {
                                    remove = true;
                                }
                                _ => {}
                            }
                            client.break_operation();
                        }
                        _ => {}
                    }

                    match client.flush_and_process(&mut thread_buffer) {
                        Err(error) => {
                            client.send_error(format!("{}", error));
                            match error {
                                TransferError::NetworkError => {
                                    remove = true;
                                }
                                _ => {}
                            }
                            client.break_operation();
                        }
                        _ => {}
                    }

                    if remove {
                        clients.remove(index as usize);
                        sockets_alive.store(clients.len(), Release);
                        fds = simpletcp::utils::get_fd_array(&clients);
                    }
                }
            }
        }

        println!("[Thread #{}] Terminating.", thread_id);
    }
}
