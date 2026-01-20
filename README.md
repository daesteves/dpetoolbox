# DPE Toolbox

<p align="center">
  <img src="https://img.shields.io/badge/version-1.0.0-blue.svg" alt="Version">
  <img src="https://img.shields.io/badge/platform-Windows-lightgrey.svg" alt="Platform">
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License">
</p>

A modern CLI toolbox for network analysis, written in Rust. Designed for network engineers and support professionals who need efficient tools for packet capture analysis, file downloads, and connectivity testing.

## ✨ Features

| Command | Description | External Dependency |
|---------|-------------|---------------------|
| `download` | Multi-threaded file downloads from URL lists | azcopy (auto-downloads) |
| `merge` | Merge PCAP files grouped by IP address | Wireshark |
| `filter` | Filter PCAP files using Wireshark display filters | Wireshark |
| `convert` | Convert Windows ETL traces to PCAP format | etl2pcapng (auto-downloads) |
| `tcpping` | TCP connectivity testing with continuous ping | None |

## 📦 Installation

### Pre-built Binaries
Download the latest release from [Releases](../../releases).

### Build from Source
```powershell
# Requires Rust toolchain
cargo build --release
```

The binary will be at `target/release/dpetoolbox.exe`

### Shell Completions (Optional)
Add tab completion to your PowerShell profile:
```powershell
dpetoolbox --completions powershell >> $PROFILE
```

## 🚀 Quick Start

```powershell
# Run interactive mode (recommended for first-time users)
dpetoolbox

# Or use CLI flags directly
dpetoolbox download -f urls.txt
dpetoolbox tcpping -t google.com -p 443
```

## 📖 Commands

### Download

Downloads files from a text file containing URLs (one per line). Uses azcopy for efficient multi-threaded downloads.

```powershell
# Basic usage
dpetoolbox download -f urls.txt

# Specify output directory and thread count
dpetoolbox download -f urls.txt -o C:\Downloads -t 8

# Download URLs from clipboard
dpetoolbox download --clipboard -o C:\Downloads
```

**Options:**
| Flag | Description | Default |
|------|-------------|---------|
| `-f, --file <FILE>` | Path to TXT file containing URLs | Required* |
| `--clipboard` | Read URLs from clipboard instead of file | - |
| `-o, --output <DIR>` | Output directory | `../<filename>/` |
| `-t, --threads <N>` | Number of parallel downloads | 4 |

*Either `--file` or `--clipboard` is required

**Features:**
- ✅ Auto-downloads azcopy if not found (stored in `%LOCALAPPDATA%\dpetoolbox\azcopy\`)
- ✅ Skips already downloaded files
- ✅ Shows progress for each download
- ✅ Summary with success/fail/skip counts

---

### Merge

Merges multiple PCAP files by IP address. Files are grouped by IP pattern in filename (e.g., `capture_10.0.0.1.pcap`) and merged into single files per IP.

```powershell
# Merge PCAPs in current directory
dpetoolbox merge -i ./pcaps

# Merge and output to different directory
dpetoolbox merge -i ./pcaps -o ./merged
```

**Options:**
| Flag | Description | Default |
|------|-------------|---------|
| `-i, --input <DIR>` | Directory containing PCAP files | Required |
| `-o, --output <DIR>` | Output directory for merged files | Same as input |

**Requirements:**
- ⚠️ Requires [Wireshark](https://www.wireshark.org/download.html) to be installed
- Uses `mergecap` command-line tool from Wireshark

---

### Filter

Filters PCAP files using Wireshark display filter syntax. Applies the filter to all PCAP files in a directory.

```powershell
# Filter by source IP
dpetoolbox filter -i ./pcaps -F "ip.src == 10.0.0.1"

# Filter HTTPS traffic and delete empty results
dpetoolbox filter -i ./pcaps -F "tcp.port == 443" -d

# Filter HTTP traffic to separate directory
dpetoolbox filter -i ./pcaps -o ./filtered -F "http"

# Complex filter example
dpetoolbox filter -i ./pcaps -F "ip.addr == 10.0.0.1 && tcp.flags.syn == 1"
```

**Options:**
| Flag | Description | Default |
|------|-------------|---------|
| `-i, --input <DIR>` | Directory containing PCAP files | Required |
| `-o, --output <DIR>` | Output directory for filtered files | Same as input |
| `-F, --filter <EXPR>` | Wireshark display filter expression | Required |
| `-d, --delete-empty` | Delete output files with 0 matching packets | false |

**Features:**
- ✅ Supports full Wireshark display filter syntax
- ✅ VXLAN decoding on common ports (65330, 65530, 10000, 20000)
- ✅ Shows packet count for each filtered file
- ✅ Option to auto-delete empty results

**Requirements:**
- ⚠️ Requires [Wireshark](https://www.wireshark.org/download.html) to be installed
- Uses `tshark` and `capinfos` command-line tools from Wireshark

---

### Convert

Converts Windows ETL (Event Trace Log) files to PCAP format for analysis in Wireshark.

```powershell
# Convert ETL files in place
dpetoolbox convert -i ./etls

# Convert to different directory
dpetoolbox convert -i ./etls -o ./pcaps
```

**Options:**
| Flag | Description | Default |
|------|-------------|---------|
| `-i, --input <DIR>` | Directory containing ETL files | Required |
| `-o, --output <DIR>` | Output directory for PCAP files | Same as input |

**Features:**
- ✅ Auto-downloads etl2pcapng if not found (stored in `%LOCALAPPDATA%\dpetoolbox\etl2pcapng\`)
- ✅ Batch converts all `.etl` files in directory
- ✅ Shows conversion progress and file sizes

---

### TCP Ping

Tests TCP connectivity to a host and port with continuous ping. Useful for testing firewall rules, load balancer health, and service availability.

```powershell
# Basic TCP ping
dpetoolbox tcpping -t google.com -p 443

# Custom timeout and interval
dpetoolbox tcpping -t 10.0.0.1 -p 22 --timeout 5000 --interval 2

# Test local service
dpetoolbox tcpping -t localhost -p 8080
```

**Options:**
| Flag | Description | Default |
|------|-------------|---------|
| `-t, --target <HOST>` | Target hostname or IP address | Required |
| `-p, --port <PORT>` | Target port number | Required |
| `--timeout <MS>` | Connection timeout in milliseconds | 2000 |
| `--interval <SECS>` | Interval between pings in seconds | 1 |

**Features:**
- ✅ No external dependencies (pure Rust implementation)
- ✅ Press `Esc` to stop and return to menu (in interactive mode)
- ✅ Timestamped output with connection latency
- ✅ Distinguishes between timeout and connection refused

**Output Example:**
```
Starting: TCP ping to google.com on port 443. Press Esc to stop.

[14:32:01] Success: Connected in 23 ms
[14:32:02] Success: Connected in 21 ms
[14:32:03] Timeout after 2000 ms
[14:32:04] Success: Connected in 24 ms
```

---

## 🖥️ Interactive Mode

Run `dpetoolbox` without arguments to enter interactive mode with a menu:

```
    _____  _____  ______   _______          _ _
   |  __ \|  __ \|  ____| |__   __|        | | |
   | |  | | |__) | |__       | | ___   ___ | | |__   _____  __
   | |  | |  ___/|  __|      | |/ _ \ / _ \| | '_ \ / _ \ \/ /
   | |__| | |    | |____     | | (_) | (_) | | |_) | (_) >  <
   |_____/|_|    |______|    |_|\___/ \___/|_|_.__/ \___/_/\_\

          by diesteve

Select an option:

❯ Download files from URL list
  Merge PCAP files by IP
  Filter PCAP files
  Convert ETL to PCAP
  TCP Ping
  Exit
```

Use arrow keys to navigate and Enter to select.

---

## ⚙️ Requirements

### System Requirements
- Windows 10/11 (x64)

### External Dependencies

| Tool | Required For | Auto-Download |
|------|--------------|---------------|
| azcopy | `download` command | ✅ Yes |
| etl2pcapng | `convert` command | ✅ Yes |
| Wireshark | `merge`, `filter` commands | ❌ Manual install |

Auto-downloaded tools are stored in `%LOCALAPPDATA%\dpetoolbox\`

### Installing Wireshark

1. Download from https://www.wireshark.org/download.html
2. During installation, ensure **TShark** component is selected
3. Default installation path is expected: `C:\Program Files\Wireshark\`

---

## 📝 License

MIT License - see [LICENSE](LICENSE) for details.

## 👤 Author

**diesteve** - DPE Network Analysis Team
