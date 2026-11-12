//! Subsonic authentication: `u` with `p` (plain or `enc:` hex) or with
//! `t` + `s`, where `t = md5(password + s)`.

use md5::{Digest, Md5};

use api_models::settings::SubsonicSettings;

use super::response::ApiError;

/// Credentials as sent by the client.
#[derive(Debug, Default, Clone, Copy)]
pub struct Credentials<'a> {
    pub user: Option<&'a str>,
    pub password: Option<&'a str>,
    pub token: Option<&'a str>,
    pub salt: Option<&'a str>,
}

/// Whether the API answers at all: enabled, with a password, and not a demo.
/// Never on Android: a phone is not a music server, and the app has no place
/// to turn the API on (the settings section is hidden there).
pub const fn is_available(settings: &SubsonicSettings, demo_mode: bool) -> bool {
    !cfg!(target_os = "android") && settings.enabled && !settings.password.is_empty() && !demo_mode
}

/// Checks `creds` against the configured user.
///
/// # Errors
/// Code 10 when no credentials were sent, 40 when they do not match.
pub fn check(settings: &SubsonicSettings, creds: Credentials<'_>) -> Result<(), ApiError> {
    let user = creds.user.ok_or_else(|| ApiError::missing("u"))?;
    let password_ok = match (creds.password, creds.token, creds.salt) {
        (Some(p), _, _) => decode_password(p).is_some_and(|p| constant_time_eq(p.as_bytes(), settings.password.as_bytes())),
        (None, Some(t), Some(s)) => {
            let expected = token(&settings.password, s);
            constant_time_eq(t.to_ascii_lowercase().as_bytes(), expected.as_bytes())
        }
        (None, Some(_), None) => return Err(ApiError::missing("s")),
        (None, None, _) => return Err(ApiError::missing("p")),
    };
    if password_ok && constant_time_eq(user.as_bytes(), settings.username.as_bytes()) {
        Ok(())
    } else {
        Err(ApiError::wrong_credentials())
    }
}

/// `md5(password + salt)` as lowercase hex.
pub fn token(password: &str, salt: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    hex(&hasher.finalize())
}

/// The plain password of a `p` parameter: as is, or hex after `enc:`.
fn decode_password(p: &str) -> Option<String> {
    let Some(hex_part) = p.strip_prefix("enc:") else {
        return Some(p.to_string());
    };
    if hex_part.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..hex_part.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex_part.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        _ = write!(s, "{b:02x}");
        s
    })
}

/// Compares without an early exit on the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> SubsonicSettings {
        SubsonicSettings {
            enabled: true,
            username: "alice".to_string(),
            password: "sesame".to_string(),
        }
    }

    fn creds<'a>(user: &'a str, password: Option<&'a str>, token: Option<&'a str>, salt: Option<&'a str>) -> Credentials<'a> {
        Credentials {
            user: Some(user),
            password,
            token,
            salt,
        }
    }

    #[test]
    fn plain_password() {
        assert!(check(&settings(), creds("alice", Some("sesame"), None, None)).is_ok());
        assert_eq!(check(&settings(), creds("alice", Some("wrong"), None, None)).unwrap_err().code, 40);
        assert_eq!(check(&settings(), creds("bob", Some("sesame"), None, None)).unwrap_err().code, 40);
    }

    #[test]
    fn hex_encoded_password() {
        // "sesame" in hex.
        assert!(check(&settings(), creds("alice", Some("enc:736573616d65"), None, None)).is_ok());
        assert!(check(&settings(), creds("alice", Some("enc:736573616D65"), None, None)).is_ok());
        assert_eq!(
            check(&settings(), creds("alice", Some("enc:7365"), None, None)).unwrap_err().code,
            40
        );
        assert_eq!(check(&settings(), creds("alice", Some("enc:zz"), None, None)).unwrap_err().code, 40);
    }

    #[test]
    fn token_and_salt() {
        // The example of the Subsonic API docs.
        assert_eq!(token("sesame", "c19b2d"), "26719a1196d2a940705a59634eb18eab");
        let t = token("sesame", "c19b2d");
        assert!(check(&settings(), creds("alice", None, Some(&t), Some("c19b2d"))).is_ok());
        assert!(check(&settings(), creds("alice", None, Some(&t.to_uppercase()), Some("c19b2d"))).is_ok());
        assert_eq!(
            check(&settings(), creds("alice", None, Some(&t), Some("other"))).unwrap_err().code,
            40
        );
        assert_eq!(check(&settings(), creds("alice", None, Some(&t), None)).unwrap_err().code, 10);
    }

    #[test]
    fn missing_credentials() {
        assert_eq!(check(&settings(), Credentials::default()).unwrap_err().code, 10);
        assert_eq!(check(&settings(), creds("alice", None, None, None)).unwrap_err().code, 10);
    }

    #[test]
    fn availability() {
        assert!(is_available(&settings(), false));
        assert!(!is_available(&settings(), true));
        let mut s = settings();
        s.password.clear();
        assert!(!is_available(&s, false));
        s = settings();
        s.enabled = false;
        assert!(!is_available(&s, false));
    }
}
