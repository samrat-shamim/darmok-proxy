//! Explicit physical-database setup commands using a dedicated setup connection owner.
use std::{fmt, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use darmok_execute::{
    NativeDatabaseAction, NativeDatabaseCompletion, NativeDatabaseError, NativeDatabaseFailure,
    NativeDatabaseSetup, NativeDatabaseSetupDisposeError, NativeDatabaseSetupError,
};
use tokio_postgres::{Config, NoTls};

#[derive(Parser)]
#[command(
    name = "darmok",
    version,
    about = "Darmok PostgreSQL database setup tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install or validate both Darmok components in one existing database.
    Init(DatabaseInput),
    /// Verify both components and the live native handler without object creation.
    Verify(DatabaseInput),
}

#[derive(Args)]
struct DatabaseInput {
    /// Environment variable containing PostgreSQL connection settings, with an explicit database.
    #[arg(long, value_name = "ENV_NAME", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    database_url_env: String,
}

enum CommandFailure {
    MissingEnvironment(String),
    NonUnicodeEnvironment(String),
    InvalidSettings(String),
    MissingDatabase(String),
    Connection(NativeDatabaseSetupError),
    Database {
        action: NativeDatabaseAction,
        original: Box<NativeDatabaseFailure>,
        disposal: Option<NativeDatabaseSetupDisposeError>,
    },
    DisposalAfterSuccess {
        action: NativeDatabaseAction,
        completion: NativeDatabaseCompletion,
        original: NativeDatabaseSetupDisposeError,
    },
}

impl fmt::Display for CommandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnvironment(name) => write!(f, "environment variable {name} is not set"),
            Self::NonUnicodeEnvironment(name) => {
                write!(f, "environment variable {name} is not valid UTF8")
            }
            Self::InvalidSettings(name) => write!(
                f,
                "invalid PostgreSQL connection settings in environment variable {name}"
            ),
            Self::MissingDatabase(name) => write!(
                f,
                "PostgreSQL connection settings in {name} must select an explicit database"
            ),
            Self::Connection(original) => {
                f.write_str("could not establish the PostgreSQL connection")?;
                if let NativeDatabaseSetupError::Connect(error) = original
                    && let Some(code) = error.code()
                {
                    write!(f, " (SQLSTATE {})", code.code())?;
                }
                Ok(())
            }
            Self::Database {
                action,
                original,
                disposal,
            } => {
                write!(
                    f,
                    "{action} failed: {}",
                    DatabaseDiagnostic(original.original())
                )?;
                if let Some(cleanup) = original.cleanup() {
                    match cleanup {
                        Ok(_) => f.write_str("; rollback confirmed")?,
                        Err(error) => write!(f, "; rollback failed: {}", SetupDiagnostic(error))?,
                    }
                }
                if let Some(error) = disposal {
                    write!(f, "; local driver disposal failed: {error}")?;
                }
                Ok(())
            }
            Self::DisposalAfterSuccess {
                action,
                completion,
                original,
            } => write!(
                f,
                "{action} completed for compatibility format version {} and server extension {}, but local driver disposal failed: {original}",
                completion.schema_version(),
                completion.server_extension_version()
            ),
        }
    }
}

struct DatabaseDiagnostic<'a>(&'a NativeDatabaseError);

impl fmt::Display for DatabaseDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(database) = self
            .0
            .backend_error()
            .and_then(tokio_postgres::Error::as_db_error)
        {
            return write!(
                f,
                "SQLSTATE {}: {}",
                database.code().code(),
                database.message()
            );
        }
        write!(f, "{}", self.0)
    }
}

struct SetupDiagnostic<'a>(&'a NativeDatabaseSetupError);

impl fmt::Display for SetupDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let NativeDatabaseSetupError::Control(control) = self.0
            && let Some(error) = control.backend_error()
            && let Some(database) = error.as_db_error()
        {
            return write!(
                f,
                "SQLSTATE {}: {}",
                database.code().code(),
                database.message()
            );
        }
        write!(f, "{}", self.0)
    }
}

fn configuration(input: DatabaseInput) -> Result<Config, CommandFailure> {
    let name = input.database_url_env;
    let value = match std::env::var(&name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => {
            return Err(CommandFailure::MissingEnvironment(name));
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(CommandFailure::NonUnicodeEnvironment(name));
        }
    };
    let config: Config = value
        .parse()
        .map_err(|_| CommandFailure::InvalidSettings(name.clone()))?;
    if config.get_dbname().is_none_or(str::is_empty) {
        return Err(CommandFailure::MissingDatabase(name));
    }
    Ok(config)
}

async fn run(command: Command) -> Result<(), CommandFailure> {
    let (action, input) = match command {
        Command::Init(input) => (NativeDatabaseAction::Initialize, input),
        Command::Verify(input) => (NativeDatabaseAction::Verify, input),
    };
    let config = configuration(input)?;
    let mut setup = NativeDatabaseSetup::connect(&config, NoTls)
        .await
        .map_err(CommandFailure::Connection)?;
    let result = match action {
        NativeDatabaseAction::Initialize => setup.initialize_database().await,
        NativeDatabaseAction::Verify => setup.verify_database().await,
    };
    let disposal = setup.dispose().await;
    match (result, disposal) {
        (Ok(completion), Ok(_)) => {
            println!(
                "Darmok {action} completed for compatibility format version {} and server extension {}",
                completion.schema_version(),
                completion.server_extension_version()
            );
            Ok(())
        }
        (Ok(completion), Err(original)) => Err(CommandFailure::DisposalAfterSuccess {
            action,
            completion,
            original,
        }),
        (Err(original), disposal) => Err(CommandFailure::Database {
            action,
            original: Box::new(original),
            disposal: disposal.err(),
        }),
    }
}

fn main() -> ExitCode {
    // clap handles help/version/usage before a runtime or connection is started.
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("darmok: cannot start local runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli.command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("darmok: {error}");
            ExitCode::FAILURE
        }
    }
}
