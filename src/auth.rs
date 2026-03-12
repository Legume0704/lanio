use sha2::{Digest, Sha512};

/// Computes a 128-character hex token from a password using a salted SHA-512 hash.
/// The same computation is mirrored in the home page JS using the WebCrypto API.
pub fn compute_token(password: &str) -> String {
    let input = format!("lanio_auth:{}", password);
    let hash = Sha512::digest(input.as_bytes());
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_128_chars() {
        let token = compute_token("any_password");
        assert_eq!(token.len(), 128);
    }

    #[test]
    fn token_is_hex() {
        let token = compute_token("any_password");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn same_password_produces_same_token() {
        assert_eq!(compute_token("secret"), compute_token("secret"));
    }

    #[test]
    fn different_passwords_produce_different_tokens() {
        assert_ne!(compute_token("secret"), compute_token("other"));
    }

    #[test]
    fn empty_password_produces_token() {
        let token = compute_token("");
        assert_eq!(token.len(), 128);
    }

    /// Known-good value computed independently to guard against regressions.
    #[test]
    fn known_value() {
        // echo -n 'lanio_auth:hunter2' | sha512sum
        let token = compute_token("hunter2");
        assert_eq!(&token[..10], "be3a059005");
    }
}
