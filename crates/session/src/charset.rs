use crate::compatibility::{CharsetInfo, MysqlCompatibilityProfile, normalize_mysql_name};

/// Resolved session charset/collation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCharset {
    pub id: u16,
    pub name: String,
    pub collation: String,
    pub max_len: u8,
}

impl From<&'static CharsetInfo> for ResolvedCharset {
    fn from(info: &'static CharsetInfo) -> Self {
        Self {
            id: info.id,
            name: info.name.to_owned(),
            collation: info.collation.to_owned(),
            max_len: info.max_len,
        }
    }
}

/// Look up charset metadata by its numeric collation ID.
pub fn lookup_charset(id: u16) -> Option<&'static CharsetInfo> {
    MysqlCompatibilityProfile::default_mysql8().lookup_charset(id)
}

/// Look up the default collation metadata for a character set name.
pub fn lookup_charset_name(name: &str) -> Option<&'static CharsetInfo> {
    MysqlCompatibilityProfile::default_mysql8().lookup_charset_name(name)
}

/// Look up charset metadata by collation name.
pub fn lookup_collation(name: &str) -> Option<&'static CharsetInfo> {
    MysqlCompatibilityProfile::default_mysql8().lookup_collation_name(name)
}

/// Resolve a character set name to its default collation metadata.
pub fn resolve_charset_name(name: &str) -> Option<ResolvedCharset> {
    lookup_charset_name(name).map(ResolvedCharset::from)
}

/// Resolve a collation name to session metadata. Only profile-declared
/// collations are accepted, so session state cannot invent collation IDs
/// from naming heuristics.
pub fn resolve_collation_name(name: &str) -> Option<ResolvedCharset> {
    lookup_collation(&normalize_mysql_name(name)).map(ResolvedCharset::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use darmok_types::mysql_const;

    #[test]
    fn lookup_utf8mb4_general_ci() {
        let info = lookup_charset(45).unwrap();
        assert_eq!(info.name, "utf8mb4");
        assert_eq!(info.collation, "utf8mb4_general_ci");
        assert_eq!(info.max_len, 4);
    }

    #[test]
    fn lookup_missing_returns_none() {
        assert!(lookup_charset(9999).is_none());
    }

    #[test]
    fn lookup_charset_name_uses_default_collation() {
        let info = lookup_charset_name("utf8mb4").unwrap();
        assert_eq!(info.id, mysql_const::charset::UTF8MB4_GENERAL_CI);
        assert_eq!(info.collation, "utf8mb4_general_ci");
    }

    #[test]
    fn lookup_charset_name_accepts_default_and_utf8mb3_aliases() {
        assert_eq!(
            lookup_charset_name("DEFAULT").unwrap().collation,
            "utf8mb4_general_ci"
        );
        assert_eq!(
            lookup_charset_name("utf8").unwrap().collation,
            "utf8mb3_general_ci"
        );
        assert_eq!(
            lookup_charset_name("utf8mb3").unwrap().collation,
            "utf8mb3_general_ci"
        );
    }

    #[test]
    fn lookup_charset_name_declares_client_metadata_charsets() {
        let cases = [
            ("big5", "big5_chinese_ci", 2),
            ("koi8r", "koi8r_general_ci", 1),
            ("hebrew", "hebrew_general_ci", 1),
            ("cp1251", "cp1251_general_ci", 1),
            ("tis620", "tis620_thai_ci", 1),
            ("ujis", "ujis_japanese_ci", 3),
        ];

        for (charset, collation, max_len) in cases {
            let info = lookup_charset_name(charset).unwrap();
            assert_eq!(info.name, charset);
            assert_eq!(info.collation, collation);
            assert_eq!(info.max_len, max_len);
        }
    }

    #[test]
    fn lookup_collation_resolves_non_default_collation() {
        let info = lookup_collation("UTF8MB4_BIN").unwrap();
        assert_eq!(info.id, mysql_const::charset::UTF8MB4_BIN);
        assert_eq!(info.name, "utf8mb4");
    }

    #[test]
    fn resolve_collation_name_accepts_declared_non_default_collations() {
        let info = resolve_collation_name("utf8mb4_unicode_520_ci").unwrap();
        assert_eq!(info.id, 246);
        assert_eq!(info.name, "utf8mb4");
        assert_eq!(info.collation, "utf8mb4_unicode_520_ci");

        let info = resolve_collation_name("latin1_general_cs").unwrap();
        assert_eq!(info.id, 49);
        assert_eq!(info.name, "latin1");
        assert_eq!(info.collation, "latin1_general_cs");
    }

    #[test]
    fn resolve_collation_name_canonicalizes_utf8_collation_aliases() {
        let info = resolve_collation_name("utf8_bin").unwrap();
        assert_eq!(info.id, mysql_const::charset::UTF8MB3_BIN);
        assert_eq!(info.name, "utf8mb3");
        assert_eq!(info.collation, "utf8mb3_bin");

        let info = resolve_collation_name("utf8_general_ci").unwrap();
        assert_eq!(info.id, mysql_const::charset::UTF8MB3_GENERAL_CI);
        assert_eq!(info.name, "utf8mb3");
        assert_eq!(info.collation, "utf8mb3_general_ci");
    }

    #[test]
    fn resolve_collation_name_rejects_undeclared_collations() {
        assert!(resolve_collation_name("made_up_general_ci").is_none());
        assert!(resolve_collation_name("utf8mb4_general_unknown").is_none());
        assert!(resolve_collation_name("utf8mb4_estonian_ci").is_none());
    }
}
