//! Actual binary processes against ordinary PostgreSQL 17/18 fixtures.
mod support;

use std::{
    process::Output,
    sync::atomic::{AtomicU64, Ordering},
};

use support::{SETTINGS_ENV, invoke};
use tokio::task::JoinHandle;
use tokio_postgres::{Client, Config, NoTls};

fn assert_success(output: &Output, action: &str) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "Darmok database {action} completed for compatibility format version 1 and server extension 1.0\n"
        )
    );
}

fn assert_database_error(output: &Output, action: &str, reason: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("darmok: database {action} failed: SQLSTATE P0001: {reason}; rollback confirmed\n")
    );
}

struct Connection {
    client: Client,
    driver: JoinHandle<Result<(), tokio_postgres::Error>>,
}

impl Connection {
    async fn new(config: &Config) -> Self {
        let (client, connection) = config
            .connect(NoTls)
            .await
            .expect("required fixture connection");
        let driver = tokio::spawn(connection);
        Self { client, driver }
    }

    async fn close(self) {
        drop(self.client);
        self.driver.await.unwrap().unwrap();
    }
}

struct Fixture {
    admin: Connection,
    observer: Connection,
    settings: String,
    database: String,
}

impl Fixture {
    async fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let database = format!("darmok_cli_{}_{}", std::process::id(), serial);
        let url = std::env::var("DARMOK_TEST_DATABASE_URL")
            .expect("required process tests need DARMOK_TEST_DATABASE_URL");
        // The declared private fixture is a PostgreSQL URI. The connector's
        // documented query parameters override the database without rewriting
        // credentials, hosts, or options. No production routing uses this helper.
        assert!(url.starts_with("postgres://") || url.starts_with("postgresql://"));
        assert!(
            !url.contains('#'),
            "fixture URI must not contain a fragment"
        );
        let separator = if url.contains('?') { '&' } else { '?' };
        let settings = format!(
            "{url}{separator}dbname={database}&application_name=darmok_cli_child_{database}"
        );
        let base = url.parse::<Config>().unwrap();
        let selected = settings.parse::<Config>().unwrap();
        assert_eq!(selected.get_dbname(), Some(database.as_str()));
        let admin = Connection::new(&base).await;
        admin
            .client
            .batch_execute(&format!(
                "CREATE DATABASE {database} TEMPLATE template0 ENCODING 'UTF8'"
            ))
            .await
            .unwrap();
        let observer = Connection::new(&selected).await;
        // Keep the observer distinct from command processes in activity checks.
        observer
            .client
            .batch_execute("SET application_name = 'darmok_cli_observer'")
            .await
            .unwrap();
        Self {
            admin,
            observer,
            settings,
            database,
        }
    }

    async fn command(&self, label: &str, initialize: bool, read_only_default: bool) -> Output {
        let mut settings = self.settings.clone();
        if read_only_default {
            settings.push_str("&options=-c%20default_transaction_read_only%3Don");
            assert_eq!(
                settings.parse::<Config>().unwrap().get_options(),
                Some("-c default_transaction_read_only=on")
            );
        }
        self.command_with_settings(label, initialize, settings)
            .await
    }

    async fn keyword_command(&self, label: &str, initialize: bool) -> Output {
        // Convert only the declared private one-host fixture's existing login
        // into the driver's documented keyword grammar. This is test input,
        // not a product credential/profile serializer or alternate connector.
        let config = self.settings.parse::<Config>().unwrap();
        let [tokio_postgres::config::Host::Tcp(host)] = config.get_hosts() else {
            panic!("keyword process fixture requires one TCP host");
        };
        let [port] = config.get_ports() else {
            panic!("keyword process fixture requires an explicit port");
        };
        let user = config.get_user().expect("fixture user must be explicit");
        let password = std::str::from_utf8(
            config
                .get_password()
                .expect("fixture password must be explicit"),
        )
        .unwrap();
        let application_name = format!("darmok_cli_child_{}", self.database);
        let mut settings = [
            ("host", host.as_str()),
            ("user", user),
            ("password", password),
            ("dbname", self.database.as_str()),
            ("application_name", application_name.as_str()),
            ("options", "-c default_transaction_read_only=on"),
        ]
        .into_iter()
        .map(|(key, value)| {
            format!(
                "{key}='{}'",
                value.replace('\\', "\\\\").replace('\'', "\\'")
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
        settings.push_str(&format!(" port={port}"));
        let selected = settings.parse::<Config>().unwrap();
        assert_eq!(selected.get_dbname(), Some(self.database.as_str()));
        assert_eq!(selected.get_hosts(), config.get_hosts());
        assert_eq!(selected.get_ports(), config.get_ports());
        assert!(selected.get_password() == config.get_password());
        self.command_with_settings(label, initialize, settings)
            .await
    }

    async fn command_with_settings(
        &self,
        label: &str,
        initialize: bool,
        settings: String,
    ) -> Output {
        let arguments = if initialize {
            vec!["init", "--database-url-env", SETTINGS_ENV]
        } else {
            vec!["verify", "--database-url-env", SETTINGS_ENV]
        };
        let label = label.to_owned();
        let output =
            tokio::task::spawn_blocking(move || invoke(&label, &arguments, Some(&settings)))
                .await
                .unwrap();
        // Process exit confirms local disposal. Observe the server independently
        // to avoid assuming that TCP close has already reached its activity view.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let active: i64 = self.admin.client.query_one(
                "SELECT pg_catalog.count(*) FROM pg_catalog.pg_stat_activity WHERE datname = $1 AND application_name = $2",
                &[&self.database, &format!("darmok_cli_child_{}", self.database)]
            ).await.unwrap().get(0);
            if active == 0 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "CLI connection did not close"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        output
    }

    async fn snapshot(&self) -> (String, Option<String>) {
        let client = &self.observer.client;
        let catalog: String = client.query_one(
            "SELECT pg_catalog.jsonb_build_object(\
                'namespace', (SELECT pg_catalog.jsonb_build_object('oid', n.oid, 'name', n.nspname, 'xmin', n.xmin::pg_catalog.text) FROM pg_catalog.pg_namespace n WHERE nspname = 'darmok'), \
                'server_namespace', (SELECT pg_catalog.jsonb_build_object('oid', n.oid, 'name', n.nspname, 'xmin', n.xmin::pg_catalog.text) FROM pg_catalog.pg_namespace n WHERE nspname = 'darmok_server'), \
                'server_extension', (SELECT (pg_catalog.to_jsonb(e) - 'extowner') || pg_catalog.jsonb_build_object('xmin', e.xmin::pg_catalog.text) FROM pg_catalog.pg_extension e WHERE extname = 'darmok_server'), \
                'relations', (SELECT pg_catalog.jsonb_agg((pg_catalog.to_jsonb(c) - 'relacl' - 'relowner') || pg_catalog.jsonb_build_object('xmin', c.xmin::pg_catalog.text) ORDER BY c.oid) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'darmok'), \
                'functions', (SELECT pg_catalog.jsonb_agg(pg_catalog.jsonb_build_object('definition', pg_catalog.pg_get_functiondef(p.oid), 'oid', p.oid, 'xmin', p.xmin::pg_catalog.text) ORDER BY p.oid) FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = 'darmok')\
            )::pg_catalog.text", &[]
        ).await.unwrap().get(0);
        let has_table: bool = client.query_one(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'darmok' AND c.relname = 'installation' AND c.relkind = 'r')", &[]
        ).await.unwrap().get(0);
        let metadata = if has_table {
            client.query_one(
                "SELECT pg_catalog.jsonb_agg(pg_catalog.jsonb_build_object('row', pg_catalog.to_jsonb(i), 'xmin', i.xmin::pg_catalog.text))::pg_catalog.text FROM darmok.installation i", &[]
            ).await.unwrap().get(0)
        } else {
            None
        };
        (catalog, metadata)
    }

    async fn close(self) {
        self.observer.close().await;
        self.admin
            .client
            .batch_execute(&format!("DROP DATABASE {}", self.database))
            .await
            .unwrap();
        let remaining: i64 = self
            .admin
            .client
            .query_one(
                "SELECT pg_catalog.count(*) FROM pg_catalog.pg_database WHERE datname = $1",
                &[&self.database],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(remaining, 0);
        self.admin.close().await;
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 CLI process CI"]
async fn fresh_repeat_and_readonly_verification_preserve_application_and_installation() {
    let fixture = Fixture::new().await;
    fixture.observer.client.batch_execute(
        "CREATE TABLE public.sample(id pg_catalog.int4 PRIMARY KEY, payload pg_catalog.text); INSERT INTO public.sample VALUES (1, 'before'), (2, NULL)"
    ).await.unwrap();
    let application = || async {
        fixture.observer.client.query_one(
            "SELECT pg_catalog.jsonb_build_object('oid', 'public.sample'::pg_catalog.regclass::pg_catalog.oid, 'rows', (SELECT pg_catalog.jsonb_agg(pg_catalog.jsonb_build_object('data', pg_catalog.to_jsonb(s), 'xmin', s.xmin::pg_catalog.text) ORDER BY id) FROM public.sample s))::pg_catalog.text", &[]
        ).await.unwrap().get::<_, String>(0)
    };
    let original_application = application().await;
    let absent = fixture.snapshot().await;
    let missing = fixture.command("fresh-missing-verify", false, true).await;
    assert_database_error(
        &missing,
        "verification",
        "Darmok schema is missing; explicit initialization is required",
    );
    assert_eq!(fixture.snapshot().await, absent);
    assert_eq!(application().await, original_application);
    let init = fixture.command("fresh-init", true, true).await;
    assert_success(&init, "initialization");
    let installed = fixture.snapshot().await;
    assert!(installed.1.is_some());
    for (label, initialize) in [
        ("fresh-repeat-init", true),
        ("fresh-verify", false),
        ("fresh-repeat-verify", false),
    ] {
        let output = fixture.command(label, initialize, true).await;
        assert_success(
            &output,
            if initialize {
                "initialization"
            } else {
                "verification"
            },
        );
        assert_eq!(fixture.snapshot().await, installed);
        assert_eq!(application().await, original_application);
    }
    for (label, initialize) in [
        ("fresh-keyword-init", true),
        ("fresh-keyword-verify", false),
    ] {
        let output = fixture.keyword_command(label, initialize).await;
        assert_success(
            &output,
            if initialize {
                "initialization"
            } else {
                "verification"
            },
        );
        assert_eq!(fixture.snapshot().await, installed);
        assert_eq!(application().await, original_application);
    }
    let helpers = fixture.observer.client.query_one(
        "SELECT darmok.substring_utf8('aé🙂z', -2::pg_catalog.int8), darmok.substring_bytes(pg_catalog.decode('4100ff42', 'hex'), 2::pg_catalog.int8, 2::pg_catalog.int8), darmok.substring_utf8(NULL::pg_catalog.text, 1::pg_catalog.int8), darmok.substring_utf8('abc', 0::pg_catalog.int8)", &[]
    ).await.unwrap();
    assert_eq!(helpers.get::<_, String>(0), "🙂z");
    assert_eq!(helpers.get::<_, Vec<u8>>(1), [0, 255]);
    assert_eq!(helpers.get::<_, Option<String>>(2), None);
    assert_eq!(helpers.get::<_, String>(3), "");
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 CLI process CI"]
async fn partial_and_changed_installations_fail_without_repair() {
    let fixture = Fixture::new().await;
    fixture
        .observer
        .client
        .batch_execute("CREATE SCHEMA darmok")
        .await
        .unwrap();
    for (state, setup, reason) in [
        (
            "partial-empty",
            "",
            "Darmok installation table definition does not match",
        ),
        (
            "partial-view",
            "CREATE VIEW darmok.installation AS SELECT true AS singleton",
            "Darmok installation table definition does not match",
        ),
    ] {
        fixture.observer.client.batch_execute(setup).await.unwrap();
        let before = fixture.snapshot().await;
        for initialize in [true, false] {
            let label = format!("{state}-{}", if initialize { "init" } else { "verify" });
            let output = fixture.command(&label, initialize, false).await;
            assert_database_error(
                &output,
                if initialize {
                    "initialization"
                } else {
                    "verification"
                },
                reason,
            );
            assert_eq!(fixture.snapshot().await, before);
        }
    }
    fixture
        .observer
        .client
        .batch_execute("DROP VIEW darmok.installation; DROP SCHEMA darmok")
        .await
        .unwrap();
    assert_success(
        &fixture.command("drift-baseline-init", true, false).await,
        "initialization",
    );
    for (state, setup, reason) in [
        (
            "drift-version",
            "UPDATE darmok.installation SET format_version = 2",
            "Darmok installation version or metadata does not match",
        ),
        (
            "drift-function",
            "UPDATE darmok.installation SET format_version = 1; CREATE OR REPLACE FUNCTION darmok.substring_utf8(pg_catalog.text, pg_catalog.int8) RETURNS pg_catalog.text LANGUAGE SQL IMMUTABLE STRICT PARALLEL SAFE AS 'SELECT $1'",
            "Darmok compatibility function definition does not match",
        ),
    ] {
        fixture.observer.client.batch_execute(setup).await.unwrap();
        let before = fixture.snapshot().await;
        for initialize in [true, false] {
            let label = format!("{state}-{}", if initialize { "init" } else { "verify" });
            let output = fixture.command(&label, initialize, false).await;
            assert_database_error(
                &output,
                if initialize {
                    "initialization"
                } else {
                    "verification"
                },
                reason,
            );
            assert_eq!(fixture.snapshot().await, before);
        }
    }
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 CLI process CI"]
async fn explicit_physical_databases_have_independent_installations() {
    let first = Fixture::new().await;
    let second = Fixture::new().await;
    let missing_database = format!("{}_missing", first.database);
    let settings = format!("{}&dbname={missing_database}", first.settings);
    assert_eq!(
        settings.parse::<Config>().unwrap().get_dbname(),
        Some(missing_database.as_str())
    );
    let unavailable = tokio::task::spawn_blocking(move || {
        invoke(
            "databases-nonexistent",
            &["init", "--database-url-env", SETTINGS_ENV],
            Some(&settings),
        )
    })
    .await
    .unwrap();
    assert_eq!(unavailable.status.code(), Some(1), "{unavailable:?}");
    assert!(unavailable.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&unavailable.stderr),
        "darmok: could not establish the PostgreSQL connection (SQLSTATE 3D000)\n"
    );
    let remaining: i64 = first
        .admin
        .client
        .query_one(
            "SELECT pg_catalog.count(*) FROM pg_catalog.pg_database WHERE datname = $1",
            &[&missing_database],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(remaining, 0);
    let second_absent = second.snapshot().await;
    assert_success(
        &first.command("databases-first-init", true, false).await,
        "initialization",
    );
    assert_success(
        &first.command("databases-first-verify", false, false).await,
        "verification",
    );
    assert_database_error(
        &second
            .command("databases-second-missing", false, false)
            .await,
        "verification",
        "Darmok schema is missing; explicit initialization is required",
    );
    assert_eq!(second.snapshot().await, second_absent);
    assert_success(
        &second.command("databases-second-init", true, false).await,
        "initialization",
    );
    let second_installed = second.snapshot().await;
    first
        .observer
        .client
        .batch_execute("UPDATE darmok.installation SET format_version = 2")
        .await
        .unwrap();
    assert_database_error(
        &first.command("databases-first-drift", false, false).await,
        "verification",
        "Darmok installation version or metadata does not match",
    );
    assert_success(
        &second.command("databases-second-verify", false, true).await,
        "verification",
    );
    assert_eq!(second.snapshot().await, second_installed);
    first.close().await;
    second.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 CLI process CI"]
async fn extension_nonmember_dependencies_and_conflicts_have_real_process_outcomes() {
    let fixture = Fixture::new().await;
    fixture
        .observer
        .client
        .batch_execute("CREATE SCHEMA darmok_server")
        .await
        .unwrap();
    let before = fixture.snapshot().await;
    assert_database_error(
        &fixture.command("extension-partial-init", true, false).await,
        "initialization",
        "Darmok server namespace exists without its extension",
    );
    assert_eq!(fixture.snapshot().await, before);
    fixture
        .observer
        .client
        .batch_execute("DROP SCHEMA darmok_server")
        .await
        .unwrap();
    assert_success(
        &fixture
            .command("extension-baseline-init", true, false)
            .await,
        "initialization",
    );
    fixture.observer.client.batch_execute("CREATE FUNCTION public.application_helper() RETURNS pg_catalog.int4 LANGUAGE sql AS 'SELECT 7'; ALTER FUNCTION public.application_helper() DEPENDS ON EXTENSION darmok_server").await.unwrap();
    let installed = fixture.snapshot().await;
    for (label, initialize) in [
        ("extension-nonmember-init", true),
        ("extension-nonmember-verify", false),
    ] {
        let output = fixture.command(label, initialize, false).await;
        assert_success(
            &output,
            if initialize {
                "initialization"
            } else {
                "verification"
            },
        );
        assert_eq!(fixture.snapshot().await, installed);
    }
    for (state, mutation, undo, reason) in [
        (
            "member",
            "ALTER EXTENSION darmok_server ADD FUNCTION public.application_helper()",
            "ALTER EXTENSION darmok_server DROP FUNCTION public.application_helper()",
            "Darmok server extension contains unexpected members",
        ),
        (
            "namespace",
            "CREATE COLLATION darmok_server.extra FROM pg_catalog.\"C\"",
            "DROP COLLATION darmok_server.extra",
            "Darmok server namespace contains unexpected objects",
        ),
    ] {
        fixture
            .observer
            .client
            .batch_execute(mutation)
            .await
            .unwrap();
        let before = fixture.snapshot().await;
        for initialize in [true, false] {
            let label = format!(
                "extension-{state}-{}",
                if initialize { "init" } else { "verify" }
            );
            let output = fixture.command(&label, initialize, false).await;
            assert_database_error(
                &output,
                if initialize {
                    "initialization"
                } else {
                    "verification"
                },
                reason,
            );
            assert_eq!(fixture.snapshot().await, before);
        }
        fixture.observer.client.batch_execute(undo).await.unwrap();
    }
    assert_success(
        &fixture
            .command("extension-restored-verify", false, false)
            .await,
        "verification",
    );
    fixture.close().await;
}
