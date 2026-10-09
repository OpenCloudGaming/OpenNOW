use crate::{Journal, now_ms, terminal};
use opennow_plugin_api::media::AcceptedMedia;
use opennow_plugin_api::provider::{Acceptance, OfferId, SecretBytes, SecretString, SessionKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;
use subtle::ConstantTimeEq;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Grant {
    token_sha256: [u8; 32],
    expires_at_ms: u64,
    accepted: AcceptedMedia,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bootstrap {
    version: u32,
    session: SessionKey,
    offer_id: OfferId,
    runtime_epoch: u64,
    expires_at_ms: u64,
    token: SecretString,
}

pub(crate) fn issue(
    session: &SessionKey,
    accepted: &AcceptedMedia,
    expires_at_ms: u64,
) -> io::Result<(Grant, SecretBytes)> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy)
        .map_err(|_| io::Error::other("Demo authorization entropy unavailable"))?;
    let token = entropy
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let bootstrap = Bootstrap {
        version: 1,
        session: session.clone(),
        offer_id: accepted.offer_id.clone(),
        runtime_epoch: accepted.runtime_epoch,
        expires_at_ms,
        token: SecretString::new(token)
            .map_err(|_| io::Error::other("Demo authorization cannot be encoded"))?,
    };
    let bytes = serde_json::to_vec(&bootstrap)
        .map_err(|_| io::Error::other("Demo authorization cannot be encoded"))?;
    Ok((
        Grant {
            token_sha256: digest,
            expires_at_ms,
            accepted: accepted.clone(),
        },
        SecretBytes::new(bytes)
            .map_err(|_| io::Error::other("Demo authorization exceeds its bound"))?,
    ))
}

pub fn authorize_media(
    data_directory: &Path,
    provider_bootstrap: &[u8],
    accepted: &AcceptedMedia,
) -> io::Result<SessionKey> {
    if provider_bootstrap.len() > 4096 {
        return Err(denied());
    }
    let bootstrap: Bootstrap = serde_json::from_slice(provider_bootstrap).map_err(|_| denied())?;
    if bootstrap.version != 1
        || bootstrap.expires_at_ms <= now_ms()
        || bootstrap.token.expose_secret().len() != 64
        || !bootstrap
            .token
            .expose_secret()
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || bootstrap.offer_id != accepted.offer_id
        || bootstrap.runtime_epoch != accepted.runtime_epoch
    {
        return Err(denied());
    }
    let state = Journal::read_state(data_directory)?;
    let session = state
        .sessions
        .iter()
        .find(|session| session.view.key == bootstrap.session)
        .ok_or_else(denied)?;
    if session.decision != Some(Acceptance::Accepted) || terminal(&session.view) {
        return Err(denied());
    }
    let grant = session.media.as_ref().ok_or_else(denied)?;
    let digest: [u8; 32] = Sha256::digest(bootstrap.token.expose_secret().as_bytes()).into();
    if grant.expires_at_ms != bootstrap.expires_at_ms
        || grant.accepted != *accepted
        || !bool::from(grant.token_sha256.ct_eq(&digest))
    {
        return Err(denied());
    }
    Ok(bootstrap.session)
}

pub fn session_is_active(data_directory: &Path, session: &SessionKey) -> io::Result<bool> {
    let state = Journal::read_state(data_directory)?;
    Ok(state.sessions.iter().any(|record| {
        &record.view.key == session
            && record.decision == Some(Acceptance::Accepted)
            && !terminal(&record.view)
    }))
}

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Demo media authorization was rejected",
    )
}
