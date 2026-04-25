use std::io::{self, Write};
use std::{fs, path::PathBuf};
use serde::{Serialize, Deserialize};
use colored::*;
use crossterm::{
    event::{read, Event, KeyCode, KeyModifiers},
    terminal::{enable_raw_mode, disable_raw_mode},
};

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
    pub fn get_config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("maazdb");
        let _ = fs::create_dir_all(&path);
        path.push("config.json");
        path
    }

    pub fn load() -> Self {
        let config_path = Self::get_config_path();
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str(&content) {
                    return config;
                }
            }
        }
        Config::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let config_path = Self::get_config_path();
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                fs::write(&config_path, json).map_err(|e| format!("Failed to write config file: {}", e))?;
                println!("{} Configuration saved to {}", "✓".green(), config_path.display());
                Ok(())
            },
            Err(e) => Err(format!("Failed to serialize config: {}", e)),
        }
    }

    pub fn display(&self) {
        println!("\n{}", "Current Configuration:".bold().cyan());
        println!("  {} Host:     {}", "1.".blue(), self.host);
        println!("  {} Port:     {}", "2.".blue(), self.port);
        println!("  {} Username: {}", "3.".blue(), self.username);
        println!("  {} Password: {}", "4.".blue(), "*".repeat(self.password.len()));
    }
}

// Helper to prompt for standard text input
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

// Helper to securely prompt for a password (masks input with '*')
fn prompt_password(prompt: &str, default_len: usize) -> String {
    let masked_default = "*".repeat(default_len);
    print!("{} [{}]: ", prompt.bold(), masked_default.cyan());
    io::stdout().flush().unwrap();

    enable_raw_mode().unwrap();
    let mut pass = String::new();
    
    loop {
        if let Ok(Event::Key(key)) = read() {
            // Handle Ctrl+C to abort
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                disable_raw_mode().unwrap();
                println!("\r\n{} Configuration cancelled.", "✗".red());
                std::process::exit(1);
            }

            match key.code {
                KeyCode::Enter => {
                    print!("\r\n");
                    break;
                }
                KeyCode::Backspace => {
                    if pass.pop().is_some() {
                        // Move cursor back, print space to erase, move cursor back again
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
    }
    disable_raw_mode().unwrap();

    pass
}

pub fn handle_config_mode() -> Result<Config, String> {
    let mut config = Config::load();
    config.display();

    println!("\n{}", "⚙️  MaazDB Configuration Wizard".bold().cyan());
    println!("{}", "Press Enter to keep the current value.\n".truecolor(150, 150, 150));

    // 1. Host
    config.host = prompt_input("Host", &config.host);
    
    // 2. Port (with validation)
    loop {
        let port_str = prompt_input("Port", &config.port.to_string());
        if let Ok(p) = port_str.parse::<u16>() {
            config.port = p;
            break;
        } else {
            println!("{} Invalid port number. Please enter a number between 1 and 65535.", "✗".red());
        }
    }

    // 3. Username
    config.username = prompt_input("Username", &config.username);
    
    // 4. Password
    let new_pass = prompt_password("Password", config.password.len());
    if !new_pass.is_empty() {
        config.password = new_pass;
    }

    println!();
    config.save()?;
    
    Ok(config)
}