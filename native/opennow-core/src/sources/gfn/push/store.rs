use crate::sources::gfn::push::PushError;
use crate::sources::gfn::push::registration::Registration;

const PUSH_SERVICE_NAME: &str = "app.opennow.push";

pub trait PushStateStore: Send + Sync {
    fn load(&self, account: &str) -> Result<Option<Registration>, PushError>;
    fn save(&self, account: &str, registration: &Registration) -> Result<(), PushError>;
    fn clear(&self, account: &str) -> Result<(), PushError>;
}

pub struct RegistrationStore {
    service: String,
}

impl RegistrationStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    pub fn default_service() -> Self {
        Self::new(PUSH_SERVICE_NAME)
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, PushError> {
        keyring::Entry::new(&self.service, &format!("registration:{account}")).map_err(|_| {
            PushError::new(
                "push_store_unavailable",
                "The OS credential store is unavailable for the push registration",
            )
        })
    }
}

impl PushStateStore for RegistrationStore {
    fn load(&self, account: &str) -> Result<Option<Registration>, PushError> {
        match self.entry(account)?.get_password() {
            Ok(encoded) => serde_json::from_str(&encoded).map(Some).map_err(|_| {
                PushError::new(
                    "push_store_corrupt",
                    "The stored push registration could not be read",
                )
            }),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(PushError::new(
                "push_store_unavailable",
                "The OS credential store is unavailable for the push registration",
            )),
        }
    }

    fn save(&self, account: &str, registration: &Registration) -> Result<(), PushError> {
        let encoded = serde_json::to_string(registration).map_err(|_| {
            PushError::new(
                "push_store_failed",
                "The push registration could not be encoded",
            )
        })?;
        self.entry(account)?.set_password(&encoded).map_err(|_| {
            PushError::new(
                "push_store_failed",
                "The push registration could not be stored",
            )
        })
    }

    fn clear(&self, account: &str) -> Result<(), PushError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(PushError::new(
                "push_store_failed",
                "The push registration could not be removed",
            )),
        }
    }
}
