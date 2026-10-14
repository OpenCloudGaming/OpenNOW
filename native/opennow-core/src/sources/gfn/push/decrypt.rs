use crate::sources::gfn::push::PushError;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use p256::ecdh;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use rand::RngCore;
use sha2::Sha256;

const AUTH_SECRET_BYTES: usize = 16;
const PUBLIC_KEY_BYTES: usize = 65;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;
const MAXIMUM_RECORD_SIZE: u32 = 16 * 1024;
const MINIMUM_RECORD_SIZE: u32 = 18;
const AES128GCM_HEADER_BYTES: usize = 21;

pub struct EceKeyPair {
    secret: SecretKey,
    public: Vec<u8>,
    auth_secret: Vec<u8>,
}

impl EceKeyPair {
    pub fn generate() -> Result<Self, PushError> {
        let mut scalar = [0_u8; 32];
        rand::rng().fill_bytes(&mut scalar);
        let secret = SecretKey::from_slice(&scalar)
            .map_err(|_| PushError::new("push_key_generation", "Could not create a push key"))?;
        let mut auth_secret = vec![0_u8; AUTH_SECRET_BYTES];
        rand::rng().fill_bytes(&mut auth_secret);
        Ok(Self {
            public: secret
                .public_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
            secret,
            auth_secret,
        })
    }

    pub fn from_parts(
        private: &[u8],
        public: &[u8],
        auth_secret: &[u8],
    ) -> Result<Self, PushError> {
        if public.len() != PUBLIC_KEY_BYTES || auth_secret.len() != AUTH_SECRET_BYTES {
            return Err(PushError::new(
                "push_key_invalid",
                "The stored push key material is malformed",
            ));
        }
        let secret = SecretKey::from_slice(private)
            .map_err(|_| PushError::new("push_key_invalid", "The stored push key is invalid"))?;
        let derived = secret.public_key().to_encoded_point(false);
        if derived.as_bytes() != public {
            return Err(PushError::new(
                "push_key_mismatch",
                "The stored push key does not match its public key",
            ));
        }
        Ok(Self {
            secret,
            public: public.to_vec(),
            auth_secret: auth_secret.to_vec(),
        })
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public
    }

    pub fn auth_secret(&self) -> &[u8] {
        &self.auth_secret
    }

    pub fn private_key(&self) -> Vec<u8> {
        self.secret.to_bytes().to_vec()
    }

    fn shared_secret(&self, server_public: &[u8]) -> Result<Vec<u8>, PushError> {
        let key = PublicKey::from_sec1_bytes(server_public).map_err(|_| {
            PushError::new(
                "push_decrypt_invalid",
                "The message key header is not a valid public key",
            )
        })?;
        let shared = ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), key.as_affine());
        Ok(shared.raw_secret_bytes().to_vec())
    }
}

pub fn decrypt_web_push(
    keys: &EceKeyPair,
    content_encoding: Option<&str>,
    crypto_key: Option<&str>,
    encryption: Option<&str>,
    body: &[u8],
) -> Result<Vec<u8>, PushError> {
    match content_encoding {
        Some(encoding) if encoding.eq_ignore_ascii_case("aes128gcm") => {
            decrypt_aes128gcm_body(keys, body)
        }
        Some(encoding) if encoding.eq_ignore_ascii_case("aesgcm") => {
            decrypt_legacy_body(keys, crypto_key, encryption, body)
        }
        Some(_) => Err(PushError::new(
            "push_decrypt_unsupported",
            "The message declares an unsupported content encoding",
        )),
        None if crypto_key.is_some() && encryption.is_some() => {
            decrypt_legacy_body(keys, crypto_key, encryption, body)
        }
        None => decrypt_aes128gcm_body(keys, body),
    }
}

fn decrypt_legacy_body(
    keys: &EceKeyPair,
    crypto_key: Option<&str>,
    encryption: Option<&str>,
    body: &[u8],
) -> Result<Vec<u8>, PushError> {
    let crypto_key = crypto_key.ok_or_else(|| {
        PushError::new(
            "push_decrypt_invalid",
            "The message is missing its key header",
        )
    })?;
    let encryption = encryption.ok_or_else(|| {
        PushError::new(
            "push_decrypt_invalid",
            "The message is missing its salt header",
        )
    })?;
    let dh = parameter(crypto_key, "dh").ok_or_else(|| {
        PushError::new(
            "push_decrypt_invalid",
            "The message is missing its key header",
        )
    })?;
    let salt = parameter(encryption, "salt").ok_or_else(|| {
        PushError::new(
            "push_decrypt_invalid",
            "The message is missing its salt header",
        )
    })?;
    decrypt_aesgcm(keys, &dh, &salt, body)
}

fn parameter(value: &str, name: &str) -> Option<Vec<u8>> {
    value.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
            .and_then(base64_url_decode)
    })
}

fn base64_url_decode(value: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value.trim())
        .ok()
}

pub fn decrypt_aes128gcm_body(keys: &EceKeyPair, body: &[u8]) -> Result<Vec<u8>, PushError> {
    if body.len() < AES128GCM_HEADER_BYTES + TAG_BYTES {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The encrypted message is too short",
        ));
    }
    let salt = &body[0..16];
    let record_size = u32::from_be_bytes([body[16], body[17], body[18], body[19]]);
    let id_length = body[20] as usize;
    if body.len() < AES128GCM_HEADER_BYTES + id_length + TAG_BYTES {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The encrypted message header is malformed",
        ));
    }
    let server_public = &body[AES128GCM_HEADER_BYTES..AES128GCM_HEADER_BYTES + id_length];
    let records = &body[AES128GCM_HEADER_BYTES + id_length..];
    let shared = keys.shared_secret(server_public)?;

    let mut key_info = Vec::with_capacity(19 + PUBLIC_KEY_BYTES * 2);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(&keys.public);
    key_info.extend_from_slice(server_public);

    let mut input_key = [0_u8; 32];
    Hkdf::<Sha256>::new(Some(&keys.auth_secret), &shared)
        .expand(&key_info, &mut input_key)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message key derivation failed"))?;
    let (content_key, base_nonce) = derive_content_keys(&input_key, salt)?;
    decode_aes128gcm_records(&content_key, &base_nonce, record_size, records)
}

pub fn derive_content_keys(
    input_key: &[u8],
    salt: &[u8],
) -> Result<([u8; 16], [u8; 12]), PushError> {
    let prk = Hkdf::<Sha256>::new(Some(salt), input_key);
    let mut content_key = [0_u8; 16];
    let mut nonce = [0_u8; 12];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut content_key)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message key derivation failed"))?;
    prk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| {
            PushError::new("push_decrypt_failed", "The message nonce derivation failed")
        })?;
    Ok((content_key, nonce))
}

pub fn decode_aes128gcm_records(
    content_key: &[u8; 16],
    base_nonce: &[u8; 12],
    record_size: u32,
    records: &[u8],
) -> Result<Vec<u8>, PushError> {
    if !(MINIMUM_RECORD_SIZE..=MAXIMUM_RECORD_SIZE).contains(&record_size) {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The message record size is outside the accepted range",
        ));
    }
    if records.is_empty() {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The encrypted message carries no record",
        ));
    }
    let cipher = Aes128Gcm::new_from_slice(content_key)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message cipher could not start"))?;
    let mut plaintext = Vec::new();
    let mut sequence = 0_u64;
    let mut remaining = records;
    while !remaining.is_empty() {
        let take = remaining.len().min(record_size as usize);
        let (record, rest) = remaining.split_at(take);
        remaining = rest;
        if record.len() < TAG_BYTES + 1 {
            return Err(PushError::new(
                "push_decrypt_invalid",
                "A message record is too short",
            ));
        }
        let last = remaining.is_empty();
        plaintext.extend_from_slice(&decrypt_record(
            &cipher, base_nonce, sequence, record, last,
        )?);
        sequence += 1;
    }
    Ok(plaintext)
}

fn decrypt_record(
    cipher: &Aes128Gcm,
    base_nonce: &[u8; 12],
    sequence: u64,
    record: &[u8],
    last: bool,
) -> Result<Vec<u8>, PushError> {
    let sequence_bytes = sequence.to_be_bytes();
    let mut nonce = *base_nonce;
    for (target, byte) in nonce[4..].iter_mut().zip(sequence_bytes.iter()) {
        *target ^= *byte;
    }
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&nonce), record)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message could not be decrypted"))?;
    let end = decrypted
        .iter()
        .rposition(|byte| *byte != 0)
        .ok_or_else(|| {
            PushError::new(
                "push_decrypt_invalid",
                "The decrypted record carries no padding delimiter",
            )
        })?;
    let delimiter = decrypted[end];
    let expected = if last { 2 } else { 1 };
    if delimiter != expected {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The decrypted record carries an invalid padding delimiter",
        ));
    }
    Ok(decrypted[..end].to_vec())
}

pub fn decrypt_aesgcm(
    keys: &EceKeyPair,
    server_public: &[u8],
    salt: &[u8],
    payload: &[u8],
) -> Result<Vec<u8>, PushError> {
    if salt.len() != SALT_BYTES {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The message salt is outside the accepted range",
        ));
    }
    if payload.len() <= TAG_BYTES + 2 {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The encrypted message is too short",
        ));
    }
    if server_public.len() != PUBLIC_KEY_BYTES {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The message key header is malformed",
        ));
    }
    let shared = keys.shared_secret(server_public)?;
    let mut input_key = [0_u8; 32];
    Hkdf::<Sha256>::new(Some(&keys.auth_secret), &shared)
        .expand(b"Content-Encoding: auth\0", &mut input_key)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message key derivation failed"))?;
    let mut context = Vec::with_capacity(134);
    context.extend_from_slice(&[0, PUBLIC_KEY_BYTES as u8]);
    context.extend_from_slice(&keys.public);
    context.extend_from_slice(&[0, PUBLIC_KEY_BYTES as u8]);
    context.extend_from_slice(server_public);
    let mut key_info = b"Content-Encoding: aesgcm\0P-256\0".to_vec();
    key_info.extend_from_slice(&context);
    let mut nonce_info = b"Content-Encoding: nonce\0P-256\0".to_vec();
    nonce_info.extend_from_slice(&context);
    let prk = Hkdf::<Sha256>::new(Some(salt), &input_key);
    let mut content_key = [0_u8; 16];
    let mut nonce = [0_u8; NONCE_BYTES];
    prk.expand(&key_info, &mut content_key)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message key derivation failed"))?;
    prk.expand(&nonce_info, &mut nonce).map_err(|_| {
        PushError::new("push_decrypt_failed", "The message nonce derivation failed")
    })?;
    let cipher = Aes128Gcm::new_from_slice(&content_key)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message cipher could not start"))?;
    let mut plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), payload)
        .map_err(|_| PushError::new("push_decrypt_failed", "The message could not be decrypted"))?;
    if plaintext.len() <= 2 {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The decrypted message is too short",
        ));
    }
    let padding = ((plaintext[0] as usize) << 8) | plaintext[1] as usize;
    let start = 2 + padding;
    if start >= plaintext.len() {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The decrypted message carries invalid padding",
        ));
    }
    if plaintext[2..start].iter().any(|byte| *byte != 0) {
        return Err(PushError::new(
            "push_decrypt_invalid",
            "The decrypted message carries invalid padding",
        ));
    }
    Ok(plaintext.split_off(start))
}
