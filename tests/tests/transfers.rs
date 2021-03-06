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
        SERVER = Some(start_server("../tests/tests/normal-config"));
    }

    wait_for_server();
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--server",
            "localhost:40788",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
            "test-file",
        ])
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

    let receiver = Command::new("cargo")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
        ])
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
        panic!("Receiver exited with a non-zero exit code.");
    }
    check_test_file("../client/test-file");
    clean_up();
}

#[test]
fn unencrypted_transfer() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(start_server("../tests/tests/normal-config"));
    }
    wait_for_server();
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--server",
            "localhost:40788",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
            "--no-encryption",
            "test-file",
        ])
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

    let receiver = Command::new("cargo")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
        ])
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
fn expired() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(start_server("../tests/tests/expire-config"));
    }
    wait_for_server();
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--server",
            "localhost:40788",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
            "--no-encryption",
            "test-file",
        ])
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
        panic!("Sender exited with a non-zero exit code.");
    }
    remove_test_file();
    let mut link = String::from_utf8(sender_output.stdout).unwrap_or_else(unwrap_clean_up);
    link = link.replace('\n', "");
    let link_args: Vec<&str> = link.split(' ').collect();

    sleep(Duration::from_secs(10));
    let receiver = Command::new("cargo")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
        ])
        .args(&link_args[1..])
        .current_dir("../client")
        .spawn()
        .unwrap();

    let receiver_output = receiver.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if receiver_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(receiver_output.stdout).unwrap()
        );
        panic!("Receiver exited with a zero exit code.");
    }
    clean_up();
}

#[test]
fn size_exceeded() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(start_server("../tests/tests/size-exceed-config"));
    }
    wait_for_server();
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--server",
            "localhost:40788",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
            "--no-encryption",
            "test-file",
        ])
        .current_dir("../client")
        .spawn()
        .unwrap_or_else(unwrap_clean_up);
    let sender_output = sender.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if sender_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(sender_output.stdout).unwrap()
        );
        panic!("Sender exited with a zero exit code.");
    }
    remove_test_file();
    clean_up();
}

#[test]
fn directory() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(start_server("../tests/tests/normal-config"));
    }

    wait_for_server();
    generate_test_dir();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--server",
            "localhost:40788",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
            "test-dir",
        ])
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
    remove_test_dir();
    let mut link = String::from_utf8(sender_output.stdout).unwrap_or_else(unwrap_clean_up);
    link = link.replace('\n', "");
    let link_args: Vec<&str> = link.split(' ').collect();

    let receiver = Command::new("cargo")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
        ])
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
        panic!("Receiver exited with a non-zero exit code.");
    }
    check_test_dir();
    clean_up();
}

#[test]
fn fingerprint_mismatch() {
    let _guard = MUTEX.deref().lock().unwrap();
    unsafe {
        SERVER = Some(start_server("../tests/tests/normal-config"));
    }
    wait_for_server();
    generate_test_file();
    let sender = Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--quiet",
            "--server",
            "localhost:40788",
            "--fingerprint",
            "0aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
            "--no-encryption",
            "test-file",
        ])
        .current_dir("../client")
        .spawn()
        .unwrap_or_else(unwrap_clean_up);
    let sender_output = sender.wait_with_output().unwrap_or_else(unwrap_clean_up);
    if sender_output.status.success() {
        clean_up();
        println!(
            "---stdout---\n {}",
            String::from_utf8(sender_output.stdout).unwrap()
        );
        panic!("Sender exited with a zero exit code.");
    }
    remove_test_file();
    clean_up();
}

fn remove_test_file() {
    let client_temp = Path::new("../client/test-file");
    if client_temp.exists() {
        fs::remove_file(client_temp).unwrap();
    }
}

fn remove_test_dir() {
    let p = Path::new("../client/test-dir");
    if p.exists() {
        fs::remove_dir_all(p).unwrap();
    }
}

fn generate_test_file() {
    let mut file = File::create("../client/test-file").unwrap();
    let mut buffer = Vec::new();
    buffer.resize(1024 * 1024 * 64, 12);
    file.write_all(&buffer).unwrap();
}

fn generate_test_dir() {
    fs::create_dir_all("../client/test-dir/a/b/c").unwrap();
    let mut file = File::create("../client/test-dir/file.a").unwrap();
    file.write_all(&[1, 2, 3]).unwrap();
    let mut file = File::create("../client/test-dir/a/b/c/file.d").unwrap();
    file.write_all(&[4, 5, 6]).unwrap();
}

fn check_test_dir() {
    let mut file = File::open("../client/test-dir/file.a").unwrap();
    let mut buf = Vec::new();
    assert_eq!(file.read_to_end(&mut buf).unwrap(), 3);
    assert_eq!(buf, vec![1, 2, 3]);

    buf.clear();

    let mut file = File::open("../client/test-dir/a/b/c/file.d").unwrap();
    assert_eq!(file.read_to_end(&mut buf).unwrap(), 3);
    assert_eq!(buf, vec![4, 5, 6]);
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
    remove_test_dir();
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

fn start_server(config: &str) -> Child {
    Command::new("cargo")
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .args(&[
            "run",
            "--",
            "--config",
            config,
            "--fingerprint",
            "8aa10297d4d6e3534f834a64c60c749e4941d6731be45f4dbd1da221f25607f1",
        ])
        .current_dir("../server")
        .spawn()
        .unwrap_or_else(unwrap_clean_up)
}

fn wait_for_server() {
    let mut buffer = [0; 6];
    unsafe {
        SERVER
            .as_mut()
            .unwrap()
            .stdout
            .as_mut()
            .unwrap()
            .read_exact(&mut buffer)
            .unwrap();
    }
}
