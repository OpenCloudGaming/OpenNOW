use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::thread;
use std::time::Duration;
use url::Url;

const CONTRIBUTORS_URL: &str =
    "https://api.github.com/repos/OpenCloudGaming/OpenNOW/contributors?per_page=100";
const SUPPORTERS_URL: &str = "https://github.com/sponsors/zortos293";
const MAXIMUM_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

pub struct ThanksService {
    client: Client,
}

impl ThanksService {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(8))
                .pool_idle_timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string())?,
        })
    }

    pub fn data(&self) -> Value {
        let (contributors, supporters) = thread::scope(|scope| {
            let contributors = scope.spawn(|| self.contributors());
            let supporters = scope.spawn(|| self.supporters());
            (
                contributors
                    .join()
                    .unwrap_or_else(|_| Err("Contributor request failed".to_owned())),
                supporters
                    .join()
                    .unwrap_or_else(|_| Err("Supporter request failed".to_owned())),
            )
        });
        let mut result = json!({
            "contributors":contributors.as_ref().cloned().unwrap_or_default(),
            "supporters":supporters.as_ref().cloned().unwrap_or_else(|_| custom_supporters())
        });
        if let Err(error) = contributors {
            result["contributorsError"] = json!(error);
        }
        if let Err(error) = supporters {
            result["supportersError"] = json!(error);
        }
        result
    }

    fn contributors(&self) -> Result<Vec<Value>, String> {
        let response = self
            .client
            .get(CONTRIBUTORS_URL)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "OpenNOW-DesktopClient")
            .send()
            .map_err(|_| "Unable to load contributors right now".to_owned())?;
        if !response.status().is_success()
            || response.content_length().unwrap_or(0) > MAXIMUM_RESPONSE_BYTES
        {
            return Err(format!(
                "GitHub contributors request failed ({})",
                response.status().as_u16()
            ));
        }
        let mut values = response
            .json::<Vec<Contributor>>()
            .map_err(|_| "GitHub contributors response was invalid".to_owned())?
            .into_iter()
            .filter_map(normalize_contributor)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| {
            right["contributions"]
                .as_u64()
                .cmp(&left["contributions"].as_u64())
                .then_with(|| left["login"].as_str().cmp(&right["login"].as_str()))
        });
        Ok(values)
    }

    fn supporters(&self) -> Result<Vec<Value>, String> {
        let response = self
            .client
            .get(SUPPORTERS_URL)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("User-Agent", "OpenNOW-DesktopClient")
            .send()
            .map_err(|_| "Unable to load supporters right now".to_owned())?;
        if !response.status().is_success()
            || response.content_length().unwrap_or(0) > MAXIMUM_RESPONSE_BYTES
        {
            return Err(format!(
                "GitHub sponsors request failed ({})",
                response.status().as_u16()
            ));
        }
        let html = response
            .text()
            .map_err(|_| "GitHub sponsors response was invalid".to_owned())?;
        let mut output = custom_supporters();
        output.extend(parse_supporters(&html));
        let mut seen = HashSet::new();
        output.retain(|value| {
            let key = value["profileUrl"]
                .as_str()
                .map(|value| format!("profile:{}", value.to_ascii_lowercase()))
                .unwrap_or_else(|| {
                    format!(
                        "name:{}|private:{}",
                        value["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_ascii_lowercase(),
                        value["isPrivate"].as_bool().unwrap_or(false)
                    )
                });
            seen.insert(key)
        });
        if output.is_empty() {
            return Err("No public supporters were found on GitHub Sponsors".to_owned());
        }
        Ok(output)
    }
}

#[derive(Deserialize)]
struct Contributor {
    login: Option<String>,
    avatar_url: Option<String>,
    html_url: Option<String>,
    contributions: Option<u64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

fn normalize_contributor(value: Contributor) -> Option<Value> {
    let login = value.login?.trim().to_owned();
    let avatar = trusted_github_url(&value.avatar_url?)?;
    let profile = trusted_github_url(&value.html_url?)?;
    let excluded = login.ends_with("[bot]")
        || value.kind.as_deref() == Some("Bot")
        || ["copilot", "claude", "cappy"]
            .iter()
            .any(|needle| login.to_ascii_lowercase().contains(needle));
    if login.is_empty() || excluded {
        return None;
    }
    Some(json!({
        "login":login,
        "avatarUrl":avatar,
        "profileUrl":profile,
        "contributions":value.contributions.unwrap_or(0)
    }))
}

fn parse_supporters(html: &str) -> Vec<Value> {
    let entry = Regex::new(r#"(?is)<div class="d-flex mb-1 mr-1"[^>]*>(.*?)</div>"#)
        .expect("supporter entry regex");
    let image = Regex::new(r#"(?is)<img[^>]+(?:alt|aria-label)="([^"]*)"[^>]*src="([^"]+)""#)
        .expect("supporter image regex");
    let href = Regex::new(r#"(?is)<a[^>]+href="([^"]+)""#).expect("supporter href regex");
    let mut values = Vec::new();
    for capture in entry.captures_iter(html).take(100) {
        let body = capture
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let private = body.to_ascii_lowercase().contains("private sponsor");
        let image_capture = image.captures(body);
        let name = if private {
            "Private".to_owned()
        } else {
            image_capture
                .as_ref()
                .and_then(|capture| capture.get(1))
                .map(|value| {
                    decode_html(value.as_str())
                        .trim_start_matches('@')
                        .trim()
                        .to_owned()
                })
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Private".to_owned())
        };
        let avatar = image_capture
            .as_ref()
            .and_then(|capture| capture.get(2))
            .and_then(|value| normalize_github_url(value.as_str()));
        let profile = (!private)
            .then(|| href.captures(body))
            .flatten()
            .and_then(|capture| capture.get(1))
            .and_then(|value| normalize_github_url(value.as_str()));
        values.push(json!({
            "name":name,
            "avatarUrl":avatar,
            "profileUrl":profile,
            "isPrivate":private || profile.is_none(),
            "source":if private { "private" } else { "github" }
        }));
    }
    values
}

fn custom_supporters() -> Vec<Value> {
    vec![json!({
        "name":"DarkevilPT",
        "avatarUrl":"https://github.com/DarkevilPT.png?size=96",
        "profileUrl":"https://github.com/DarkevilPT",
        "isPrivate":false,
        "source":"custom"
    })]
}

fn normalize_github_url(value: &str) -> Option<String> {
    let value = decode_html(value.trim());
    let absolute = if value.starts_with("//") {
        format!("https:{value}")
    } else if value.starts_with('/') {
        format!("https://github.com{value}")
    } else {
        value
    };
    trusted_github_url(&absolute)
}

fn trusted_github_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    (parsed.scheme() == "https"
        && matches!(
            host.as_str(),
            "github.com" | "avatars.githubusercontent.com" | "user-images.githubusercontent.com"
        ))
    .then(|| parsed.to_string())
}

fn decode_html(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contributor_and_supporter_inputs_are_bounded_and_trusted() {
        assert!(
            normalize_contributor(Contributor {
                login: Some("dependabot[bot]".to_owned()),
                avatar_url: Some("https://avatars.githubusercontent.com/u/1".to_owned()),
                html_url: Some("https://github.com/apps/dependabot".to_owned()),
                contributions: Some(3),
                kind: Some("Bot".to_owned()),
            })
            .is_none()
        );
        assert!(trusted_github_url("https://evil.example/avatar.png").is_none());
        let supporters = parse_supporters(
            r#"<div class="d-flex mb-1 mr-1"><a href="/player"><img alt="@player" src="https://avatars.githubusercontent.com/u/2"></a></div>"#,
        );
        assert_eq!(supporters.len(), 1);
        assert_eq!(supporters[0]["name"], "player");
        assert_eq!(supporters[0]["profileUrl"], "https://github.com/player");
    }
}
