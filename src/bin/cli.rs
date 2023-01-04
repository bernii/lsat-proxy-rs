use ansi_term::{self, Colour};
use clap::{Parser, Subcommand};

#[tokio::main]
async fn main() {
    banner("LSAT-proxy CLI tool");
    let cli = Cli::parse();

    match cli.command {
        Commands::Stats {} => {
            app_stats();
        }
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about = "LSAT-Proxy management CLI tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// gets usage stats data
    Stats {},
}

/// Prints out the `cli` tool banner
fn banner(quote: &str) {
    const BTC: &str = r"
        ──▄▄█▀▀▀▀▀█▄▄──
        ▄█▀░░▄░▄░░░░▀█▄
        █░░░▀█▀▀▀▀▄░░░█
        █░░░░█▄▄▄▄▀░░░█
        █░░░░█░░░░█░░░█
        ▀█▄░▀▀█▀█▀░░▄█▀
        ──▀▀█▄▄▄▄▄█▀▀──";
    let text = format!("{:-^34}\n{}\n", quote, Colour::Yellow.paint(BTC));
    println!("{}", text);
}

fn app_stats() {}
