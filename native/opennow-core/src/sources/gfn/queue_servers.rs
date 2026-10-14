use crate::service_error::ServiceError;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const QUEUE_URL: &str = "https://api.printedwaste.com/gfn/queue/";
const MAPPING_URL: &str = "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING";
const MAX_BODY: usize = 256 * 1024;
const MAX_ZONES: usize = 128;
const PROBE_WORKERS: usize = 32;
const MAX_AGE_SECONDS: u64 = 15 * 60;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    zone_id: String,
    title: String,
    region: String,
    queue_position: u64,
    eta_ms: Option<u64>,
    last_updated: u64,
    ping_ms: Option<u64>,
    streaming_base_url: String,
    alternate_count: usize,
}

fn error(message: impl Into<String>) -> ServiceError {
    ServiceError {
        code: "queue_servers_failed",
        message: message.into(),
    }
}

pub(super) fn zone_url(zone: &str) -> Option<String> {
    if !(9..=14).contains(&zone.len()) {
        return None;
    }
    let parts = zone.split('-').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0] != "NP"
        || !(3..=8).contains(&parts[1].len())
        || !parts[1]
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        || !parts[1].bytes().next()?.is_ascii_uppercase()
        || parts[2].len() != 2
        || !parts[2].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(format!(
        "https://{}.cloudmatchbeta.nvidiagrid.net/",
        zone.to_ascii_lowercase()
    ))
}

fn client(url: &str) -> Result<Client, ServiceError> {
    let parsed = url::Url::parse(url).map_err(|_| error("Queue endpoint is invalid"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| error("Queue endpoint has no host"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| error("Queue endpoint has no port"))?;
    let addresses = crate::network::resolve_bounded(host.to_owned(), port)
        .filter(|addresses| !addresses.is_empty())
        .ok_or_else(|| error("Queue endpoint could not be resolved"))?;
    crate::requests::check()?;
    Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(5))
        .resolve_to_addrs(host, &addresses)
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(0)
        .no_proxy()
        .user_agent("opennow-core")
        .build()
        .map_err(|_| error("Could not create the queue HTTP client"))
}

fn fetch(url: &str) -> Result<Value, ServiceError> {
    crate::requests::check()?;
    let client = client(url)?;
    let mut response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .map_err(|_| error("Queue data request failed"))?;
    crate::requests::check()?;
    if !response.status().is_success() {
        return Err(error(format!(
            "Queue data returned HTTP {}",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BODY as u64)
    {
        return Err(error("Queue data response is too large"));
    }
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        crate::requests::check()?;
        let count = response
            .read(&mut chunk)
            .map_err(|_| error("Could not read queue data"))?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > MAX_BODY {
            return Err(error("Queue data response is too large"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    crate::requests::check()?;
    serde_json::from_slice(&bytes).map_err(|_| error("Queue data is not valid JSON"))
}

fn payload_data(payload: &Value) -> Result<&Map<String, Value>, ServiceError> {
    if payload["status"].as_bool() != Some(true) {
        return Err(error("Queue data did not report success"));
    }
    let data = payload["data"]
        .as_object()
        .ok_or_else(|| error("Queue data is missing its data object"))?;
    if data.len() > 256 {
        return Err(error("Queue data contains too many entries"));
    }
    Ok(data)
}

fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.len() <= 128 && !text.chars().any(char::is_control))
}

fn continent(region: &str) -> &str {
    match region {
        "US" => "North America",
        "CA" => "Canada",
        "EU" => "Europe",
        "JP" => "Japan",
        "KR" => "South Korea",
        "THAI" => "Southeast Asia",
        "MY" => "Malaysia",
        _ => region,
    }
}

fn parse_zones(queue: &Value, mapping: &Value, now: u64) -> Result<Vec<Location>, ServiceError> {
    let data = payload_data(queue)?;
    let mapping = payload_data(mapping).ok();
    let mut zones = Vec::new();
    for (zone_id, raw) in data {
        let Some(streaming_base_url) = zone_url(zone_id) else {
            continue;
        };
        let entry = mapping
            .and_then(|mapping| mapping.get(zone_id))
            .unwrap_or(&Value::Null);
        if entry["nuked"].as_bool() == Some(true) {
            continue;
        }
        let (Some(queue_position), Some(last_updated), Some(region)) = (
            raw["QueuePosition"].as_u64(),
            raw["Last Updated"].as_u64(),
            text(&raw["Region"]),
        ) else {
            continue;
        };
        if last_updated == 0
            || last_updated > now.saturating_add(60)
            || now.saturating_sub(last_updated) > MAX_AGE_SECONDS
        {
            continue;
        }
        zones.push(Location {
            zone_id: zone_id.clone(),
            title: text(&entry["title"]).unwrap_or(zone_id).to_owned(),
            region: text(&entry["region"])
                .unwrap_or_else(|| continent(region))
                .to_owned(),
            queue_position,
            eta_ms: raw["eta"].as_u64(),
            last_updated,
            ping_ms: None,
            streaming_base_url,
            alternate_count: 0,
        });
    }
    if zones.len() > MAX_ZONES {
        return Err(error("Queue data contains too many usable zones"));
    }
    if zones.is_empty() {
        return Err(error("Queue data has no fresh usable NVIDIA zones"));
    }
    Ok(zones)
}

fn score(zone: &Location, max_ping: u64, max_queue: u64) -> f64 {
    0.75 * zone.ping_ms.unwrap_or(max_ping) as f64 / max_ping as f64
        + 0.25 * zone.queue_position as f64 / max_queue as f64
}

fn best(zones: &[Location]) -> usize {
    let measured = zones.iter().any(|zone| zone.ping_ms.is_some());
    let pool = zones
        .iter()
        .enumerate()
        .filter(|(_, zone)| !measured || zone.ping_ms.is_some())
        .collect::<Vec<_>>();
    let max_ping = pool
        .iter()
        .filter_map(|(_, zone)| zone.ping_ms)
        .max()
        .unwrap_or(1)
        .max(1);
    let max_queue = pool
        .iter()
        .map(|(_, zone)| zone.queue_position)
        .max()
        .unwrap_or(1)
        .max(1);
    let selected = pool
        .iter()
        .min_by(|(_, a), (_, b)| {
            score(a, max_ping, max_queue)
                .total_cmp(&score(b, max_ping, max_queue))
                .then_with(|| a.ping_ms.cmp(&b.ping_ms))
                .then_with(|| a.queue_position.cmp(&b.queue_position))
                .then_with(|| a.zone_id.cmp(&b.zone_id))
        })
        .map(|(index, _)| *index)
        .unwrap_or(0);
    if zones[selected].ping_ms.is_some_and(|ping| ping > 100) {
        return pool
            .iter()
            .min_by_key(|(_, zone)| (zone.ping_ms, zone.queue_position, &zone.zone_id))
            .map(|(index, _)| *index)
            .unwrap_or(selected);
    }
    selected
}

fn locations(zones: Vec<Location>) -> Value {
    let mut groups = BTreeMap::<(String, String), Vec<Location>>::new();
    for zone in zones {
        groups
            .entry((zone.region.clone(), zone.title.clone()))
            .or_default()
            .push(zone);
    }
    let mut locations = groups
        .into_values()
        .map(|group| {
            let mut primary = group[best(&group)].clone();
            primary.alternate_count = group.len() - 1;
            primary
        })
        .collect::<Vec<_>>();
    let recommended = (!locations.is_empty()).then(|| locations[best(&locations)].zone_id.clone());
    locations.sort_by(|a, b| {
        (Some(&b.zone_id) == recommended.as_ref())
            .cmp(&(Some(&a.zone_id) == recommended.as_ref()))
            .then_with(|| a.region.cmp(&b.region))
            .then_with(|| a.title.cmp(&b.title))
    });
    json!({"locations":locations,"recommendedZoneId":recommended})
}

pub fn list() -> Result<Value, ServiceError> {
    crate::requests::check()?;
    let cancellation = crate::requests::current();
    let (queue, mapping) = std::thread::scope(|scope| {
        let mapping =
            scope.spawn(|| crate::requests::scope(cancellation.clone(), || fetch(MAPPING_URL)));
        let queue = fetch(QUEUE_URL);
        (
            queue,
            mapping
                .join()
                .unwrap_or_else(|_| Err(error("Queue mapping worker failed"))),
        )
    });
    crate::requests::check()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| error("System clock is invalid"))?
        .as_secs();
    let mut zones = parse_zones(&queue?, &mapping.unwrap_or(Value::Null), now)?;
    for batch in zones.chunks_mut(PROBE_WORKERS) {
        crate::requests::check()?;
        std::thread::scope(|scope| {
            for zone in batch {
                let cancellation = cancellation.clone();
                scope.spawn(move || {
                    zone.ping_ms = crate::requests::scope(cancellation, || {
                        crate::network::measure_queue_region(&zone.streaming_base_url)
                    });
                });
            }
        });
    }
    crate::requests::check()?;
    Ok(locations(zones))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    fn queue() -> Value {
        json!({"status":true,"data":{
            "NP-NEW9-01":{"QueuePosition":3,"Last Updated":10000,"Region":"US","eta":60000},
            "NP-NEW9-02":{"QueuePosition":10,"Last Updated":10000,"Region":"US"}
        }})
    }

    fn options() -> Vec<Location> {
        parse_zones(&queue(), &Value::Null, 10000).unwrap()
    }

    #[test]
    fn strict_zone_identity_allows_unknown_locations_not_host_injection_or_alliance() {
        for zone in ["NP-NEW9-01", "NP-LAX-03", "NP-SJC6-04"] {
            assert!(zone_url(zone).is_some());
        }
        for zone in [
            "NPA-GKR-SEL-01",
            "np-lax-03",
            "NP-LAX-3",
            "NP-LAX-003",
            "NP-LAX-03.evil",
            "NP-LAX-03/path",
            "NP-LAX-03@evil",
            "NP--03",
            "NP-123-01",
            "NP-LAX-03\n",
            "NP-LAX-03-EXTRA",
        ] {
            assert!(zone_url(zone).is_none(), "{zone}");
        }
        assert_eq!(
            zone_url("NP-LAX-03").unwrap(),
            "https://np-lax-03.cloudmatchbeta.nvidiagrid.net/"
        );
    }

    #[test]
    fn queue_errors_are_explicit_and_mapping_failure_preserves_raw_ids() {
        for payload in [
            Value::Null,
            json!({"status":false,"data":{}}),
            json!({"status":true}),
            json!({"status":true,"data":[]}),
            json!({"status":true,"data":{}}),
        ] {
            assert!(parse_zones(&payload, &Value::Null, 10000).is_err());
        }
        for mapping in [
            Value::Null,
            json!({"status":false}),
            json!({"status":true,"data":{}}),
        ] {
            let zones = parse_zones(&queue(), &mapping, 10000).unwrap();
            assert_eq!(zones[0].title, "NP-NEW9-01");
            assert_eq!(zones[0].region, "North America");
            assert_eq!(zones[0].eta_ms, Some(60000));
            assert_eq!(zones[1].eta_ms, None);
        }
    }

    #[test]
    fn oversized_zone_sets_fail_before_any_probe_work() {
        let mut data = Map::new();
        for index in 0..=MAX_ZONES {
            data.insert(
                format!("NP-LOC{index}-01"),
                json!({"QueuePosition":1,"Last Updated":10000,"Region":"US"}),
            );
        }
        assert_eq!(
            parse_zones(&json!({"status":true,"data":data}), &Value::Null, 10000)
                .unwrap_err()
                .message,
            "Queue data contains too many usable zones"
        );
        for index in 0..257 {
            data.insert(index.to_string(), Value::Null);
        }
        assert_eq!(
            payload_data(&json!({"status":true,"data":data}))
                .unwrap_err()
                .message,
            "Queue data contains too many entries"
        );
    }

    #[test]
    fn nuked_stale_future_and_malformed_rows_are_not_selectable() {
        let mut payload = queue();
        for (id, updated, position) in [
            ("NP-OLD-01", 9099, json!(0)),
            ("NP-FUT-01", 10061, json!(0)),
            ("NP-BAD-01", 10000, json!(-1)),
            ("NP-BAD-02", 10000, json!(1.5)),
            ("NP-BAD-03", 10000, json!("1")),
            ("NPA-BAD-01", 10000, json!(1)),
        ] {
            payload["data"][id] =
                json!({"QueuePosition":position,"Last Updated":updated,"Region":"US"});
        }
        let mapping = json!({"status":true,"data":{"NP-NEW9-02":{"nuked":true}}});
        let zones = parse_zones(&payload, &mapping, 10000).unwrap();
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].zone_id, "NP-NEW9-01");
        assert!(parse_zones(&payload, &mapping, 20000).is_err());
    }

    #[test]
    fn recommendation_weights_latency_and_queue_and_guards_high_latency() {
        let mut zones = options();
        zones[0].ping_ms = Some(20);
        zones[0].queue_position = 100;
        zones[1].ping_ms = Some(25);
        zones[1].queue_position = 0;
        assert_eq!(best(&zones), 1);
        zones[1].ping_ms = Some(80);
        assert_eq!(best(&zones), 0);
        zones[0].ping_ms = Some(110);
        zones[1].ping_ms = Some(120);
        assert_eq!(best(&zones), 0);
        zones[0].ping_ms = None;
        assert_eq!(best(&zones), 1);
        zones[1].ping_ms = None;
        assert_eq!(best(&zones), 1);
    }

    #[test]
    fn grouped_locations_use_measured_primary_and_recommend_returned_id() {
        let mapping = json!({"status":true,"data":{
            "NP-NEW9-01":{"title":" New location ","region":" US West "},
            "NP-NEW9-02":{"title":"New location","region":"US West"}
        }});
        let mut zones = parse_zones(&queue(), &mapping, 10000).unwrap();
        zones[1].ping_ms = Some(20);
        let result = locations(zones);
        assert_eq!(result["locations"].as_array().unwrap().len(), 1);
        assert_eq!(result["locations"][0]["title"], "New location");
        assert_eq!(result["locations"][0]["alternateCount"], 1);
        assert_eq!(result["locations"][0]["zoneId"], "NP-NEW9-02");
        assert_eq!(
            result["recommendedZoneId"],
            result["locations"][0]["zoneId"]
        );
    }

    fn fetch_response(response: Vec<u8>) -> Result<Value, ServiceError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 8192];
            let count = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]).to_ascii_lowercase();
            assert!(!request.contains("authorization:"));
            assert!(!request.contains("cookie:"));
            let _ = stream.write_all(&response);
        });
        let result = fetch(&url);
        worker.join().unwrap();
        result
    }

    #[test]
    fn http_rejects_errors_redirects_invalid_json_and_oversized_streamed_bodies() {
        for response in [
            "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n!",
            "HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n",
        ] {
            assert!(fetch_response(response.as_bytes().to_vec()).is_err());
        }
        let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        response.resize(response.len() + MAX_BODY + 1, b' ');
        assert_eq!(
            fetch_response(response).unwrap_err().message,
            "Queue data response is too large"
        );
        assert_eq!(
            fetch_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn cancelled_queue_request_never_starts_fetches_or_probes() {
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("queue", "queue.servers.list").unwrap();
        requests.cancel("queue");
        crate::requests::scope(permit.token.clone(), || {
            assert_eq!(list().unwrap_err().code, "cancelled");
            assert_eq!(fetch("http://127.0.0.1:1").unwrap_err().code, "cancelled");
            assert_eq!(
                crate::network::measure_queue_region("http://127.0.0.1:1"),
                None
            );
        });
    }
}
