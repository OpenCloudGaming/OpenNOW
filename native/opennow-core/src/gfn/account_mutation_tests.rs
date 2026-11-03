use super::tests::{auth_fixture, test_service};
use super::*;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Remote {
    url: String,
    mutations: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

fn prepare_request_stream(stream: &std::net::TcpStream) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
}

impl Remote {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mutations = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicBool::new(false));
        let count = mutations.clone();
        let stop = done.clone();
        let worker = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                prepare_request_stream(&stream);
                let mut reader = BufReader::new(&mut stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let mut status = 200;
                let payload = if first.contains("/graphql ") {
                    let query = request["query"].as_str().unwrap_or("");
                    if query.contains("appStoreDefinitions") {
                        json!({"data":{"appStoreDefinitions":[{"store":"STEAM","label":"Steam","features":[
                            {"__typename":"AccountLinkingSso","supported":true},
                            {"__typename":"AccountGamesSyncing","supported":true}
                        ]}]}})
                    } else if query.contains("genreDefinitions") {
                        json!({"data":{"genreDefinitions":[]}})
                    } else if query.contains("subscriptionDefinitions") {
                        json!({"data":{"subscriptionDefinitions":[]}})
                    } else {
                        json!({"data":{"userAccount":{"storesData":[],"subscriptions":[]}}})
                    }
                } else {
                    count.fetch_add(1, Ordering::SeqCst);
                    if first.starts_with("POST /sync/") {
                        status = 202;
                    }
                    json!({"ok":true,"login_url":"https://login.example.invalid/"})
                };
                let body = payload.to_string();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            url,
            mutations,
            done,
            worker: Some(worker),
        }
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        let result = self.worker.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}

#[test]
fn accepted_nonblocking_socket_waits_for_delayed_request_bytes() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    server.set_nonblocking(true).unwrap();
    prepare_request_stream(&server);
    let (sent, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(&mut server)
            .read_line(&mut line)
            .map(|_| line);
        sent.send(result).unwrap();
    });
    assert!(
        matches!(
            received.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ),
        "accepted socket did not wait for request bytes"
    );
    client.write_all(b"GET /delayed HTTP/1.1\r\n").unwrap();
    assert_eq!(
        received
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap(),
        "GET /delayed HTTP/1.1\r\n"
    );
    worker.join().unwrap();
}

fn failed_remote() -> Remote {
    Remote {
        url: String::new(),
        mutations: Arc::new(AtomicUsize::new(0)),
        done: Arc::new(AtomicBool::new(false)),
        worker: Some(std::thread::spawn(|| panic!("fixture worker failure"))),
    }
}

#[test]
fn remote_worker_failure_is_reported_on_normal_drop() {
    assert!(std::panic::catch_unwind(|| drop(failed_remote())).is_err());
}

#[test]
fn remote_worker_failure_during_unwind_preserves_initial_panic() {
    let result = std::panic::catch_unwind(|| {
        let _remote = failed_remote();
        panic!("initial fixture failure");
    });
    assert_eq!(
        result.unwrap_err().downcast_ref::<&str>(),
        Some(&"initial fixture failure")
    );
}

fn fixture(remote: &Remote) -> (GfnService, PathBuf) {
    let (mut service, path) = test_service(&remote.url);
    service.endpoints.graphql = format!("{}/graphql", remote.url);
    service.endpoints.account_linking = remote.url.clone();
    service
        .persistent_storage
        .set_test_base_url(remote.url.clone());
    let mut state = service.state.lock().unwrap();
    state.session = Some(auth_fixture("account-a"));
    state.restore_attempted = true;
    drop(state);
    (service, path)
}

type Mutation = fn(&GfnService, &Value) -> Result<Value, ServiceError>;
const MUTATIONS: [(&str, Mutation); 4] = [
    ("storage.reset", |service, params| {
        service.reset_persistent_storage(params)
    }),
    ("connections.unlink", |service, params| {
        service.unlink_account_connection(params, &json!({}))
    }),
    ("connections.sync", |service, params| {
        service.sync_account_connection(params, &json!({}))
    }),
    ("connections.link.start", |service, params| {
        service.start_account_link(params, &json!({}))
    }),
];

#[test]
fn stale_account_mutations_never_reach_the_remote() {
    for (name, mutate) in MUTATIONS {
        for change in ["account", "provider", "generation", "missing"] {
            let remote = Remote::new();
            let (service, path) = fixture(&remote);
            let mut params = json!({"confirmed":true,"provider":"STEAM",
                "scope":scoped_result(json!({}), &auth_fixture("account-a"), 0)["scope"]});
            {
                let mut state = service.state.lock().unwrap();
                match change {
                    "account" => state.session = Some(auth_fixture("account-b")),
                    "provider" => {
                        let provider = &mut state.session.as_mut().unwrap().provider;
                        provider.idp_id = "another-provider".into();
                        let provider = provider.clone();
                        state.providers = vec![provider];
                    }
                    "generation" => state.generation += 1,
                    "missing" => {
                        params.as_object_mut().unwrap().remove("scope");
                    }
                    _ => unreachable!(),
                }
            }
            let result = mutate(&service, &params);
            assert_eq!(
                remote.mutations.load(Ordering::SeqCst),
                0,
                "{name}/{change} sent a remote mutation"
            );
            assert_eq!(result.unwrap_err().code, "stale_account", "{name}/{change}");
            service.account_connections.cancel_pending();
            drop(service);
            std::fs::remove_dir_all(path).ok();
        }
    }
}

#[test]
fn current_account_mutations_reach_only_the_mock_remote() {
    for (name, mutate) in MUTATIONS {
        let remote = Remote::new();
        let (service, path) = fixture(&remote);
        let params = json!({"confirmed":true,"provider":"STEAM",
            "scope":scoped_result(json!({}), &auth_fixture("account-a"), 0)["scope"]});
        let result =
            mutate(&service, &params).unwrap_or_else(|error| panic!("{name}: {}", error.code));
        assert_eq!(remote.mutations.load(Ordering::SeqCst), 1, "{name}");
        assert_eq!(result["scope"], params["scope"]);
        service.account_connections.cancel_pending();
        drop(service);
        std::fs::remove_dir_all(path).ok();
    }
}
