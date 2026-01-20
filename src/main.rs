mod commands;
mod utils;

use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(name = "dpetoolbox")]
#[command(author = "diesteve")]
#[command(version = "0.1.0")]
#[command(about = "DPE Network Analysis Toolbox", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download files from a list of URLs (uses azcopy for multi-threaded downloads)
    Download {
        /// Path to the TXT file containing URLs (one per line)
        #[arg(short, long)]
        file: String,

        /// Output directory for downloaded files (default: same as input file directory)
        #[arg(short, long)]
        output: Option<String>,

        /// Number of parallel downloads (default: 4)
        #[arg(short, long, default_value = "4")]
        threads: u32,
    },
}

fn show_banner() {
    println!();
    println!("{}", "    _____  _____  ______   _______          _ _                 ".cyan());
    println!("{}", "   |  __ \\|  __ \\|  ____| |__   __|        | | |                ".cyan());
    println!("{}", "   | |  | | |__) | |__       | | ___   ___ | | |__   _____  __  ".cyan());
    println!("{}", "   | |  | |  ___/|  __|      | |/ _ \\ / _ \\| | '_ \\ / _ \\ \\/ /  ".cyan());
    println!("{}", "   | |__| | |    | |____     | | (_) | (_) | | |_) | (_) >  <   ".cyan());
    println!("{}", "   |_____/|_|    |______|    |_|\\___/ \\___/|_|_.__/ \\___/_/\\_\\  ".cyan());
    println!();
    println!("{}", "          by diesteve                                          ".magenta());
    println!();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    show_banner();
    
    let cli = Cli::parse();

    match cli.command {
        Commands::Download { file, output, threads } => {
            commands::download::run(&file, output.as_deref(), threads).await?;
        }
    }

    Ok(())
}

