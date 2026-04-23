mod config;
mod monitor;
mod connection;

use std::env;
use colored::*;
use config::Config;

fn print_help() {
    println!("{}", "MaazDB CLI - Help Menu".bold().cyan());
    println!("Usage: maazdb-cli [OPTIONS]\n");
    
    println!("{}", "Options (Supports -, --, or no dashes):".bold().yellow());
    println!("  {:<22} {}", "-h, --h, --help".green(), "Print this help message and exit");
    println!("  {:<22} {}", "-c, --c, --config".green(), "Open the interactive configuration wizard");
    println!("  {:<22} {}", "-m, --m, --monitor".green(), "Launch the real-time server monitor");
    
    println!("\n{}", "Examples:".bold().yellow());
    println!("  maazdb-cli               {}", "(Starts the database REPL)".truecolor(150, 150, 150));
    println!("  maazdb-cli --config      {}", "(Configures connection settings)".truecolor(150, 150, 150));
    println!("  maazdb-cli -m            {}", "(Starts the local server monitor)".truecolor(150, 150, 150));
    
    println!("\n{}", "Note for Cargo users:".bold().blue());
    println!("  If you are running this via cargo, you MUST use '--' before the flags:");
    println!("  cargo run -- -c");
    println!("  cargo run -- --config");
}

fn main() {
    // Enable ANSI color support on Windows
    #[cfg(windows)]
    {
        let _ = enable_ansi_support::enable_ansi_support();
        #[cfg(windows)]
        {
            use winapi::um::consoleapi::SetConsoleMode;
            use winapi::um::processenv::GetStdHandle;
            use winapi::um::winbase::STD_OUTPUT_HANDLE;
            use winapi::um::wincon::ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            
            unsafe {
                let handle = GetStdHandle(STD_OUTPUT_HANDLE);
                let mut mode = 0;
                if winapi::um::consoleapi::GetConsoleMode(handle, &mut mode) != 0 {
                    SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }

    let args: Vec<String> = env::args().collect();
    
    // Handle CLI Arguments
    if args.len() > 1 {
        let arg = args[1].as_str();
        
        // Strip all leading '-' characters so that -c, --c, -config, and --config all work the same
        let clean_arg = arg.trim_start_matches('-');
        
        match clean_arg {
            "h" | "help" => {
                print_help();
                std::process::exit(0);
            }
            "c" | "config" => {
                let _ = config::handle_config_mode();
                std::process::exit(0);
            }
            "m" | "monitor" => {
                let cfg = Config::load();
                if let Err(e) = monitor::run_local_monitor(cfg.port) {
                    eprintln!("{} {}", "Monitor error:".red().bold(), e);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            _ => {
                eprintln!("{} Unknown argument: '{}'", "✗ Error:".red().bold(), arg);
                println!("Type {} for a list of valid commands.", "maazdb-cli --help".green());
                std::process::exit(1);
            }
        }
    }

    // Clear the screen before starting the REPL
    print!("\x1B[2J\x1B[1;1H");

    let cfg = Config::load();
    let config_path = Config::get_config_path();

    println!("{}", "--------------------------------------------------".bright_blue());
    println!("  {} v13.4", "MaazDB CLI".bold().cyan());
    println!("  Powered by maazdb-rs");
    println!("  Type 'help' for commands or 'exit' to quit.");
    println!("  {} Use '--config' to modify connection settings", "Tip:".yellow());
    println!("  {} Config file: {}", "📁".blue(), config_path.display());
    println!("{}", "--------------------------------------------------".bright_blue());

    // Start the interactive database shell
    connection::run_repl(cfg);
}