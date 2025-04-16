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

### Command Line Operations

The application supports several command-line operations for PostgreSQL database management:

```bash
# List all databases
postgres_manager list

# Create a new database
postgres_manager create <name>

# Clone a database
postgres_manager clone <name>

# Drop a database
postgres_manager drop <name>

# Drop a database with force (terminates all connections)
postgres_manager drop-force <name>

# Rename a database
postgres_manager rename <old_name> <new_name>

# Set database owner
postgres_manager set-owner <name> <owner>

# Dump a database to a file
postgres_manager dump <name> <output>

# Restore a database from a file
postgres_manager restore <name> <restore_file>

# Launch the interactive TUI browser
postgres_manager browse-snapshots
```

### Configuration

The application can be configured using either command-line arguments or environment variables. Environment variables take precedence over default values but command-line arguments take precedence over environment variables.

#### Environment Variables

Create a `.env` file in the project root with the following variables:

```
# S3 Configuration
S3_BUCKET=your-bucket-name
S3_REGION=us-west-2
S3_PREFIX=backups/
S3_ENDPOINT_URL=
S3_ACCESS_KEY_ID=
S3_SECRET_ACCESS_KEY=
S3_PATH_STYLE=false

# PostgreSQL Configuration
PG_HOST=localhost
PG_PORT=5432
PG_USERNAME=postgres
PG_PASSWORD=
PG_USE_SSL=false
PG_DB_NAME=postgres
```

A template file `.env.example` is provided for reference.

#### Command Line Arguments

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
- Bottom: List of available backups
- Top: Configuration and status information

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

## Snapshot Testing

This project uses [insta](https://insta.rs/) for snapshot testing. Snapshot tests help ensure UI components and data structures maintain consistent behavior across code changes.

### Running Snapshot Tests

```bash
# Run all tests including snapshot tests
cargo test

# Run only snapshot tests
cargo test --test browser_tests
cargo test --test renderer_tests
```

### Updating Snapshots

When you make intentional changes to the UI or data structures, snapshot tests will fail. To update the snapshots:

1. Run the tests to generate new snapshot files:
   ```bash
   cargo test
   ```

2. Review and accept the changes using the insta review tool:
   ```bash
   cargo insta review
   ```
   This will open an interactive interface to review and accept/reject changes.

3. Alternatively, you can automatically accept all changes:
   ```bash
   cargo insta accept
   ```

### Adding New Snapshot Tests

To add a new snapshot test:

1. Import the necessary components:
   ```rust
   use insta::assert_debug_snapshot;
   ```

2. Create a test function and use the snapshot assertion:
   ```rust
   #[test]
   fn test_my_component() {
       let component = MyComponent::new();
       assert_debug_snapshot!(component);
   }
   ```

3. Run the test once to generate the initial snapshot.

Snapshot files are stored in the `tests/snapshots/` directory and should be committed to version control.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
