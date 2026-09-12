//! One Store RPC contains one complete upstream page, never an entire catalog.
use crate::gfn::ServiceError;
use serde_json::{Value, json};

// Reserve ample room for the protocol envelope within the unchanged 1 MiB limit.
pub const RESULT_BUDGET: usize = 768 * 1024;
const MAX_CURSOR_BYTES: usize = 4096;

pub struct PageRequest {
    pub limit: usize,
    pub cursor: String,
    pub search: String,
}

impl PageRequest {
    pub fn parse(params: &Value) -> Result<Self, ServiceError> {
        let text = |key: &str, max: usize| -> Result<String, ServiceError> {
            let value = match params.get(key) {
                None => "",
                Some(Value::String(value)) => value,
                _ => {
                    return Err(error(
                        "invalid_params",
                        "Store cursor and search must be strings",
                    ));
                }
            };
            if value.len() > max {
                return Err(error(
                    "invalid_params",
                    "Store cursor or search exceeds its size limit",
                ));
            }
            Ok(value.to_owned())
        };
        Ok(Self {
            limit: params["limit"].as_u64().unwrap_or(100).clamp(1, 100) as usize,
            cursor: text("cursor", MAX_CURSOR_BYTES)?,
            search: text("searchQuery", 512)?.trim().to_owned(),
        })
    }
}

fn error(code: &'static str, message: &str) -> ServiceError {
    ServiceError {
        code,
        message: message.to_owned(),
    }
}

fn encoded_size(value: &Value) -> Result<usize, ServiceError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| {
            error(
                "invalid_upstream_response",
                "Store result could not be encoded",
            )
        })
}

pub fn bounded_result(value: Value) -> Result<Value, ServiceError> {
    if encoded_size(&value)? > RESULT_BUDGET {
        return Err(error(
            "catalog_response_too_large",
            "Store response exceeds the safe page size",
        ));
    }
    Ok(value)
}

pub fn fetch_bounded_page(
    limit: usize,
    mut fetch: impl FnMut(usize) -> Result<Value, ServiceError>,
) -> Result<Value, ServiceError> {
    let mut count = limit.clamp(1, 100);
    loop {
        crate::requests::check()?;
        let page = fetch(count)?;
        crate::requests::check()?;
        if encoded_size(&page)? <= RESULT_BUDGET {
            return Ok(page);
        }
        if count == 1 {
            return Err(error(
                "catalog_response_too_large",
                "A Store game exceeds the safe page size",
            ));
        }
        count = (count / 2).max(1);
    }
}

pub fn page_result(
    cursor: &str,
    games: Vec<Value>,
    info: &Value,
    fetched_at: u64,
) -> Result<Value, ServiceError> {
    let has_more = info["hasNextPage"].as_bool().ok_or_else(|| {
        error(
            "invalid_upstream_response",
            "Store response has no pagination state",
        )
    })?;
    let next = info["endCursor"].as_str().unwrap_or("");
    if has_more && (next.is_empty() || next == cursor || next.len() > MAX_CURSOR_BYTES) {
        return Err(error(
            "invalid_upstream_response",
            "Store pagination did not advance",
        ));
    }
    Ok(json!({
        "count":games.len(), "totalCount":info["totalCount"].as_u64().unwrap_or(games.len() as u64),
        "games":games, "hasNextPage":has_more, "nextCursor":if has_more {next} else {""},
        "source":"store-browse", "fetchedAt":fetched_at
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_sized_pages_keep_the_full_hundred_game_batch() {
        let request = PageRequest::parse(&json!({"limit": 100})).unwrap();
        let mut calls = 0;
        let page = fetch_bounded_page(request.limit, |count| {
            calls += 1;
            assert_eq!(count, 100);
            page_result(
                "",
                (0..count).map(|id| json!({"id": id})).collect(),
                &json!({"hasNextPage": true, "endCursor": "after-100", "totalCount": 2500}),
                0,
            )
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(page["count"], 100);
        assert_eq!(page["nextCursor"], "after-100");
    }

    #[test]
    fn cancelled_fetch_does_not_retry_an_oversized_page() {
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("page", "catalog.store.browse").unwrap();
        let mut calls = 0;
        let result = crate::requests::scope(permit.token.clone(), || {
            fetch_bounded_page(100, |_| {
                calls += 1;
                requests.cancel("page");
                Ok(json!({"items":["x".repeat(RESULT_BUDGET)]}))
            })
        });
        assert_eq!(result.unwrap_err().code, "cancelled");
        assert_eq!(calls, 1);
    }

    #[test]
    fn page_requests_are_bounded_and_preserve_opaque_cursors() {
        let request = PageRequest::parse(
            &json!({"limit":2500,"cursor":"opaque+/=cursor","searchQuery":" game "}),
        )
        .unwrap();
        assert_eq!(request.limit, 100);
        assert_eq!(request.cursor, "opaque+/=cursor");
        assert_eq!(request.search, "game");
        assert!(PageRequest::parse(&json!({"cursor":"x".repeat(4097)})).is_err());
        assert!(PageRequest::parse(&json!({"searchQuery":5})).is_err());
    }

    #[test]
    fn oversized_pages_refetch_smaller_without_skipping_items() {
        let mut requested = Vec::new();
        let page = fetch_bounded_page(100, |count| {
            requested.push(count);
            let games = (0..count).map(|i| json!({"id":i,"title":"界\"".repeat(4000)})).collect();
            page_result("start", games, &json!({"hasNextPage":true,"endCursor":format!("offset-{count}"),"totalCount":2500}), 0)
        }).unwrap();
        assert!(requested.len() > 1);
        let count = *requested.last().unwrap();
        assert_eq!(page["count"], count);
        assert_eq!(page["nextCursor"], format!("offset-{count}"));
        let framed = json!({"type":"response","id":"999999","ok":true,"result":page});
        assert!(serde_json::to_vec(&framed).unwrap().len() < 1024 * 1024);
    }

    #[test]
    fn unpageable_records_and_chrome_return_scoped_errors() {
        let huge = json!({"items":["x".repeat(RESULT_BUDGET)]});
        assert_eq!(
            bounded_result(huge.clone()).unwrap_err().code,
            "catalog_response_too_large"
        );
        let mut attempts = 0;
        let result = fetch_bounded_page(100, |_| {
            attempts += 1;
            Ok(huge.clone())
        });
        assert_eq!(result.unwrap_err().code, "catalog_response_too_large");
        assert_eq!(attempts, 7);
    }

    #[test]
    fn empty_final_pages_and_invalid_continuations_are_distinct() {
        let end = page_result(
            "last",
            vec![],
            &json!({"hasNextPage":false,"totalCount":12}),
            0,
        )
        .unwrap();
        assert_eq!(end["hasNextPage"], false);
        assert_eq!(end["nextCursor"], "");
        for next in ["", "last"] {
            assert!(
                page_result(
                    "last",
                    vec![],
                    &json!({"hasNextPage":true,"endCursor":next}),
                    0
                )
                .is_err()
            );
        }
        assert!(page_result("last", vec![], &json!({}), 0).is_err());
    }

    #[test]
    fn network_errors_do_not_trigger_size_retries() {
        let mut attempts = 0;
        let result = fetch_bounded_page(100, |_| {
            attempts += 1;
            Err(error("upstream_error", "HTTP 503"))
        });
        assert_eq!(result.unwrap_err().code, "upstream_error");
        assert_eq!(attempts, 1);
    }
}
