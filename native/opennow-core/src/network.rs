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

pub fn ping_regions(params: &Value) -> Result<Value, String> {
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
    let inputs = regions
        .iter()
        .map(|region| region["url"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    let results = Arc::new(Mutex::new(vec![Value::Null; inputs.len()]));
    thread::scope(|scope| {
        for (index, region_url) in inputs.into_iter().enumerate() {
            let output = Arc::clone(&results);
            let cancellation = cancellation.clone();
            scope.spawn(move || {
                let result = crate::requests::scope(cancellation, || measure_region(&region_url));
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

fn measure_region(region_url: &str) -> Value {
    if crate::requests::check().is_err() {
        return Value::Null;
    }
    let (host, addresses) = match region_endpoint(region_url) {
        Ok(endpoint) => endpoint,
        Err(error) => return json!({"url":region_url,"pingMs":null,"error":error}),
    };

    // Local VPN tunnels and transparent proxies complete TCP handshakes on the
    // spot, so a TCP connect measures the tunnel, not the region. An HTTPS
    // response must be produced by the origin, so latency is an HTTPS RTT on a
    // reused connection. Redirects stay disabled so a fast origin that 30x's to
    // a slow host cannot inflate the sample. Addresses are pinned into the
    // client so reqwest does not start a DNS job that outlives CONNECT_TIMEOUT
    // when the Client drops.
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(CONNECT_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
    for address in &addresses {
        builder = builder.resolve(&host, *address);
    }
    let client = match builder.build() {
        Ok(client) => client,
        Err(_) => {
            return json!({"url":region_url,"pingMs":null,"error":"HTTPS client unavailable"})
        }
    };

    if crate::requests::check().is_err() {
        return Value::Null;
    }
    if client.head(region_url).send().is_err() {
        return json!({"url":region_url,"pingMs":null,"error":"HTTPS warm-up request failed"});
    }

    let mut samples = Vec::with_capacity(3);
    let mut baseline_ms: Option<u128> = None;
    for attempt in 0..3 {
        if crate::requests::check().is_err() {
            return Value::Null;
        }
        if attempt > 0 {
            thread::sleep(Duration::from_millis(100));
        }
        let Some(sample_ms) = https_round_trip(&client, region_url) else {
            continue;
        };
        let sample_ms = match accept_keep_alive_sample(sample_ms, baseline_ms) {
            Some(ms) => ms,
            None => {
                // Server closed the pooled connection; the next request pays for
                // a fresh TLS handshake. Retry once, then drop the outlier so it
                // does not mix ~174 ms handshakes into ~20 ms keep-alive RTTs.
                if crate::requests::check().is_err() {
                    return Value::Null;
                }
                match https_round_trip(&client, region_url)
                    .and_then(|ms| accept_keep_alive_sample(ms, baseline_ms))
                {
                    Some(ms) => ms,
                    None => continue,
                }
            }
        };
        baseline_ms = Some(baseline_ms.map_or(sample_ms, |base| base.min(sample_ms)));
        samples.push(sample_ms);
    }
    if samples.is_empty() {
        return json!({"url":region_url,"pingMs":null,"error":"All HTTPS measurements failed"});
    }
    let total = samples.iter().copied().sum::<u128>();
    let average = total.div_ceil(samples.len() as u128);
    json!({"url":region_url,"pingMs":u64::try_from(average).unwrap_or(u64::MAX)})
}

fn https_round_trip(client: &reqwest::blocking::Client, region_url: &str) -> Option<u128> {
    let started = Instant::now();
    // Any HTTP response, including errors, proves the round trip; only
    // transport failure counts as a failed sample.
    client.head(region_url).send().ok()?;
    Some(started.elapsed().as_millis())
}

fn accept_keep_alive_sample(sample_ms: u128, baseline_ms: Option<u128>) -> Option<u128> {
    let Some(baseline) = baseline_ms else {
        return Some(sample_ms);
    };
    let reconnect_floor = baseline.saturating_mul(3).max(baseline.saturating_add(50));
    if sample_ms >= reconnect_floor && sample_ms >= 100 {
        return None;
    }
    Some(sample_ms)
}

fn region_endpoint(region_url: &str) -> Result<(String, Vec<SocketAddr>), String> {
    let parsed = Url::parse(region_url).map_err(|_| "Invalid region URL".to_owned())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("Unsupported region URL scheme".to_owned());
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| "Region URL has no host".to_owned())?
        .to_owned();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "Region URL has no TCP port".to_owned())?;
    let addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| "Region host could not be resolved".to_owned())?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err("Region host has no routable address".to_owned());
    }
    Ok((host, addresses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

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
    fn cancelled_region_measurement_does_not_start_network_work() {
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("ping", "network.regions.ping").unwrap();
        requests.cancel("ping");
        crate::requests::scope(permit.token.clone(), || {
            assert_eq!(
                ping_regions(&json!({"regions":[]})).unwrap_err(),
                "Request cancelled"
            );
            assert_eq!(measure_region("https://unresolvable.invalid"), Value::Null);
        });
    }

    #[test]
    fn region_ping_rejects_invalid_and_unbounded_input() {
        let invalid = ping_regions(&json!({"regions":[{"url":"file:///tmp/nope"}]}))
            .expect("invalid regions return per-item results");
        assert_eq!(invalid["results"][0]["pingMs"], Value::Null);
        assert_eq!(
            invalid["results"][0]["error"],
            "Unsupported region URL scheme"
        );

        let regions = (0..=MAX_REGIONS)
            .map(|_| json!({"url":"https://example.invalid"}))
            .collect::<Vec<_>>();
        assert!(ping_regions(&json!({"regions":regions})).is_err());
    }

    #[test]
    fn offline_region_is_a_scoped_result_instead_of_a_request_failure() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral port");
        let port = listener.local_addr().expect("listener address").port();
        drop(listener);

        let result = ping_regions(&json!({
            "regions":[{"url":format!("http://127.0.0.1:{port}")}]
        }))
        .expect("offline measurements remain serializable");
        assert_eq!(result["results"][0]["pingMs"], Value::Null);
        assert_eq!(result["results"][0]["error"], "HTTPS warm-up request failed");
    }
}
