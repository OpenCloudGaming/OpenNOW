use super::GfnModule;
use crate::sources::contract::SourceError;
use opennow_plugin_api::provider::{AccountId, AccountKey, AuthorityId, SessionId, SessionKey};
use serde_json::Value;

impl GfnModule {
    pub(super) fn legacy_control_key(
        &self,
        params: &Value,
    ) -> Result<Option<SessionKey>, SourceError> {
        let id = params["sessionId"]
            .as_str()
            .or_else(|| params["session"]["sessionId"].as_str())
            .unwrap_or("");
        let Some((id, scope)) = self
            .service
            .session_control_owner(id)
            .map_err(SourceError::from)?
        else {
            return Ok(None);
        };
        Ok(Some(SessionKey {
            account: Some(account_key(&scope["providerIdpId"], &scope["userId"])?),
            remote_id: SessionId::new(id).map_err(|_| invalid())?,
        }))
    }
    pub(super) fn legacy_account_key(&self, params: &Value) -> Result<AccountKey, SourceError> {
        let envelope = self.service.session().map_err(SourceError::from)?;
        let account = &envelope["session"];
        if account.is_null() {
            return Err(SourceError::new(
                "authentication_required",
                "Sign in before starting a session",
            ));
        }
        let expected = serde_json::json!({"providerIdpId":account["provider"]["idpId"],"userId":account["user"]["userId"],"generation":envelope["generation"]});
        if params["scope"] != expected {
            return Err(SourceError::new(
                "stale_account",
                "The launch account changed before admission",
            ));
        }
        account_key(&account["provider"]["idpId"], &account["user"]["userId"])
    }

    pub(super) fn legacy_session_state(
        &self,
        method: &str,
        params: &Value,
        value: &Value,
    ) -> Result<Option<(SessionKey, bool)>, SourceError> {
        let session = &value["session"];
        let terminal = value["termination"].as_object().is_some_and(|termination| {
            termination.get("resumable") == Some(&Value::Bool(false))
                && (termination.get("source").and_then(Value::as_str) == Some("cloudmatch-http")
                    && termination.get("httpStatus").and_then(Value::as_u64) == Some(404)
                    || termination.get("source").and_then(Value::as_str)
                        == Some("cloudmatch-session-status")
                        && termination.get("status").and_then(Value::as_u64) == Some(7))
        });
        let stopped = method == "session.stop" && value["stopped"] == true;
        let id = session["sessionId"].as_str().or_else(|| {
            (terminal || stopped)
                .then(|| {
                    value["termination"]["sessionId"]
                        .as_str()
                        .or_else(|| value["sessionId"].as_str())
                        .or_else(|| params["sessionId"].as_str())
                })
                .flatten()
        });
        let Some(id) = id else {
            return Ok(None);
        };
        let scope = session.get("ownerScope").unwrap_or(&value["scope"]);
        let fallback = if scope.is_null() {
            self.service
                .session_owner_scope(id)
                .map_err(SourceError::from)?
        } else {
            None
        };
        let scope = fallback.as_ref().unwrap_or(scope);
        let account = account_key(&scope["providerIdpId"], &scope["userId"])?;
        Ok(Some((
            SessionKey {
                account: Some(account),
                remote_id: SessionId::new(id).map_err(|_| invalid())?,
            },
            terminal || stopped || session["status"] == 7,
        )))
    }
}

fn account_key(authority: &Value, account: &Value) -> Result<AccountKey, SourceError> {
    Ok(AccountKey {
        authority: AuthorityId::new(authority.as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())?,
        account: AccountId::new(account.as_str().ok_or_else(invalid)?).map_err(|_| invalid())?,
    })
}

fn invalid() -> SourceError {
    SourceError::new(
        "session_owner_mismatch",
        "The provider did not establish session ownership",
    )
}
