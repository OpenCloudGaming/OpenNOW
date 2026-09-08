use crate::gfn::ServiceError;
use reqwest::blocking::{RequestBuilder, Response};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant, SystemTime};

const REQUEST_INTERVAL: Duration = Duration::from_millis(50);
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);

pub fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, ServiceError> {
    loop {
        crate::requests::check()?;
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::WouldBlock) => std::thread::sleep(REQUEST_INTERVAL),
            Err(TryLockError::Poisoned(_)) => panic!("Store request state poisoned"),
        }
    }
}

#[derive(Default)]
struct Schedule {
    next_request: Option<Instant>,
    cooldown: Option<Instant>,
}

#[derive(Default)]
pub struct StoreRequests(Mutex<Schedule>);

fn rate_limited(remaining: Duration) -> ServiceError {
    ServiceError {
        code: "rate_limited",
        message: format!(
            "The Store is rate limited. Try again in {} seconds.",
            remaining.as_secs().saturating_add(1)
        ),
    }
}

impl StoreRequests {
    pub fn send(&self, request: RequestBuilder, context: &str) -> Result<Response, ServiceError> {
        let mut schedule = lock(&self.0)?;
        if let Some(remaining) = schedule
            .cooldown
            .and_then(|until| until.checked_duration_since(Instant::now()))
        {
            return Err(rate_limited(remaining));
        }
        while let Some(remaining) = schedule
            .next_request
            .and_then(|until| until.checked_duration_since(Instant::now()))
        {
            crate::requests::check()?;
            std::thread::sleep(remaining.min(REQUEST_INTERVAL));
        }
        crate::requests::check()?;
        let response = request.send();
        schedule.next_request = Some(Instant::now() + REQUEST_INTERVAL);
        let response = response.map_err(|error| ServiceError::network(context, error))?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let delay = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    value
                        .parse::<u64>()
                        .ok()
                        .map(Duration::from_secs)
                        .or_else(|| {
                            httpdate::parse_http_date(value)
                                .ok()
                                .and_then(|date| date.duration_since(SystemTime::now()).ok())
                        })
                })
                .unwrap_or(DEFAULT_COOLDOWN)
                .max(REQUEST_INTERVAL);
            let now = Instant::now();
            schedule.cooldown = Some(now.checked_add(delay).unwrap_or(now + DEFAULT_COOLDOWN));
            return Err(rate_limited(delay));
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    fn server(responses: Vec<String>) -> (String, std::thread::JoinHandle<Vec<Instant>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut arrivals = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    assert!(reader.read_until(b'\n', &mut request).unwrap() > 0);
                    assert!(request.len() <= 4096);
                }
                arrivals.push(Instant::now());
                stream.write_all(response.as_bytes()).unwrap();
            }
            arrivals
        });
        (url, worker)
    }

    #[test]
    fn concurrent_store_requests_are_paced() {
        let (url, server) = server(
            vec!["HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(); 4],
        );
        let requests = Arc::new(StoreRequests::default());
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| requests.send(client.get(&url), "test").unwrap());
            }
        });
        let arrivals = server.join().unwrap();
        assert!(
            arrivals
                .windows(2)
                .all(|pair| pair[1].duration_since(pair[0]) >= REQUEST_INTERVAL)
        );
    }

    #[test]
    fn rate_limits_stop_subsequent_requests_including_retries() {
        for header in [
            "Retry-After: 120\r\n".to_owned(),
            format!(
                "Retry-After: {}\r\n",
                httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(120))
            ),
            "Retry-After: invalid\r\n".to_owned(),
            String::new(),
        ] {
            let (url, server) = server(vec![format!(
                "HTTP/1.1 429 Too Many Requests\r\n{header}Content-Length: 0\r\nConnection: close\r\n\r\n"
            )]);
            let requests = StoreRequests::default();
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .build()
                .unwrap();
            assert_eq!(
                requests.send(client.get(&url), "test").unwrap_err().code,
                "rate_limited"
            );
            server.join().unwrap();
            for _ in 0..100 {
                assert_eq!(
                    requests.send(client.get(&url), "test").unwrap_err().code,
                    "rate_limited"
                );
            }
            let remaining = requests
                .0
                .lock()
                .unwrap()
                .cooldown
                .unwrap()
                .duration_since(Instant::now());
            assert!(
                remaining
                    >= Duration::from_secs(if header.contains("120") || header.contains("GMT") {
                        118
                    } else {
                        58
                    })
            );
        }
    }

    #[test]
    fn cooldown_expires_and_cancelled_waiters_never_send() {
        let (url, server) = server(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        ]);
        let requests = StoreRequests::default();
        requests.0.lock().unwrap().cooldown = Some(Instant::now() - Duration::from_secs(1));
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        requests.send(client.get(&url), "test").unwrap();
        server.join().unwrap();
        let active = Arc::new(crate::requests::Requests::default());
        let permit = active.admit("cancelled", "catalog.store.local").unwrap();
        active.cancel("cancelled");
        let _held = requests.0.lock().unwrap();
        crate::requests::scope(permit.token.clone(), || {
            assert_eq!(
                requests.send(client.get(&url), "test").unwrap_err().code,
                "cancelled"
            );
        });
    }
}
