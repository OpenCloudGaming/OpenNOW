use opennow_plugin_api::media::*;
use opennow_plugin_api::provider::*;
use opennow_plugin_api::{CatalogQuery, Coverage};
use opennow_sdk_demo::{CATALOG_REVISION, DemoProvider, fixture_video, now_ms};
use std::fs::{self, OpenOptions};
use std::io::Write;

fn text<const N: usize>(value: &str) -> Text<N> {
    Text::new(value).unwrap()
}
fn call(provider: &mut DemoProvider, request: ProviderRequest) -> ProviderResponseV2 {
    let envelope = HostRequestV2 {
        v: Version2,
        epoch: 7.try_into().unwrap(),
        id: text("test"),
        timeout_ms: 10000,
        request,
    };
    let reply = provider.handle(&envelope);
    reply.validate_for(&envelope).unwrap();
    reply
}
fn success(response: ProviderResponseV2) -> ProviderReply {
    match response.outcome {
        ProviderOutcome::Success { reply } => *reply,
        ProviderOutcome::Failure { error } => panic!("unexpected {error:?}"),
    }
}
fn error(response: ProviderResponseV2) -> ProviderErrorCode {
    match response.outcome {
        ProviderOutcome::Failure { error } => error.code,
        ProviderOutcome::Success { .. } => panic!("unexpected success"),
    }
}
fn begin(provider: &mut DemoProvider, authority: &str, remember: bool) -> AttemptId {
    match success(call(
        provider,
        ProviderRequest::AuthBegin(BeginAuth {
            authority: Some(AuthorityId::new(authority).unwrap()),
            kind: AuthKind::Pairing,
            remember,
        }),
    )) {
        ProviderReply::AuthBegin(AuthState::Pending {
            challenge:
                AuthChallenge::Pairing {
                    attempt,
                    code: Some(code),
                    ..
                },
        }) => {
            assert_eq!(code.expose_secret(), "SDK-DEMO");
            attempt
        }
        _ => panic!("wrong challenge"),
    }
}
fn approve(provider: &mut DemoProvider, attempt: &AttemptId) {
    assert!(matches!(
        success(call(
            provider,
            ProviderRequest::AuthPoll(AuthAttempt {
                attempt: attempt.clone()
            })
        )),
        ProviderReply::AuthPoll(AuthState::Authorized { .. })
    ));
}
fn complete(provider: &mut DemoProvider, attempt: &AttemptId) -> AccountScope {
    match success(call(
        provider,
        ProviderRequest::AuthComplete(CompleteAuth {
            attempt: attempt.clone(),
            proof: None,
        }),
    )) {
        ProviderReply::AuthComplete(AuthState::SignedIn { account, revision }) => AccountScope {
            account: account.key,
            revision,
        },
        _ => panic!("not signed in"),
    }
}
fn login(provider: &mut DemoProvider, authority: &str) -> AccountScope {
    let attempt = begin(provider, authority, true);
    approve(provider, &attempt);
    complete(provider, &attempt)
}
fn target() -> LaunchTarget {
    LaunchTarget {
        game: GameId::new("demo-01").unwrap(),
        variant: VariantId::new("fixture").unwrap(),
    }
}
fn offer() -> NativeOffer {
    NativeOffer {
        version: 1,
        offer_id: OfferId::new("native-offer").unwrap(),
        runtime_epoch: 9,
        expires_at_ms: now_ms() + 60000,
        video_formats: List::new(vec![VideoSupport {
            encoding: VideoEncoding::H264AnnexB,
            bit_depth: 8,
            chroma: Chroma::Yuv420,
            dynamic_range: DynamicRange::Sdr,
            max_width: 1920,
            max_height: 1080,
            max_fps: 60,
        }])
        .unwrap(),
        audio_formats: List::new(vec![AudioFormat {
            codec: AudioCodec::Opus,
            sample_rate: 48000,
            channels: 2,
        }])
        .unwrap(),
        input: InputCapabilities {
            keyboard: true,
            relative_mouse: true,
            absolute_mouse: true,
            text: true,
            gamepad_slots: 4,
            rumble: true,
        },
        limits: MediaLimits {
            max_video_access_unit_bytes: 1048576,
            max_audio_packet_bytes: 65536,
            max_buffered_video_bytes: 2097152,
            max_buffered_video_frames: 4,
            max_buffered_audio_ms: 100,
            max_control_message_bytes: 65536,
            max_pending_input_events: 128,
        },
    }
}
fn create_request(scope: &AccountScope, operation: &str) -> CreateSession {
    CreateSession {
        scope: Some(scope.clone()),
        operation: OperationId::new(operation).unwrap(),
        target: target(),
        catalog_revision: text(CATALOG_REVISION),
        settings_revision: 0,
        preferences: StreamPreferences {
            video: RequestedVideo {
                width: 320,
                height: 180,
                encoding: None,
                fps: None,
                bit_depth: 8,
                chroma: Chroma::Yuv420,
                hdr: false,
            },
            bitrate_kbps: 10000,
        },
        offer: offer(),
    }
}
fn create(
    provider: &mut DemoProvider,
    scope: &AccountScope,
    operation: &str,
) -> (SessionView, AllocationTicket) {
    let response = call(
        provider,
        ProviderRequest::SessionCreate(create_request(scope, operation)),
    );
    let ticket = response.allocation.clone().unwrap();
    let ProviderReply::SessionCreate(reply) = success(response) else {
        panic!("not created")
    };
    (reply.session, ticket)
}
fn settle(provider: &mut DemoProvider, ticket: &AllocationTicket, decision: Acceptance) {
    assert!(matches!(
        success(call(
            provider,
            ProviderRequest::SessionResolveAllocation(ResolveAllocation {
                operation: ticket.operation.clone(),
                receipt: ticket.receipt.clone(),
                decision
            })
        )),
        ProviderReply::SessionResolveAllocation(CleanupState::Resolved)
    ));
}

#[test]
fn pairing_approval_is_distinct_from_commit_and_idempotent_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let attempt = begin(&mut provider, "demo-blue", true);
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::AuthComplete(CompleteAuth {
                attempt: attempt.clone(),
                proof: None
            })
        )),
        ProviderErrorCode::AuthRequired
    );
    approve(&mut provider, &attempt);
    assert!(matches!(
        success(call(&mut provider, ProviderRequest::AuthStatus(Empty {}))),
        ProviderReply::AuthStatus(AuthState::SignedOut)
    ));
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = complete(&mut provider, &attempt);
    assert_eq!(complete(&mut provider, &attempt), scope);
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    assert_eq!(complete(&mut provider, &attempt), scope);
    assert!(matches!(
        success(call(&mut provider, ProviderRequest::AuthStatus(Empty {}))),
        ProviderReply::AuthStatus(AuthState::SignedIn { .. })
    ));
}

#[test]
fn pairing_cancel_and_temporary_login_do_not_restore_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let cancelled = begin(&mut provider, "demo-blue", true);
    success(call(
        &mut provider,
        ProviderRequest::AuthCancel(AuthAttempt {
            attempt: cancelled.clone(),
        }),
    ));
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::AuthPoll(AuthAttempt {
                attempt: cancelled.clone()
            })
        )),
        ProviderErrorCode::Cancelled
    );
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::AuthComplete(CompleteAuth {
                attempt: cancelled,
                proof: None
            })
        )),
        ProviderErrorCode::Cancelled
    );
    let temporary = begin(&mut provider, "demo-blue", false);
    approve(&mut provider, &temporary);
    let scope = complete(&mut provider, &temporary);
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    assert!(matches!(
        success(call(&mut provider, ProviderRequest::AuthStatus(Empty {}))),
        ProviderReply::AuthStatus(AuthState::SignedOut)
    ));
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::AccountsSelect(SelectAccount {
                account: scope.account,
                pin: None
            })
        )),
        ProviderErrorCode::AuthRequired
    );
}

#[test]
fn catalog_pages_are_scoped_searchable_and_launch_authorized() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    let request = CatalogRequest {
        scope: CatalogScope::Account {
            scope: scope.clone(),
        },
        query: CatalogQuery {
            limit: 3,
            ..Default::default()
        },
    };
    let ProviderReply::CatalogLibrary(first) = success(call(
        &mut provider,
        ProviderRequest::CatalogLibrary(request.clone()),
    )) else {
        panic!("not page")
    };
    assert_eq!(first.items.len(), 3);
    assert_eq!(first.coverage, Coverage::Complete);
    assert_eq!(first.items[0].title.as_str(), "OpenNOW SDK demo 01");
    let mut second = request.clone();
    second.query.cursor = first.next_cursor.map(|cursor| cursor.as_str().to_owned());
    let ProviderReply::CatalogLibrary(page) = success(call(
        &mut provider,
        ProviderRequest::CatalogLibrary(second.clone()),
    )) else {
        panic!("not page")
    };
    assert_eq!(page.items[0].id.as_str(), "demo-04");
    second.query.query = "24".into();
    assert_eq!(
        error(call(&mut provider, ProviderRequest::CatalogLibrary(second))),
        ProviderErrorCode::ScopeChanged
    );
    let ProviderReply::CatalogDetails(details) = success(call(
        &mut provider,
        ProviderRequest::CatalogDetails(GameRequest {
            scope: request.scope,
            game: target().game,
        }),
    )) else {
        panic!("not details")
    };
    assert!(
        details
            .description
            .unwrap()
            .as_str()
            .contains("No commercial game")
    );
    assert!(matches!(
        success(call(
            &mut provider,
            ProviderRequest::LaunchInspect(InspectLaunch {
                scope: Some(scope),
                target: target(),
                catalog_revision: text(CATALOG_REVISION)
            })
        )),
        ProviderReply::LaunchInspect(LaunchDecision::Ready { .. })
    ));
}

#[test]
fn creates_receipts_and_stop_are_durable_and_never_allocate_twice() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    let (session, ticket) = create(&mut provider, &scope, "create-1");
    let (replayed, replayed_ticket) = create(&mut provider, &scope, "create-1");
    assert_eq!(replayed, session);
    assert_eq!(replayed_ticket, ticket);
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::SessionPrepare(PrepareSession {
                session: session.key.clone(),
                offer: offer()
            })
        )),
        ProviderErrorCode::SessionNotReady
    );
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::SessionCreate(create_request(&scope, "another-create"))
        )),
        ProviderErrorCode::BusyBeforeDispatch
    );
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let recovered = success(call(
        &mut provider,
        ProviderRequest::SessionReconcile(ReconcileSession {
            scope: Some(scope.clone()),
            operation: ticket.operation.clone(),
            session: Some(session.key.clone()),
        }),
    ));
    let ProviderReply::SessionReconcile(Reconciliation::PendingAllocation {
        session: recovered_session,
        ticket: recovered_ticket,
    }) = recovered
    else {
        panic!("Unsettled receipt was not recovered");
    };
    assert_eq!(recovered_session, session);
    assert_eq!(recovered_ticket, ticket);
    settle(&mut provider, &ticket, Acceptance::Accepted);
    settle(&mut provider, &ticket, Acceptance::Accepted);
    assert!(matches!(
        success(call(
            &mut provider,
            ProviderRequest::SessionReconcile(ReconcileSession {
                scope: Some(scope.clone()),
                operation: ticket.operation.clone(),
                session: Some(session.key.clone())
            })
        )),
        ProviderReply::SessionReconcile(Reconciliation::Active { .. })
    ));
    let native_offer = offer();
    let ProviderReply::SessionPrepare(prepared) = success(call(
        &mut provider,
        ProviderRequest::SessionPrepare(PrepareSession {
            session: session.key.clone(),
            offer: native_offer.clone(),
        }),
    )) else {
        panic!("not prepared")
    };
    assert_eq!(prepared.accepted.video, fixture_video());
    assert_eq!(prepared.accepted.audio.unwrap().sample_rate, 48000);
    assert!(!prepared.accepted.input.relative_mouse);
    assert!(!prepared.accepted.input.text);
    assert_eq!(prepared.accepted.input.gamepad_slots, 1);
    let stop = StopSession {
        session: session.key.clone(),
        operation: OperationId::new("stop-1").unwrap(),
    };
    success(call(
        &mut provider,
        ProviderRequest::SessionStop(stop.clone()),
    ));
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    assert!(matches!(
        success(call(&mut provider, ProviderRequest::SessionStop(stop))),
        ProviderReply::SessionStop(CleanupState::Resolved)
    ));
    assert!(matches!(
        success(call(
            &mut provider,
            ProviderRequest::SessionReconcile(ReconcileSession {
                scope: Some(scope),
                operation: ticket.operation,
                session: Some(session.key)
            })
        )),
        ProviderReply::SessionReconcile(Reconciliation::Terminal {
            reason: TerminalReason::UserStopped,
            ..
        })
    ));
}

#[test]
fn switching_accounts_cannot_retarget_or_delete_an_original_seat() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let blue = login(&mut provider, "demo-blue");
    let (session, ticket) = create(&mut provider, &blue, "owned-blue");
    settle(&mut provider, &ticket, Acceptance::Accepted);
    let orange = login(&mut provider, "demo-orange");
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::AccountsRemove(blue.account.clone())
        )),
        ProviderErrorCode::CleanupRequired
    );
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::SessionReconcile(ReconcileSession {
                scope: Some(orange.clone()),
                operation: ticket.operation.clone(),
                session: Some(session.key.clone())
            })
        )),
        ProviderErrorCode::ScopeChanged
    );
    let forged = SessionKey {
        account: Some(orange.account),
        remote_id: session.key.remote_id.clone(),
    };
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::SessionStop(StopSession {
                session: forged,
                operation: OperationId::new("bad-stop").unwrap()
            })
        )),
        ProviderErrorCode::SessionNotFound
    );
    success(call(
        &mut provider,
        ProviderRequest::SessionPoll(session.key.clone()),
    ));
    success(call(
        &mut provider,
        ProviderRequest::SessionStop(StopSession {
            session: session.key,
            operation: OperationId::new("good-stop").unwrap(),
        }),
    ));
    success(call(
        &mut provider,
        ProviderRequest::AccountsRemove(blue.account),
    ));
}

#[test]
fn rejected_or_unknown_allocations_do_not_claim_cleanup_success() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    let (session, ticket) = create(&mut provider, &scope, "reject-create");
    settle(&mut provider, &ticket, Acceptance::Rejected);
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::SessionResolveAllocation(ResolveAllocation {
                operation: ticket.operation.clone(),
                receipt: ticket.receipt,
                decision: Acceptance::Accepted
            })
        )),
        ProviderErrorCode::InvalidRequest
    );
    assert!(matches!(
        success(call(
            &mut provider,
            ProviderRequest::SessionReconcile(ReconcileSession {
                scope: Some(scope.clone()),
                operation: ticket.operation,
                session: Some(session.key)
            })
        )),
        ProviderReply::SessionReconcile(Reconciliation::Terminal {
            reason: TerminalReason::AllocationRejected,
            ..
        })
    ));
    assert!(matches!(
        success(call(
            &mut provider,
            ProviderRequest::SessionReconcile(ReconcileSession {
                scope: Some(scope),
                operation: OperationId::new("unknown-create").unwrap(),
                session: None
            })
        )),
        ProviderReply::SessionReconcile(Reconciliation::Unknown { .. })
    ));
}

#[test]
fn recovered_pending_receipt_can_be_rejected_by_its_original_owner_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    let (session, ticket) = create(&mut provider, &scope, "lost-create-response");
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let response = success(call(
        &mut provider,
        ProviderRequest::SessionReconcile(ReconcileSession {
            scope: Some(scope.clone()),
            operation: ticket.operation.clone(),
            session: None,
        }),
    ));
    let ProviderReply::SessionReconcile(Reconciliation::PendingAllocation {
        session: recovered_session,
        ticket: recovered_ticket,
    }) = response
    else {
        panic!("Lost fresh allocation was not reported pending");
    };
    assert_eq!(recovered_session, session);
    assert_eq!(recovered_ticket, ticket);
    settle(&mut provider, &recovered_ticket, Acceptance::Rejected);
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    settle(&mut provider, &recovered_ticket, Acceptance::Rejected);
    let response = success(call(
        &mut provider,
        ProviderRequest::SessionReconcile(ReconcileSession {
            scope: Some(scope),
            operation: ticket.operation,
            session: Some(session.key),
        }),
    ));
    assert!(matches!(
        response,
        ProviderReply::SessionReconcile(Reconciliation::Terminal {
            reason: TerminalReason::AllocationRejected,
            ..
        })
    ));
}

#[test]
fn journal_has_one_writer_recovers_partial_tail_and_refuses_corrupt_records() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    assert!(DemoProvider::open(directory.path()).is_err());
    drop(provider);
    let path = directory.path().join("state.ndjson");
    let original = fs::read(&path).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{partial-write")
        .unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
    let ProviderReply::AccountsList(accounts) =
        success(call(&mut provider, ProviderRequest::AccountsList(Empty {})))
    else {
        panic!("no accounts")
    };
    assert_eq!(accounts.selected, Some(scope.account));
    drop(provider);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{corrupt-record}\n")
        .unwrap();
    let corrupt = fs::read(&path).unwrap();
    assert!(DemoProvider::open(directory.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), corrupt);
}

#[test]
fn rejected_preallocation_intent_has_durable_proof_but_missing_history_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    let mut unsupported = create_request(&scope, "unsupported-create");
    unsupported.preferences.video.encoding = Some(VideoEncoding::HevcAnnexB);
    assert_eq!(
        error(call(
            &mut provider,
            ProviderRequest::SessionCreate(unsupported.clone())
        )),
        ProviderErrorCode::UnsupportedFeature
    );
    drop(provider);
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let proof = call(
        &mut provider,
        ProviderRequest::SessionReconcile(ReconcileSession {
            scope: Some(scope.clone()),
            operation: unsupported.operation.clone(),
            session: None,
        }),
    );
    assert!(matches!(
        success(proof),
        ProviderReply::SessionReconcile(Reconciliation::NotAllocated { .. })
    ));
    let mut retry = unsupported;
    retry.preferences.video.encoding = None;
    assert_eq!(
        error(call(&mut provider, ProviderRequest::SessionCreate(retry))),
        ProviderErrorCode::UnsupportedFeature
    );
    assert!(matches!(
        success(call(
            &mut provider,
            ProviderRequest::SessionReconcile(ReconcileSession {
                scope: Some(scope),
                operation: OperationId::new("missing-create").unwrap(),
                session: None
            })
        )),
        ProviderReply::SessionReconcile(Reconciliation::Unknown { .. })
    ));
}

#[test]
fn media_bootstrap_is_authorized_by_durable_control_state_not_self_assertion() {
    use opennow_sdk_demo::authorization::{authorize_media, session_is_active};
    let directory = tempfile::tempdir().unwrap();
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let scope = login(&mut provider, "demo-blue");
    let (session, ticket) = create(&mut provider, &scope, "media-auth");
    settle(&mut provider, &ticket, Acceptance::Accepted);
    let ProviderReply::SessionPrepare(prepared) = success(call(
        &mut provider,
        ProviderRequest::SessionPrepare(PrepareSession {
            session: session.key.clone(),
            offer: offer(),
        }),
    )) else {
        panic!("not prepared")
    };
    let bytes = prepared.bootstrap.expose_secret();
    assert_eq!(
        authorize_media(directory.path(), bytes, &prepared.accepted).unwrap(),
        session.key
    );
    let mut forged: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let token = forged["token"].as_str().unwrap().to_owned();
    assert!(
        !String::from_utf8(fs::read(directory.path().join("state.ndjson")).unwrap())
            .unwrap()
            .contains(&token)
    );
    forged["token"] = serde_json::json!("0".repeat(64));
    assert!(
        authorize_media(
            directory.path(),
            &serde_json::to_vec(&forged).unwrap(),
            &prepared.accepted
        )
        .is_err()
    );
    let mut wrong_format = prepared.accepted.clone();
    wrong_format.video.fps = 49;
    assert!(authorize_media(directory.path(), bytes, &wrong_format).is_err());
    assert!(session_is_active(directory.path(), &session.key).unwrap());
    drop(provider);
    assert!(session_is_active(directory.path(), &session.key).unwrap());
    let mut provider = DemoProvider::open(directory.path()).unwrap();
    let ProviderReply::SessionPrepare(renewed) = success(call(
        &mut provider,
        ProviderRequest::SessionPrepare(PrepareSession {
            session: session.key.clone(),
            offer: offer(),
        }),
    )) else {
        panic!("not renewed")
    };
    assert!(authorize_media(directory.path(), bytes, &prepared.accepted).is_err());
    authorize_media(
        directory.path(),
        renewed.bootstrap.expose_secret(),
        &renewed.accepted,
    )
    .unwrap();
    assert!(session_is_active(directory.path(), &session.key).unwrap());
    success(call(
        &mut provider,
        ProviderRequest::SessionStop(StopSession {
            session: session.key.clone(),
            operation: OperationId::new("stop-authorized").unwrap(),
        }),
    ));
    assert!(!session_is_active(directory.path(), &session.key).unwrap());
    assert!(
        authorize_media(
            directory.path(),
            renewed.bootstrap.expose_secret(),
            &renewed.accepted
        )
        .is_err()
    );
}
