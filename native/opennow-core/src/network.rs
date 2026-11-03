use hyper_util::client::legacy::connect::HttpInfo;
use serde_json::{Value, json};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

const MAX_REGIONS: usize = 32;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const QUEUE_PROBE_TIMEOUT: Duration = Duration::from_millis(750);
const RESOLVER_WORKERS: usize = 32;

struct ResolveJob {
    host: String,
    port: u16,
    deadline: Instant,
    cancellation: crate::requests::Cancellation,
    result: mpsc::SyncSender<Vec<SocketAddr>>,
}

fn resolver() -> Option<&'static mpsc::SyncSender<ResolveJob>> {
    static RESOLVER: OnceLock<Option<mpsc::SyncSender<ResolveJob>>> = OnceLock::new();
    RESOLVER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<ResolveJob>(RESOLVER_WORKERS);
            let receiver = Arc::new(Mutex::new(receiver));
            for index in 0..RESOLVER_WORKERS {
                let receiver = receiver.clone();
                thread::Builder::new()
                    .name(format!("queue-dns-{index}"))
                    .spawn(move || {
                        loop {
                            let job = match receiver.lock() {
                                Ok(receiver) => receiver.recv(),
                                Err(_) => return,
                            };
                            let Ok(job) = job else { return };
                            if job.cancellation.cancelled() || Instant::now() >= job.deadline {
                                continue;
                            }
                            let addresses = (job.host.as_str(), job.port)
                                .to_socket_addrs()
                                .map(|addresses| addresses.take(8).collect())
                                .unwrap_or_default();
                            let _ = job.result.try_send(addresses);
                        }
                    })
                    .ok()?;
            }
            Some(sender)
        })
        .as_ref()
}

pub(super) fn resolve_bounded(host: String, port: u16) -> Option<Vec<SocketAddr>> {
    let cancellation = crate::requests::current();
    if cancellation.cancelled() {
        return None;
    }
    let deadline = Instant::now() + QUEUE_PROBE_TIMEOUT;
    let (result, receive) = mpsc::sync_channel(1);
    resolver()?
        .try_send(ResolveJob {
            host,
            port,
            deadline,
            cancellation: cancellation.clone(),
            result,
        })
        .ok()?;
    receive_addresses(&receive, deadline, &cancellation)
}

fn receive_addresses(
    receive: &mpsc::Receiver<Vec<SocketAddr>>,
    deadline: Instant,
    cancellation: &crate::requests::Cancellation,
) -> Option<Vec<SocketAddr>> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || cancellation.cancelled() {
            return None;
        }
        match receive.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(addresses) => return Some(addresses),
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

pub(super) fn measure_queue_region(region_url: &str) -> Option<u64> {
    if crate::requests::check().is_err() {
        return None;
    }
    let parsed = Url::parse(region_url).ok()?;
    let addresses = resolve_bounded(
        parsed.host_str()?.to_owned(),
        parsed.port_or_known_default()?,
    )?;
    let _ = bounded_tcp_ping(&addresses);
    let mut samples = Vec::with_capacity(2);
    for attempt in 0..2 {
        if crate::requests::check().is_err() {
            return None;
        }
        if attempt > 0 {
            thread::sleep(Duration::from_millis(50));
        }
        if let Some(sample) = bounded_tcp_ping(&addresses) {
            samples.push(sample);
        }
    }
    (!samples.is_empty()).then(|| samples.iter().sum::<u64>().div_ceil(samples.len() as u64))
}

fn bounded_tcp_ping(addresses: &[SocketAddr]) -> Option<u64> {
    let started = Instant::now();
    for address in addresses {
        let remaining = QUEUE_PROBE_TIMEOUT.saturating_sub(started.elapsed());
        if remaining.is_zero() || crate::requests::check().is_err() {
            return None;
        }
        if TcpStream::connect_timeout(address, remaining).is_ok() {
            return Some(started.elapsed().as_millis() as u64);
        }
    }
    None
}

struct RegionResolver {
    cancellation: crate::requests::Cancellation,
    jobs: Option<mpsc::SyncSender<ResolveJob>>,
}

impl reqwest::dns::Resolve for RegionResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let cancellation = self.cancellation.clone();
        let jobs = self.jobs.clone();
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let deadline = Instant::now() + QUEUE_PROBE_TIMEOUT;
            let (result, receive) = mpsc::sync_channel(1);
            let error = || std::io::Error::other("Region DNS unavailable");
            cancellation.check().map_err(|_| error())?;
            jobs.ok_or_else(error)?
                .try_send(ResolveJob {
                    host,
                    port: 0,
                    deadline,
                    cancellation: cancellation.clone(),
                    result,
                })
                .map_err(|_| error())?;
            loop {
                if cancellation.cancelled() || Instant::now() >= deadline {
                    return Err(error().into());
                }
                match receive.try_recv() {
                    Ok(addresses) if !addresses.is_empty() => {
                        return Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs);
                    }
                    Ok(_) | Err(mpsc::TryRecvError::Disconnected) => return Err(error().into()),
                    Err(mpsc::TryRecvError::Empty) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        })
    }
}

pub fn ping_regions(params: &Value, settings: &Value) -> Result<Value, String> {
    let cancellation = crate::requests::current();
    cancellation.check().map_err(|error| error.message)?;
    let regions = params["regions"]
        .as_array()
        .ok_or_else(|| "network.regions.ping requires a regions array".to_owned())?;
    if regions.len() > MAX_REGIONS {
        return Err(format!(
            "At most {MAX_REGIONS} regions can be measured at once"
        ));
    }
    let proxy = crate::proxy::config_from_settings(settings)?;
    let inputs = regions
        .iter()
        .map(|region| region["url"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    let results = Arc::new(Mutex::new(vec![Value::Null; inputs.len()]));
    thread::scope(|scope| {
        for (index, region_url) in inputs.into_iter().enumerate() {
            let proxy = proxy.as_ref();
            let output = Arc::clone(&results);
            let cancellation = cancellation.clone();
            scope.spawn(move || {
                let result =
                    crate::requests::scope(cancellation, || measure_region(&region_url, proxy));
                output.lock().expect("region ping output poisoned")[index] = result;
            });
        }
    });
    cancellation.check().map_err(|error| error.message)?;
    let results = Arc::try_unwrap(results)
        .map_err(|_| "Region measurement workers did not finish".to_owned())?
        .into_inner()
        .map_err(|_| "Region measurement output was poisoned".to_owned())?;
    Ok(json!({"results":results}))
}

fn measure_region(region_url: &str, proxy: Option<&crate::proxy::ProxyConfig>) -> Value {
    if crate::requests::check().is_err() {
        return Value::Null;
    }
    if let Err(error) = region_endpoint(region_url) {
        return json!({"url":region_url,"pingMs":null,"error":error});
    }

    // Local VPN tunnels and transparent proxies complete TCP handshakes on the
    // spot, so a TCP connect measures the tunnel, not the region. An HTTPS
    // response must be produced by the origin, so latency is an HTTPS RTT on a
    // reused connection. Redirects stay disabled so a fast origin that 30x's to
    // a slow host cannot inflate the sample.
    let builder = reqwest::blocking::Client::builder()
        .timeout(CONNECT_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(RegionResolver {
            cancellation: crate::requests::current(),
            jobs: resolver().cloned(),
        }));
    let client = match crate::proxy::builder_for_config(builder, proxy).and_then(|builder| {
        builder
            .build()
            .map_err(|_| "HTTPS client unavailable".to_owned())
    }) {
        Ok(client) => client,
        Err(_) => return json!({"url":region_url,"pingMs":null,"error":"HTTPS client unavailable"}),
    };

    if crate::requests::check().is_err() {
        return Value::Null;
    }
    let mut previous_connection = match https_round_trip(&client, region_url) {
        Some((_, connection)) => connection,
        None => {
            return json!({"url":region_url,"pingMs":null,"error":"HTTPS warm-up request failed"});
        }
    };

    let mut samples = Vec::with_capacity(3);
    for attempt in 0..3 {
        if crate::requests::check().is_err() {
            return Value::Null;
        }
        if attempt > 0 {
            thread::sleep(Duration::from_millis(100));
        }
        for _ in 0..2 {
            if crate::requests::check().is_err() {
                return Value::Null;
            }
            let Some((sample_ms, connection)) = https_round_trip(&client, region_url) else {
                previous_connection = None;
                break;
            };
            if let Some(sample_ms) =
                accept_keep_alive_sample(sample_ms, connection, &mut previous_connection)
            {
                samples.push(sample_ms);
                break;
            }
        }
    }
    if samples.is_empty() {
        return json!({"url":region_url,"pingMs":null,"error":"All HTTPS measurements failed"});
    }
    let total = samples.iter().copied().sum::<u128>();
    let average = total.div_ceil(samples.len() as u128);
    json!({"url":region_url,"pingMs":u64::try_from(average).unwrap_or(u64::MAX)})
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ConnectionIdentity {
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
}

fn https_round_trip(
    client: &reqwest::blocking::Client,
    region_url: &str,
) -> Option<(u128, Option<ConnectionIdentity>)> {
    let started = Instant::now();
    let response = client.head(region_url).send().ok()?;
    let elapsed_ms = started.elapsed().as_millis();
    let connection = response
        .extensions()
        .get::<HttpInfo>()
        .map(|info| ConnectionIdentity {
            local_addr: info.local_addr(),
            remote_addr: info.remote_addr(),
        });
    Some((elapsed_ms, connection))
}

fn accept_keep_alive_sample(
    sample_ms: u128,
    connection: Option<ConnectionIdentity>,
    previous_connection: &mut Option<ConnectionIdentity>,
) -> Option<u128> {
    let reused = connection.is_some() && connection == *previous_connection;
    *previous_connection = connection;
    reused.then_some(sample_ms)
}

fn region_endpoint(region_url: &str) -> Result<(), String> {
    crate::requests::check().map_err(|error| error.message)?;
    let parsed = Url::parse(region_url).map_err(|_| "Invalid region URL".to_owned())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("Unsupported region URL scheme".to_owned());
    }
    parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| "Region URL has no host".to_owned())?;
    parsed
        .port_or_known_default()
        .ok_or_else(|| "Region URL has no TCP port".to_owned())?;
    Ok(())
}

#[cfg(all(test, windows))]
#[path = "network_windows_tests.rs"]
mod windows_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn measure_loopback_region(requests_per_connection: &[usize]) -> (Value, Vec<usize>) {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let finished = Arc::new(AtomicBool::new(false));
        let stop = finished.clone();
        let requests_per_connection = requests_per_connection.to_vec();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut requests = Vec::new();
            while !stop.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline, "loopback fixture deadline");
                let mut connection = match listener.accept() {
                    Ok((connection, _)) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("loopback accept failed: {error}"),
                };
                connection.set_nonblocking(false).unwrap();
                connection
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                connection
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let limit = requests_per_connection
                    .get(requests.len())
                    .copied()
                    .unwrap_or(1);
                requests.push(0);
                for index in 0..limit {
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        assert!(Instant::now() < deadline, "loopback request deadline");
                        let mut byte = [0];
                        if connection.read(&mut byte).unwrap() == 0 {
                            return requests;
                        }
                        request.push(byte[0]);
                        assert!(request.len() < 4096);
                    }
                    assert!(request.starts_with(b"HEAD "));
                    thread::sleep(Duration::from_millis(10));
                    let persistence = if index + 1 == limit {
                        "close"
                    } else {
                        "keep-alive"
                    };
                    write!(
                        connection,
                        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: {persistence}\r\n\r\n"
                    )
                    .unwrap();
                    *requests.last_mut().unwrap() += 1;
                }
            }
            requests
        });
        let result = ping_regions(
            &json!({
                "regions":[{"url":format!("http://127.0.0.1:{port}")}]
            }),
            &json!({}),
        );
        finished.store(true, Ordering::Release);
        let requests = worker.join().unwrap();
        (result.unwrap()["results"][0].clone(), requests)
    }

    #[test]
    fn region_measurement_accepts_keep_alive_round_trips() {
        let (result, requests) = measure_loopback_region(&[4]);
        assert!(result["pingMs"].is_u64(), "{result}");
        assert_eq!(requests, vec![4]);
    }

    #[test]
    fn region_measurement_rejects_connections_closed_after_every_response() {
        let (result, requests) = measure_loopback_region(&[1; 7]);
        assert_eq!(result["pingMs"], Value::Null, "{result}");
        assert_eq!(result["error"], "All HTTPS measurements failed");
        assert_eq!(requests, vec![1; 7]);
    }

    #[test]
    fn region_measurement_discards_first_reconnect_then_measures_reuse() {
        let (result, requests) = measure_loopback_region(&[1, 4]);
        assert!(result["pingMs"].is_u64(), "{result}");
        assert_eq!(requests, vec![1, 4]);
    }

    #[test]
    fn keep_alive_samples_require_matching_transport_endpoints_not_fast_responses() {
        let first = ConnectionIdentity {
            local_addr: "127.0.0.1:40000".parse().unwrap(),
            remote_addr: "127.0.0.1:443".parse().unwrap(),
        };
        let reconnected = ConnectionIdentity {
            local_addr: "127.0.0.1:40001".parse().unwrap(),
            ..first
        };
        let different_peer = ConnectionIdentity {
            remote_addr: "127.0.0.2:443".parse().unwrap(),
            ..reconnected
        };
        let mut previous = None;
        assert_eq!(
            accept_keep_alive_sample(1, Some(first), &mut previous),
            None
        );
        assert_eq!(
            accept_keep_alive_sample(500, Some(first), &mut previous),
            Some(500)
        );
        assert_eq!(
            accept_keep_alive_sample(1, Some(reconnected), &mut previous),
            None
        );
        assert_eq!(
            accept_keep_alive_sample(20, Some(reconnected), &mut previous),
            Some(20)
        );
        assert_eq!(
            accept_keep_alive_sample(1, Some(different_peer), &mut previous),
            None
        );
    }

    #[test]
    fn keep_alive_samples_fail_closed_without_connection_metadata() {
        let connection = ConnectionIdentity {
            local_addr: "127.0.0.1:40000".parse().unwrap(),
            remote_addr: "127.0.0.1:443".parse().unwrap(),
        };
        let mut previous = Some(connection);
        assert_eq!(accept_keep_alive_sample(1, None, &mut previous), None);
        assert!(previous.is_none());
        assert_eq!(accept_keep_alive_sample(1, None, &mut previous), None);
        assert_eq!(
            accept_keep_alive_sample(1, Some(connection), &mut previous),
            None
        );
        assert_eq!(
            accept_keep_alive_sample(10, Some(connection), &mut previous),
            Some(10)
        );
    }

    #[test]
    fn queue_tcp_measurement_warms_up_then_takes_two_samples_without_http() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = thread::spawn(move || {
            use std::io::Read;
            for _ in 0..3 {
                let (mut connection, _) = listener.accept().unwrap();
                connection
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                assert_eq!(connection.read(&mut [0]).unwrap(), 0);
            }
        });
        assert!(measure_queue_region(&format!("http://127.0.0.1:{port}")).is_some());
        worker.join().unwrap();
    }

    #[test]
    fn stalled_resolution_obeys_deadline_and_cancellation() {
        let (_sender, receive) = mpsc::sync_channel(1);
        let cancellation = crate::requests::Cancellation::default();
        let started = Instant::now();
        assert!(
            receive_addresses(&receive, started + Duration::from_millis(20), &cancellation)
                .is_none()
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("dns", "queue.servers.list").unwrap();
        requests.cancel("dns");
        assert!(
            receive_addresses(
                &receive,
                Instant::now() + Duration::from_secs(60),
                &permit.token
            )
            .is_none()
        );
        crate::requests::scope(permit.token.clone(), || {
            assert!(resolve_bounded("unresolvable.invalid".into(), 443).is_none());
        });
    }

    #[test]
    fn stalled_proxy_dns_and_client_drop_obey_the_resolver_deadline() {
        for scheme in ["http", "https", "socks4", "socks5"] {
            let elapsed = stalled_proxy_dns(scheme, false);
            assert!(elapsed >= QUEUE_PROBE_TIMEOUT, "{scheme}: {elapsed:?}");
            assert!(
                elapsed < QUEUE_PROBE_TIMEOUT + Duration::from_secs(1),
                "{scheme}: {elapsed:?}"
            );
        }
    }

    #[test]
    fn cancelled_proxy_dns_does_not_wait_for_resolution_or_client_drop() {
        for scheme in ["http", "https", "socks4", "socks5"] {
            assert!(
                stalled_proxy_dns(scheme, true) < Duration::from_secs(1),
                "{scheme}"
            );
        }
    }

    fn stalled_proxy_dns(scheme: &str, cancel: bool) -> Duration {
        let (jobs, receive) = mpsc::sync_channel(1);
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("dns", "network.regions.ping").unwrap();
        let cancellation = permit.token.clone();
        thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let proxy =
                    crate::proxy::normalize_proxy_url(&format!("{scheme}://proxy.invalid:8080"))
                        .unwrap();
                let builder = reqwest::blocking::Client::builder()
                    .timeout(CONNECT_TIMEOUT)
                    .connect_timeout(CONNECT_TIMEOUT)
                    .dns_resolver(Arc::new(RegionResolver {
                        cancellation,
                        jobs: Some(jobs),
                    }));
                let client = crate::proxy::builder_for_config(builder, Some(&proxy))
                    .unwrap()
                    .build()
                    .unwrap();
                let started = Instant::now();
                assert!(client.head("http://127.0.0.1:80/probe").send().is_err());
                drop(client);
                started.elapsed()
            });
            let mut stalled_job = receive.recv_timeout(Duration::from_secs(1)).unwrap();
            if stalled_job.host == "127.0.0.1" {
                stalled_job
                    .result
                    .send(vec!["127.0.0.1:0".parse().unwrap()])
                    .unwrap();
                stalled_job = receive.recv_timeout(Duration::from_secs(1)).unwrap();
            }
            assert_eq!(stalled_job.host, "proxy.invalid", "{scheme}");
            if cancel {
                requests.cancel("dns");
            }
            let elapsed = worker.join().unwrap();
            drop(stalled_job);
            elapsed
        })
    }

    #[test]
    fn cancelled_region_measurement_does_not_start_network_work() {
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("ping", "network.regions.ping").unwrap();
        requests.cancel("ping");
        crate::requests::scope(permit.token.clone(), || {
            assert_eq!(
                ping_regions(&json!({"regions":[]}), &json!({})).unwrap_err(),
                "Request cancelled"
            );
            assert_eq!(
                measure_region("https://unresolvable.invalid", None),
                Value::Null
            );
        });
    }

    #[test]
    fn cancelled_region_endpoint_does_not_resolve_numeric_hosts() {
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("dns", "network.regions.ping").unwrap();
        requests.cancel("dns");
        let started = Instant::now();
        crate::requests::scope(permit.token.clone(), || {
            assert!(region_endpoint("http://127.0.0.1:443").is_err());
        });
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn region_measurement_bounds_a_stalled_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            use std::io::Read;
            let started = Instant::now();
            loop {
                match listener.accept() {
                    Ok((mut connection, _)) => {
                        connection.set_nonblocking(false).unwrap();
                        connection
                            .set_read_timeout(Some(CONNECT_TIMEOUT + Duration::from_secs(2)))
                            .unwrap();
                        let mut request = Vec::new();
                        connection.read_to_end(&mut request).unwrap();
                        assert!(request.starts_with(b"HEAD "));
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(started.elapsed() < CONNECT_TIMEOUT + Duration::from_secs(2));
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("loopback accept failed: {error}"),
                }
            }
        });
        let started = Instant::now();
        let result = ping_regions(
            &json!({
                "regions":[{"url":format!("http://127.0.0.1:{port}")}]
            }),
            &json!({}),
        )
        .unwrap();
        assert_eq!(result["results"][0]["pingMs"], Value::Null);
        assert_eq!(
            result["results"][0]["error"],
            "HTTPS warm-up request failed"
        );
        assert!(started.elapsed() < QUEUE_PROBE_TIMEOUT + CONNECT_TIMEOUT + Duration::from_secs(2));
        worker.join().unwrap();
    }

    #[test]
    fn region_ping_rejects_invalid_and_unbounded_input() {
        let invalid = ping_regions(&json!({"regions":[{"url":"file:///tmp/nope"}]}), &json!({}))
            .expect("invalid regions return per-item results");
        assert_eq!(invalid["results"][0]["pingMs"], Value::Null);
        assert_eq!(
            invalid["results"][0]["error"],
            "Unsupported region URL scheme"
        );

        let regions = (0..=MAX_REGIONS)
            .map(|_| json!({"url":"https://example.invalid"}))
            .collect::<Vec<_>>();
        assert!(ping_regions(&json!({"regions":regions}), &json!({})).is_err());
    }

    #[test]
    fn offline_region_is_a_scoped_result_instead_of_a_request_failure() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral port");
        let port = listener.local_addr().expect("listener address").port();
        drop(listener);

        let result = ping_regions(
            &json!({
                "regions":[{"url":format!("http://127.0.0.1:{port}")}]
            }),
            &json!({}),
        )
        .expect("offline measurements remain serializable");
        assert_eq!(result["results"][0]["pingMs"], Value::Null);
        assert_eq!(
            result["results"][0]["error"],
            "HTTPS warm-up request failed"
        );
    }
}
