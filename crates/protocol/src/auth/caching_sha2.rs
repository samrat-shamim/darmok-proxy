use darmok_types::error::Result;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Generate a client-side scramble using `caching_sha2_password`.
///
/// The client sends: `SHA256(password) XOR SHA256(SHA256(SHA256(password)) + nonce)`.
/// This is useful for testing and for acting as a MySQL client.
pub fn generate_scramble(password: &[u8], nonce: &[u8]) -> Result<Vec<u8>> {
    if password.is_empty() {
        return Ok(Vec::new());
    }

    let stage1 = Sha256::digest(password);
    let stage2 = Sha256::digest(stage1);

    let mut hasher = Sha256::new();
    hasher.update(stage2);
    hasher.update(nonce);
    let check = hasher.finalize();

    Ok(stage1
        .iter()
        .zip(check.iter())
        .map(|(lhs, rhs)| lhs ^ rhs)
        .collect())
}

/// Verify a client's scramble response for `caching_sha2_password`.
///
/// The proxy stores `SHA256(SHA256(password))` for each user.
/// To verify:
/// 1. Compute `check = SHA256(stored_sha256 + nonce)`
/// 2. XOR `check` with `client_response` to recover candidate `SHA256(password)`
/// 3. Hash the candidate once more and compare with `stored_sha256`
///
/// Returns `true` if the client provided the correct password scramble.
pub fn verify_scramble(client_response: &[u8], nonce: &[u8], stored_sha256: &[u8]) -> bool {
    if client_response.len() != 32 || stored_sha256.len() != 32 {
        return false;
    }

    // Step 1: SHA256(stored_sha256 + nonce)
    let mut hasher = Sha256::new();
    hasher.update(stored_sha256);
    hasher.update(nonce);
    let check = hasher.finalize();

    // Step 2: XOR to recover candidate SHA256(password)
    let candidate_stage1: [u8; 32] =
        std::array::from_fn(|index| client_response[index] ^ check[index]);

    // Step 3: SHA256(candidate) should equal stored_sha256
    let candidate_stage2 = Sha256::digest(candidate_stage1);
    bool::from(candidate_stage2.as_slice().ct_eq(stored_sha256))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn caching_sha2_matches_known_scramble() {
        let nonce: Vec<u8> = (1u8..=20).collect();
        let actual = generate_scramble(b"secret", &nonce).unwrap();
        // Expected value computed with correct caching_sha2 order:
        // SHA256(SHA256(SHA256(password)) + nonce), not SHA256(nonce + SHA256(SHA256(password)))
        let expected = [
            0x74, 0x6E, 0xBE, 0x20, 0x5D, 0x56, 0xA0, 0x70, 0x7A, 0xCB, 0x3E, 0x79, 0x6E, 0x83,
            0x4E, 0x0D, 0xD7, 0xB1, 0xD6, 0x17, 0x43, 0xB2, 0x6B, 0xD5, 0x20, 0x2C, 0x7A, 0x62,
            0x32, 0x30, 0xC7, 0xC9,
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn caching_sha2_empty_password_is_empty_response() {
        assert_eq!(generate_scramble(b"", b"nonce").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn verify_scramble_accepts_valid_response() {
        let password = b"secret";
        let nonce: Vec<u8> = (1u8..=20).collect();

        // Compute stored value: SHA256(SHA256(password))
        let stage1 = Sha256::digest(password);
        let double_sha256 = Sha256::digest(stage1);

        let client_response = generate_scramble(password, &nonce).unwrap();
        assert!(verify_scramble(
            &client_response,
            &nonce,
            double_sha256.as_slice()
        ));
    }

    #[test]
    fn verify_scramble_rejects_wrong_password() {
        let nonce: Vec<u8> = (1u8..=20).collect();

        let stage1 = Sha256::digest(b"correct_password");
        let double_sha256 = Sha256::digest(stage1);

        let wrong_response = generate_scramble(b"wrong_password", &nonce).unwrap();
        assert!(!verify_scramble(
            &wrong_response,
            &nonce,
            double_sha256.as_slice()
        ));
    }

    #[test]
    fn verification_checks_every_response_byte_and_binds_the_nonce() {
        let nonce: Vec<u8> = (1u8..=20).collect();
        let stored = Sha256::digest(Sha256::digest(b"secret"));
        let response = generate_scramble(b"secret", &nonce).unwrap();
        for index in 0..32 {
            let mut altered = response.clone();
            altered[index] ^= 1;
            assert!(!verify_scramble(&altered, &nonce, &stored));
        }
        let mut other_nonce = nonce.clone();
        other_nonce[19] ^= 1;
        assert!(!verify_scramble(&response, &other_nonce, &stored));
    }

    #[test]
    fn verify_scramble_rejects_bad_lengths() {
        assert!(!verify_scramble(&[0u8; 31], &[1u8; 20], &[2u8; 32]));
        assert!(!verify_scramble(&[0u8; 32], &[1u8; 20], &[2u8; 31]));
    }
}
