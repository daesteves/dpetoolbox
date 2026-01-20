# DPE Toolbox

A modern CLI for network analysis, written in Rust.

## Features

- **download** - Multi-threaded file downloads from URL lists (uses azcopy)
- More coming soon: merge, filter, convert, tcpping, jit

## Installation

### Pre-built binaries
Download from [Releases](../../releases).

### Build from source
```bash
cargo build --release
```

## Usage

```bash
# Show help
dpetoolbox --help

# Download files from a URL list (4 parallel downloads)
dpetoolbox download -f urls.txt -o C:\Downloads -t 4
```

### Download Command

Downloads files from a TXT file containing URLs (one per line). Uses azcopy for efficient multi-threaded downloads.

```bash
dpetoolbox download --file <FILE> [--output <DIR>] [--threads <N>]
```

Options:
- `-f, --file` - Path to TXT file containing URLs (required)
- `-o, --output` - Output directory (default: creates subfolder based on input filename)
- `-t, --threads` - Number of parallel downloads (default: 4)

Features:
- Auto-downloads azcopy if not found
- Skips already downloaded files
- Shows progress for each download
- Summary with success/fail/skip counts

## Requirements

- Windows 10/11
- azcopy (auto-downloaded if missing)

## License

MIT
