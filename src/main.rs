use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// path to Cargo.toml
    #[clap(short, long, value_parser)]
    build_log: std::path::PathBuf,
}

/// read cargo log files for dependency information
pub(crate) mod log;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let packages = log::read_packages(&args.build_log)?;

    for (id, usage) in packages {
        println!("{} {}", id, usage.versions)
    }

    Ok(())
}
