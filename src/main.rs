mod commands;
mod utils;
mod web;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use colored::{Colorize, control::set_virtual_terminal};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
use std::io;

#[derive(Parser)]
#[command(name = "dpetoolbox")]
#[command(author = "Diogo Esteves")]
#[command(version = "2.0.2")]
#[command(about = "DPE Network Analysis Toolbox", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Generate shell completions
    #[arg(long, value_name = "SHELL")]
    completions: Option<Shell>,

    /// Launch interactive CLI mode instead of Web UI
    #[arg(long)]
    cli: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Download files from a list of URLs (uses azcopy for multi-threaded downloads)
    #[command(after_help = "EXAMPLES:
    dpetoolbox download -f urls.txt
    dpetoolbox download -f urls.txt -o ./downloads -t 8
    dpetoolbox download --clipboard")]
    Download {
        /// Path to the TXT file containing URLs (one per line)
        #[arg(short, long, required_unless_present = "clipboard")]
        file: Option<String>,

        /// Read URLs from clipboard instead of file
        #[arg(long)]
        clipboard: bool,

        /// Output directory for downloaded files (default: parent/<filename>)
        #[arg(short, long)]
        output: Option<String>,

        /// Number of parallel downloads (default: 4)
        #[arg(short, long, default_value = "4")]
        threads: u32,
    },
    /// Merge PCAP files (by IP address if detected, otherwise all into one; requires Wireshark/mergecap)
    #[command(after_help = "EXAMPLES:
    dpetoolbox merge -i ./pcaps
    dpetoolbox merge -i ./pcaps -o ./merged

If filenames end with _X.X.X.X.pcap, files are merged per IP.
Otherwise, all PCAP files are merged into a single merged.pcap.")]
    Merge {
        /// Directory containing PCAP files to merge
        #[arg(short, long)]
        input: String,

        /// Output directory for merged files (default: same as input)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Filter PCAP files using Wireshark display filter (requires Wireshark/tshark)
    #[command(after_help = "EXAMPLES:
    dpetoolbox filter -i ./pcaps -F \"ip.src == 10.0.0.1\"
    dpetoolbox filter -f capture.pcap -F \"tcp.port == 443\"
    dpetoolbox filter -i ./pcaps -F \"tcp.port == 443\" -d
    dpetoolbox filter -i ./pcaps -o ./filtered -F \"http\"")]
    Filter {
        /// Directory containing PCAP files to filter
        #[arg(short, long, required_unless_present = "file")]
        input: Option<String>,

        /// Single PCAP file to filter
        #[arg(short, long, required_unless_present = "input")]
        file: Option<String>,

        /// Output directory for filtered files (default: same as input)
        #[arg(short, long)]
        output: Option<String>,

        /// Wireshark display filter (e.g., 'ip.src == 1.2.3.4')
        #[arg(short = 'F', long)]
        filter: String,

        /// Delete empty files (files with 0 matching packets)
        #[arg(short, long, default_value = "false")]
        delete_empty: bool,
    },
    /// Convert ETL files to PCAP format (uses etl2pcapng, auto-downloads if needed)
    #[command(after_help = "EXAMPLES:
    dpetoolbox convert -i ./etls
    dpetoolbox convert -i ./etls -o ./pcaps")]
    Convert {
        /// Directory containing ETL files to convert
        #[arg(short, long)]
        input: String,

        /// Output directory for PCAP files (default: same as input)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// TCP ping - test TCP connectivity to a host:port
    #[command(after_help = "EXAMPLES:
    dpetoolbox tcpping -t google.com -p 443
    dpetoolbox tcpping -t 10.0.0.1 -p 22 --timeout 5000
    dpetoolbox tcpping -t myserver.local -p 80 --interval 2")]
    Tcpping {
        /// Target hostname or IP address
        #[arg(short, long)]
        target: String,

        /// Target port
        #[arg(short, long)]
        port: u16,

        /// Connection timeout in milliseconds (default: 2000)
        #[arg(long, default_value = "2000")]
        timeout: u64,

        /// Interval between pings in seconds (default: 1)
        #[arg(long, default_value = "1")]
        interval: u64,
    },
    /// Start the web UI server
    #[command(after_help = "EXAMPLES:
    dpetoolbox serve
    dpetoolbox serve --port 8080")]
    Serve {
        /// Port to listen on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Show PCAP file summary and statistics (requires Wireshark)
    #[command(after_help = "EXAMPLES:
    dpetoolbox summary -f capture.pcap
    dpetoolbox summary -i ./pcaps")]
    Summary {
        /// Single PCAP file to summarize
        #[arg(short, long, required_unless_present = "input")]
        file: Option<String>,

        /// Directory containing PCAP files to summarize
        #[arg(short, long, required_unless_present = "file")]
        input: Option<String>,
    },
    /// List and export PCAP conversations/flows (requires Wireshark)
    #[command(after_help = "EXAMPLES:
    dpetoolbox conversations -f capture.pcap
    dpetoolbox conversations -f capture.pcap --export 1
    dpetoolbox conversations -f capture.pcap --export 3 -o ./flows")]
    Conversations {
        /// PCAP file to analyze
        #[arg(short, long)]
        file: String,

        /// Export conversation by index number (shown in listing)
        #[arg(short, long)]
        export: Option<usize>,

        /// Output directory for exported flow (default: same as input)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Show top talkers (endpoints by traffic) in a PCAP file (requires Wireshark)
    #[command(after_help = "EXAMPLES:
    dpetoolbox toptalkers -f capture.pcap
    dpetoolbox toptalkers -f capture.pcap -n 20")]
    Toptalkers {
        /// PCAP file to analyze
        #[arg(short, long)]
        file: String,

        /// Number of top talkers to show (default: 50)
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,
    },
    /// IPv4 subnet calculator
    #[command(after_help = "EXAMPLES:
    dpetoolbox subnet 192.168.1.0/24
    dpetoolbox subnet 10.0.0.0/8")]
    Subnet {
        /// IP address in CIDR notation (e.g., 192.168.1.0/24)
        cidr: String,
    },
}

fn show_banner() {
    println!();
    println!("{}", r"   _____  _____  ______   _______          _ _".cyan());
    println!("{}", r"  |  __ \|  __ \|  ____| |__   __|        | | |".cyan());
    println!("{}", r"  | |  | | |__) | |__       | | ___   ___ | | |__   _____  __".cyan());
    println!("{}", r"  | |  | |  ___/|  __|      | |/ _ \ / _ \| | '_ \ / _ \ \/ /".cyan());
    println!("{}", r"  | |__| | |    | |____     | | (_) | (_) | | |_) | (_) >  <".cyan());
    println!("{}", r"  |_____/|_|    |______|    |_|\___/ \___/|_|_.__/ \___/_/\_\".cyan());
    println!();
    println!("{}", "         by Diogo Esteves".magenta());
    println!();
}

/// Interactive menu options
const MENU_OPTIONS: &[&str] = &[
    "Download files from URL list",
    "Merge PCAP files",
    "Filter PCAP files",
    "Convert ETL to PCAP",
    "PCAP Summary",
    "PCAP Conversations",
    "PCAP Top Talkers",
    "IPv4 Subnet Calculator",
    "TCP Ping",
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
                // Merge PCAP files
                if let Err(e) = interactive_merge() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            2 => {
                // Filter PCAP files
                if let Err(e) = interactive_filter() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            3 => {
                // Convert ETL to PCAP
                if let Err(e) = interactive_convert().await {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            4 => {
                // PCAP Summary
                if let Err(e) = interactive_summary() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            5 => {
                // PCAP Conversations
                if let Err(e) = interactive_conversations() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            6 => {
                // PCAP Top Talkers
                if let Err(e) = interactive_toptalkers() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            7 => {
                // IPv4 Subnet Calculator
                if let Err(e) = interactive_subnet() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            8 => {
                // TCP Ping
                if let Err(e) = interactive_tcpping() {
                    println!("{} {}", "Error:".red().bold(), e);
                }
            }
            9 => {
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

    // Prompt for output directory - default to parent/filename_stem
    let file_path = std::path::Path::new(&file);
    let parent = file_path.parent().unwrap_or(std::path::Path::new("."));
    let stem = file_path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "downloads".to_string());
    let default_output = parent.join(&stem).to_string_lossy().to_string();

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

/// Interactive merge prompts
fn interactive_merge() -> Result<()> {
    let theme = ColorfulTheme::default();

    // Prompt for source directory
    let input: String = Input::with_theme(&theme)
        .with_prompt("Directory containing PCAP files")
        .interact_text()?;

    // Validate directory exists
    if !std::path::Path::new(&input).exists() {
        anyhow::bail!("Directory not found: {}", input);
    }

    // Prompt for output directory
    let output: String = Input::with_theme(&theme)
        .with_prompt("Output directory for merged files")
        .default(input.clone())
        .interact_text()?;

    println!();

    // Run the merge
    let output_opt = if output == input { None } else { Some(output.as_str()) };
    commands::merge::run(&input, output_opt)
}

/// Interactive filter prompts
fn interactive_filter() -> Result<()> {
    let theme = ColorfulTheme::default();

    // Ask whether to filter a single file or a directory
    let mode = Select::with_theme(&theme)
        .with_prompt("Filter mode")
        .items(&["Filter a single PCAP file", "Filter all PCAP files in a directory"])
        .default(0)
        .interact()?;

    let (input_dir, single_file) = if mode == 0 {
        let file: String = Input::with_theme(&theme)
            .with_prompt("Path to PCAP file")
            .interact_text()?;
        if !std::path::Path::new(&file).exists() {
            anyhow::bail!("File not found: {}", file);
        }
        (None, Some(file))
    } else {
        let input: String = Input::with_theme(&theme)
            .with_prompt("Directory containing PCAP files")
            .interact_text()?;
        if !std::path::Path::new(&input).exists() {
            anyhow::bail!("Directory not found: {}", input);
        }
        (Some(input), None)
    };

    // Prompt for output directory
    let default_output = if let Some(ref dir) = input_dir {
        dir.clone()
    } else {
        let p = std::path::Path::new(single_file.as_ref().unwrap());
        p.parent().unwrap_or(std::path::Path::new(".")).to_string_lossy().to_string()
    };

    let output: String = Input::with_theme(&theme)
        .with_prompt("Output directory for filtered files")
        .default(default_output.clone())
        .interact_text()?;

    // Prompt for filter
    let filter: String = Input::with_theme(&theme)
        .with_prompt("Wireshark display filter (e.g., 'ip.src == 1.2.3.4')")
        .interact_text()?;

    if filter.is_empty() {
        anyhow::bail!("Filter is required");
    }

    // Prompt for delete empty files
    let delete_empty = Confirm::with_theme(&theme)
        .with_prompt("Delete empty filtered files (0 packets)?")
        .default(false)
        .interact()?;

    println!();

    let output_opt = if output == default_output { None } else { Some(output.as_str()) };

    if let Some(file) = single_file {
        commands::filter::run_single(&file, output_opt, &filter, delete_empty)
    } else {
        commands::filter::run(input_dir.as_ref().unwrap(), output_opt, &filter, delete_empty)
    }
}

/// Interactive convert prompts
async fn interactive_convert() -> Result<()> {
    let theme = ColorfulTheme::default();

    // Prompt for source directory
    let input: String = Input::with_theme(&theme)
        .with_prompt("Directory containing ETL files")
        .interact_text()?;

    // Validate directory exists
    if !std::path::Path::new(&input).exists() {
        anyhow::bail!("Directory not found: {}", input);
    }

    // Prompt for output directory
    let output: String = Input::with_theme(&theme)
        .with_prompt("Output directory for PCAP files")
        .default(input.clone())
        .interact_text()?;

    println!();

    // Run the convert
    let output_opt = if output == input { None } else { Some(output.as_str()) };
    commands::convert::run(&input, output_opt).await
}

/// Interactive tcpping prompts
fn interactive_tcpping() -> Result<()> {
    let theme = ColorfulTheme::default();

    // Prompt for target
    let target: String = Input::with_theme(&theme)
        .with_prompt("Target hostname or IP")
        .interact_text()?;

    if target.is_empty() {
        anyhow::bail!("Target is required");
    }

    // Prompt for port
    let port: u16 = Input::with_theme(&theme)
        .with_prompt("Port")
        .interact_text()?;

    // Prompt for timeout
    let timeout: u64 = Input::with_theme(&theme)
        .with_prompt("Timeout (ms)")
        .default(2000)
        .interact_text()?;

    // Prompt for interval
    let interval: u64 = Input::with_theme(&theme)
        .with_prompt("Interval (seconds)")
        .default(1)
        .interact_text()?;

    println!();

    // Run tcpping
    commands::tcpping::run(&target, port, timeout, interval)
}

/// Interactive summary prompts
fn interactive_summary() -> Result<()> {
    let theme = ColorfulTheme::default();

    let mode = Select::with_theme(&theme)
        .with_prompt("Summarize mode")
        .items(&["Summarize a single PCAP file", "Summarize all PCAP files in a directory"])
        .default(0)
        .interact()?;

    if mode == 0 {
        let file: String = Input::with_theme(&theme)
            .with_prompt("Path to PCAP file")
            .interact_text()?;
        if !std::path::Path::new(&file).exists() {
            anyhow::bail!("File not found: {}", file);
        }
        println!();
        commands::summary::run_single(&file)
    } else {
        let input: String = Input::with_theme(&theme)
            .with_prompt("Directory containing PCAP files")
            .interact_text()?;
        if !std::path::Path::new(&input).exists() {
            anyhow::bail!("Directory not found: {}", input);
        }
        println!();
        commands::summary::run(&input)
    }
}

/// Interactive conversations prompts
fn interactive_conversations() -> Result<()> {
    let theme = ColorfulTheme::default();

    let file: String = Input::with_theme(&theme)
        .with_prompt("Path to PCAP file")
        .interact_text()?;

    if !std::path::Path::new(&file).exists() {
        anyhow::bail!("File not found: {}", file);
    }

    println!();

    let conversations = commands::conversations::run(&file)?;

    if conversations.is_empty() {
        return Ok(());
    }

    let export = Confirm::with_theme(&theme)
        .with_prompt("Export a conversation to a separate PCAP?")
        .default(false)
        .interact()?;

    if export {
        let index: usize = Input::with_theme(&theme)
            .with_prompt(format!("Conversation number to export (1-{})", conversations.len()))
            .interact_text()?;

        let output: String = Input::with_theme(&theme)
            .with_prompt("Output directory")
            .default(
                std::path::Path::new(&file)
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_string_lossy()
                    .to_string(),
            )
            .interact_text()?;

        println!();
        commands::conversations::run_export(&file, index, Some(output.as_str()))?;
    }

    Ok(())
}

/// Interactive top talkers prompts
fn interactive_toptalkers() -> Result<()> {
    let theme = ColorfulTheme::default();

    let file: String = Input::with_theme(&theme)
        .with_prompt("Path to PCAP file")
        .interact_text()?;

    if !std::path::Path::new(&file).exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let limit: usize = Input::with_theme(&theme)
        .with_prompt("Number of top talkers to show")
        .default(50)
        .interact_text()?;

    println!();
    commands::toptalkers::run(&file, limit)
}

/// Interactive subnet calculator prompts
fn interactive_subnet() -> Result<()> {
    let theme = ColorfulTheme::default();

    let cidr: String = Input::with_theme(&theme)
        .with_prompt("IP address in CIDR notation (e.g., 192.168.1.0/24)")
        .interact_text()?;

    println!();
    commands::subnet::run(&cidr)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Enable ANSI color support on Windows
    set_virtual_terminal(true).ok();

    let cli = Cli::parse();

    // Handle shell completion generation (no banner)
    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "dpetoolbox", &mut io::stdout());
        return Ok(());
    }

    show_banner();

    match cli.command {
        Some(Commands::Download { file, clipboard, output, threads }) => {
            if clipboard {
                // Read URLs from clipboard
                let mut clip = arboard::Clipboard::new()
                    .map_err(|e| anyhow::anyhow!("Failed to access clipboard: {}", e))?;
                let text = clip.get_text()
                    .map_err(|e| anyhow::anyhow!("Failed to read clipboard: {}", e))?;
                
                // Create temp file with clipboard content
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join("dpetoolbox_clipboard_urls.txt");
                std::fs::write(&temp_file, &text)?;
                
                let result = commands::download::run(
                    temp_file.to_str().unwrap(),
                    output.as_deref(),
                    threads
                ).await;
                
                // Cleanup temp file
                std::fs::remove_file(&temp_file).ok();
                result?;
            } else if let Some(file) = file {
                commands::download::run(&file, output.as_deref(), threads).await?;
            } else {
                anyhow::bail!("Either --file or --clipboard must be provided");
            }
        }
        Some(Commands::Merge { input, output }) => {
            commands::merge::run(&input, output.as_deref())?;
        }
        Some(Commands::Filter { input, file, output, filter, delete_empty }) => {
            if let Some(file) = file {
                commands::filter::run_single(&file, output.as_deref(), &filter, delete_empty)?;
            } else if let Some(input) = input {
                commands::filter::run(&input, output.as_deref(), &filter, delete_empty)?;
            } else {
                anyhow::bail!("Either --input or --file must be provided");
            }
        }
        Some(Commands::Convert { input, output }) => {
            commands::convert::run(&input, output.as_deref()).await?;
        }
        Some(Commands::Tcpping { target, port, timeout, interval }) => {
            commands::tcpping::run(&target, port, timeout, interval)?;
        }
        Some(Commands::Serve { port }) => {
            web::serve(port).await?;
        }
        Some(Commands::Summary { file, input }) => {
            if let Some(file) = file {
                commands::summary::run_single(&file)?;
            } else if let Some(input) = input {
                commands::summary::run(&input)?;
            } else {
                anyhow::bail!("Either --file or --input must be provided");
            }
        }
        Some(Commands::Conversations { file, export, output }) => {
            if let Some(index) = export {
                commands::conversations::run_export(&file, index, output.as_deref())?;
            } else {
                commands::conversations::run(&file)?;
            }
        }
        Some(Commands::Toptalkers { file, limit }) => {
            commands::toptalkers::run(&file, limit)?;
        }
        Some(Commands::Subnet { cidr }) => {
            commands::subnet::run(&cidr)?;
        }
        None => {
            if cli.cli {
                // --cli flag: run interactive menu mode
                interactive_mode().await?;
            } else {
                // Default: launch Web UI
                web::serve(3000).await?;
            }
        }
    }

    Ok(())
}
