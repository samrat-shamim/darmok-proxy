use darmok_types::error::Result;

/// Authenticate using `mysql_clear_password` (plaintext, TLS required).
pub fn authenticate(password: &[u8]) -> Result<Vec<u8>> {
    let mut response = Vec::with_capacity(password.len() + 1);
    response.extend_from_slice(password);
    response.push(0);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::authenticate;

    #[test]
    fn clear_text_auth_is_null_terminated() {
        assert_eq!(authenticate(b"secret").unwrap(), b"secret\0");
        assert_eq!(authenticate(b"").unwrap(), b"\0");
    }
}
