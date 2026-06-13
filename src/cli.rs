use clap::{Parser, Subcommand};

const ABOUT: &str = r#"
██████╗  ██████╗  ██████╗██╗  ██╗
██╔══██╗██╔═══██╗██╔════╝██║ ██╔╝
██████╔╝██║   ██║██║     █████╔╝
██╔══██╗██║   ██║██║     ██╔═██╗
██║  ██║╚██████╔╝╚██████╗██║  ██╗
╚═╝  ╚═╝ ╚═════╝  ╚═════╝╚═╝  ╚═╝

Build multiplayer worlds with Lua.
Docs: https://github.com/sunsetlover36/rock/blob/main/DOCS.md"#;

#[derive(Parser)]
#[command(name = "rock")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = ABOUT)]
#[command(arg_required_else_help = true)]
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
