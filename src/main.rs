mod commands;
mod utils;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::{Input, Select, theme::ColorfulTheme};

#[derive(Parser)]
#[command(name = "dpetoolbox")]
#[command(author = "diesteve")]
#[command(version = "0.1.0")]
#[command(about = "DPE Network Analysis Toolbox", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
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

/// Interactive menu options
const MENU_OPTIONS: &[&str] = &[
    "Download files from URL list",
    "Exit",
];

/// Run interactive menu mode
async fn interactive_mode() -> Result<()> {
    loop {
        println!("{}", "Select an option:".white().bold());
        println!();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(MENU_OPTIONS)
            .default(0)
            .interact()?;

        println!();

        match selection {
            0 => {
                // Download files
                if let Err(e) = interactive_download().await {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            1 => {
                // Exit
                println!("{}", "Goodbye!".cyan());
                break;
            }
            _ => unreachable!(),
        }

        println!();
    }

    Ok(())
}

/// Interactive download prompts
async fn interactive_download() -> Result<()> {
    let theme = ColorfulTheme::default();

    // Prompt for file path
    let file: String = Input::with_theme(&theme)
        .with_prompt("Path to TXT file containing URLs")
        .interact_text()?;

    // Validate file exists
    if !std::path::Path::new(&file).exists() {
        anyhow::bail!("File not found: {}", file);
    }

    // Prompt for output directory
    let default_output = std::path::Path::new(&file)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let output: String = Input::with_theme(&theme)
        .with_prompt("Output directory")
        .default(default_output)
        .interact_text()?;

    // Prompt for threads
    let threads: u32 = Input::with_theme(&theme)
        .with_prompt("Number of parallel downloads")
        .default(4)
        .interact_text()?;

    println!();

    // Run the download
    let output_opt = if output.is_empty() { None } else { Some(output.as_str()) };
    commands::download::run(&file, output_opt, threads).await
}

#[tokio::main]
async fn main() -> Result<()> {
    show_banner();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Download { file, output, threads }) => {
            commands::download::run(&file, output.as_deref(), threads).await?;
        }
        None => {
            // No subcommand provided - run interactive mode
            interactive_mode().await?;
        }
    }

    Ok(())
}
