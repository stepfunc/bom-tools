use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    PrintLog {
        #[clap(value_parser)]
        build_log: std::path::PathBuf,
    },
}

/// read cargo log files for dependency information
pub(crate) mod log;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::PrintLog { build_log } => {
            let packages = log::read_packages(&build_log)?;

            for (id, usage) in packages {
                println!("{} {}", id, usage.versions)
            }
        }
    }

    Ok(())
}
