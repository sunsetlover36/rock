use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rock")]
#[command(author = "Luther Blissett")]
#[command(version = "0.2.2")]
#[command(about = "ROCK game engine", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    // Start the engine
    Ignite,

    // Create a new gamemode
    Genesis { name: String },
    // Install a geode
    // Accrete {
    //     geode_name: String,
    // },

    // Doctor
    // Scan,
}
