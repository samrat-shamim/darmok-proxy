use crate::error::{ProxyError, Result};

/// An exact PostgreSQL schema identifier for an explicitly configured route.
///
/// Names preserve their spelling. In particular, `public` and `PUBLIC` are
/// different schemas. Quoting is applied when SQL is generated, not at input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PgSchema {
    name: String,
}

impl PgSchema {
    /// Validate a schema name for a PostgreSQL server with the standard
    /// 63-byte identifier limit. Startup must verify that backend limit.
    /// Reserved namespaces and role grants are checked by route validation.
    /// Search-path metanames are rejected because quoting does not make them
    /// select literal schemas.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.len() > 63 || name.contains('\0') {
            return Err(ProxyError::config(
                "PostgreSQL schema names must contain 1–63 UTF-8 bytes and no NUL",
            ));
        }
        if matches!(name.as_str(), "$user" | "pg_temp") {
            return Err(ProxyError::config(
                "PostgreSQL search_path metanames cannot be routed as literal schemas",
            ));
        }
        Ok(Self { name })
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub fn quoted(&self) -> String {
        format!("\"{}\"", self.name.replace('"', "\"\""))
    }

    /// Select the application schema as the creation target. PostgreSQL searches
    /// pg_catalog implicitly before this schema, without making it the creation
    /// target. The implicit temporary relation namespace remains available too.
    pub fn search_path_sql(&self) -> String {
        format!("SET search_path TO {}", self.quoted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_names_preserve_case_and_unicode() {
        assert_ne!(
            PgSchema::new("public").unwrap(),
            PgSchema::new("PUBLIC").unwrap()
        );
        assert_eq!(PgSchema::new("分析").unwrap().as_str(), "分析");
    }

    #[test]
    fn schema_sql_quotes_the_entire_identifier() {
        let schema = PgSchema::new("sales\"; RESET ALL; --").unwrap();
        assert_eq!(schema.quoted(), "\"sales\"\"; RESET ALL; --\"");
        assert_eq!(
            schema.search_path_sql(),
            "SET search_path TO \"sales\"\"; RESET ALL; --\""
        );
        assert_eq!(PgSchema::new("a.b").unwrap().quoted(), "\"a.b\"");
    }

    #[test]
    fn invalid_names_are_rejected_without_truncation_or_normalization() {
        for name in [
            "".to_owned(),
            "a\0b".to_owned(),
            "a".repeat(64),
            "界".repeat(22),
        ] {
            assert!(PgSchema::new(name).is_err());
        }
        assert!(PgSchema::new("a".repeat(63)).is_ok());
        assert!(PgSchema::new("界".repeat(21)).is_ok());
    }

    #[test]
    fn search_path_metanames_fail_explicitly_without_case_folding() {
        assert!(PgSchema::new("$user").is_err());
        assert!(PgSchema::new("pg_temp").is_err());
        assert!(PgSchema::new("$USER").is_ok());
        assert!(PgSchema::new("PG_TEMP").is_ok());
    }
}
