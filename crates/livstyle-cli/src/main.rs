mod auth;
mod esp;
mod iterate;
mod judge_eval;
mod octos_probe;
mod smoke;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "livstyle", version, about = "livstyle headless commands")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the Cursor API key. The key is read from an env file, never from argv.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Cursor SDK Bridge checks.
    Cursor {
        #[command(subcommand)]
        command: CursorCommand,
    },
    /// System One judge.
    Judge {
        #[command(subcommand)]
        command: JudgeCommand,
    },
    /// Chip and board profiles.
    Profiles {
        #[arg(long, default_value = "profiles")]
        dir: PathBuf,
    },
    /// Detected ESP-IDF installs and per-chip support.
    Toolchain {
        #[arg(long, default_value = "profiles")]
        profiles: PathBuf,
    },
    /// USB serial devices (read from sysfs; ports are not opened).
    Devices {
        #[arg(long, default_value = "profiles")]
        profiles: PathBuf,
    },
    /// ESP32 inner loop: USB once, then LAN container replace (Toit/Jaguar).
    Iterate {
        /// app, container, base, realtime, driver, bringup, sdkconfig
        #[arg(long, default_value = "app")]
        change: String,
        /// ready or none
        #[arg(long, default_value = "ready")]
        base: String,
    },
    /// Octos UI protocol.
    Octos {
        #[command(subcommand)]
        command: OctosCommand,
    },
    /// Read chip type and revision with esptool. Resets the board.
    ChipInfo {
        /// Port (/dev/ttyACM0) or device id from `livstyle devices`.
        device: String,
        /// Skip the reset confirmation.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Show where to write CURSOR_API_KEY.
    Cursor,
    /// Show which credentials are configured (fingerprints only).
    Status,
    /// Remove the stored Cursor API key.
    Logout,
}

#[derive(Subcommand)]
enum CursorCommand {
    /// Spike 1: spawn the bridge, create an agent, stream, cancel, replay.
    Smoke {
        /// Model id; defaults to the first model the account offers.
        #[arg(long)]
        model: Option<String>,
        /// Only run the checks that need no API key.
        #[arg(long)]
        offline: bool,
    },
}

#[derive(Subcommand)]
enum OctosCommand {
    /// Spike 2: hello, capabilities, and whether a session can be opened.
    Probe {
        #[arg(long, default_value = "ws://127.0.0.1:50080/api/ui-protocol/ws")]
        url: String,
        #[arg(long, default_value = "123456")]
        token: String,
    },
}

#[derive(Subcommand)]
enum JudgeCommand {
    /// Spike 4: run the labelled intervention set through a judge.
    Eval(judge_eval::EvalArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Auth { command } => match command {
            AuthCommand::Cursor => auth::login(),
            AuthCommand::Status => auth::status(),
            AuthCommand::Logout => auth::logout(),
        },
        Command::Cursor {
            command: CursorCommand::Smoke { model, offline },
        } => smoke::run(model.as_deref(), offline),
        Command::Judge {
            command: JudgeCommand::Eval(args),
        } => judge_eval::run(&args),
        Command::Profiles { dir } => profiles(&dir),
        Command::Toolchain { profiles } => esp::toolchain(&profiles),
        Command::Devices { profiles } => esp::devices(&profiles),
        Command::Iterate { change, base } => iterate::run(&change, &base),
        Command::Octos {
            command: OctosCommand::Probe { url, token },
        } => octos_probe::probe(&url, &token),
        Command::ChipInfo { device, yes } => esp::chip_info(&device, yes),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn profiles(dir: &std::path::Path) -> Result<(), String> {
    let reg = livstyle_profiles::Registry::load_dir(dir).map_err(|e| e.to_string())?;
    println!("{:<16} {:<8} {:<14} min IDF", "chip", "arch", "tier");
    for chip in reg.chips() {
        println!(
            "{:<16} {:<8} {:<14} {}",
            chip.chip,
            format!("{:?}", chip.arch).to_lowercase(),
            format!("{:?}", chip.support_tier).to_lowercase(),
            chip.idf.min_version
        );
    }
    println!();
    println!("{:<30} {:<16} tier", "board", "chip");
    for board in reg.boards() {
        println!(
            "{:<30} {:<16} {}",
            board.id,
            board.chip,
            format!("{:?}", reg.effective_tier(&board.id)).to_lowercase()
        );
    }
    Ok(())
}
