# LogScope

LogScope is a Rust-based log analysis tool with both CLI and TUI workflows. It can parse plain text logs, JSON line logs, and Spring/Logback-style logs, then generate statistics, searchable results, operational insights, and exportable reports.

## Features

- Parse multiple log files in one run.
- Auto-detect JSON content even when the file extension is `.log`.
- Support plain text lines in the form `timestamp level source message`.
- Support JSON lines with `timestamp`, `level`, `source`, and `message` fields.
- Support date-less Spring/Logback lines such as `15:19:18.955 [main] INFO  app.Service - [run,52] - started`.
- Extract structured `key=value` fields from messages, including values such as `duration_ms`, `request_id`, and `job_id`.
- Analyze totals, level counts, source counts, top sources, repeated error patterns, slow requests, correlated activity, and per-file severity.
- Search by keyword, level, source, and time range.
- Export reports as Markdown, JSON, or HTML.
- Provide an interactive TUI with log browsing, filtering, file picker, report preview, and HTML export.

## Requirements

- Rust toolchain with Cargo.
- Windows, macOS, or Linux terminal for CLI usage.
- A terminal with ANSI support for the TUI.

## Build

```powershell
cargo build
```

For a release build:

```powershell
cargo build --release
```

## Run

Show the command list:

```powershell
cargo run -- --help
```

Analyze a plain text log:

```powershell
cargo run -- analyze samples/plain.log --parser text
```

Analyze multiple files with automatic parser detection:

```powershell
cargo run -- analyze samples/plain.log samples/json.log samples/worker.log --parser auto
```

Search logs:

```powershell
cargo run -- search samples/worker.log --keyword timeout
cargo run -- search samples/worker.log --level error
cargo run -- search samples/worker.log --source worker
```

Search by time range:

```powershell
cargo run -- search samples/worker.log --start 2026-06-24T10:00:00Z --end 2026-06-24T10:06:30Z
```

Export a report:

```powershell
cargo run -- report samples/worker.log --format markdown --output report.md
cargo run -- report samples/worker.log --format json --output report.json
cargo run -- report samples/worker.log --format html --output report.html
```

Start the TUI:

```powershell
cargo run -- tui
```

Start the TUI with initial files:

```powershell
cargo run -- tui samples/worker.log samples/json.log --parser auto
```

## TUI Controls

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move selected log entry |
| `/` | Start keyword search |
| `e` | Show `ERROR` and `FATAL` entries |
| `w` | Show `WARN`, `ERROR`, and `FATAL` entries |
| `c` | Clear filters |
| `o` | Open file picker |
| `Space` | Mark a file in the file picker |
| `Enter` | Load selected or marked file(s) |
| `Backspace` | Go to parent directory in the file picker |
| `r` | Export current loaded data to `logscope-tui-report.html` |
| `q` / `Esc` | Request quit, press again to confirm |

The TUI can load multiple files at once. Each parsed entry is tagged with its physical source file, so summaries and reports can show file-level activity.

## Supported Log Formats

Plain text:

```text
2026-06-12T10:00:00Z INFO api service started
2026-06-12T10:01:00Z WARN worker retry scheduled
2026-06-12T10:02:00Z ERROR api database timeout duration_ms=1500
```

JSON line:

```json
{"timestamp":"2026-06-12T10:00:00Z","level":"INFO","source":"api","message":"service started"}
{"timestamp":"2026-06-12T10:02:00Z","level":"ERROR","source":"worker","message":"database timeout duration_ms=1500"}
```

Spring/Logback-style date-less log:

```text
15:19:18.955 [background-preinit] INFO  o.h.v.i.util.Version - [<clinit>,21] - Hibernate Validator started
```

Date-less logs keep their original display timestamp in the UI. Internally, LogScope uses a synthetic date only for sorting.

## Configuration File

Commands can read defaults from a TOML config file.

```toml
input = "samples/worker.log"
parser = "text"

[report]
path = "reports/summary.html"
format = "html"
```

Use it with:

```powershell
cargo run -- analyze --config logscope.toml
cargo run -- report --config logscope.toml
cargo run -- tui --config logscope.toml
```

Explicit CLI arguments take precedence over config values.

## Project Structure

```text
src/
  analyzer.rs       Log statistics, rankings, error patterns, slow requests, insights
  cli.rs            Clap command definitions and command execution
  config.rs         TOML configuration loading
  model.rs          Shared domain models
  parser.rs         Plain text, JSON line, and Spring/Logback parsing
  report/           Markdown, JSON, HTML report writers and report sections
  tui.rs            Terminal setup and event loop
  tui/app.rs        TUI state, filtering, file picker, export behavior
  tui/view.rs       TUI rendering and styling
  utils.rs          Safe file writing helper
tests/              Integration tests
samples/            Sample log files
docs/               Assignment notes and commit plan
```

## Testing

Format code:

```powershell
cargo fmt
```

Run tests:

```powershell
cargo test
```

Run Clippy with warnings treated as errors:

```powershell
cargo clippy -- -D warnings
```

The current test suite covers parser behavior, CLI flows, config loading, report generation, TUI state logic, Unicode rendering safeguards, and sample files.

## Notes

- Large files are parsed through streaming helpers where possible, then rendered in the TUI through a visible scroll window.
- Error pattern grouping normalizes dynamic values such as IP addresses, UUIDs, numeric IDs, and path segments.
- HTML reports are dependency-free and use embedded CSS for metric cards, bar charts, and donut charts.
