pub mod transfer {
    use openssl::error::ErrorStack;
    use openssl::symm::{Cipher, Crypter, Mode};
    use rand::prelude::StdRng;
    use rand::{RngCore, SeedableRng};
    use simpletcp::simpletcp::{Message, MessageError, TcpStream};
    use std::fmt::{Display, Formatter};
    use std::io::{ErrorKind, Read, Write};
    use std::net::ToSocketAddrs;
    use std::string::FromUtf8Error;
    use std::time::Instant;
    use std::{fmt, io};

    #[macro_export]
    macro_rules! printinfoln {
    ($q:expr, $($x:expr), *) => {
        if !$q {
        println!($($x,)*);
        }
    };
}

    #[macro_export]
    macro_rules! printinfo {
    ($q:expr, $($x:expr), *) => {
        if !$q {
        print!($($x,)*);
        }
    };
}

    pub enum TransferError {
        NetworkError(simpletcp::simpletcp::Error),
        ServerError,
        EncryptionError,
        CorruptedMessage,
        SizeLimitExceeded,
    }

    impl Display for TransferError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match &self {
                TransferError::NetworkError(net_err) => f.write_str(&format!("{:?}", net_err)),
                TransferError::ServerError => f.write_str("ServerError"),
                TransferError::EncryptionError => f.write_str("EncryptionError"),
                TransferError::CorruptedMessage => f.write_str("CorruptedMessage"),
                TransferError::SizeLimitExceeded => f.write_str("SizeLimitExceeded"),
            }
        }
    }

    impl From<simpletcp::simpletcp::Error> for TransferError {
        fn from(net_err: simpletcp::simpletcp::Error) -> Self {
            TransferError::NetworkError(net_err)
        }
    }

    impl From<ErrorStack> for TransferError {
        fn from(_: ErrorStack) -> Self {
            TransferError::EncryptionError
        }
    }

    impl From<MessageError> for TransferError {
        fn from(_: MessageError) -> Self {
            TransferError::CorruptedMessage
        }
    }

    impl From<FromUtf8Error> for TransferError {
        fn from(_: FromUtf8Error) -> Self {
            TransferError::CorruptedMessage
        }
    }

    pub trait FormatSize {
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

    impl FormatSize for usize {
        fn format_size(self) -> String {
            (self as u64).format_size()
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

    pub struct Upload {
        conn: TcpStream,
        crypter: Option<Crypter>,
        encrypt_buffer: Vec<u8>,
        id: Vec<u8>,
        key: Option<[u8; 32]>,
        uploaded: usize,
        time: Instant,
        quiet: bool,
    }

    impl Upload {
        pub fn new<A: ToSocketAddrs>(
            addr: A,
            encrypt: bool,
            quiet: bool,
            size: usize,
        ) -> Result<Self, TransferError> {
            let mut conn = TcpStream::connect(&addr)?;
            conn.wait_until_ready()?;
            let mut message = Message::new();
            message.write_i32(0);
            conn.write_blocking(&message)?;

            let msg = conn.read_timeout(5000)?;
            let id;
            match msg {
                None => {
                    return Err(TransferError::ServerError);
                }
                Some(mut msg) => {
                    id = msg.read_buffer()?;
                    let max_size = msg.read_u64()?;
                    if size > max_size as usize {
                        return Err(TransferError::SizeLimitExceeded);
                    }
                }
            }

            let mut crypter = None;
            let mut key_opt = None;
            let mut uploaded = 0;
            if encrypt {
                let mut rng = StdRng::from_entropy();
                let mut key = [0; 32];
                let mut iv = [0; 16];

                rng.fill_bytes(&mut key);
                rng.fill_bytes(&mut iv);
                key_opt = Some(key);

                crypter = Some(Crypter::new(
                    Cipher::aes_256_cbc(),
                    Mode::Encrypt,
                    &key,
                    Some(&iv),
                )?);

                let mut message = Message::new();
                message.write_u8(1);
                message.write_buffer(&iv);
                conn.write_blocking(&message)?;
                uploaded += iv.len();
            }

            Ok(Self {
                conn,
                crypter,
                encrypt_buffer: vec![0; 256],
                id,
                key: key_opt,
                uploaded,
                quiet,
                time: Instant::now(),
            })
        }

        pub fn finalize(&mut self) -> Result<(), TransferError> {
            match &mut self.crypter {
                None => {}
                Some(crypter) => {
                    let bytes_encrypted = crypter.finalize(&mut self.encrypt_buffer)?;
                    let mut message = Message::new();
                    message.write_u8(1);
                    message.write_buffer(&self.encrypt_buffer[..bytes_encrypted]);

                    self.conn.write_blocking(&message)?;
                }
            }

            let mut message = Message::new();
            message.write_u8(0);
            self.conn.write_blocking(&message)?;

            let confirmation = self.conn.read_timeout(5000)?;
            match confirmation {
                None => return Err(TransferError::ServerError),
                Some(mut msg) => {
                    if msg.read_i8().unwrap() != 1 {
                        match msg.read_buffer() {
                            Ok(description) => {
                                println!("\x1b[KReceived an error message:");
                                println!("\n{}\n", String::from_utf8(description)?);
                            }
                            _ => {}
                        }
                        return Err(TransferError::ServerError);
                    }
                }
            }

            Ok(())
        }

        pub fn check_for_error(&mut self) -> Result<(), TransferError> {
            match self.conn.read_timeout(0)? {
                None => {}
                Some(mut msg) => {
                    if msg.read_i8().unwrap() == -1 {
                        match msg.read_buffer() {
                            Ok(description) => {
                                println!("\x1b[KReceived an error message:");
                                println!("\n{}\n", String::from_utf8(description)?);
                            }
                            _ => {}
                        }
                        return Err(TransferError::ServerError);
                    }
                }
            }
            Ok(())
        }

        pub fn id(&self) -> &Vec<u8> {
            &self.id
        }

        pub fn key(&self) -> Option<&[u8; 32]> {
            self.key.as_ref()
        }
    }

    impl Write for Upload {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if buffer.len() > self.encrypt_buffer.len() - 256 {
                self.encrypt_buffer.resize(buffer.len() + 256, 0);
            }
            let mut message = Message::new();
            message.write_u8(1);
            match &mut self.crypter {
                None => {
                    message.write_buffer(buffer);
                }
                Some(crypter) => {
                    let bytes_encrypted = crypter.update(buffer, &mut self.encrypt_buffer)?;
                    message.write_buffer(&self.encrypt_buffer[..bytes_encrypted]);
                }
            }

            match self.conn.write_blocking(&message) {
                Err(_) => {
                    return Err(io::Error::new(ErrorKind::ConnectionReset, "NetworkError"));
                }
                _ => {}
            }

            self.uploaded += buffer.len();
            let time = self.time.elapsed().as_micros() as f64;
            let speed = self.uploaded as f64 / time * 1000000.0;

            printinfoln!(
                self.quiet,
                "Uploaded {:^12} @ {:^12}  \x1b[1A\x1b[0G",
                self.uploaded.format_size(),
                format!("{}/s", speed.format_size())
            );

            if self.check_for_error().is_err() {
                return Err(io::Error::new(ErrorKind::ConnectionReset, "NetworkError"));
            }

            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            unimplemented!();
        }

        fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            return match self.write(buf) {
                Ok(_) => Ok(()),
                Err(err) => Err(err),
            };
        }
    }

    pub struct Download {
        conn: TcpStream,
        crypter: Option<Crypter>,
        key: Option<[u8; 32]>,
        decrypt_buffer: Vec<u8>,
        downloaded: usize,
        time: Instant,
        finalized: bool,
        quiet: bool,
    }

    impl Download {
        pub fn new<A: ToSocketAddrs>(
            addr: A,
            id: &[u8; 32],
            key: Option<[u8; 32]>,
            quiet: bool,
        ) -> Result<Self, TransferError> {
            let mut conn = TcpStream::connect(addr)?;
            let mut message = Message::new();
            message.write_i32(1);
            message.write_buffer(id);
            conn.wait_until_ready()?;
            conn.write_blocking(&message)?;
            Ok(Self {
                conn,
                crypter: None,
                key,
                decrypt_buffer: Vec::new(),
                finalized: false,
                time: Instant::now(),
                downloaded: 0,
                quiet,
            })
        }

        fn print_stats(&mut self, n: usize) {
            self.downloaded += n;
            let time = self.time.elapsed().as_micros() as f64;
            let size = self.downloaded;
            let speed = size as f64 / time * 1000000.0;

            printinfoln!(
                self.quiet,
                "Downloaded {:^12} @ {:^12}\x1b[1A\x1b[0G",
                size.format_size(),
                format!("{}/s", speed.format_size())
            );
        }
    }

    impl Read for Download {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.decrypt_buffer.len() != 0 {
                let mut bytes = self.decrypt_buffer.len();
                if self.decrypt_buffer.len() > buf.len() {
                    bytes = buf.len();
                }
                buf[..bytes].copy_from_slice(&self.decrypt_buffer[..bytes]);
                self.decrypt_buffer.drain(..bytes);
                self.print_stats(bytes);
                return Ok(bytes);
            }

            if self.finalized {
                self.print_stats(0);
                return Ok(0);
            }

            let mut buffer;
            let mut message;
            match self.conn.read_blocking() {
                Ok(msg) => {
                    message = msg;
                }
                Err(_) => {
                    return Err(io::Error::new(ErrorKind::ConnectionReset, "NetworkError"));
                }
            }
            let cont;
            match message.read_i8() {
                Ok(v) => {
                    cont = v;
                }
                Err(_) => {
                    return Err(io::Error::new(ErrorKind::ConnectionReset, "NetworkError"));
                }
            }
            return if cont == -1 {
                match message.read_buffer() {
                    Ok(description) => {
                        println!("Received an error message:");
                        println!("\n{}\n", String::from_utf8(description).unwrap());
                    }
                    _ => {}
                }
                Err(io::Error::new(ErrorKind::ConnectionReset, "NetworkError"))
            } else if cont == 0 {
                match &mut self.crypter {
                    None => {
                        self.finalized = true;
                        self.print_stats(0);
                        Ok(0)
                    }
                    Some(crypter) => {
                        self.finalized = true;
                        if buf.len() < 16 {
                            self.decrypt_buffer.resize(16, 0);
                            crypter.finalize(&mut self.decrypt_buffer)?;
                            buf.copy_from_slice(&self.decrypt_buffer[..buf.len()]);
                            self.decrypt_buffer.drain(..buf.len());
                            self.print_stats(buf.len());
                            Ok(buf.len())
                        } else {
                            let bytes_decrypted = crypter.finalize(buf)?;
                            self.print_stats(bytes_decrypted);
                            Ok(bytes_decrypted)
                        }
                    }
                }
            } else {
                match message.read_buffer() {
                    Ok(b) => {
                        buffer = b;
                    }
                    Err(_) => {
                        return Err(io::Error::new(ErrorKind::ConnectionReset, "NetworkError"));
                    }
                }
                if self.key.is_some() && self.crypter.is_none() {
                    if buffer.len() < 16 {
                        panic!("IV split");
                    }

                    self.crypter = Some(Crypter::new(
                        Cipher::aes_256_cbc(),
                        Mode::Decrypt,
                        &self.key.unwrap(),
                        Some(&buffer[..16]),
                    )?);

                    buffer.drain(..16);
                }

                match &mut self.crypter {
                    None => {
                        return if buffer.len() <= buf.len() {
                            buf[..buffer.len()].copy_from_slice(&buffer);
                            self.print_stats(buffer.len());
                            Ok(buffer.len())
                        } else {
                            buf.copy_from_slice(&buffer[..buf.len()]);
                            self.decrypt_buffer.resize(buffer.len() - buf.len(), 0);
                            self.decrypt_buffer.copy_from_slice(&buffer[buf.len()..]);
                            self.print_stats(buf.len());
                            Ok(buf.len())
                        }
                    }
                    Some(crypter) => {
                        if buf.len() < buffer.len() + 16 {
                            self.decrypt_buffer.resize(buffer.len() + 16, 0);
                            let bytes_decrypted =
                                crypter.update(&buffer, &mut self.decrypt_buffer)?;
                            self.decrypt_buffer.truncate(bytes_decrypted);

                            if bytes_decrypted > buf.len() {
                                buf.copy_from_slice(&self.decrypt_buffer[..buf.len()]);
                                self.decrypt_buffer.drain(..buf.len());
                                self.print_stats(buf.len());
                                Ok(buf.len())
                            } else {
                                buf[..bytes_decrypted].copy_from_slice(&self.decrypt_buffer);
                                self.decrypt_buffer.clear();
                                self.print_stats(bytes_decrypted);
                                Ok(bytes_decrypted)
                            }
                        } else {
                            let bytes_decrypted = crypter.update(&buffer, buf)?;
                            self.print_stats(bytes_decrypted);
                            Ok(bytes_decrypted)
                        }
                    }
                }
            };
        }
    }
}
