use clap::{Parser, Subcommand};

pub mod app;

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    RPC,
    Consumer,
    Producer,
    Projector,
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::RPC => app::rpc::main().await,
        Commands::Consumer => app::consumer::main().await,
        Commands::Producer => app::producer::main().await,
        Commands::Projector => app::projector::main().await,
    }
}
