use darmok_types::error::Result;
use sha1::{Digest, Sha1};
use subtle::ConstantTimeEq;

/// Generate a client-side scramble using `mysql_native_password` (SHA1-based).
///
/// The client sends: `SHA1(password) XOR SHA1(nonce + SHA1(SHA1(password)))`.
/// This is useful for testing and for acting as a MySQL client.
pub fn generate_scramble(password: &[u8], nonce: &[u8]) -> Result<Vec<u8>> {
    if password.is_empty() {
        return Ok(Vec::new());
    }

    let stage1 = Sha1::digest(password);
    let stage2 = Sha1::digest(stage1);

    let mut hasher = Sha1::new();
    hasher.update(nonce);
    hasher.update(stage2);
    let check = hasher.finalize();

    Ok(stage1
        .iter()
        .zip(check.iter())
        .map(|(lhs, rhs)| lhs ^ rhs)
        .collect())
}

/// Verify a client's scramble response for `mysql_native_password`.
///
/// The proxy stores `SHA1(SHA1(password))` (the "double SHA1") for each user.
/// To verify:
/// 1. Compute `check = SHA1(nonce + stored_double_sha1)`
/// 2. XOR `check` with `client_response` to recover candidate `SHA1(password)`
/// 3. Hash the candidate once more and compare with `stored_double_sha1`
///
/// Returns `true` if the client provided the correct password scramble.
pub fn verify_scramble(client_response: &[u8], nonce: &[u8], stored_double_sha1: &[u8]) -> bool {
    if client_response.len() != 20 || stored_double_sha1.len() != 20 {
        return false;
    }

    // Step 1: SHA1(nonce + stored_double_sha1)
    let mut hasher = Sha1::new();
    hasher.update(nonce);
    hasher.update(stored_double_sha1);
    let check = hasher.finalize();

    // Step 2: XOR to recover candidate SHA1(password)
    let candidate_stage1: [u8; 20] =
        std::array::from_fn(|index| client_response[index] ^ check[index]);

    // Step 3: SHA1(candidate) should equal stored_double_sha1
    let candidate_stage2 = Sha1::digest(candidate_stage1);
    bool::from(candidate_stage2.as_slice().ct_eq(stored_double_sha1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::{Digest, Sha1};

    #[test]
    fn native_password_matches_known_scramble() {
        let nonce: Vec<u8> = (1u8..=20).collect();
        let actual = generate_scramble(b"secret", &nonce).unwrap();
        let expected = [
            0xB3, 0x2B, 0xB3, 0xA5, 0x83, 0xE1, 0x34, 0x0C, 0x0A, 0x11, 0x08, 0xD5, 0x8B, 0x1B,
            0xE4, 0x97, 0x81, 0xAD, 0x8C, 0x2F,
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn native_password_empty_password_is_empty_response() {
        assert_eq!(generate_scramble(b"", b"nonce").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn verify_scramble_accepts_valid_response() {
        let password = b"secret";
        let nonce: Vec<u8> = (1u8..=20).collect();

        // Compute stored_double_sha1 = SHA1(SHA1(password))
        let stage1 = Sha1::digest(password);
        let double_sha1 = Sha1::digest(stage1);

        let client_response = generate_scramble(password, &nonce).unwrap();
        assert!(verify_scramble(
            &client_response,
            &nonce,
            double_sha1.as_slice()
        ));
    }

    #[test]
    fn verify_scramble_rejects_wrong_password() {
        let nonce: Vec<u8> = (1u8..=20).collect();

        let stage1 = Sha1::digest(b"correct_password");
        let double_sha1 = Sha1::digest(stage1);

        let wrong_response = generate_scramble(b"wrong_password", &nonce).unwrap();
        assert!(!verify_scramble(
            &wrong_response,
            &nonce,
            double_sha1.as_slice()
        ));
    }

    #[test]
    fn verification_checks_every_response_byte_and_binds_the_nonce() {
        let nonce: Vec<u8> = (1u8..=20).collect();
        let stored = Sha1::digest(Sha1::digest(b"secret"));
        let response = generate_scramble(b"secret", &nonce).unwrap();
        for index in 0..20 {
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
        assert!(!verify_scramble(&[0u8; 19], &[1u8; 20], &[2u8; 20]));
        assert!(!verify_scramble(&[0u8; 20], &[1u8; 20], &[2u8; 19]));
    }
}
