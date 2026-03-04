use crate::types::{ApiError, ErrorCode};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use std::collections::BTreeMap;

pub type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct UidMapper {
    root: String,
    secret: Vec<u8>,
    mapping: BTreeMap<String, String>,
}

impl UidMapper {
    pub fn new(
        uid_root: Option<&str>,
        deterministic_secret: Option<&str>,
    ) -> Result<Self, ApiError> {
        let root = uid_root.unwrap_or("2.25").trim().to_string();
        if !is_valid_uid_root(&root) {
            return Err(ApiError::new(
                ErrorCode::InvalidInput,
                format!("Invalid UID root '{root}'"),
            ));
        }

        let secret = if let Some(value) = deterministic_secret {
            value.as_bytes().to_vec()
        } else {
            let mut random = vec![0_u8; 32];
            rand::thread_rng().fill_bytes(&mut random);
            random
        };

        Ok(Self {
            root,
            secret,
            mapping: BTreeMap::new(),
        })
    }

    pub fn map_uid(&mut self, source_uid: &str) -> Result<String, ApiError> {
        if let Some(existing) = self.mapping.get(source_uid) {
            return Ok(existing.clone());
        }

        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!("Failed to initialize UID mapper: {error}"),
            )
        })?;
        mac.update(source_uid.as_bytes());
        let digest = mac.finalize().into_bytes();

        let mut digits = big_digits_from_bytes(&digest);
        if digits.starts_with('0') {
            digits.replace_range(0..1, "1");
        }

        let mut candidate = format!("{}.{}", self.root, digits);
        if candidate.len() > 64 {
            candidate.truncate(64);
            while candidate.ends_with('.') {
                candidate.pop();
            }
        }

        if !is_valid_uid(&candidate) {
            return Err(ApiError::new(
                ErrorCode::Internal,
                "Failed to produce DICOM-valid pseudonymous UID",
            ));
        }

        self.mapping
            .insert(source_uid.to_string(), candidate.clone());
        Ok(candidate)
    }

    pub fn map_token(
        &self,
        source_value: &str,
        prefix: &str,
        token_length: usize,
    ) -> Result<String, ApiError> {
        if token_length == 0 {
            return Err(ApiError::new(
                ErrorCode::InvalidInput,
                "token_length must be greater than zero",
            ));
        }

        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!("Failed to initialize token mapper: {error}"),
            )
        })?;
        mac.update(source_value.as_bytes());
        let digest = mac.finalize().into_bytes();
        let hex_digest = hex::encode(digest);
        let take = std::cmp::min(token_length, hex_digest.len());
        Ok(format!("{prefix}{}", &hex_digest[..take]))
    }

    pub fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }
}

fn big_digits_from_bytes(bytes: &[u8]) -> String {
    let mut n = bytes.to_vec();
    if n.iter().all(|v| *v == 0) {
        return "0".to_string();
    }

    let mut digits = String::new();
    while n.iter().any(|v| *v != 0) {
        let mut carry = 0u16;
        for byte in &mut n {
            let value = (carry << 8) | u16::from(*byte);
            *byte = (value / 10) as u8;
            carry = value % 10;
        }
        digits.push(char::from(b'0' + carry as u8));
        while matches!(n.first(), Some(0)) {
            n.remove(0);
        }
        if n.is_empty() {
            break;
        }
    }

    digits.chars().rev().collect()
}

fn is_valid_uid_root(input: &str) -> bool {
    if input.is_empty() || input.len() > 62 {
        return false;
    }
    if input.starts_with('.') || input.ends_with('.') || input.contains("..") {
        return false;
    }
    input.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_valid_uid(uid: &str) -> bool {
    if uid.is_empty() || uid.len() > 64 {
        return false;
    }
    if uid.starts_with('.') || uid.ends_with('.') || uid.contains("..") {
        return false;
    }

    uid.split('.')
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}
