use serde_json::{Value, json};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

const MAX_REGIONS: usize = 32;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

pub fn ping_regions(params: &Value) -> Result<Value, String> {
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
            scope.spawn(move || {
                let result = measure_region(&region_url);
                output.lock().expect("region ping output poisoned")[index] = result;
            });
        }
    });
    let results = Arc::try_unwrap(results)
        .map_err(|_| "Region measurement workers did not finish".to_owned())?
        .into_inner()
        .map_err(|_| "Region measurement output was poisoned".to_owned())?;
    Ok(json!({"results":results}))
}

fn measure_region(region_url: &str) -> Value {
    let endpoint = match region_endpoint(region_url) {
        Ok(endpoint) => endpoint,
        Err(error) => return json!({"url":region_url,"pingMs":null,"error":error}),
    };
    let _ = tcp_ping(&endpoint);
    let mut samples = Vec::with_capacity(3);
    for attempt in 0..3 {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(100));
        }
        if let Some(sample) = tcp_ping(&endpoint) {
            samples.push(sample);
        }
    }
    if samples.is_empty() {
        return json!({"url":region_url,"pingMs":null,"error":"All TCP measurements failed"});
    }
    let total = samples.iter().copied().sum::<u128>();
    let average = total.div_ceil(samples.len() as u128);
    json!({"url":region_url,"pingMs":u64::try_from(average).unwrap_or(u64::MAX)})
}

fn region_endpoint(region_url: &str) -> Result<Vec<SocketAddr>, String> {
    let parsed = Url::parse(region_url).map_err(|_| "Invalid region URL".to_owned())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("Unsupported region URL scheme".to_owned());
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| "Region URL has no host".to_owned())?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "Region URL has no TCP port".to_owned())?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| "Region host could not be resolved".to_owned())?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err("Region host has no routable address".to_owned());
    }
    Ok(addresses)
}

fn tcp_ping(addresses: &[SocketAddr]) -> Option<u128> {
    let started = Instant::now();
    for address in addresses {
        if TcpStream::connect_timeout(address, CONNECT_TIMEOUT).is_ok() {
            return Some(started.elapsed().as_millis());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

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
        assert_eq!(result["results"][0]["error"], "All TCP measurements failed");
    }
}
