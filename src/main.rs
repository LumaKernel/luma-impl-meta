use clap::{Parser, Subcommand, ValueEnum};
use luma_impl_meta::{fix::fix, watch_fix::watch_fix};

#[derive(Parser)]
#[command()]
#[group()]
struct Cli {
    #[arg(value_enum, default_value = "info")]
    log_level: LogLevel,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, ValueEnum)]
#[value()]
enum LogLevel {
    #[value()]
    Off,
    #[value()]
    Error,
    #[value()]
    Info,
    #[value()]
    Debug,
    #[value()]
    Trace,
}
impl From<LogLevel> for log::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Off => log::LevelFilter::Off,
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

#[derive(Subcommand)]
#[command()]
enum Command {
    #[command()]
    Fix {
        #[arg(short, long, default_value = ".")]
        root_dir: String,
    },

    #[command()]
    WatchFix {
        #[arg(short, long, default_value = ".")]
        root_dir: String,
    },
}

fn main() {
    let mut builder = env_logger::Builder::from_env("LUMA_LIB_RS_META_LOG");
    let cli = Cli::parse();
    let log_level = cli.log_level.into();
    log::set_max_level(log_level);
    builder.filter_level(log_level);
    builder.init();
    match cli.command {
        Command::Fix { root_dir } => {
            log::info!("fixing...");
            match fix(&root_dir) {
                Ok(_) => log::info!("done"),
                Err(err) => {
                    log::error!("error: {}", err.join("\n"));
                }
            }
        }
        Command::WatchFix { root_dir } => {
            log::info!("watching...");
            match watch_fix(&root_dir) {
                Ok(_) => log::info!("done"),
                Err(err) => {
                    log::error!("error: {:?}", err);
                }
            }
        }
    }
}
