#[macro_use]
extern crate lazy_static;

use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::ops::Deref;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

static mut SERVER: Option<Child> = None;
lazy_static! {
    static ref MUTEX: Mutex<()> = Mutex::new(());
}

#[test]
fn encrypted_transfer() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(
            Command::new("cargo")
                .stderr(Stdio::inherit())
                .stdout(Stdio::inherit())
                .args(&["run", "--", "--config", "../tests/tests/normal-config"])
                .current_dir("../server")
                .spawn()
                .unwrap_or_else(unwrap_clean_up),
        );
    }
    sleep(Duration::from_secs(2));
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&["run", "--", "--quiet", "test-file"])
        .current_dir("../client")
        .spawn()
        .unwrap_or_else(unwrap_clean_up);
    let sender_output = sender.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if !sender_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(sender_output.stdout).unwrap()
        );
        panic!("Sender exited with non-zero exit code.");
    }
    remove_test_file();
    let mut link = String::from_utf8(sender_output.stdout).unwrap_or_else(unwrap_clean_up);
    link = link.replace('\n', "");
    let link_args: Vec<&str> = link.split(' ').collect();

    println!("{:?}", link_args);
    let receiver = Command::new("cargo")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&["run", "--", "--quiet"])
        .args(&link_args[1..])
        .current_dir("../client")
        .spawn()
        .unwrap();

    let receiver_output = receiver.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if !receiver_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(receiver_output.stdout).unwrap()
        );
        panic!("Receiver exited with non-zero exit code.");
    }
    check_test_file("../client/test-file");
    clean_up();
}

#[test]
fn unencrypted_transfer() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(
            Command::new("cargo")
                .stderr(Stdio::inherit())
                .stdout(Stdio::inherit())
                .args(&["run", "--", "--config", "../tests/tests/normal-config"])
                .current_dir("../server")
                .spawn()
                .unwrap_or_else(unwrap_clean_up),
        );
    }
    sleep(Duration::from_secs(2));
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&["run", "--", "--quiet", "--no-encryption", "test-file"])
        .current_dir("../client")
        .spawn()
        .unwrap_or_else(unwrap_clean_up);
    let sender_output = sender.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if !sender_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(sender_output.stdout).unwrap()
        );
        panic!("Sender exited with non-zero exit code.");
    }
    remove_test_file();
    let mut link = String::from_utf8(sender_output.stdout).unwrap_or_else(unwrap_clean_up);
    link = link.replace('\n', "");
    let link_args: Vec<&str> = link.split(' ').collect();

    println!("{:?}", link_args);
    let receiver = Command::new("cargo")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&["run", "--", "--quiet"])
        .args(&link_args[1..])
        .current_dir("../client")
        .spawn()
        .unwrap();

    let receiver_output = receiver.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if !receiver_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(receiver_output.stdout).unwrap()
        );
        panic!("Receiver exited with non-zero exit code.");
    }
    check_test_file("../client/test-file");
    clean_up();
}

fn remove_test_file() {
    let client_temp = Path::new("../client/test-file");
    if client_temp.exists() {
        fs::remove_file(client_temp).unwrap();
    }
}

fn generate_test_file() {
    let mut file = File::create("../client/test-file").unwrap();
    let mut buffer = Vec::new();
    buffer.resize(1024 * 1024 * 64, 12);
    file.write_all(&buffer).unwrap();
}

fn check_test_file<P: AsRef<Path>>(path: P) {
    let mut file = File::open(path).unwrap();
    let mut buf = Vec::new();
    buf.resize(1024 * 1024, 0);
    loop {
        let bytes_read = file.read(&mut buf).unwrap();
        if bytes_read == 0 {
            break;
        }
        assert_eq!(buf[..bytes_read].iter().find(|&&x| x != 12), None);
    }
}

fn unwrap_clean_up<T, E>(_: E) -> T {
    clean_up();
    panic!();
}

fn clean_up() {
    remove_test_file();
    let uploads = Path::new("../server/test-uploads");
    if uploads.exists() {
        fs::remove_dir_all(uploads).unwrap();
    }
    unsafe {
        if SERVER.is_some() {
            SERVER.as_mut().unwrap().kill().unwrap();
        }
    }
}
