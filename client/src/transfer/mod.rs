pub mod transfer {
    use openssl::error::ErrorStack;
    use openssl::symm::{Cipher, Crypter, Mode};
    use rand::prelude::StdRng;
    use rand::{RngCore, SeedableRng};
    use simpletcp::simpletcp::{Message, MessageError, TcpStream};
    use std::net::ToSocketAddrs;

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
}
