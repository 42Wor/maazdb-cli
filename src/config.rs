// src/config.rs
use std::io::{self, Write};
use std::{fs, path::PathBuf};
use serde::{Serialize, Deserialize};
use colored::*;
use crossterm::{
    event::{read, Event, KeyCode, KeyModifiers},
    terminal::{enable_raw_mode, disable_raw_mode},
};
use directories::ProjectDirs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "127.0.0.1".to_string(),
            port: 8888,
            username: "admin".to_string(),
            password: "admin".to_string(),
        }
    }
}

impl Config {
    /// Returns the OS-specific path for the client configuration file
    /// Linux:   ~/.config/maazdb/client/config.json
    /// macOS:   ~/Library/Application Support/maazdb/client/config.json
    /// Windows: %APPDATA%\maazdb\client\config.json
    pub fn get_config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "maazdb", "maazdb") {
            let mut path = proj_dirs.config_dir().to_path_buf();
            path.push("client");
            // Ensure the directory exists
            let _ = fs::create_dir_all(&path);
            path.push("config.json");
            path
        } else {
            // Fallback to local directory
            PathBuf::from("config.json")
        }
    }

    /// Loads configuration from disk or returns defaults if not found
    pub fn load() -> Self {
        let config_path = Self::get_config_path();
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<Config>(&content) {
                    return config;
                }
            }
        }
        Config::default()
    }

    /// Saves the current configuration to the OS-specific path
    pub fn save(&self) -> Result<(), String> {
        let config_path = Self::get_config_path();
        
        // Ensure parent directory exists before saving
        if let Some(parent) = config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                fs::write(&config_path, json)
                    .map_err(|e| format!("Failed to write config file: {}", e))?;
                println!(
                    "{} Configuration saved to: {}", 
                    "✓".green().bold(), 
                    config_path.display().to_string().dimmed()
                );
                Ok(())
            },
            Err(e) => Err(format!("Failed to serialize config: {}", e)),
        }
    }

    /// Prints the current settings to the console
    pub fn display(&self) {
        println!("\n{}", "Current Configuration:".bold().cyan());
        println!("  {} Host:     {}", "1.".blue(), self.host);
        println!("  {} Port:     {}", "2.".blue(), self.port);
        println!("  {} Username: {}", "3.".blue(), self.username);
        let masked_pass = if self.password.is_empty() { 
            "(empty)".dimmed().to_string() 
        } else { 
            "*".repeat(self.password.len()) 
        };
        println!("  {} Password: {}", "4.".blue(), masked_pass);
    }
}

/// Helper to prompt for standard text input with a default value
fn prompt_input(prompt: &str, default: &str) -> String {
    print!("{} [{}]: ", prompt.bold(), default.cyan());
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();
    
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Helper to securely prompt for a password (masks input with '*')
fn prompt_password(prompt: &str, current_pass: &str) -> String {
    let masked_default = if current_pass.is_empty() {
        "none".to_string()
    } else {
        "*".repeat(current_pass.len())
    };

    print!("{} [{}]: ", prompt.bold(), masked_default.cyan());
    io::stdout().flush().unwrap();

    enable_raw_mode().unwrap();
    let mut pass = String::new();
    
    let result = loop {
        if let Ok(Event::Key(key)) = read() {
            // Handle Ctrl+C to abort
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                disable_raw_mode().unwrap();
                println!("\n{} Configuration cancelled.", "✗".red());
                std::process::exit(1);
            }

            match key.code {
                KeyCode::Enter => {
                    break if pass.is_empty() { current_pass.to_string() } else { pass };
                }
                KeyCode::Backspace => {
                    if pass.pop().is_some() {
                        print!("\x08 \x08");
                        io::stdout().flush().unwrap();
                    }
                }
                KeyCode::Char(c) => {
                    pass.push(c);
                    print!("*");
                    io::stdout().flush().unwrap();
                }
                _ => {}
            }
        }
    };
    
    disable_raw_mode().unwrap();
    println!(); // New line after enter
    result
}

/// The main entry point for the configuration wizard
pub fn handle_config_mode() -> Result<Config, String> {
    let mut config = Config::load();
    
    println!("\n{}", "⚙️  MaazDB Configuration Wizard".bold().cyan());
    println!("{}", "Press Enter to keep the current value.\n".truecolor(150, 150, 150));

    config.display();
    println!();

    // 1. Host
    config.host = prompt_input("Host", &config.host);
    
    // 2. Port (with validation)
    loop {
        let port_str = prompt_input("Port", &config.port.to_string());
        if let Ok(p) = port_str.parse::<u16>() {
            config.port = p;
            break;
        } else {
            println!("{} Invalid port. Please enter a number (1-65535).", "✗".red());
        }
    }

    // 3. Username
    config.username = prompt_input("Username", &config.username);
    
    // 4. Password
    config.password = prompt_password("Password", &config.password);

    println!();
    config.save()?;
    
    Ok(config)
}