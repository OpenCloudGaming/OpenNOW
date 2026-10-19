use super::mock::{MockBackend, Recorded, Reply};
use super::*;
use std::sync::mpsc::{self, Receiver};

const ACCOUNT_TOKEN: &str = "tok_account_0123456789";

fn user(id: &str) -> Value {
    json!({"id":id, "discordId":"42", "username":"mara", "displayName":"Mara", "avatarUrl":null})
}

fn fast_timing() -> Timing {
    Timing {
        ping: Duration::from_millis(300),
        silence: Duration::from_secs(3),
        auth: Duration::from_secs(1),
        connect: Duration::from_secs(1),
        presence: Duration::from_millis(400),
        backoff_base: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
        tick: Duration::from_millis(20),
    }
}

struct Harness {
    service: SocialService,
    events: Receiver<Value>,
    seen: Vec<Value>,
    _directory: tempfile::TempDir,
}

impl Harness {
    fn new(backend_url: &str, linked: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let vault = TokenVault::memory(directory.path().join("social"), true);
        if linked {
            vault
                .save(
                    ACCOUNT_KEY,
                    &json!({"token":ACCOUNT_TOKEN, "user":user("usr_1")}).to_string(),
                )
                .unwrap();
        }
        let (output, events) = mpsc::channel();
        let service = SocialService::start(
            vault,
            ACCOUNT_KEY.to_owned(),
            output,
            &json!({"socialBackendUrl":backend_url}),
            fast_timing(),
        )
        .unwrap();
        Self {
            service,
            events,
            seen: Vec::new(),
            _directory: directory,
        }
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, SocialError> {
        self.service.handle(method, &params)
    }

    fn state(&self) -> Value {
        self.call("social.state.get", json!({})).unwrap()
    }

    fn event(&mut self, name: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Ok(event) = self.events.recv_timeout(remaining) else {
                let names = self
                    .seen
                    .iter()
                    .map(|event| event["name"].clone())
                    .collect::<Vec<_>>();
                panic!("no {name} event; saw {names:?}");
            };
            self.seen.push(event.clone());
            if event["type"] == "event" && event["name"] == name && matches(&event["payload"]) {
                return event["payload"].clone();
            }
        }
    }

    fn connection(&mut self, expected: &str) {
        self.event("social.connection.changed", |payload| {
            payload["state"] == expected
        });
    }

    fn vault(&self) -> &TokenVault {
        &self.service.shared.vault
    }

    fn all_output(&mut self) -> String {
        while let Ok(event) = self.events.try_recv() {
            self.seen.push(event);
        }
        serde_json::to_string(&self.seen).unwrap()
    }
}

fn echo(request: &Recorded) -> Reply {
    if request.method == "DELETE" {
        Reply::empty(204)
    } else {
        Reply::json(200, json!({"echo":request.path}))
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "condition did not become true");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn backend_url_setting_requires_https_except_on_loopback() {
    assert_eq!(normalize_backend_url("").unwrap(), "");
    assert_eq!(
        normalize_backend_url(" https://Social.Example.com/ ").unwrap(),
        "https://social.example.com"
    );
    assert_eq!(
        normalize_backend_url("http://localhost:8787").unwrap(),
        "http://localhost:8787"
    );
    assert_eq!(
        normalize_backend_url("http://127.0.0.1:9000").unwrap(),
        "http://127.0.0.1:9000"
    );
    assert_eq!(
        normalize_backend_url("http://[::1]:9000").unwrap(),
        "http://[::1]:9000"
    );
    for invalid in [
        "http://social.example.com",
        "ftp://social.example.com",
        "https://user:pass@social.example.com",
        "https://social.example.com/v1",
        "https://social.example.com/?a=1",
        "social.example.com",
    ] {
        assert!(normalize_backend_url(invalid).is_err(), "{invalid}");
    }
    let config = Config::from_settings(&json!({"socialBackendUrl":""}));
    assert_eq!(
        config.base.as_str(),
        "https://opennow-social-production.up.railway.app/"
    );
}

#[test]
fn every_method_maps_to_its_backend_request() {
    let backend = MockBackend::start(echo);
    let harness = Harness::new(&backend.url, true);
    let game =
        json!({"appId":"100", "title":"Baldur's Gate 3", "artUrl":"https://img.example/bg3.png"});
    let cases: Vec<(&str, Value, &str, &str, Value)> = vec![
        (
            "social.settings.update",
            json!({"shareActivity":false}),
            "PATCH",
            "/v1/me/settings",
            json!({"shareActivity":false}),
        ),
        (
            "social.friends.get",
            json!({}),
            "GET",
            "/v1/friends",
            Value::Null,
        ),
        (
            "social.users.lookup",
            json!({"username":"@Mara Two"}),
            "GET",
            "/v1/users/lookup?username=%40Mara+Two",
            Value::Null,
        ),
        (
            "social.friends.request",
            json!({"username":"mara"}),
            "POST",
            "/v1/friends/requests",
            json!({"username":"mara"}),
        ),
        (
            "social.friends.accept",
            json!({"requestId":"frq_1"}),
            "POST",
            "/v1/friends/requests/frq_1/accept",
            json!({}),
        ),
        (
            "social.friends.decline",
            json!({"requestId":"frq_1"}),
            "POST",
            "/v1/friends/requests/frq_1/decline",
            json!({}),
        ),
        (
            "social.friends.cancel",
            json!({"requestId":"frq_1"}),
            "DELETE",
            "/v1/friends/requests/frq_1",
            Value::Null,
        ),
        (
            "social.friends.remove",
            json!({"userId":"usr_2"}),
            "DELETE",
            "/v1/friends/usr_2",
            Value::Null,
        ),
        (
            "social.friends.block",
            json!({"userId":"usr_2"}),
            "POST",
            "/v1/blocks",
            json!({"userId":"usr_2"}),
        ),
        (
            "social.friends.unblock",
            json!({"userId":"usr_2"}),
            "DELETE",
            "/v1/blocks/usr_2",
            Value::Null,
        ),
        (
            "coop.lobby.create",
            json!({"game":game, "maxGuests":3}),
            "POST",
            "/v1/lobbies",
            json!({"game":game, "maxGuests":3}),
        ),
        (
            "coop.lobby.update",
            json!({"lobbyId":"lob_1", "state":"streaming"}),
            "PATCH",
            "/v1/lobbies/lob_1",
            json!({"state":"streaming"}),
        ),
        (
            "coop.lobby.update",
            json!({"lobbyId":"lob_1", "maxGuests":1}),
            "PATCH",
            "/v1/lobbies/lob_1",
            json!({"maxGuests":1}),
        ),
        (
            "coop.lobby.regenerateCode",
            json!({"lobbyId":"lob_1"}),
            "POST",
            "/v1/lobbies/lob_1/code",
            json!({}),
        ),
        (
            "coop.lobby.close",
            json!({"lobbyId":"lob_1"}),
            "DELETE",
            "/v1/lobbies/lob_1",
            Value::Null,
        ),
        (
            "coop.lobby.leave",
            json!({"lobbyId":"lob_1"}),
            "POST",
            "/v1/lobbies/lob_1/leave",
            json!({}),
        ),
        (
            "coop.lobby.invite",
            json!({"lobbyId":"lob_1", "userId":"usr_2"}),
            "POST",
            "/v1/lobbies/lob_1/invites",
            json!({"userId":"usr_2"}),
        ),
        (
            "coop.invite.cancel",
            json!({"inviteId":"inv_1"}),
            "DELETE",
            "/v1/invites/inv_1",
            Value::Null,
        ),
        (
            "coop.invite.accept",
            json!({"inviteId":"inv_1"}),
            "POST",
            "/v1/invites/inv_1/accept",
            json!({}),
        ),
        (
            "coop.invite.decline",
            json!({"inviteId":"inv_1"}),
            "POST",
            "/v1/invites/inv_1/decline",
            json!({}),
        ),
        (
            "coop.invites.get",
            json!({}),
            "GET",
            "/v1/invites",
            Value::Null,
        ),
        (
            "coop.join.code",
            json!({"code":"K7Q-4MX"}),
            "POST",
            "/v1/lobbies/join",
            json!({"code":"K7Q-4MX"}),
        ),
        (
            "coop.join.ask",
            json!({"lobbyId":"lob_1"}),
            "POST",
            "/v1/lobbies/lob_1/join-requests",
            json!({}),
        ),
        (
            "coop.join.approved",
            json!({"requestId":"jrq_1"}),
            "POST",
            "/v1/join-requests/jrq_1/join",
            json!({}),
        ),
        (
            "coop.joinRequest.respond",
            json!({"requestId":"jrq_1", "approve":true}),
            "POST",
            "/v1/join-requests/jrq_1/approve",
            json!({}),
        ),
        (
            "coop.joinRequest.respond",
            json!({"requestId":"jrq_1", "approve":false}),
            "POST",
            "/v1/join-requests/jrq_1/decline",
            json!({}),
        ),
        (
            "coop.member.kick",
            json!({"lobbyId":"lob_1", "userId":"anon_2"}),
            "POST",
            "/v1/lobbies/lob_1/members/anon_2/kick",
            json!({}),
        ),
        (
            "coop.member.slot",
            json!({"lobbyId":"lob_1", "userId":"usr_2", "slot":2}),
            "POST",
            "/v1/lobbies/lob_1/members/usr_2/slot",
            json!({"slot":2}),
        ),
        (
            "coop.iceServers.get",
            json!({}),
            "GET",
            "/v1/ice-servers",
            Value::Null,
        ),
    ];
    for (method, params, http_method, path, body) in cases {
        backend.clear_requests();
        let result = harness
            .call(method, params)
            .unwrap_or_else(|error| panic!("{method}: {error:?}"));
        let requests = backend.requests();
        assert_eq!(requests.len(), 1, "{method}");
        let request = &requests[0];
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            (http_method, path),
            "{method}"
        );
        assert_eq!(request.body, body, "{method}");
        assert_eq!(
            request.authorization.as_deref(),
            Some(&*format!("Bearer {ACCOUNT_TOKEN}")),
            "{method}"
        );
        if http_method == "DELETE" {
            assert_eq!(result, json!({}), "{method}: 204 maps to an empty result");
        } else {
            assert_eq!(result["echo"], path, "{method}");
        }
    }
}

#[test]
fn invalid_params_are_rejected_before_any_request() {
    let backend = MockBackend::start(echo);
    let harness = Harness::new(&backend.url, true);
    for (method, params) in [
        ("social.friends.accept", json!({"requestId":"../me"})),
        ("social.friends.remove", json!({})),
        ("social.users.lookup", json!({"username":""})),
        (
            "social.settings.update",
            json!({"presenceVisibility":"everyone"}),
        ),
        ("social.settings.update", json!({"color":"red"})),
        ("social.settings.update", json!({})),
        ("social.presence.set", json!({"activity":"dancing"})),
        (
            "social.presence.set",
            json!({"activity":"in_queue", "queuePosition":-1}),
        ),
        (
            "coop.lobby.create",
            json!({"game":{"appId":"1","title":"x","artUrl":null}, "maxGuests":4}),
        ),
        (
            "coop.lobby.create",
            json!({"game":{"appId":"1"}, "maxGuests":2}),
        ),
        (
            "coop.lobby.update",
            json!({"lobbyId":"lob_1", "state":"ended"}),
        ),
        ("coop.lobby.update", json!({"lobbyId":"lob_1"})),
        (
            "coop.member.slot",
            json!({"lobbyId":"lob_1", "userId":"usr_2", "slot":0}),
        ),
        ("coop.joinRequest.respond", json!({"requestId":"jrq_1"})),
        ("coop.join.code", json!({"code":"K7Q 4MX"})),
        (
            "coop.signal.send",
            json!({"lobbyId":"lob_1", "to":"usr_2", "payload":"offer"}),
        ),
    ] {
        let error = harness.call(method, params.clone()).unwrap_err();
        assert_eq!(error.code, "invalid_params", "{method} {params}");
    }
    assert!(backend.requests().is_empty());
}

#[test]
fn backend_errors_map_to_stable_codes() {
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/users/lookup?username=ghost" => Reply::error(404, "user_not_found"),
        "/v1/friends/requests" => Reply::error(409, "already_friends"),
        "/v1/lobbies/join" => Reply::error(410, "code_expired"),
        "/v1/friends" => Reply::empty(503),
        "/v1/invites" => Reply::empty(400),
        _ => Reply::json(200, json!([1, 2])),
    });
    let mut harness = Harness::new(&backend.url, true);
    let code = |harness: &Harness, method: &str, params: Value| {
        harness.call(method, params).unwrap_err().code
    };
    assert_eq!(
        code(&harness, "social.users.lookup", json!({"username":"ghost"})),
        "user_not_found"
    );
    assert_eq!(
        code(
            &harness,
            "social.friends.request",
            json!({"username":"mara"})
        ),
        "already_friends"
    );
    assert_eq!(
        code(&harness, "coop.join.code", json!({"code":"K7Q4MX"})),
        "code_expired"
    );
    assert_eq!(
        code(&harness, "social.friends.get", json!({})),
        "social_offline"
    );
    assert_eq!(
        code(&harness, "coop.invites.get", json!({})),
        "invalid_upstream_response"
    );
    assert_eq!(
        code(&harness, "coop.iceServers.get", json!({})),
        "invalid_upstream_response"
    );
    let error = harness
        .call("social.users.lookup", json!({"username":"ghost"}))
        .unwrap_err();
    assert_eq!(error.message, "mock user_not_found (404)");

    backend.set_handler(|_| Reply::error(401, "unauthorized"));
    assert_eq!(
        code(&harness, "social.friends.get", json!({})),
        "social_not_linked"
    );
    let changed = harness.event("social.account.changed", |payload| {
        payload["account"].is_null()
    });
    assert_eq!(changed["connection"], "unlinked");
    assert_eq!(harness.vault().load(ACCOUNT_KEY).unwrap(), None);
    assert_eq!(
        code(&harness, "social.friends.get", json!({})),
        "social_not_linked"
    );
    assert_eq!(
        code(
            &harness,
            "coop.lobby.create",
            json!({"game":{"appId":"1","title":"x","artUrl":null},"maxGuests":1})
        ),
        "social_not_linked"
    );

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let unreachable = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let offline = Harness::new(&unreachable, true);
    assert_eq!(
        offline
            .call("social.friends.get", json!({}))
            .unwrap_err()
            .code,
        "social_offline"
    );
    assert_eq!(
        offline
            .call("social.link.start", json!({}))
            .unwrap_err()
            .code,
        "social_offline"
    );
    assert!(!harness.all_output().contains(ACCOUNT_TOKEN));
}

#[test]
fn a_waking_backend_is_retried_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static FRIENDS: AtomicUsize = AtomicUsize::new(0);
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/friends" if FRIENDS.fetch_add(1, Ordering::SeqCst) == 0 => Reply::empty(502),
        "/v1/friends" => Reply::json(200, json!({"friends":[]})),
        "/v1/invites" => Reply::empty(503),
        "/v1/lobbies/join" => Reply::error(503, "overloaded"),
        _ => echo(request),
    });
    let harness = Harness::new(&backend.url, true);
    assert_eq!(
        harness.call("social.friends.get", json!({})).unwrap()["friends"],
        json!([])
    );
    assert_eq!(
        harness
            .call("coop.invites.get", json!({}))
            .unwrap_err()
            .code,
        "social_offline"
    );
    assert_eq!(
        harness
            .call("coop.join.code", json!({"code":"K7Q4MX"}))
            .unwrap_err()
            .code,
        "overloaded"
    );
    let count = |path: &str| {
        backend
            .requests()
            .iter()
            .filter(|request| request.path == path)
            .count()
    };
    assert_eq!(count("/v1/friends"), 2);
    assert_eq!(count("/v1/invites"), 2);
    assert_eq!(count("/v1/lobbies/join"), 1);

    let failure = |status| super::backend::Failure {
        status,
        transient: true,
        error: SocialError::new("social_offline", "offline"),
    };
    assert!(failure(None).waking(&reqwest::Method::GET));
    assert!(!failure(None).waking(&reqwest::Method::POST));
    assert!(failure(Some(502)).waking(&reqwest::Method::POST));
    assert!(failure(Some(503)).waking(&reqwest::Method::DELETE));
    assert!(!failure(Some(500)).waking(&reqwest::Method::GET));
}

fn link_backend(poll_replies: Vec<Value>, expires_in: chrono::Duration) -> MockBackend {
    let replies = Mutex::new(poll_replies.into_iter());
    let expires_at = (chrono::Utc::now() + expires_in).to_rfc3339();
    MockBackend::start(move |request| match request.path.as_str() {
        "/v1/auth/discord/start" => Reply::json(
            201,
            json!({
                "loginId":"login_1", "pollSecret":"poll_secret_value",
                "loginUrl":"https://discord.com/oauth2/authorize?client_id=1&scope=identify&state=s",
                "expiresAt":expires_at, "intervalMs":100,
            }),
        ),
        "/v1/auth/discord/poll" => {
            assert_eq!(
                request.body,
                json!({"loginId":"login_1", "pollSecret":"poll_secret_value"})
            );
            Reply::json(
                200,
                replies
                    .lock()
                    .unwrap()
                    .next()
                    .unwrap_or(json!({"status":"pending"})),
            )
        }
        "/v1/me" => Reply::json(
            200,
            json!({"user":user("usr_9"), "settings":{"presenceVisibility":"friends","shareActivity":true,"joinRequests":"friends","botDmInvites":true}}),
        ),
        _ => Reply::json(
            200,
            json!({"friends":[], "incoming":[], "outgoing":[], "blocked":[]}),
        ),
    })
}

#[test]
fn discord_link_polls_until_complete_and_persists_the_token() {
    let backend = link_backend(
        vec![
            json!({"status":"pending"}),
            json!({"status":"complete", "token":"tok_linked_secret", "user":user("usr_9")}),
        ],
        chrono::Duration::minutes(10),
    );
    let mut harness = Harness::new(&backend.url, false);
    let started = harness.call("social.link.start", json!({})).unwrap();
    assert_eq!(started["loginId"], "login_1");
    assert!(
        started["loginUrl"]
            .as_str()
            .unwrap()
            .starts_with("https://discord.com/")
    );
    assert!(started.get("pollSecret").is_none());
    harness.event("social.account.changed", |payload| {
        payload["link"] == json!({"loginId":"login_1","status":"pending"})
    });
    let complete = harness.event("social.account.changed", |payload| {
        payload["link"]["status"] == "complete"
    });
    assert_eq!(complete["account"]["user"]["id"], "usr_9");
    assert_eq!(backend.count("POST", "/v1/auth/discord/poll"), 2);
    let stored: Value =
        serde_json::from_str(&harness.vault().load(ACCOUNT_KEY).unwrap().unwrap()).unwrap();
    assert_eq!(stored["token"], "tok_linked_secret");
    let mut socket = backend.accept("tok_linked_secret", user("usr_9"));
    harness.connection("connected");
    wait_until(|| harness.state()["account"]["settings"]["botDmInvites"] == true);
    assert!(harness.all_output().contains(r#""botDmInvites":true"#));
    assert!(socket.recv_type("ping", Duration::from_secs(2)).is_some());
    let output = harness.all_output();
    assert!(!output.contains("tok_linked_secret"));
    assert!(!output.contains("poll_secret_value"));
    assert!(harness.state()["account"]["linked"] == true);
}

#[test]
fn discord_link_reports_denied_expired_and_cancelled() {
    let denied = link_backend(
        vec![json!({"status":"denied"})],
        chrono::Duration::minutes(10),
    );
    let mut harness = Harness::new(&denied.url, false);
    harness.call("social.link.start", json!({})).unwrap();
    harness.event("social.account.changed", |payload| {
        payload["link"]["status"] == "denied"
    });
    assert!(harness.state()["account"].is_null());

    let expired = link_backend(
        vec![json!({"status":"expired"})],
        chrono::Duration::minutes(10),
    );
    let mut harness = Harness::new(&expired.url, false);
    harness.call("social.link.start", json!({})).unwrap();
    harness.event("social.account.changed", |payload| {
        payload["link"]["status"] == "expired"
    });

    let deadline = link_backend(Vec::new(), chrono::Duration::milliseconds(700));
    let mut harness = Harness::new(&deadline.url, false);
    harness.call("social.link.start", json!({})).unwrap();
    harness.event("social.account.changed", |payload| {
        payload["link"]["status"] == "expired"
    });
    assert!(deadline.count("POST", "/v1/auth/discord/poll") >= 2);

    let cancelled = link_backend(Vec::new(), chrono::Duration::minutes(10));
    let mut harness = Harness::new(&cancelled.url, false);
    harness.call("social.link.start", json!({})).unwrap();
    wait_until(|| cancelled.count("POST", "/v1/auth/discord/poll") >= 1);
    harness
        .call("social.link.cancel", json!({"loginId":"login_1"}))
        .unwrap();
    harness.event("social.account.changed", |payload| {
        payload["link"]["status"] == "cancelled"
    });
    let polls = cancelled.count("POST", "/v1/auth/discord/poll");
    thread::sleep(Duration::from_millis(400));
    assert!(cancelled.count("POST", "/v1/auth/discord/poll") <= polls + 1);
    assert!(harness.state().get("link").is_none());
    assert!(harness.vault().load(ACCOUNT_KEY).unwrap().is_none());
}

#[test]
fn token_vault_prefers_the_os_store_and_falls_back_to_a_private_file() {
    let directory = tempfile::tempdir().unwrap();
    let secure = TokenVault::memory(directory.path().join("secure"), true);
    assert_eq!(
        secure.save(ACCOUNT_KEY, r#"{"token":"a"}"#).unwrap(),
        "secure-store"
    );
    assert!(!directory.path().join("secure").exists());
    assert_eq!(
        secure.load(ACCOUNT_KEY).unwrap().as_deref(),
        Some(r#"{"token":"a"}"#)
    );

    let root = directory.path().join("fallback");
    let fallback = TokenVault::memory(root.clone(), false);
    assert_eq!(
        fallback.save(ACCOUNT_KEY, r#"{"token":"b"}"#).unwrap(),
        "local-file"
    );
    let files = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(
        fallback.load(ACCOUNT_KEY).unwrap().as_deref(),
        Some(r#"{"token":"b"}"#)
    );
    assert!(fallback.save(ACCOUNT_KEY, "not json").is_err());
    assert!(fallback.save(ACCOUNT_KEY, &"x".repeat(4096)).is_err());
    assert_eq!(
        fallback.delete(ACCOUNT_KEY).unwrap_err(),
        "locked",
        "the locked OS store reports its own failure"
    );
    assert!(
        std::fs::read_dir(&root).unwrap().next().is_none(),
        "the fallback file is removed anyway"
    );
    assert_eq!(fallback.load(ACCOUNT_KEY).unwrap_err(), "locked");
    secure.delete(ACCOUNT_KEY).unwrap();
    assert_eq!(secure.load(ACCOUNT_KEY).unwrap(), None);
}

#[test]
fn the_os_store_entry_is_scoped_to_the_data_directory() {
    let first = account_key(Path::new("/profiles/a"));
    assert_eq!(first, account_key(Path::new("/profiles/a")));
    assert_ne!(first, account_key(Path::new("/profiles/b")));
    assert!(first.starts_with("discord-account:"));
    assert_eq!(first.len(), "discord-account:".len() + 16);
}

#[test]
fn a_saved_account_is_restored_without_exposing_the_token() {
    let backend = MockBackend::start(echo);
    let mut harness = Harness::new(&backend.url, true);
    let state = harness.state();
    assert_eq!(state["account"]["user"], user("usr_1"));
    assert!(state["guest"].is_null());
    backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    assert!(!harness.all_output().contains(ACCOUNT_TOKEN));
    assert!(!format!("{:?}", Token(ACCOUNT_TOKEN.to_owned())).contains(ACCOUNT_TOKEN));
}

#[test]
fn unlink_signs_out_then_deletes_the_saved_token() {
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/auth/logout" if request.method == "POST" => Reply::empty(500),
        _ => echo(request),
    });
    let mut harness = Harness::new(&backend.url, true);
    let _socket = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    assert_eq!(
        harness.call("social.unlink", json!({})).unwrap_err().code,
        "social_offline"
    );
    assert!(harness.vault().load(ACCOUNT_KEY).unwrap().is_some());

    backend.set_handler(echo);
    assert_eq!(harness.call("social.unlink", json!({})).unwrap(), json!({}));
    let requests = backend.requests();
    assert!(
        !requests
            .iter()
            .any(|request| request.method == "DELETE" && request.path == "/v1/me")
    );
    let logouts = requests
        .into_iter()
        .filter(|request| request.method == "POST" && request.path == "/v1/auth/logout")
        .collect::<Vec<_>>();
    assert_eq!(logouts.len(), 2);
    assert_eq!(
        logouts[1].authorization.as_deref(),
        Some(&*format!("Bearer {ACCOUNT_TOKEN}"))
    );
    assert_eq!(harness.vault().load(ACCOUNT_KEY).unwrap(), None);
    harness.connection("unlinked");
    assert!(harness.state()["account"].is_null());
    assert_eq!(harness.call("social.unlink", json!({})).unwrap(), json!({}));
}

#[test]
fn presence_is_coalesced_to_one_send_per_interval_and_replayed_after_reconnect() {
    let backend = MockBackend::start(echo);
    let mut harness = Harness::new(&backend.url, true);
    let mut socket = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    let service = &harness.service;
    let set = |activity: &str| {
        let queue_position = if activity == "in_queue" {
            json!(7)
        } else {
            Value::Null
        };
        service
            .handle("social.presence.set", &json!({"activity":activity, "game":null, "since":null, "queuePosition":queue_position, "lobbyId":null}))
            .unwrap();
    };
    let frames = thread::scope(|scope| {
        scope.spawn(|| {
            for activity in [
                "browsing_library",
                "browsing_store",
                "in_settings",
                "online",
                "in_queue",
            ] {
                set(activity);
            }
        });
        socket.collect("presence.set", Duration::from_millis(1_300))
    });
    assert!(!frames.is_empty() && frames.len() <= 2, "{frames:?}");
    let last = &frames.last().unwrap().1;
    assert_eq!(
        *last,
        json!({"type":"presence.set", "activity":"in_queue", "game":null, "since":null, "queuePosition":7, "lobbyId":null})
    );
    if let [(first, _), (second, _)] = frames.as_slice() {
        assert!(second.duration_since(*first) >= Duration::from_millis(350));
    }

    let (mut burst, producer_elapsed) = thread::scope(|scope| {
        let producer = scope.spawn(|| {
            let started = Instant::now();
            for activity in ["playing", "in_lobby", "idle", "online", "playing", "idle"] {
                set(activity);
                thread::sleep(Duration::from_millis(150));
            }
            started.elapsed()
        });
        let frames = socket.collect("presence.set", Duration::from_millis(1_500));
        (frames, producer.join().unwrap())
    });
    burst.extend(socket.collect("presence.set", Duration::from_millis(1_000)));
    let allowed = producer_elapsed.as_millis() / 400 + 2;
    assert!(
        burst.len() as u128 <= allowed,
        "six sets over {producer_elapsed:?} with a 400 ms interval allow {allowed} sends: {burst:?}"
    );
    assert_eq!(burst.last().unwrap().1["activity"], "idle");
    for pair in burst.windows(2) {
        assert!(pair[1].0.duration_since(pair[0].0) >= Duration::from_millis(350));
    }

    socket.sever();
    harness.connection("offline");
    let mut replayed = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    let frame = replayed
        .recv_type("presence.set", Duration::from_secs(2))
        .expect("replayed presence");
    assert_eq!(frame["activity"], "idle");
}

#[test]
fn the_socket_pings_reconnects_and_reports_connection_state() {
    let backend = MockBackend::start(echo);
    let mut harness = Harness::new(&backend.url, true);
    let mut first = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    let started = Instant::now();
    assert!(first.recv_type("ping", Duration::from_secs(2)).is_some());
    let second_ping = first.recv_type("ping", Duration::from_secs(2));
    assert!(second_ping.is_some());
    assert!(started.elapsed() >= Duration::from_millis(500));
    first.sever();
    harness.connection("offline");
    harness.connection("connecting");
    let mut second = backend.next_socket();
    assert_eq!(
        second.recv(Duration::from_secs(2)).unwrap()["token"],
        ACCOUNT_TOKEN
    );
    drop(second);
    harness.connection("offline");
    let _third = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");

    let unlinked = Harness::new(&backend.url, false);
    assert_eq!(unlinked.state()["connection"], "unlinked");
}

#[test]
fn a_silent_server_is_treated_as_disconnected() {
    let backend = MockBackend::start(echo);
    let directory = tempfile::tempdir().unwrap();
    let vault = TokenVault::memory(directory.path().join("social"), true);
    vault
        .save(
            ACCOUNT_KEY,
            &json!({"token":ACCOUNT_TOKEN, "user":user("usr_1")}).to_string(),
        )
        .unwrap();
    let (output, events) = mpsc::channel();
    let timing = Timing {
        silence: Duration::from_millis(500),
        ping: Duration::from_secs(30),
        ..fast_timing()
    };
    let _service = SocialService::start(
        vault,
        ACCOUNT_KEY.to_owned(),
        output,
        &json!({"socialBackendUrl":backend.url}),
        timing,
    )
    .unwrap();
    let _socket = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    let states = (0..3)
        .map(|_| {
            loop {
                let event = events.recv_timeout(Duration::from_secs(5)).unwrap();
                if event["name"] == "social.connection.changed" {
                    break event["payload"]["state"].clone();
                }
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [json!("connected"), json!("offline"), json!("connecting")]
    );
}

#[test]
fn socket_messages_become_core_events() {
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/friends" => Reply::json(
            200,
            json!({"friends":[{"user":user("usr_2"), "presence":{"status":"online"}, "since":"2026-10-01T00:00:00Z"}], "incoming":[], "outgoing":[], "blocked":[]}),
        ),
        _ => echo(request),
    });
    let mut harness = Harness::new(&backend.url, true);
    let mut socket = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    harness.event("social.friends.changed", |_| true);
    wait_until(|| backend.count("GET", "/v1/friends") == 1);
    let invite = json!({"id":"inv_1", "lobbyId":"lob_1", "state":"pending"});
    let request = json!({"id":"jrq_1", "lobbyId":"lob_1", "state":"pending"});
    let cases = [
        (
            json!({"type":"presence.update", "userId":"usr_2", "presence":{"status":"offline"}}),
            "social.presence.changed",
            json!({"userId":"usr_2", "presence":{"status":"offline"}}),
        ),
        (
            json!({"type":"invite.received", "invite":invite}),
            "coop.invite.received",
            json!({"invite":invite}),
        ),
        (
            json!({"type":"invite.updated", "invite":invite}),
            "coop.invite.updated",
            json!({"invite":invite}),
        ),
        (
            json!({"type":"joinRequest.received", "joinRequest":request}),
            "coop.joinRequest.received",
            json!({"joinRequest":request}),
        ),
        (
            json!({"type":"joinRequest.updated", "joinRequest":request}),
            "coop.joinRequest.updated",
            json!({"joinRequest":request}),
        ),
        (
            json!({"type":"lobby.updated", "lobby":{"id":"lob_1"}}),
            "coop.lobby.updated",
            json!({"lobby":{"id":"lob_1"}}),
        ),
        (
            json!({"type":"lobby.ended", "lobbyId":"lob_1", "reason":"host_lost"}),
            "coop.lobby.ended",
            json!({"lobbyId":"lob_1", "reason":"host_lost"}),
        ),
        (
            json!({"type":"lobby.kicked", "lobbyId":"lob_1"}),
            "coop.lobby.kicked",
            json!({"lobbyId":"lob_1"}),
        ),
        (
            json!({"type":"signal", "lobbyId":"lob_1", "from":"usr_2", "payload":{"kind":"offer", "sdp":"v=0"}}),
            "coop.signal",
            json!({"lobbyId":"lob_1", "from":"usr_2", "payload":{"kind":"offer", "sdp":"v=0"}}),
        ),
    ];
    for (frame, name, payload) in cases {
        socket.send(frame);
        assert_eq!(harness.event(name, |_| true), payload, "{name}");
    }
    socket.send(json!({"type":"pong"}));
    socket.send(json!({"type":"unknown.kind"}));
    socket.send(json!({"type":"friends.changed"}));
    let snapshot = harness.event("social.friends.changed", |_| true);
    assert_eq!(snapshot["friends"][0]["user"]["id"], "usr_2");
    assert_eq!(backend.count("GET", "/v1/friends"), 2);
}

#[test]
fn signals_require_a_connection_and_a_bounded_queue() {
    let backend = MockBackend::start(echo);
    let mut harness = Harness::new(&backend.url, true);
    let signal = json!({"lobbyId":"lob_1", "to":"usr_2", "payload":{"kind":"candidate", "candidate":"c", "mid":null}});
    let mut socket = backend.next_socket();
    assert_eq!(
        harness
            .call("coop.signal.send", signal.clone())
            .unwrap_err()
            .code,
        "social_offline"
    );
    assert!(socket.recv(Duration::from_secs(2)).is_some());
    socket.send(json!({"type":"ready", "user":user("usr_1"), "guest":null}));
    harness.connection("connected");
    harness.call("coop.signal.send", signal.clone()).unwrap();
    let sent = socket.recv_type("signal", Duration::from_secs(2)).unwrap();
    assert_eq!(
        sent,
        json!({"type":"signal", "lobbyId":"lob_1", "to":"usr_2", "payload":{"kind":"candidate", "candidate":"c", "mid":null}})
    );
    let oversized =
        json!({"lobbyId":"lob_1", "to":"usr_2", "payload":{"sdp":"x".repeat(16 * 1024)}});
    assert_eq!(
        harness
            .call("coop.signal.send", oversized)
            .unwrap_err()
            .code,
        "invalid_params"
    );

    let idle = Harness::new(&backend.url, false);
    {
        let mut state = idle.service.shared.lock();
        state.shutdown = true;
        state.connection = Connection::Connected;
    }
    for _ in 0..MAXIMUM_OUTBOUND_FRAMES {
        idle.call("coop.signal.send", signal.clone()).unwrap();
    }
    assert_eq!(
        idle.call("coop.signal.send", signal).unwrap_err().code,
        "busy"
    );
    assert_eq!(
        idle.service.shared.lock().outbound.len(),
        MAXIMUM_OUTBOUND_FRAMES
    );
}

#[test]
fn ice_servers_are_cached_until_ten_minutes_before_expiry() {
    let ttl = Arc::new(Mutex::new(21_600_u64));
    let reply_ttl = Arc::clone(&ttl);
    let backend = MockBackend::start(move |_| {
        Reply::json(
            200,
            json!({"iceServers":[{"urls":["stun:nl.example:3478"]}], "relays":[], "ttlSeconds":*reply_ttl.lock().unwrap()}),
        )
    });
    let harness = Harness::new(&backend.url, true);
    let first = harness.call("coop.iceServers.get", json!({})).unwrap();
    let second = harness.call("coop.iceServers.get", json!({})).unwrap();
    assert_eq!(first, second);
    assert_eq!(backend.count("GET", "/v1/ice-servers"), 1);

    harness.service.shared.lock().ice = None;
    *ttl.lock().unwrap() = 300;
    harness.call("coop.iceServers.get", json!({})).unwrap();
    harness.call("coop.iceServers.get", json!({})).unwrap();
    assert_eq!(backend.count("GET", "/v1/ice-servers"), 3);
}

#[test]
fn joining_by_code_without_an_account_uses_a_guest_token() {
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/auth/guest" => {
            assert_eq!(request.authorization, None);
            Reply::json(
                200,
                json!({"token":"tok_guest_secret", "guest":{"id":"anon_7", "displayName":request.body["displayName"]}}),
            )
        }
        "/v1/lobbies/join" => Reply::json(
            200,
            json!({"lobby":{"id":"lob_1"}, "slot":1, "iceServers":[]}),
        ),
        _ => echo(request),
    });
    let mut harness = Harness::new(&backend.url, false);
    let joined = harness
        .call(
            "coop.join.code",
            json!({"code":"k7q4mx", "guestDisplayName":" Mara "}),
        )
        .unwrap();
    assert_eq!(joined["slot"], 1);
    let requests = backend.requests();
    assert_eq!(requests[0].body, json!({"displayName":"Mara"}));
    assert_eq!(
        requests[1].authorization.as_deref(),
        Some("Bearer tok_guest_secret")
    );
    assert_eq!(requests[1].body, json!({"code":"k7q4mx"}));
    assert_eq!(
        harness.state()["guest"],
        json!({"id":"anon_7", "displayName":"Mara"})
    );
    harness
        .call("coop.join.code", json!({"code":"K7Q-4MX"}))
        .unwrap();
    assert_eq!(backend.count("POST", "/v1/auth/guest"), 1);
    harness
        .call("coop.lobby.leave", json!({"lobbyId":"lob_1"}))
        .unwrap();
    assert_eq!(
        harness
            .call("social.friends.get", json!({}))
            .unwrap_err()
            .code,
        "social_not_linked"
    );
    let mut socket = backend.next_socket();
    assert_eq!(
        socket.recv(Duration::from_secs(2)).unwrap(),
        json!({"type":"auth", "token":"tok_guest_secret"})
    );
    socket.send(json!({"type":"ready", "user":null, "guest":{"id":"anon_7"}}));
    harness.connection("connected");
    assert!(!harness.all_output().contains("tok_guest_secret"));
}

#[test]
fn changing_the_backend_url_reconnects_to_the_new_backend() {
    let old = MockBackend::start(echo);
    let new = MockBackend::start(echo);
    let mut harness = Harness::new(&old.url, true);
    let _socket = old.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    harness
        .service
        .configure(&json!({"socialBackendUrl":new.url}));
    let _moved = new.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    harness.call("social.friends.get", json!({})).unwrap();
    wait_until(|| new.count("GET", "/v1/friends") >= 2);
}

#[test]
fn the_socket_tunnels_through_an_http_connect_proxy() {
    let backend = MockBackend::start(echo);
    let backend_address = backend.url.trim_start_matches("http://").to_owned();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("http://user:p%40ss@{}", proxy.local_addr().unwrap());
    let (seen_tx, seen) = mpsc::channel();
    thread::spawn(move || {
        let (mut client, _) = proxy.accept().unwrap();
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            client.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        seen_tx.send(String::from_utf8(head).unwrap()).unwrap();
        let upstream = std::net::TcpStream::connect(&backend_address).unwrap();
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .unwrap();
        let (mut client_read, mut upstream_write) =
            (client.try_clone().unwrap(), upstream.try_clone().unwrap());
        thread::spawn(move || std::io::copy(&mut client_read, &mut upstream_write));
        let (mut upstream_read, mut client_write) = (upstream, client);
        let _ = std::io::copy(&mut upstream_read, &mut client_write);
    });
    let config = Config::from_settings(&json!({
        "socialBackendUrl":backend.url, "sessionProxyEnabled":true, "sessionProxyUrl":proxy_url,
    }));
    let mut client = socket::connect(&config, Duration::from_secs(5)).unwrap();
    let head = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let port = backend.url.rsplit(':').next().unwrap();
    assert!(
        head.starts_with(&format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n")),
        "{head}"
    );
    assert!(
        head.contains("Proxy-Authorization: Basic dXNlcjpwQHNz\r\n"),
        "{head}"
    );
    let mut peer = backend.next_socket();
    client
        .send(tungstenite::Message::text(r#"{"type":"ping"}"#))
        .unwrap();
    assert_eq!(
        peer.recv(Duration::from_secs(2)).unwrap(),
        json!({"type":"ping"})
    );
}

#[test]
fn close_code_4010_unlinks_without_reconnecting() {
    let backend = MockBackend::start(echo);
    let mut harness = Harness::new(&backend.url, true);
    let socket = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    socket.close_with(4010);
    let changed = harness.event("social.account.changed", |payload| {
        payload["account"].is_null()
    });
    assert_eq!(changed["connection"], "unlinked");
    assert_eq!(harness.vault().load(ACCOUNT_KEY).unwrap(), None);
    assert!(
        backend
            .next_socket_within(Duration::from_millis(800))
            .is_none()
    );
}

#[test]
fn close_code_4003_rechecks_the_account_and_unlinks_on_401() {
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/me" => Reply::error(401, "unauthorized"),
        _ => echo(request),
    });
    let mut harness = Harness::new(&backend.url, true);
    let mut socket = backend.next_socket();
    assert_eq!(socket.recv(Duration::from_secs(2)).unwrap()["type"], "auth");
    socket.close_with(4003);
    harness.event("social.account.changed", |payload| {
        payload["account"].is_null()
    });
    assert_eq!(backend.count("GET", "/v1/me"), 1);
    assert_eq!(harness.vault().load(ACCOUNT_KEY).unwrap(), None);
}

#[test]
fn restarts_and_error_frames_keep_the_session() {
    let backend = MockBackend::start(echo);
    let mut harness = Harness::new(&backend.url, true);
    let mut socket = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    socket.send(json!({"type":"error", "error":{"code":"payload_too_large", "message":"too big"}}));
    socket.send(json!({"type":"lobby.kicked", "lobbyId":"lob_1"}));
    assert_eq!(
        harness.event("coop.lobby.kicked", |_| true),
        json!({"lobbyId":"lob_1"})
    );
    socket.close_with(1012);
    harness.connection("offline");
    let _again = backend.accept(ACCOUNT_TOKEN, user("usr_1"));
    harness.connection("connected");
    assert!(harness.vault().load(ACCOUNT_KEY).unwrap().is_some());
}

#[test]
fn coop_report_posts_a_bounded_lobby_report() {
    let backend = MockBackend::start(|request| match request.path.as_str() {
        "/v1/lobbies/lob_gone/reports" => Reply::error(404, "lobby_not_found"),
        "/v1/lobbies/lob_busy/reports" => Reply::empty(503),
        _ => Reply::json(200, json!({"ok":true})),
    });
    let harness = Harness::new(&backend.url, true);
    let report = json!({
        "lobbyId":"lob_1", "event":"first_frame", "path":"relay", "relayId":"nl",
        "codec":"h264", "decoder":"hardware", "connectMs":1234, "firstFrameMs":2345, "reason":null,
    });
    assert_eq!(
        harness.call("coop.report", report.clone()).unwrap(),
        json!({"ok":true})
    );
    let requests = backend.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path.as_str()),
        ("POST", "/v1/lobbies/lob_1/reports")
    );
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some(&*format!("Bearer {ACCOUNT_TOKEN}"))
    );
    let mut body = report.clone();
    body.as_object_mut().unwrap().remove("lobbyId");
    assert_eq!(requests[0].body, body);

    backend.clear_requests();
    harness
        .call(
            "coop.report",
            json!({"lobbyId":"lob_1", "event":"ended", "reason":"host ended"}),
        )
        .unwrap();
    assert_eq!(
        backend.requests()[0].body,
        json!({"event":"ended", "path":null, "relayId":null, "codec":null, "decoder":null,
               "connectMs":null, "firstFrameMs":null, "reason":"host ended"})
    );

    backend.clear_requests();
    let with = |key: &str, value: Value| {
        let mut params = report.clone();
        params[key] = value;
        params
    };
    for params in [
        with("event", json!("started")),
        with("event", Value::Null),
        with("path", json!("lan")),
        with("decoder", json!("gpu")),
        with("relayId", json!("x".repeat(65))),
        with("codec", json!("h 264")),
        with("lobbyId", json!("l".repeat(65))),
        with("connectMs", json!(-1)),
        with("connectMs", json!(1.5)),
        with("firstFrameMs", json!(600_000)),
        with("reason", json!("r".repeat(257))),
        with("reason", json!("")),
    ] {
        assert_eq!(
            harness
                .call("coop.report", params.clone())
                .unwrap_err()
                .code,
            "invalid_params",
            "{params}"
        );
    }
    assert!(backend.requests().is_empty());
    harness
        .call("coop.report", with("firstFrameMs", json!(599_999)))
        .unwrap();
    harness
        .call("coop.report", with("reason", json!("r".repeat(256))))
        .unwrap();

    assert_eq!(
        harness
            .call("coop.report", with("lobbyId", json!("lob_gone")))
            .unwrap_err()
            .code,
        "lobby_not_found"
    );
    backend.clear_requests();
    assert_eq!(
        harness
            .call("coop.report", with("lobbyId", json!("lob_busy")))
            .unwrap_err()
            .code,
        "social_offline"
    );
    assert!(backend.count("POST", "/v1/lobbies/lob_busy/reports") >= 1);

    let unlinked = Harness::new(&backend.url, false);
    assert_eq!(
        unlinked.call("coop.report", report).unwrap_err().code,
        "social_not_linked"
    );
}

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
