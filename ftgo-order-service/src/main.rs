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
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::RPC => app::rpc::main().await,
        Commands::Consumer => {
            app::consumer::main();
            Ok(())
        }
        Commands::Producer => {
            app::producer::main();
            Ok(())
        }
    }
}
