pub mod config {
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;
    use std::process::exit;
    use std::str::FromStr;

    pub struct Config {
        expiration: u64,
        thread_count: u64,
        uploads: String,
    }

    impl Config {
        pub fn new<P: AsRef<Path>>(config_file: P) -> Self {
            let file = File::open(config_file);
            if file.is_err() {
                println!("Failed to open config file.");
            }

            let mut expiration = 10800;
            let mut thread_count = 8;
            let mut uploads = String::from("uploads");
            let mut file = file.unwrap();
            let mut str = String::new();
            file.read_to_string(&mut str).unwrap();
            let mut line = 0;
            for pair in str.split('\n') {
                line += 1;
                let mut pair = pair.to_owned();
                pair.retain(|x| !x.is_whitespace());
                if pair.starts_with('#') || pair.len() == 0 {
                    continue;
                }
                let mut split = pair.splitn(2, '=').into_iter();

                let key = split.next().unwrap();
                let value = split.next();
                if value.is_none() {
                    println!(
                        "Config parsing failed: not a key-pair value at line {}",
                        line
                    );
                    exit(1);
                }
                let value = value.unwrap();

                if key == "EXPIRATION_TIME" {
                    let parse = u64::from_str(value);
                    if parse.is_err() {
                        println!(
                            "Config parsing failed: failed to parse \"{}\" as u64 at line {}",
                            value, line
                        );
                        exit(1);
                    }

                    expiration = parse.unwrap();
                } else if key == "THREAD_COUNT" {
                    let parse = u64::from_str(value);
                    if parse.is_err() {
                        println!(
                            "Config parsing failed: failed to parse \"{}\" as u64 at line {}",
                            value, line
                        );
                        exit(1);
                    }

                    thread_count = parse.unwrap();
                } else if key == "UPLOADS" {
                    uploads = String::from(value);
                } else {
                    println!("Warning! Found unknown key {} in config file.", key);
                }
            }

            Self {
                expiration,
                thread_count,
                uploads,
            }
        }

        pub fn expiration(&self) -> u64 {
            self.expiration
        }
        pub fn thread_count(&self) -> u64 {
            self.thread_count
        }
        pub fn uploads(&self) -> &String {
            &self.uploads
        }
    }

    impl Clone for Config {
        fn clone(&self) -> Self {
            Self {
                expiration: self.expiration,
                thread_count: self.thread_count,
                uploads: self.uploads.clone(),
            }
        }
    }
}
