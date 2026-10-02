//! Explicit physical-database schema commands using the existing native owner.
use std::{fmt, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use darmok_execute::{
    NativeBackend, NativeBackendDisposeError, NativeBackendError, NativeSchemaFailure,
};
use tokio_postgres::{Config, NoTls};

#[derive(Parser)]
#[command(
    name = "darmok",
    version,
    about = "Darmok PostgreSQL compatibility schema tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install or verify an exact existing compatibility schema in one database.
    Init(DatabaseInput),
    /// Inspect a physical database's compatibility schema.
    #[command(subcommand)]
    Schema(SchemaCommand),
}

#[derive(Subcommand)]
enum SchemaCommand {
    /// Verify an existing installation without creating or repairing objects.
    Verify(DatabaseInput),
}

#[derive(Args)]
struct DatabaseInput {
    /// Environment variable containing PostgreSQL connection settings, with an explicit database.
    #[arg(long, value_name = "ENV_NAME", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    database_url_env: String,
}

#[derive(Clone, Copy)]
enum SchemaAction {
    Initialize,
    Verify,
}

impl fmt::Display for SchemaAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Initialize => "schema initialization",
            Self::Verify => "schema verification",
        })
    }
}

enum CommandFailure {
    MissingEnvironment(String),
    NonUnicodeEnvironment(String),
    InvalidSettings(String),
    MissingDatabase(String),
    Connection(NativeBackendError),
    Schema {
        action: SchemaAction,
        original: Box<NativeSchemaFailure>,
        disposal: Option<NativeBackendDisposeError>,
    },
    DisposalAfterSuccess {
        action: SchemaAction,
        version: i32,
        original: NativeBackendDisposeError,
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
                if let NativeBackendError::Connect(error) = original
                    && let Some(code) = error.code()
                {
                    write!(f, " (SQLSTATE {})", code.code())?;
                }
                Ok(())
            }
            Self::Schema {
                action,
                original,
                disposal,
            } => {
                write!(
                    f,
                    "{action} failed: {}",
                    BackendDiagnostic(original.original())
                )?;
                if let Some(cleanup) = original.cleanup() {
                    match cleanup {
                        Ok(_) => f.write_str("; rollback confirmed")?,
                        Err(error) => write!(f, "; rollback failed: {}", BackendDiagnostic(error))?,
                    }
                }
                if let Some(error) = disposal {
                    write!(f, "; local driver disposal failed: {error}")?;
                }
                Ok(())
            }
            Self::DisposalAfterSuccess {
                action,
                version,
                original,
            } => write!(
                f,
                "{action} completed for format version {version}, but local driver disposal failed: {original}"
            ),
        }
    }
}

struct BackendDiagnostic<'a>(&'a NativeBackendError);

impl fmt::Display for BackendDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let NativeBackendError::Control(control) = self.0
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
        Command::Init(input) => (SchemaAction::Initialize, input),
        Command::Schema(SchemaCommand::Verify(input)) => (SchemaAction::Verify, input),
    };
    let config = configuration(input)?;
    let mut backend = NativeBackend::connect(&config, NoTls)
        .await
        .map_err(CommandFailure::Connection)?;
    let result = match action {
        SchemaAction::Initialize => backend.initialize_schema().await,
        SchemaAction::Verify => backend.verify_schema().await,
    };
    let disposal = backend.dispose().await;
    match (result, disposal) {
        (Ok(completion), Ok(_)) => {
            println!(
                "Darmok {action} completed for format version {}",
                completion.version()
            );
            Ok(())
        }
        (Ok(completion), Err(original)) => Err(CommandFailure::DisposalAfterSuccess {
            action,
            version: completion.version(),
            original,
        }),
        (Err(original), disposal) => Err(CommandFailure::Schema {
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
