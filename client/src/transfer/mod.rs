pub mod transfer {
    use openssl::error::ErrorStack;
    use openssl::symm::{Cipher, Crypter, Mode};
    use rand::prelude::StdRng;
    use rand::{RngCore, SeedableRng};
    use simpletcp::simpletcp::{Message, MessageError, TcpStream};
    use std::net::ToSocketAddrs;
    use std::convert::TryInto;

    #[derive(Debug)]
    pub enum TransferError {
        NetworkError,
        ServerError,
        EncryptionError,
        CorruptedMessage,
    }

    impl From<simpletcp::simpletcp::Error> for TransferError {
        fn from(_: simpletcp::simpletcp::Error) -> Self {
            TransferError::NetworkError
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

    pub struct Upload {
        conn: TcpStream,
        crypter: Option<Crypter>,
        encrypt_buffer: Vec<u8>,
        id: Vec<u8>,
        key: Option<[u8; 32]>,
    }

    impl Upload {
        pub fn new<A: ToSocketAddrs>(addr: A, encrypt: bool) -> Result<Self, TransferError> {
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
                }
            }

            let mut crypter = None;
            let mut key_opt = None;
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
            }

            Ok(Self {
                conn,
                crypter,
                encrypt_buffer: vec![0; 256],
                id,
                key: key_opt,
            })
        }

        pub fn write_filename(&mut self, name: &str) -> Result<(), TransferError>{
            let mut buf = Vec::new();
            let len = name.len() as u32;
            let len_bytes = len.to_le_bytes();
            buf.extend_from_slice(&len_bytes);
            buf.extend_from_slice(name.as_bytes());
            self.send(&buf)?;
            Ok(())
        }

        pub fn send(&mut self, buffer: &[u8]) -> Result<(), TransferError> {
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

            self.conn.write_blocking(&message)?;

            Ok(())
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
                    if msg.read_u8().unwrap() != 1 {
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

    pub struct Download {
        conn: TcpStream,
        crypter: Option<Crypter>,
        key: Option<[u8; 32]>,
        decrypt_buffer: Vec<u8>,
        finalized: bool,
        filename: Vec<u8>
    }

    impl Download {
        pub fn new<A: ToSocketAddrs>(
            addr: A,
            id: &[u8; 32],
            key: Option<[u8; 32]>,
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
                filename: Vec::new()
            })
        }

        pub fn read(&mut self) -> Result<(Vec<u8>, bool), TransferError> {
            if self.finalized {
                return Ok((Vec::new(), false));
            }

            let mut buffer;
            let mut message = self.conn.read_blocking()?;
            let cont = message.read_u8()?;
            if cont == 0 {
                match &mut self.crypter {
                    None => { return Ok((Vec::new(), false));},
                    Some(crypter) => {
                        let bytes_decrypted = crypter.finalize(&mut self.decrypt_buffer)?;
                        self.finalized = true;
                        buffer = self.decrypt_buffer[..bytes_decrypted].to_vec();
                    }
                };
            } else {
                buffer = message.read_buffer()?;
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
                    None => {},
                    Some(crypter) => {
                        if self.decrypt_buffer.len() < buffer.len() + 256 {
                            self.decrypt_buffer.resize(buffer.len() + 256, 0);
                        }
                        let bytes_decrypted = crypter.update(&buffer, &mut self.decrypt_buffer)?;

                        buffer = self.decrypt_buffer[..bytes_decrypted].to_vec();
                    }
                };
            }

            if self.filename.is_empty() && !buffer.is_empty() {
                let len = u32::from_le_bytes(buffer[..4].try_into().unwrap()) as usize;
                self.filename.extend_from_slice(&buffer[4..4+len]);
                buffer = buffer[4+len..].to_vec();
            }

            Ok((buffer,true))
        }

        pub fn filename(self) -> String{
            String::from_utf8(self.filename).unwrap()
        }
    }
}
