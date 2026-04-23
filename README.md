# MaazDB CLI 

A powerful, modular command-line interface for **MaazDB**, built with Rust. It features a real-time server monitor, an interactive configuration wizard, and a secure TLS 1.3 database shell.

## ✨ Features

- **Interactive REPL:** A full-featured database shell with syntax highlighting (via colors) and support for multi-line queries.
- **Real-Time Monitor:** A high-performance dashboard to track:
    - **System Stats:** Global CPU/RAM and per-core CPU usage.
    - **Process Isolation:** Dedicated tracking for `maazdb-server` (CPU, RAM, Virtual Memory, Uptime).
    - **Network I/O:** Per-connection bandwidth tracking (RX/TX speeds and totals).
    - **Disk I/O:** Real-time read/write speeds and total data processed.
    - **Performance History:** 6 dynamic graphs showing historical trends.
    - **Storage:** Database and table-level size breakdown.
- **Interactive Setup:** A step-by-step configuration wizard with secure password masking.
- **Script Support:** Execute SQL files using the `SOURCE 'path/to/file.sql';` command.
- **Cross-Platform:** Full support for Linux and Windows (including ANSI color support).

---

## 🛠 Installation

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)
- `ss` utility (for advanced network tracking on Linux)

### Build from Source
```bash
git clone https://github.com/yourusername/maazdb-cli.git
cd maazdb-cli
cargo build --release
```
The binary will be available at `./target/release/maazdb-cli`.

---

## 🚀 Usage

### 1. Start the Database Shell (REPL)
Connect to your MaazDB instance using the saved configuration:
```bash
./maazdb-cli
```

### 2. Configuration Wizard
Configure host, port, and credentials interactively:
```bash
./maazdb-cli --config
```
*Note: You can also use `-c`, `--c`, or just `config`.*

### 3. Server Monitor
Launch the real-time performance dashboard:
```bash
./maazdb-cli --monitor
```
*Note: You can also use `-m`, `--m`, or just `monitor`.*

### 4. Help Menu
```bash
./maazdb-cli --help
```

---

## 💡 Cargo Users
If you are running the project using `cargo run`, you must use the `--` separator to pass flags to the application instead of Cargo:

```bash
cargo run -- --config
cargo run -- --monitor
```

---

## 📂 Project Structure

The project is split into four main modules for maintainability:

- `main.rs`: Entry point and argument parsing.
- `config.rs`: Configuration loading, saving, and the Interactive Wizard.
- `connection.rs`: REPL logic, TLS 1.3 connection, and table formatting.
- `monitor.rs`: System/Process telemetry and TUI dashboard logic.

---

## ⚙️ Configuration
The CLI stores its settings in a `config.json` file located in your system's standard configuration directory:
- **Linux:** `~/.config/maazdb/config.json`
- **Windows:** `%APPDATA%\maazdb\config.json`

The monitor also automatically discovers your `maazdb.toml` server configuration to resolve data directories and disk usage.

---

## 🛡 License
Distributed under the MIT License. See `LICENSE` for more information.

## 🤝 Credits
Powered by [maazdb-rs](https://github.com/42Wor).
