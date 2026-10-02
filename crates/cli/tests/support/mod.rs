use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Output},
    time::SystemTime,
};

pub const SETTINGS_ENV: &str = "DARMOK_CLI_PROCESS_DATABASE_URL";

pub fn invoke(label: &str, arguments: &[&str], settings: Option<&str>) -> Output {
    let binary = env!("CARGO_BIN_EXE_darmok");
    let mut command = Command::new(binary);
    command.args(arguments).env("NO_COLOR", "1");
    command.env_remove(SETTINGS_ENV);
    if let Some(settings) = settings {
        command.env(SETTINGS_ENV, settings);
    }
    let started = SystemTime::now();
    let output = command.output().expect("the actual darmok binary must run");
    let finished = SystemTime::now();
    // Evidence is optional; the functional tests never depend on its presence.
    // When requested, every child keeps its actual status and unmodified bytes.
    if let Some(root) = std::env::var_os("DARMOK_CLI_PROCESS_EVIDENCE_DIR") {
        let directory = PathBuf::from(root).join(label);
        fs::create_dir_all(&directory).unwrap();
        for (name, data) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(name))
                .unwrap()
                .write_all(data)
                .unwrap();
        }
        let selected_database = settings
            .and_then(|value| value.parse::<tokio_postgres::Config>().ok())
            .and_then(|config| config.get_dbname().map(str::to_owned));
        let receipt = serde_json::json!({
            "binary": binary,
            "arguments": arguments,
            "settings_environment": SETTINGS_ENV,
            "settings_provided": settings.is_some(),
            "selected_database": selected_database,
            "started_unix_ns": started.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos().to_string(),
            "finished_unix_ns": finished.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos().to_string(),
            "exit_code": output.status.code(),
            "success": output.status.success(),
        });
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("receipt.json"))
            .unwrap();
        serde_json::to_writer_pretty(&mut file, &receipt).unwrap();
        writeln!(file).unwrap();
    }
    output
}
