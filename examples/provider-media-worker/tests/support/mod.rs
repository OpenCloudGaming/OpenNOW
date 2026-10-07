use opennow_plugin_api::media::*;
use opennow_plugin_api::provider::*;
use opennow_sdk_demo::{CATALOG_REVISION, DemoProvider, now_ms};

pub struct Authorization {
    pub directory: tempfile::TempDir,
    pub provider: Option<DemoProvider>,
    pub session: SessionKey,
    pub prepared: PreparedWorker,
    pub offer: NativeOffer,
}

fn call(provider: &mut DemoProvider, request: ProviderRequest) -> ProviderResponseV2 {
    let envelope = HostRequestV2 {
        v: Version2,
        epoch: 7.try_into().unwrap(),
        id: Text::new("worker-test").unwrap(),
        timeout_ms: 10000,
        request,
    };
    let response = provider.handle(&envelope);
    response.validate_for(&envelope).unwrap();
    response
}

fn success(response: ProviderResponseV2) -> ProviderReply {
    match response.outcome {
        ProviderOutcome::Success { reply } => *reply,
        ProviderOutcome::Failure { error } => panic!("provider failed: {error:?}"),
    }
}

impl Authorization {
    pub fn another_session(&mut self) -> (SessionKey, PreparedWorker) {
        self.stop();
        let provider = self.provider.as_mut().unwrap();
        let ProviderReply::AuthBegin(AuthState::Pending {
            challenge: AuthChallenge::Pairing { attempt, .. },
        }) = success(call(
            provider,
            ProviderRequest::AuthBegin(BeginAuth {
                authority: Some(AuthorityId::new("demo-orange").unwrap()),
                kind: AuthKind::Pairing,
                remember: true,
            }),
        ))
        else {
            panic!("expected second pairing");
        };
        success(call(
            provider,
            ProviderRequest::AuthPoll(AuthAttempt {
                attempt: attempt.clone(),
            }),
        ));
        let ProviderReply::AuthComplete(AuthState::SignedIn { account, revision }) = success(call(
            provider,
            ProviderRequest::AuthComplete(CompleteAuth {
                attempt,
                proof: None,
            }),
        )) else {
            panic!("expected signed in account");
        };
        let response = call(
            provider,
            ProviderRequest::SessionCreate(CreateSession {
                scope: Some(AccountScope {
                    account: account.key,
                    revision,
                }),
                operation: OperationId::new("worker-test-second-create").unwrap(),
                target: LaunchTarget {
                    game: GameId::new("demo-01").unwrap(),
                    variant: VariantId::new("fixture").unwrap(),
                },
                catalog_revision: Text::new(CATALOG_REVISION).unwrap(),
                settings_revision: 0,
                preferences: StreamPreferences {
                    video: RequestedVideo {
                        width: 320,
                        height: 180,
                        encoding: Some(VideoEncoding::H264AnnexB),
                        fps: Some(50),
                        bit_depth: 8,
                        chroma: Chroma::Yuv420,
                        hdr: false,
                    },
                    bitrate_kbps: 10000,
                },
                offer: self.offer.clone(),
            }),
        );
        let ticket = response.allocation.clone().unwrap();
        let ProviderReply::SessionCreate(created) = success(response) else {
            panic!("expected second session");
        };
        success(call(
            provider,
            ProviderRequest::SessionResolveAllocation(ResolveAllocation {
                operation: ticket.operation,
                receipt: ticket.receipt,
                decision: Acceptance::Accepted,
            }),
        ));
        let session = created.session.key;
        let ProviderReply::SessionPrepare(prepared) = success(call(
            provider,
            ProviderRequest::SessionPrepare(PrepareSession {
                session: session.clone(),
                offer: self.offer.clone(),
            }),
        )) else {
            panic!("expected second prepared worker");
        };
        (session, prepared)
    }

    pub fn new(lifetime_ms: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut provider = DemoProvider::open(directory.path()).unwrap();
        let ProviderReply::AuthBegin(AuthState::Pending {
            challenge: AuthChallenge::Pairing { attempt, .. },
        }) = success(call(
            &mut provider,
            ProviderRequest::AuthBegin(BeginAuth {
                authority: Some(AuthorityId::new("demo-blue").unwrap()),
                kind: AuthKind::Pairing,
                remember: true,
            }),
        ))
        else {
            panic!("expected pairing");
        };
        success(call(
            &mut provider,
            ProviderRequest::AuthPoll(AuthAttempt {
                attempt: attempt.clone(),
            }),
        ));
        let ProviderReply::AuthComplete(AuthState::SignedIn { account, revision }) = success(call(
            &mut provider,
            ProviderRequest::AuthComplete(CompleteAuth {
                attempt,
                proof: None,
            }),
        )) else {
            panic!("expected sign in");
        };
        let offer = NativeOffer {
            version: 1,
            offer_id: OfferId::new("worker-test-offer").unwrap(),
            runtime_epoch: 9,
            expires_at_ms: now_ms() + lifetime_ms,
            video_formats: List::new(vec![VideoSupport {
                encoding: VideoEncoding::H264AnnexB,
                bit_depth: 8,
                chroma: Chroma::Yuv420,
                dynamic_range: DynamicRange::Sdr,
                max_width: 320,
                max_height: 180,
                max_fps: 50,
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
                max_video_access_unit_bytes: 256 * 1024,
                max_audio_packet_bytes: 1275,
                max_buffered_video_bytes: 512 * 1024,
                max_buffered_video_frames: 2,
                max_buffered_audio_ms: 100,
                max_control_message_bytes: 4096,
                max_pending_input_events: 32,
            },
        };
        let response = call(
            &mut provider,
            ProviderRequest::SessionCreate(CreateSession {
                scope: Some(AccountScope {
                    account: account.key,
                    revision,
                }),
                operation: OperationId::new("worker-test-create").unwrap(),
                target: LaunchTarget {
                    game: GameId::new("demo-01").unwrap(),
                    variant: VariantId::new("fixture").unwrap(),
                },
                catalog_revision: Text::new(CATALOG_REVISION).unwrap(),
                settings_revision: 0,
                preferences: StreamPreferences {
                    video: RequestedVideo {
                        width: 320,
                        height: 180,
                        encoding: Some(VideoEncoding::H264AnnexB),
                        fps: Some(50),
                        bit_depth: 8,
                        chroma: Chroma::Yuv420,
                        hdr: false,
                    },
                    bitrate_kbps: 10000,
                },
                offer: offer.clone(),
            }),
        );
        let ticket = response.allocation.clone().unwrap();
        let ProviderReply::SessionCreate(created) = success(response) else {
            panic!("expected session");
        };
        success(call(
            &mut provider,
            ProviderRequest::SessionResolveAllocation(ResolveAllocation {
                operation: ticket.operation,
                receipt: ticket.receipt,
                decision: Acceptance::Accepted,
            }),
        ));
        let session = created.session.key;
        let ProviderReply::SessionPrepare(prepared) = success(call(
            &mut provider,
            ProviderRequest::SessionPrepare(PrepareSession {
                session: session.clone(),
                offer: offer.clone(),
            }),
        )) else {
            panic!("expected prepared worker");
        };
        Self {
            directory,
            provider: Some(provider),
            session,
            prepared,
            offer,
        }
    }

    pub fn restart_and_renew(&mut self) {
        self.provider.take();
        let mut provider = DemoProvider::open(self.directory.path()).unwrap();
        self.offer.runtime_epoch += 1;
        self.offer.expires_at_ms = now_ms() + 60000;
        let ProviderReply::SessionPrepare(prepared) = success(call(
            &mut provider,
            ProviderRequest::SessionPrepare(PrepareSession {
                session: self.session.clone(),
                offer: self.offer.clone(),
            }),
        )) else {
            panic!("expected renewed worker");
        };
        self.prepared = prepared;
        self.provider = Some(provider);
    }

    pub fn stop(&mut self) {
        success(call(
            self.provider.as_mut().unwrap(),
            ProviderRequest::SessionStop(StopSession {
                session: self.session.clone(),
                operation: OperationId::new("worker-test-stop").unwrap(),
            }),
        ));
    }
}
