use std::io::{self, Write};
use std::{fs, path::Path, path::PathBuf, env};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::{VecDeque, HashMap};
use colored::*;
use crossterm::{
    cursor::{Hide, Show},
    event::{poll, read, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, size},
};
use sysinfo::{System, CpuRefreshKind, RefreshKind, MemoryRefreshKind, ProcessRefreshKind, Disks, Users};

#[derive(Clone)]
struct ConnStats {
    state: String,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_rate: u64,
    tx_rate: u64,
}

fn parse_hex_ip_port(hex_str: &str) -> Option<(String, u16)> {
    let parts: Vec<&str> = hex_str.split(':').collect();
    if parts.len() != 2 { return None; }
    let ip_hex = parts[0];
    let port_hex = parts[1];

    let port = u16::from_str_radix(port_hex, 16).ok()?;

    if ip_hex.len() == 8 {
        let b1 = u8::from_str_radix(&ip_hex[0..2], 16).ok()?;
        let b2 = u8::from_str_radix(&ip_hex[2..4], 16).ok()?;
        let b3 = u8::from_str_radix(&ip_hex[4..6], 16).ok()?;
        let b4 = u8::from_str_radix(&ip_hex[6..8], 16).ok()?;
        Some((format!("{}.{}.{}.{}", b4, b3, b2, b1), port))
    } else if ip_hex.len() == 32 {
        let mut ipv6 = String::new();
        for i in 0..8 {
            let chunk = &ip_hex[i*4..(i+1)*4];
            ipv6.push_str(chunk);
            if i < 7 { ipv6.push(':'); }
        }
        Some((ipv6, port))
    } else {
        Some(("0.0.0.0".to_string(), port))
    }
}

// Fetches per-connection network stats (Bytes RX/TX) using `ss` on Linux.
// Falls back to `/proc/net/tcp` if `ss` is unavailable (without byte counts).
fn get_process_network_stats(port: u16, prev_stats: &HashMap<String, ConnStats>, time_delta: f64) -> HashMap<String, ConnStats> {
    let mut new_stats = HashMap::new();

    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("ss")
            .args(&["-ntpi", &format!("( sport = :{} )", port)])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut current_peer = String::new();
            let mut current_state = String::new();
            let mut rx = 0;
            let mut tx = 0;

            let  commit = |peer: &str, state: &str, r: u64, t: u64, map: &mut HashMap<String, ConnStats>| {
                if !peer.is_empty() {
                    let mut rx_rate = 0;
                    let mut tx_rate = 0;
                    if let Some(prev) = prev_stats.get(peer) {
                        if r >= prev.rx_bytes && time_delta > 0.0 { rx_rate = ((r - prev.rx_bytes) as f64 / time_delta) as u64; }
                        if t >= prev.tx_bytes && time_delta > 0.0 { tx_rate = ((t - prev.tx_bytes) as f64 / time_delta) as u64; }
                    }
                    map.insert(peer.to_string(), ConnStats {
                        state: state.to_string(),
                        rx_bytes: r,
                        tx_bytes: t,
                        rx_rate,
                        tx_rate,
                    });
                }
            };

            for line in stdout.lines() {
                let line = line.trim_end();
                if line.starts_with("State") { continue; }
                
                if !line.starts_with(|c: char| c.is_whitespace()) {
                    commit(&current_peer, &current_state, rx, tx, &mut new_stats);
                    rx = 0; tx = 0;
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        current_state = parts[0].to_string();
                        current_peer = parts[4].to_string();
                    } else {
                        current_peer.clear();
                    }
                } else {
                    for token in line.split_whitespace() {
                        if token.starts_with("bytes_received:") {
                            rx = token.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                        } else if token.starts_with("bytes_sent:") {
                            tx = token.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                        } else if token.starts_with("bytes_acked:") && tx == 0 {
                            tx = token.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                        }
                    }
                }
            }
            commit(&current_peer, &current_state, rx, tx, &mut new_stats);
        }
    }

    // Fallback if `ss` didn't yield results
    if new_stats.is_empty() {
        let mut parse_proc = |path: &str| {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        if let Some((_, local_port)) = parse_hex_ip_port(parts[1]) {
                            if local_port == port {
                                if let Some((rem_ip, rem_port)) = parse_hex_ip_port(parts[2]) {
                                    let state = match parts[3] {
                                        "01" => "ESTABLISHED", "02" => "SYN_SENT", "03" => "SYN_RECV",
                                        "04" => "FIN_WAIT1", "05" => "FIN_WAIT2", "06" => "TIME_WAIT",
                                        "07" => "CLOSE", "08" => "CLOSE_WAIT", "09" => "LAST_ACK",
                                        "0A" => "LISTEN", "0B" => "CLOSING", _ => "UNKNOWN",
                                    };
                                    if state == "LISTEN" { continue; }
                                    let peer = format!("{}:{}", rem_ip, rem_port);
                                    new_stats.insert(peer, ConnStats {
                                        state: state.to_string(),
                                        rx_bytes: 0, tx_bytes: 0, rx_rate: 0, tx_rate: 0,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        };
        parse_proc("/proc/net/tcp");
        parse_proc("/proc/net/tcp6");
    }

    new_stats
}

fn format_size_compact(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn get_dir_size(path: impl AsRef<Path>) -> u64 {
    let mut size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    size += get_dir_size(entry.path());
                } else {
                    size += metadata.len();
                }
            }
        }
    }
    size
}

fn get_server_config_content() -> Option<(String, PathBuf)> {
    let mut paths = vec![
        PathBuf::from("maazdb.toml"),
        PathBuf::from("config.toml"),
        PathBuf::from("maazdb.config"),
        PathBuf::from("../maazdb.toml"),
        PathBuf::from("../maazdb/maazdb.toml"),
        PathBuf::from("../../maazdb/maazdb.toml"),
        PathBuf::from("/run/media/maaz/S/project_maazdb/maazdb/maazdb.toml"),
    ];

    if let Ok(mut current_dir) = std::env::current_dir() {
        for _ in 0..4 {
            paths.push(current_dir.join("maazdb.toml"));
            paths.push(current_dir.join("maazdb").join("maazdb.toml"));
            if !current_dir.pop() { break; }
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            paths.push(exe_dir.join("maazdb.toml"));
            paths.push(exe_dir.join("config.toml"));
        }
    }

    if let Some(mut p) = dirs::config_dir() {
        p.push("maazdb");
        p.push("maazdb.toml");
        paths.push(p);
    }

    #[cfg(unix)]
    paths.push(PathBuf::from("/etc/maazdb/maazdb.toml"));

    #[cfg(windows)]
    if let Ok(pd) = env::var("ProgramData") {
        paths.push(PathBuf::from(format!(r"{}\maazdb\maazdb.toml", pd)));
    }

    for path in paths {
        if let Ok(content) = fs::read_to_string(&path) {
            return Some((content, path));
        }
    }
    None
}

fn get_data_dir_from_config() -> String {
    if let Some((content, config_path)) = get_server_config_content() {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("data_dir") {
                if let Some(path_str) = line.split('=').nth(1) {
                    let path_str = path_str.trim().trim_matches('"').trim_matches('\'');
                    if !path_str.is_empty() {
                        let mut path = PathBuf::from(path_str);
                        if path.is_relative() {
                            if let Some(parent) = config_path.parent() {
                                path = parent.join(path);
                            }
                        }
                        return path.to_string_lossy().to_string();
                    }
                }
            }
        }
    }
    let mut default_path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    default_path.push("maazdb");
    default_path.push("data");
    default_path.to_string_lossy().to_string()
}

fn get_port_from_toml(fallback: u16) -> u16 {
    if let Some((content, _)) = get_server_config_content() {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("port") {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() == 2 {
                    if let Ok(port) = parts[1].trim().parse::<u16>() {
                        return port;
                    }
                }
            }
        }
    }
    fallback
}

fn get_storage_stats(base_path: &str) -> Vec<(String, u64, Vec<(String, u64, u64, u64)>)> {
    let mut dbs = Vec::new();
    if let Ok(entries) = fs::read_dir(base_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let db_name = entry.file_name().to_string_lossy().to_string();
                let db_size = get_dir_size(&path);
                
                let mut tables = Vec::new();
                if let Ok(table_entries) = fs::read_dir(&path) {
                    for t_entry in table_entries.flatten() {
                        let t_path = t_entry.path();
                        if t_path.is_dir() {
                            let table_name = t_entry.file_name().to_string_lossy().to_string();
                            
                            let mut col_size = 0;
                            if let Ok(files) = fs::read_dir(&t_path) {
                                for f in files.flatten() {
                                    let f_path = f.path();
                                    if f_path.extension().and_then(|s| s.to_str()) == Some("bin") {
                                        col_size += f.metadata().map(|m| m.len()).unwrap_or(0);
                                    }
                                }
                            }
                            
                            let index_dir = t_path.join("index");
                            let mut idx_size = get_dir_size(&index_dir);
                            if let Ok(files) = fs::read_dir(&t_path) {
                                for f in files.flatten() {
                                    let f_path = f.path();
                                    if f_path.extension().and_then(|s| s.to_str()) == Some("sparse") {
                                        idx_size += f.metadata().map(|m| m.len()).unwrap_or(0);
                                    }
                                }
                            }
                            
                            let total_table_size = col_size + idx_size;
                            tables.push((table_name, total_table_size, col_size, idx_size));
                        }
                    }
                }
                tables.sort_by(|a, b| b.1.cmp(&a.1)); 
                dbs.push((db_name, db_size, tables));
            }
        }
    }
    dbs.sort_by(|a, b| b.1.cmp(&a.1)); 
    dbs
}

fn draw_bar(percentage: f32, width: usize) -> String {
    let percentage = if percentage.is_nan() { 0.0 } else { percentage.max(0.0).min(100.0) };
    let filled = ((percentage / 100.0) * width as f32).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    
    let bar_char = "\u{2588}"; // █
    
    let mut bar = String::new();
    for i in 0..filled {
        let color = if i < width / 3 {
            bar_char.green()
        } else if i < width * 2 / 3 {
            bar_char.yellow()
        } else {
            bar_char.red()
        };
        bar.push_str(&color.to_string());
    }
    bar.push_str(&" ".repeat(empty));
    format!("[{}]", bar)
}

fn draw_history_graph(history: &VecDeque<f32>, width: usize, min_max: f32) -> (String, f32) {
    let mut max_val = history.iter().fold(0.0f32, |a, &b| a.max(b));
    if max_val < min_max { max_val = min_max; }
    
    let mut graph = String::new();
    let pad_len = width.saturating_sub(history.len());
    graph.push_str(&" ".repeat(pad_len));

    for &val in history {
        let height = ((val / max_val) * 8.0) as usize;
        let block = match height {
            0 if val > 0.0 => "\u{2581}", 
            0 => " ",
            1 => "\u{2581}",
            2 => "\u{2582}",
            3 => "\u{2583}",
            4 => "\u{2584}",
            5 => "\u{2585}",
            6 => "\u{2586}",
            7 => "\u{2587}",
            _ => "\u{2588}",
        };
        
        let colored_block = if height > 6 { block.red() }
        else if height > 3 { block.yellow() }
        else { block.green() };
        
        graph.push_str(&colored_block.to_string());
    }
    
    (graph, max_val)
}

fn chunk_header(title: &str, width: usize) -> String {
    let clean_title = format!(" {} ", title);
    let left_line = "━━";
    let right_len = width.saturating_sub(clean_title.len() + 6);
    let right_line = "━".repeat(right_len);
    format!("  {}{}{}\x1B[K", 
        left_line.truecolor(100, 100, 100), 
        clean_title.bold().cyan(), 
        right_line.truecolor(100, 100, 100))
}

fn current_time_str() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let hours = (now / 3600) % 24;
    let minutes = (now / 60) % 60;
    let seconds = now % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn pad_right(s: &str, width: usize) -> String {
    let mut visible_len = 0;
    let mut is_ansi = false;
    for c in s.chars() {
        if c == '\x1b' { is_ansi = true; continue; }
        if is_ansi {
            if (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') { is_ansi = false; }
            continue;
        }
        visible_len += 1;
    }
    
    if visible_len >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible_len))
    }
}

pub fn run_local_monitor(cli_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything())
            .with_processes(ProcessRefreshKind::everything())
    );
    
    let mut disks = Disks::new_with_refreshed_list();
    let mut users = Users::new_with_refreshed_list();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let mut last_update = Instant::now() - Duration::from_secs(2);
    let data_path = get_data_dir_from_config();
    let actual_db_port = get_port_from_toml(cli_port);
    
    let mut cpu_history: VecDeque<f32> = VecDeque::new();
    let mut ram_history: VecDeque<f32> = VecDeque::new();
    let mut net_rx_history: VecDeque<f32> = VecDeque::new();
    let mut net_tx_history: VecDeque<f32> = VecDeque::new();
    let mut disk_read_history: VecDeque<f32> = VecDeque::new();
    let mut disk_write_history: VecDeque<f32> = VecDeque::new();

    let mut alerts: VecDeque<String> = VecDeque::new();
    let mut was_running = false;
    let mut first_check = true;
    let mut last_cpu_alert = Instant::now() - Duration::from_secs(60);
    let mut last_ram_alert = Instant::now() - Duration::from_secs(60);

    let mut prev_conn_stats: HashMap<String, ConnStats> = HashMap::new();

    loop {
        if last_update.elapsed() >= Duration::from_millis(500) {
            sys.refresh_all();
            disks.refresh(true);
            users.refresh();
            
            let time_delta = last_update.elapsed().as_secs_f64();
            
            let mut server_cpu = 0.0;
            let mut server_ram_mb = 0u64;
            let mut server_pid = 0;
            let mut is_running = false;
            
            let mut process_read_rate = 0;
            let mut process_write_rate = 0;
            let mut total_read_bytes = 0;
            let mut total_write_bytes = 0;
            
            let mut server_uptime = 0;
            let mut server_user = String::new();
            let mut server_vmem_mb = 0;

            let my_pid = std::process::id();
            let mut target_process = None;

            for (pid, process) in sys.processes() {
                if pid.as_u32() == my_pid { continue; }
                let process_name = process.name().to_string_lossy().to_lowercase();
                if process_name == "maazdb-server" || process_name == "maazdb-server.exe" {
                    target_process = Some((pid, process));
                    break;
                }
            }

            if target_process.is_none() {
                for (pid, process) in sys.processes() {
                    if pid.as_u32() == my_pid { continue; }
                    let process_name = process.name().to_string_lossy().to_lowercase();
                    if process_name.contains("maazdb") {
                        target_process = Some((pid, process));
                        break;
                    }
                }
            }

            if let Some((pid, process)) = target_process {
                server_cpu = process.cpu_usage();
                server_ram_mb = process.memory() / 1024 / 1024;
                server_vmem_mb = process.virtual_memory() / 1024 / 1024;
                server_pid = pid.as_u32();
                server_uptime = process.run_time();
                
                if let Some(user_id) = process.user_id() {
                    if let Some(user) = users.get_user_by_id(user_id) {
                        server_user = user.name().to_string();
                    }
                }

                is_running = true;
                
                let disk_usage = process.disk_usage();
                total_read_bytes = disk_usage.total_read_bytes;
                total_write_bytes = disk_usage.total_written_bytes;

                if time_delta > 0.0 {
                    process_read_rate = (disk_usage.read_bytes as f64 / time_delta) as u64;
                    process_write_rate = (disk_usage.written_bytes as f64 / time_delta) as u64;
                }
            }

            // Get Process-Specific Network Stats
            let current_conn_stats = get_process_network_stats(actual_db_port, &prev_conn_stats, time_delta);
            prev_conn_stats = current_conn_stats.clone();

            let mut proc_net_rx_rate = 0;
            let mut proc_net_tx_rate = 0;
            let mut proc_net_rx_total = 0;
            let mut proc_net_tx_total = 0;

            for stats in current_conn_stats.values() {
                proc_net_rx_rate += stats.rx_rate;
                proc_net_tx_rate += stats.tx_rate;
                proc_net_rx_total += stats.rx_bytes;
                proc_net_tx_total += stats.tx_bytes;
            }

            let sys_cpu_usage = sys.global_cpu_usage();
            let ram_total = sys.total_memory() / 1024 / 1024;
            let ram_used = sys.used_memory() / 1024 / 1024;
            let ram_pct = if ram_total > 0 { (ram_used as f32 / ram_total as f32) * 100.0 } else { 0.0 };
            let process_ram_pct = if is_running { (server_ram_mb as f32 / 1024.0) * 100.0 } else { 0.0 };

            if !first_check {
                if !is_running && was_running {
                    alerts.push_front(format!("[{}] ❌ CRITICAL: MaazDB process crashed or went OFFLINE!", current_time_str()));
                } else if is_running && !was_running {
                    alerts.push_front(format!("[{}] ✅ MaazDB process is ONLINE (PID: {})", current_time_str(), server_pid));
                }
            }
            was_running = is_running;
            first_check = false;

            if is_running {
                if server_cpu > 95.0 && last_cpu_alert.elapsed() > Duration::from_secs(10) {
                    alerts.push_front(format!("[{}] ⚠️ WARNING: High CPU usage detected ({:.1}%)", current_time_str(), server_cpu));
                    last_cpu_alert = Instant::now();
                }
                if process_ram_pct > 95.0 && last_ram_alert.elapsed() > Duration::from_secs(10) {
                    alerts.push_front(format!("[{}] ⚠️ WARNING: High RAM usage detected ({:.1} MB)", current_time_str(), server_ram_mb));
                    last_ram_alert = Instant::now();
                }
            }

            let mut disk_free = 0;
            let mut disk_total = 0;
            let mut best_match_len = 0;
            
            let data_path_abs = std::fs::canonicalize(&data_path).unwrap_or_else(|_| PathBuf::from(&data_path));
            let data_path_str = data_path_abs.to_string_lossy().to_string();

            for disk in &disks {
                let mnt = disk.mount_point().to_string_lossy().to_string();
                if data_path_str.starts_with(&mnt) && mnt.len() > best_match_len {
                    best_match_len = mnt.len();
                    disk_free = disk.available_space();
                    disk_total = disk.total_space();
                }
            }

            if disk_total > 0 && (disk_free as f64 / disk_total as f64) < 0.05 {
                alerts.push_front(format!("[{}] ❌ CRITICAL: Disk space is below 5%!", current_time_str()));
            }

            while alerts.len() > 4 { alerts.pop_back(); }

            let (term_width, term_height) = size().unwrap_or((120, 40));
            let t_width = term_width as usize;
            let graph_w = t_width.saturating_sub(10) / 2;

            cpu_history.push_back(server_cpu);
            ram_history.push_back(server_ram_mb as f32);
            net_rx_history.push_back(proc_net_rx_rate as f32);
            net_tx_history.push_back(proc_net_tx_rate as f32);
            disk_read_history.push_back(process_read_rate as f32);
            disk_write_history.push_back(process_write_rate as f32);

            while cpu_history.len() > graph_w { cpu_history.pop_front(); }
            while ram_history.len() > graph_w { ram_history.pop_front(); }
            while net_rx_history.len() > graph_w { net_rx_history.pop_front(); }
            while net_tx_history.len() > graph_w { net_tx_history.pop_front(); }
            while disk_read_history.len() > graph_w { disk_read_history.pop_front(); }
            while disk_write_history.len() > graph_w { disk_write_history.pop_front(); }

            let storage_stats = get_storage_stats(&data_path);
            let total_storage: u64 = storage_stats.iter().map(|db| db.1).sum();

            execute!(stdout, crossterm::cursor::MoveTo(0, 0))?;
            let mut lines: Vec<String> = Vec::new();
            
            let os_info = std::env::consts::OS;
            let header = format!("  MAAZDB SERVER MONITOR ({})  ", os_info.to_uppercase());
            let exit_msg = "Press 'Q' or 'Ctrl+C' to exit";
            let padding = t_width.saturating_sub(header.len() + exit_msg.len() + 4);
            lines.push(format!("{}{}{}\x1B[K", 
                header.black().on_cyan().bold(), 
                " ".repeat(padding), 
                exit_msg.truecolor(150, 150, 150)));
            lines.push("\x1B[K".to_string());
            
            // --- CHUNK 1: SYSTEM (Global & Per-Core) ---
            lines.push(chunk_header("SYSTEM", t_width));
            lines.push(format!("  Global CPU: {} {:>5.1}%   |   Global RAM: {} {:>6} MB\x1B[K", 
                draw_bar(sys_cpu_usage, 20), sys_cpu_usage, 
                draw_bar(ram_pct, 20), ram_used));
            
            // Render Per-Core CPU Grid
            let cpus = sys.cpus();
            let cols = (t_width / 30).max(1);
            let mut current_line = String::from("  ");
            for (i, cpu) in cpus.iter().enumerate() {
                let usage = cpu.cpu_usage();
                let col_str = format!("Core {:>2}: {} {:>5.1}%   ", i, draw_bar(usage, 10), usage);
                current_line.push_str(&col_str);
                
                if (i + 1) % cols == 0 || i == cpus.len() - 1 {
                    lines.push(format!("{}\x1B[K", current_line));
                    current_line = String::from("  ");
                }
            }
            lines.push("\x1B[K".to_string());

            // --- CHUNK 2: MAAZDB PROCESS ---
            lines.push(chunk_header("MAAZDB PROCESS", t_width));
            let status_text = if is_running { format!("ONLINE (PID: {})", server_pid).green().to_string() } else { "OFFLINE".red().to_string() };
            let uptime_str = if is_running {
                let h = server_uptime / 3600;
                let m = (server_uptime % 3600) / 60;
                let s = server_uptime % 60;
                format!("{:02}:{:02}:{:02}", h, m, s)
            } else { "N/A".to_string() };
            let user_str = if server_user.is_empty() { "N/A".to_string() } else { server_user.clone() };

            lines.push(format!("  Status: {:<20} | Uptime: {:<10} | User: {:<10}\x1B[K", 
                status_text, uptime_str.cyan(), user_str.yellow()));
            
            lines.push(format!("  CPU: {} {:>5.1}%   |   RAM: {} {:>6} MB (VMem: {} MB)\x1B[K", 
                draw_bar(server_cpu, 20), server_cpu, 
                draw_bar(process_ram_pct, 20), server_ram_mb, server_vmem_mb));
            
            lines.push(format!("  Net RX: {:<12} (Tot: {:<10}) | Disk Read:  {:<12} (Tot: {:<10})\x1B[K", 
                format!("{}/s", format_size_compact(proc_net_rx_rate)).cyan(),
                format_size_compact(proc_net_rx_total).truecolor(150, 150, 150),
                format!("{}/s", format_size_compact(process_read_rate)).yellow(),
                format_size_compact(total_read_bytes).truecolor(150, 150, 150)));
                
            lines.push(format!("  Net TX: {:<12} (Tot: {:<10}) | Disk Write: {:<12} (Tot: {:<10})\x1B[K", 
                format!("{}/s", format_size_compact(proc_net_tx_rate)).magenta(),
                format_size_compact(proc_net_tx_total).truecolor(150, 150, 150),
                format!("{}/s", format_size_compact(process_write_rate)).green(),
                format_size_compact(total_write_bytes).truecolor(150, 150, 150)));
            lines.push("\x1B[K".to_string());

            // --- CHUNK 3: PROCESS PERFORMANCE HISTORY ---
            lines.push(chunk_header("PROCESS PERFORMANCE HISTORY", t_width));
            
            let (cpu_g, cpu_m) = draw_history_graph(&cpu_history, graph_w, 100.0);
            let (ram_g, ram_m) = draw_history_graph(&ram_history, graph_w, 100.0);
            let (rx_g, rx_m) = draw_history_graph(&net_rx_history, graph_w, 1024.0);
            let (tx_g, tx_m) = draw_history_graph(&net_tx_history, graph_w, 1024.0);
            let (r_g, r_m) = draw_history_graph(&disk_read_history, graph_w, 1024.0);
            let (w_g, w_m) = draw_history_graph(&disk_write_history, graph_w, 1024.0);

            let title_cpu = pad_right(&format!("CPU Usage (Max: {:.1}%)", cpu_m).blue().to_string(), graph_w + 2);
            let title_ram = format!("RAM Usage (Max: {:.1} MB)", ram_m).blue().to_string();
            lines.push(format!("  {}{}\x1B[K", title_cpu, title_ram));
            lines.push(format!("  {}  {}\x1B[K", cpu_g, ram_g));
            lines.push("\x1B[K".to_string());

            let title_rx = pad_right(&format!("Network RX (Max: {}/s)", format_size_compact(rx_m as u64)).cyan().to_string(), graph_w + 2);
            let title_tx = format!("Network TX (Max: {}/s)", format_size_compact(tx_m as u64)).magenta().to_string();
            lines.push(format!("  {}{}\x1B[K", title_rx, title_tx));
            lines.push(format!("  {}  {}\x1B[K", rx_g, tx_g));
            lines.push("\x1B[K".to_string());

            let title_r = pad_right(&format!("Disk Read (Max: {}/s)", format_size_compact(r_m as u64)).yellow().to_string(), graph_w + 2);
            let title_w = format!("Disk Write (Max: {}/s)", format_size_compact(w_m as u64)).green().to_string();
            lines.push(format!("  {}{}\x1B[K", title_r, title_w));
            lines.push(format!("  {}  {}\x1B[K", r_g, w_g));
            lines.push("\x1B[K".to_string());

            // --- CHUNK 4: CLIENT CONNECTIONS ---
            lines.push(chunk_header(&format!("CLIENT CONNECTIONS (Port {})", actual_db_port), t_width));
            if current_conn_stats.is_empty() {
                lines.push(format!("  {} No active client connections\x1B[K", "ℹ".yellow()));
            } else {
                let header_str = format!("  {:<22} | {:<11} | {:<10} | {:<10} | {:<10} | {:<10}", 
                    "REMOTE IP:PORT", "STATE", "RX RATE", "TX RATE", "TOTAL RX", "TOTAL TX");
                lines.push(format!("{}\x1B[K", header_str.truecolor(150, 150, 150)));
                
                let mut count = 0;
                for (peer, stats) in &current_conn_stats {
                    if count >= 5 { break; }
                    let peer_trunc = if peer.len() > 22 { format!("{}..", &peer[0..20]) } else { peer.clone() };
                    
                    lines.push(format!("  {:<22} | {:<11} | {:<10} | {:<10} | {:<10} | {:<10}\x1B[K", 
                        peer_trunc.cyan(),
                        stats.state.yellow(),
                        format!("{}/s", format_size_compact(stats.rx_rate)),
                        format!("{}/s", format_size_compact(stats.tx_rate)),
                        format_size_compact(stats.rx_bytes).truecolor(150, 150, 150),
                        format_size_compact(stats.tx_bytes).truecolor(150, 150, 150)
                    ));
                    count += 1;
                }
                if current_conn_stats.len() > 5 {
                    lines.push(format!("  ... and {} more\x1B[K", current_conn_stats.len() - 5).truecolor(100, 100, 100).to_string());
                }
            }
            lines.push("\x1B[K".to_string());

            // --- CHUNK 5: STORAGE ---
            let disk_info = if disk_total > 0 {
                format!("Disk Free: {} / {}", format_size_compact(disk_free).green(), format_size_compact(disk_total))
            } else {
                "Disk Free: Unknown".to_string()
            };

            lines.push(chunk_header(&format!("STORAGE (Path: '{}')", data_path), t_width));
            lines.push(format!("  Total DB Size: {} | {}\x1B[K", format_size_compact(total_storage).cyan().bold(), disk_info));
            
            if storage_stats.is_empty() {
                lines.push(format!("  {} No databases found\x1B[K", "ℹ".yellow()));
            } else {
                for (db_name, db_size, tables) in &storage_stats {
                    lines.push(format!("  📁 {} [{}]\x1B[K", 
                        db_name.blue().bold(), 
                        format_size_compact(*db_size).yellow()));
                    
                    for (i, (table_name, t_size, col_size, idx_size)) in tables.iter().enumerate() {
                        let branch = if i == tables.len() - 1 { "└─" } else { "├─" };
                        lines.push(format!("    {} 📄 {} [{}] (Data: {}, Index: {})\x1B[K", 
                            branch,
                            table_name.green(), 
                            format_size_compact(*t_size),
                            format_size_compact(*col_size).truecolor(150, 150, 150),
                            format_size_compact(*idx_size).truecolor(150, 150, 150)
                        ));
                    }
                }
            }
            lines.push("\x1B[K".to_string());

            // --- CHUNK 6: ALERTS ---
            lines.push(chunk_header("SYSTEM & PROCESS ALERTS", t_width));
            if alerts.is_empty() {
                lines.push(format!("  {} No recent errors or warnings detected.\x1B[K", "✓".green()));
            } else {
                for alert in &alerts {
                    if alert.contains("CRITICAL") || alert.contains("❌") {
                        lines.push(format!("  {}\x1B[K", alert.red()));
                    } else if alert.contains("WARNING") || alert.contains("⚠️") {
                        lines.push(format!("  {}\x1B[K", alert.yellow()));
                    } else {
                        lines.push(format!("  {}\x1B[K", alert.green()));
                    }
                }
            }

            let current_lines = lines.len();
            for _ in current_lines..term_height as usize {
                lines.push("\x1B[K".to_string());
            }
            
            write!(stdout, "{}", lines.join("\r\n"))?;
            stdout.flush()?;
            
            last_update = Instant::now();
        }

        if poll(Duration::from_millis(50))? {
            if let Event::Key(key_event) = read()? {
                if key_event.code == KeyCode::Char('q') || 
                   key_event.code == KeyCode::Char('Q') || 
                   key_event.code == KeyCode::Esc {
                    break;
                }
                if key_event.code == KeyCode::Char('c') && 
                   key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
            }
        }
    }

    execute!(stdout, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    println!("{} Exited local monitor gracefully.", "✓".green());
    Ok(())
}