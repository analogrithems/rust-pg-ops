# PostgreSQL S3 Backup Manager

A terminal-based utility for managing PostgreSQL database backups in S3. This tool provides an interactive TUI (Terminal User Interface) for listing, downloading, and restoring PostgreSQL database backups stored in S3.

## Features

- Interactive TUI interface
- S3 backup browsing and management
- Secure credential handling
- Progress indicators for downloads
- Support for custom S3 endpoints (e.g., MinIO)
- SSL and non-SSL PostgreSQL connections

## Prerequisites

- Rust (latest stable version)
- PostgreSQL client libraries
- AWS credentials with S3 access

## Installation

1. Clone the repository
2. Build the project:
```bash
cargo build --release
```
3. The binary will be available at `target/release/postgres_manager`

## Usage

### Command Line Arguments

```bash
postgres_manager [OPTIONS]
```

Options:
- `--bucket <BUCKET>`: S3 bucket name
- `--region <REGION>`: AWS region (default: us-west-2)
- `--prefix <PREFIX>`: S3 key prefix for backups
- `--endpoint-url <URL>`: Custom S3 endpoint URL (for MinIO, etc.)
- `--access-key-id <KEY>`: AWS access key ID
- `--secret-access-key <KEY>`: AWS secret access key
- `--path-style`: Use path-style S3 addressing

### Interactive Interface

The application provides a split-screen interface:
- Left side: List of available backups
- Right side: Configuration and status information

Navigation:
- Arrow keys: Move through backup list
- Tab: Switch between sections
- Enter: Select/confirm
- Esc: Cancel/back

### Configuration Fields

S3 Settings:
- [b] Bucket: S3 bucket name
- [r] Region: AWS region
- [p] Prefix: Key prefix for backups
- [E] Endpoint: Custom S3 endpoint URL
- [a] Access Key ID: AWS access key
- [s] Secret Key: AWS secret key
- [y] Path Style: Toggle path-style addressing

PostgreSQL Settings:
- [h] Host: Database host
- [P] Port: Database port
- [d] Database: Database name
- [u] Username: Database user
- [f] Password: Database password
- [S] SSL Mode: Toggle SSL connection

### Operations

1. **Browsing Backups**:
   - Use arrow keys to navigate
   - Backups are sorted by date (newest first)
   - Each entry shows: filename, size, and last modified date

2. **Downloading Backups**:
   - Select a backup using arrow keys
   - Press Enter to start download
   - Progress bar shows download status

3. **Restoring Backups**:
   - After download, confirm restoration
   - Press 'y' to proceed or 'n' to cancel
   - Progress indicators show restoration status

## Security

- Credentials are masked in logs and UI
- AWS credentials can be provided via environment variables
- Supports secure SSL connections to PostgreSQL
- Sensitive information is never displayed in plain text

## Logging

The application uses environment variable `RUST_LOG` for log level control:
```bash
RUST_LOG=debug ./postgres_manager   # For detailed logging
RUST_LOG=info ./postgres_manager    # For normal operation
```

## Error Handling

- Clear error messages in the UI
- Detailed logging for troubleshooting
- Graceful handling of network issues
- Validation of all configuration settings

## Building from Source

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.
