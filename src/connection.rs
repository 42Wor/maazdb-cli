use std::io::{self, Write};
use std::fs;
use maazdb_rs::MaazDB;
use comfy_table::{Table, presets, Attribute, Cell};
use colored::*;
use serde_json::Value;
use crate::config::Config;

pub fn print_pretty_table(json_response: &str) -> bool {
    let parsed: Result<Value, _> = serde_json::from_str(json_response);
    match parsed {
        Ok(v) => {
            if let (Some(headers), Some(data)) = (v["headers"].as_array(), v["data"].as_array()) {
                let mut table = Table::new();
                table
                    .load_preset(presets::UTF8_FULL)
                    .set_content_arrangement(comfy_table::ContentArrangement::Dynamic);

                let header_row: Vec<Cell> = headers.iter()
                    .map(|h| Cell::new(h.as_str().unwrap_or("")).add_attribute(Attribute::Bold).fg(comfy_table::Color::Cyan))
                    .collect();
                table.set_header(header_row);

                for row in data {
                    if let Some(row_arr) = row.as_array() {
                        let row_cells: Vec<Cell> = row_arr.iter()
                            .map(|val| {
                                let val_str = if val.is_string() {
                                    val.as_str().unwrap_or("").to_string()
                                } else if val.is_number() {
                                    val.to_string()
                                } else if val.is_boolean() {
                                    val.as_bool().unwrap_or(false).to_string()
                                } else if val.is_null() {
                                    "NULL".to_string()
                                } else {
                                    val.to_string()
                                };
                                Cell::new(val_str)
                            })
                            .collect();
                        table.add_row(row_cells);
                    }
                }

                if data.is_empty() {
                    println!("{}", "Empty set".yellow());
                } else {
                    println!("{}", table);
                    let row_count = data.len();
                    println!("{} {} in set", row_count.to_string().bold(), if row_count == 1 { "row" } else { "rows" });
                }
                return true;
            }
            false
        },
        Err(_) => false,
    }
}

pub fn execute_query(db: &mut MaazDB, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() { return true; }
    
    let query_without_comments = query.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    
    if query_without_comments.is_empty() { return true; }
    
    match db.query(&query_without_comments) {
        Ok(response) => {
            if !response.is_empty() {
                if !print_pretty_table(&response) {
                    println!("{}", response.green());
                }
            }
            true
        },
        Err(e) => {
            eprintln!("{} {}", "ERROR:".red().bold(), e);
            true 
        }
    }
}

pub fn run_repl(config: Config) {
    let mut retry_count = 0;
    let max_retries = 1;
    
    loop {
        println!("\n{} Connecting with:", "Attempting".cyan());
        println!("  {} Host: {}", "•".blue(), config.host);
        println!("  {} Port: {}", "•".blue(), config.port);
        println!("  {} Username: {}", "•".blue(), config.username);
        println!("  {} Password: {}", "•".blue(), "*".repeat(config.password.len()));
        
        print!("\nConnecting... ");
        io::stdout().flush().unwrap();

        match MaazDB::connect(&config.host, config.port, &config.username, &config.password) {
            Ok(mut db) => {
                println!("{}", "Success!".green().bold());
                println!("✓ Connected via TLS 1.3\n");
                
                let mut query_buffer = String::new();

                loop {
                    if query_buffer.is_empty() {
                        print!("{}", "maazdb> ".bold().bright_green());
                    } else {
                        print!("{}", "    -> ".bold().green());
                    }
                    io::stdout().flush().unwrap();

                    let mut input = String::new();
                    if io::stdin().read_line(&mut input).is_err() {
                        break;
                    }
                    let trimmed = input.trim();

                    if trimmed.eq_ignore_ascii_case("exit") { 
                        println!("Bye!");
                        break; 
                    }
                    if trimmed.is_empty() { continue; }
                    if trimmed.starts_with("--") { continue; }

                    query_buffer.push_str(trimmed);
                    query_buffer.push(' ');

                    if trimmed.ends_with(';') {
                        let final_query = query_buffer.trim().trim_end_matches(';').to_string();
                        query_buffer.clear();

                        if final_query.to_uppercase().starts_with("SOURCE") {
                            let path_part: Vec<&str> = final_query.splitn(2, ' ').collect();
                            if path_part.len() < 2 {
                                eprintln!("{}", "Usage: SOURCE 'path/to/file.sql';".yellow());
                                continue;
                            }
                            let path_str = path_part[1].trim().trim_matches('\'').trim_matches('\"');
                            
                            println!("{} {}", "Reading script:".blue(), path_str);
                            
                            match fs::read_to_string(path_str) {
                                Ok(content) => {
                                    for cmd in content.split(';') {
                                        let mut clean_cmd = String::new();
                                        for line in cmd.lines() {
                                            let line_trimmed = line.trim();
                                            if !line_trimmed.is_empty() && !line_trimmed.starts_with("--") {
                                                clean_cmd.push_str(line_trimmed);
                                                clean_cmd.push(' ');
                                            }
                                        }
                                        
                                        if !clean_cmd.trim().is_empty() {
                                            println!("{}", format!("Running: {}", clean_cmd.trim()).truecolor(100, 100, 100));
                                            execute_query(&mut db, &clean_cmd);
                                        }
                                    }
                                    println!("{}", "Script execution finished.".blue());
                                },
                                Err(e) => eprintln!("{} {}", "Failed to read file:".red(), e),
                            }
                        } else {
                            execute_query(&mut db, &final_query);
                        }
                    }
                }
                
                db.close();
                println!("{} Connection closed.", "✓".green());
                break;
            },
            Err(e) => {
                println!("{}", "Failed!".red().bold());
                retry_count += 1;
                
                if retry_count >= max_retries {
                    eprintln!("\n{} {}", "Connection Failed:".red().bold(), e);
                    break;
                }
            }
        }
    }
}