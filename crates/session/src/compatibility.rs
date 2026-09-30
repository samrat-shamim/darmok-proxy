use darmok_types::mysql_const;

/// One authoritative MySQL compatibility profile for the proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MysqlCompatibilityProfile {
    pub server_version: &'static str,
    pub version_comment: &'static str,
    pub version_compile_os: &'static str,
    pub default_charset: &'static str,
    pub default_collation: &'static str,
    pub default_sql_mode: &'static str,
    pub default_time_zone: &'static str,
    pub default_transaction_isolation: &'static str,
    pub max_allowed_packet: u64,
    pub wait_timeout_seconds: u64,
    pub interactive_timeout_seconds: u64,
    charsets: &'static [MysqlCharset],
    collations: &'static [CharsetInfo],
    storage_engines: &'static [MysqlStorageEngine],
}

/// Metadata for a MySQL character set / default collation row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MysqlCharset {
    pub name: &'static str,
    pub description: &'static str,
    pub default_collation: &'static str,
    pub max_len: u8,
    pub backend_client_encoding: Option<&'static str>,
}

/// Metadata for a MySQL collation row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharsetInfo {
    pub id: u16,
    pub name: &'static str,
    pub collation: &'static str,
    pub max_len: u8,
    pub is_default: bool,
    pub compiled: bool,
    pub sort_len: u8,
    pub pad_attribute: &'static str,
}

/// Metadata for a MySQL storage engine row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MysqlStorageEngine {
    pub name: &'static str,
    pub support: &'static str,
    pub comment: &'static str,
    pub transactions: &'static str,
    pub xa: &'static str,
    pub savepoints: &'static str,
}

pub static MYSQL8_COMPATIBILITY_PROFILE: MysqlCompatibilityProfile = MysqlCompatibilityProfile {
    server_version: concat!("8.4.0-darmok-", env!("CARGO_PKG_VERSION")),
    version_comment: "Darmok MySQL-to-PostgreSQL proxy",
    version_compile_os: "Linux",
    default_charset: "utf8mb4",
    default_collation: "utf8mb4_general_ci",
    default_sql_mode: "STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ERROR_FOR_DIVISION_BY_ZERO,NO_ENGINE_SUBSTITUTION",
    default_time_zone: "UTC",
    default_transaction_isolation: "READ-COMMITTED",
    max_allowed_packet: 67_108_864,
    wait_timeout_seconds: 28_800,
    interactive_timeout_seconds: 28_800,
    charsets: MYSQL8_CHARSETS,
    collations: MYSQL8_COLLATIONS,
    storage_engines: MYSQL8_STORAGE_ENGINES,
};

static MYSQL8_CHARSETS: &[MysqlCharset] = &[
    MysqlCharset {
        name: "ascii",
        description: "US ASCII",
        default_collation: "ascii_general_ci",
        max_len: 1,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "big5",
        description: "Big5 Traditional Chinese",
        default_collation: "big5_chinese_ci",
        max_len: 2,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "binary",
        description: "Binary pseudo charset",
        default_collation: "binary",
        max_len: 1,
        backend_client_encoding: Some("UTF8"),
    },
    MysqlCharset {
        name: "cp1251",
        description: "Windows Cyrillic",
        default_collation: "cp1251_general_ci",
        max_len: 1,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "hebrew",
        description: "ISO 8859-8 Hebrew",
        default_collation: "hebrew_general_ci",
        max_len: 1,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "koi8r",
        description: "KOI8-R Relcom Russian",
        default_collation: "koi8r_general_ci",
        max_len: 1,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "latin1",
        description: "cp1252 West European",
        default_collation: "latin1_swedish_ci",
        max_len: 1,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "tis620",
        description: "TIS620 Thai",
        default_collation: "tis620_thai_ci",
        max_len: 1,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "ujis",
        description: "EUC-JP Japanese",
        default_collation: "ujis_japanese_ci",
        max_len: 3,
        backend_client_encoding: None,
    },
    MysqlCharset {
        name: "utf8mb3",
        description: "UTF-8 Unicode",
        default_collation: "utf8mb3_general_ci",
        max_len: 3,
        backend_client_encoding: Some("UTF8"),
    },
    MysqlCharset {
        name: "utf8mb4",
        description: "UTF-8 Unicode",
        default_collation: "utf8mb4_general_ci",
        max_len: 4,
        backend_client_encoding: Some("UTF8"),
    },
];

static MYSQL8_COLLATIONS: &[CharsetInfo] = &[
    CharsetInfo {
        id: 11,
        name: "ascii",
        collation: "ascii_general_ci",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::BIG5_CHINESE_CI,
        name: "big5",
        collation: "big5_chinese_ci",
        max_len: 2,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::BINARY,
        name: "binary",
        collation: "binary",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "NO PAD",
    },
    CharsetInfo {
        id: mysql_const::charset::CP1251_GENERAL_CI,
        name: "cp1251",
        collation: "cp1251_general_ci",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::HEBREW_GENERAL_CI,
        name: "hebrew",
        collation: "hebrew_general_ci",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::KOI8R_GENERAL_CI,
        name: "koi8r",
        collation: "koi8r_general_ci",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::LATIN1_SWEDISH_CI,
        name: "latin1",
        collation: "latin1_swedish_ci",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::LATIN1_GENERAL_CI,
        name: "latin1",
        collation: "latin1_general_ci",
        max_len: 1,
        is_default: false,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::LATIN1_GENERAL_CS,
        name: "latin1",
        collation: "latin1_general_cs",
        max_len: 1,
        is_default: false,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::TIS620_THAI_CI,
        name: "tis620",
        collation: "tis620_thai_ci",
        max_len: 1,
        is_default: true,
        compiled: true,
        sort_len: 4,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UJIS_JAPANESE_CI,
        name: "ujis",
        collation: "ujis_japanese_ci",
        max_len: 3,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB3_GENERAL_CI,
        name: "utf8mb3",
        collation: "utf8mb3_general_ci",
        max_len: 3,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB3_BIN,
        name: "utf8mb3",
        collation: "utf8mb3_bin",
        max_len: 3,
        is_default: false,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB3_UNICODE_CI,
        name: "utf8mb3",
        collation: "utf8mb3_unicode_ci",
        max_len: 3,
        is_default: false,
        compiled: true,
        sort_len: 8,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB4_GENERAL_CI,
        name: "utf8mb4",
        collation: "utf8mb4_general_ci",
        max_len: 4,
        is_default: true,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB4_BIN,
        name: "utf8mb4",
        collation: "utf8mb4_bin",
        max_len: 4,
        is_default: false,
        compiled: true,
        sort_len: 1,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB4_UNICODE_CI,
        name: "utf8mb4",
        collation: "utf8mb4_unicode_ci",
        max_len: 4,
        is_default: false,
        compiled: true,
        sort_len: 8,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: 246,
        name: "utf8mb4",
        collation: "utf8mb4_unicode_520_ci",
        max_len: 4,
        is_default: false,
        compiled: true,
        sort_len: 8,
        pad_attribute: "PAD SPACE",
    },
    CharsetInfo {
        id: mysql_const::charset::UTF8MB4_0900_AI_CI,
        name: "utf8mb4",
        collation: "utf8mb4_0900_ai_ci",
        max_len: 4,
        is_default: false,
        compiled: true,
        sort_len: 0,
        pad_attribute: "NO PAD",
    },
];

static MYSQL8_STORAGE_ENGINES: &[MysqlStorageEngine] = &[
    MysqlStorageEngine {
        name: "InnoDB",
        support: "DEFAULT",
        comment: "Supports transactions, row-level locking, and foreign keys",
        transactions: "YES",
        xa: "YES",
        savepoints: "YES",
    },
    MysqlStorageEngine {
        name: "MEMORY",
        support: "YES",
        comment: "Hash based, stored in memory, useful for temporary tables",
        transactions: "NO",
        xa: "NO",
        savepoints: "NO",
    },
    MysqlStorageEngine {
        name: "PERFORMANCE_SCHEMA",
        support: "YES",
        comment: "Performance Schema",
        transactions: "NO",
        xa: "NO",
        savepoints: "NO",
    },
];

impl MysqlCompatibilityProfile {
    pub const fn default_mysql8() -> &'static Self {
        &MYSQL8_COMPATIBILITY_PROFILE
    }

    pub fn charsets(&'static self) -> &'static [MysqlCharset] {
        self.charsets
    }

    pub fn collations(&'static self) -> &'static [CharsetInfo] {
        self.collations
    }

    pub fn storage_engines(&'static self) -> &'static [MysqlStorageEngine] {
        self.storage_engines
    }

    pub fn default_collation_info(&'static self) -> &'static CharsetInfo {
        self.lookup_collation_name(self.default_collation)
            .expect("default MySQL compatibility collation must be declared")
    }

    pub fn default_storage_engine(&'static self) -> &'static MysqlStorageEngine {
        self.storage_engines
            .iter()
            .find(|engine| engine.support.eq_ignore_ascii_case("DEFAULT"))
            .expect("default MySQL compatibility storage engine must be declared")
    }

    pub fn lookup_charset(&'static self, id: u16) -> Option<&'static CharsetInfo> {
        self.collations.iter().find(|collation| collation.id == id)
    }

    pub fn lookup_charset_name(&'static self, name: &str) -> Option<&'static CharsetInfo> {
        let charset = self.lookup_charset_entry(name)?;
        self.lookup_collation_name(charset.default_collation)
    }

    pub fn lookup_charset_entry(&'static self, name: &str) -> Option<&'static MysqlCharset> {
        let normalized = normalize_mysql_name(name);
        let name = self.normalize_charset_alias(&normalized);
        self.charsets.iter().find(|charset| charset.name == name)
    }

    pub fn lookup_collation_name(&'static self, name: &str) -> Option<&'static CharsetInfo> {
        let normalized = normalize_mysql_name(name);
        let normalized = Self::normalize_collation_alias(&normalized);
        self.collations
            .iter()
            .find(|collation| collation.collation == normalized)
    }

    pub fn lookup_storage_engine(&'static self, name: &str) -> Option<&'static MysqlStorageEngine> {
        let normalized = normalize_mysql_name(name);
        self.storage_engines
            .iter()
            .find(|engine| engine.name.eq_ignore_ascii_case(&normalized))
    }

    pub fn backend_client_encoding_for_charset(&'static self, name: &str) -> Option<&'static str> {
        self.lookup_charset_entry(name)?.backend_client_encoding
    }

    fn normalize_charset_alias<'a>(&'static self, normalized: &'a str) -> &'a str {
        match normalized {
            "default" => self.default_charset,
            "utf8" => "utf8mb3",
            other => other,
        }
    }

    fn normalize_collation_alias(normalized: &str) -> String {
        normalized
            .strip_prefix("utf8_")
            .map(|suffix| format!("utf8mb3_{suffix}"))
            .unwrap_or_else(|| normalized.to_owned())
    }
}

pub fn normalize_mysql_name(name: &str) -> String {
    name.trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
}
