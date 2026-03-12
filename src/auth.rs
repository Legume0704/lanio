use sha2::{Digest, Sha512};

/// Computes a 256-character hex token from a password using two salted SHA-512 hashes.
/// The same computation is mirrored in the home page JS using the WebCrypto API.
pub fn compute_token(password: &str) -> String {
    let input1 = format!("lanio_auth_a:{}", password);
    let input2 = format!("lanio_auth_b:{}", password);

    let hash1 = Sha512::digest(input1.as_bytes());
    let hash2 = Sha512::digest(input2.as_bytes());

    let hex1: String = hash1.iter().map(|b| format!("{:02x}", b)).collect();
    let hex2: String = hash2.iter().map(|b| format!("{:02x}", b)).collect();

    format!("{}{}", hex1, hex2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_256_chars() {
        let token = compute_token("any_password");
        assert_eq!(token.len(), 256);
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
        assert_eq!(token.len(), 256);
    }

    /// Known-good value computed independently to guard against regressions.
    #[test]
    fn known_value() {
        // echo -n 'lanio_auth_a:hunter2' | sha512sum  => first 128 chars
        // echo -n 'lanio_auth_b:hunter2' | sha512sum  => last 128 chars
        let token = compute_token("hunter2");
        assert_eq!(&token[..10], "d6e22e9b78"); // first 10 hex chars of hash_a
    }
}
