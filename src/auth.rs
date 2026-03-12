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
