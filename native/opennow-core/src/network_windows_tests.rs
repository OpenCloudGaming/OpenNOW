use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use windows_sys::Win32::System::Registry::*;

const WORKER_KEY: &str = "OPENNOW_PROXY_REGISTRY_TEST_KEY";
const KEY_PREFIX: &str = "Software\\OpenNOW-ProxyDiscoveryTest-";
const TEST_NAME: &str =
    "network::windows_tests::native_proxy_discovery_uses_isolated_current_user_settings";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

struct Key(HKEY);

impl Key {
    fn create(parent: HKEY, path: &str) -> Self {
        let mut key = null_mut();
        let mut disposition = 0;
        assert_eq!(
            unsafe {
                RegCreateKeyExW(
                    parent,
                    wide(path).as_ptr(),
                    0,
                    null(),
                    REG_OPTION_VOLATILE,
                    KEY_ALL_ACCESS,
                    null(),
                    &mut key,
                    &mut disposition,
                )
            },
            0,
            "create isolated registry fixture"
        );
        assert_eq!(disposition, REG_CREATED_NEW_KEY, "fixture keys must be new");
        Self(key)
    }

    fn set(&self, name: &str, kind: u32, data: &[u8]) {
        assert_eq!(
            unsafe {
                RegSetValueExW(
                    self.0,
                    wide(name).as_ptr(),
                    0,
                    kind,
                    data.as_ptr(),
                    data.len().try_into().unwrap(),
                )
            },
            0,
            "write isolated registry fixture"
        );
    }

    fn set_string(&self, name: &str, text: &str) {
        let bytes = wide(text)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        self.set(name, REG_SZ, &bytes);
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

struct Fixture {
    _key: Key,
    path: String,
}

impl Fixture {
    fn create() -> Self {
        let path = format!("{KEY_PREFIX}{:032x}", rand::random::<u128>());
        let key = Key::create(HKEY_CURRENT_USER, &path);
        Self { _key: key, path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.path).as_ptr()) };
        assert_eq!(status, 0, "delete only the unique registry fixture");
    }
}

struct Override;

impl Override {
    fn install(key: &Key) -> Self {
        assert_eq!(
            unsafe { RegOverridePredefKey(HKEY_CURRENT_USER, key.0) },
            0,
            "install process-scoped HKCU fixture"
        );
        Self
    }
}

impl Drop for Override {
    fn drop(&mut self) {
        let status = unsafe { RegOverridePredefKey(HKEY_CURRENT_USER, null_mut()) };
        assert_eq!(status, 0, "restore process-scoped HKCU mapping");
    }
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn native_proxy_discovery_uses_isolated_current_user_settings() {
    if let Ok(path) = std::env::var(WORKER_KEY) {
        let suffix = path
            .strip_prefix(KEY_PREFIX)
            .expect("fixture-only key prefix");
        assert_eq!(suffix.len(), 32);
        assert!(suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
        run_worker(&path);
        return;
    }
    let fixture = Fixture::create();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", TEST_NAME, "--nocapture", "--test-threads=1"])
        .env(WORKER_KEY, &fixture.path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "REQUEST_METHOD",
    ] {
        command.env_remove(key);
    }
    let mut worker = Worker(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = worker.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "isolated Windows proxy worker timed out"
        );
        thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    worker
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    worker
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        status.success(),
        "isolated worker failed\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("isolated native proxy received 8 HEAD requests"),
        "{stdout}"
    );
    drop(worker);
    drop(fixture);
}

fn run_worker(path: &str) {
    let mut raw_key = null_mut();
    assert_eq!(
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(path).as_ptr(),
                0,
                KEY_ALL_ACCESS,
                &mut raw_key,
            )
        },
        0
    );
    let root = Key(raw_key);
    let settings = Key::create(
        root.0,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    settings.set("ProxyEnable", REG_DWORD, &1u32.to_le_bytes());
    settings.set_string("ProxyServer", &format!("localhost:{}", address.port()));
    settings.set_string("ProxyOverride", "");
    let mapping = Override::install(&root);
    let stop = AtomicBool::new(false);
    let count = AtomicUsize::new(0);
    thread::scope(|scope| {
        let server = scope.spawn(|| {
            while !stop.load(Ordering::Acquire) {
                let mut connection = match listener.accept() {
                    Ok((connection, _)) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("loopback proxy accept failed: {error}"),
                };
                connection.set_nonblocking(false).unwrap();
                connection.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
                connection.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                let mut bytes = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    let mut byte = [0];
                    match connection.read(&mut byte) {
                        Ok(0) => break,
                        Ok(_) => {
                            bytes.push(byte[0]);
                            assert!(bytes.len() < 4096);
                            if bytes.ends_with(b"\r\n\r\n") {
                                assert!(bytes.starts_with(b"HEAD http://region.invalid/probe HTTP/1.1\r\n"));
                                count.fetch_add(1, Ordering::Relaxed);
                                if connection.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n").is_err() {
                                    break;
                                }
                                bytes.clear();
                            }
                        }
                        Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                        Err(_) => break,
                    }
                }
            }
        });
        let results = std::panic::catch_unwind(|| {
            [
                json!({}),
                json!({"sessionProxyEnabled":false,"sessionProxyUrl":"invalid"}),
            ]
            .map(|settings| {
                ping_regions(
                    &json!({"regions":[{"url":"http://region.invalid/probe"}]}),
                    &settings,
                )
            })
        });
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        for result in results.unwrap() {
            let result = result.unwrap();
            assert!(result["results"][0]["pingMs"].is_u64(), "{result}");
        }
    });
    assert_eq!(count.load(Ordering::Relaxed), 8);
    println!("isolated native proxy received 8 HEAD requests");
    drop(mapping);
    drop(settings);
    drop(root);
}
