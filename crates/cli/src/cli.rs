//! Argument grammar. One enum variant per backend command group.

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "halo-cli",
    version,
    about = "Headless Halo — proxy, history and security tools from a terminal",
    long_about = "Every Halo backend operation, without the window. Commands share the \
                  desktop app's database, so history captured by either side is visible \
                  to both."
)]
pub struct Cli {
    /// Emit machine-readable JSON instead of a table.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start, stop or inspect the MITM proxy.
    Proxy(ProxyArgs),
    /// Browse and prune captured HTTP traffic.
    History(HistoryArgs),
    /// Manage capture sessions.
    Session(SessionArgs),
    /// MITM CA certificate: show, export, trust, regenerate.
    Cert(CertArgs),
    /// App data: disk usage, artifact pruning, full reset.
    Storage(StorageArgs),
    /// Send one crafted request and print the response.
    Repeater(RepeaterArgs),
    /// TCP connect scan against a host.
    Scan(ScanArgs),
    /// Saved requests: collections, stashes, endpoints, contexts, chronicle.
    Library(LibraryArgs),
    /// Scratch notes.
    Notes(NotesArgs),

    /// Internal: the detached proxy child. Not for direct use.
    #[command(hide = true)]
    Serve(ServeArgs),
}

// ── Proxy ──────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct ProxyArgs {
    #[command(subcommand)]
    pub command: ProxyCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProxyCommand {
    /// Start the proxy. Blocks until Ctrl-C unless --detach.
    Start {
        /// HTTP listener port.
        #[arg(short, long, default_value_t = 8888)]
        port: u16,
        /// HTTPS MITM listener port.
        #[arg(long, default_value_t = 8443)]
        tls_port: u16,
        /// Run in the background and return once the listener is ready.
        #[arg(short, long)]
        detach: bool,
    },
    /// Stop a detached proxy.
    Stop,
    /// Show whether the proxy is listening, and on which ports.
    Status,
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    #[arg(long)]
    pub port: u16,
    #[arg(long)]
    pub tls_port: u16,
}

// ── History ────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub command: HistoryCommand,
}

#[derive(Subcommand, Debug)]
pub enum HistoryCommand {
    /// List recent requests, newest first.
    Ls {
        /// Maximum rows to return.
        #[arg(short = 'n', long, default_value_t = 30)]
        limit: u32,
        /// Comma-separated methods, e.g. `GET,POST`.
        #[arg(short, long)]
        method: Option<String>,
        /// Comma-separated status codes or classes, e.g. `404,5xx`.
        #[arg(short, long)]
        status: Option<String>,
        /// Comma-separated host filter.
        #[arg(short = 'H', long)]
        host: Option<String>,
        /// Substring match on URL, path, headers or body.
        #[arg(short = 'q', long)]
        search: Option<String>,
    },
    /// Print one request and response in full.
    Show {
        /// Log id, as printed by `history ls`.
        id: String,
    },
    /// Delete one entry.
    Rm { id: String },
    /// Delete everything, or everything older than a window.
    Clear {
        /// Delete all captured traffic.
        #[arg(long, conflicts_with_all = ["before", "date"])]
        all: bool,
        /// Keep only traffic newer than this window: today, week, month, custom.
        #[arg(long, value_name = "WINDOW")]
        before: Option<String>,
        /// Date for `--before custom`, as YYYY-MM-DD.
        #[arg(long, value_name = "YYYY-MM-DD")]
        date: Option<String>,
    },
}

// ── Session ────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Subcommand, Debug)]
pub enum SessionCommand {
    /// List capture sessions, marking the active one.
    Ls,
    /// Create a session and make it active.
    New { name: String },
    /// Switch the active session.
    Use { id: String },
    /// Delete a session's captured traffic.
    Clear { id: String },
}

// ── Cert ───────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct CertArgs {
    #[command(subcommand)]
    pub command: CertCommand,
}

#[derive(Subcommand, Debug)]
pub enum CertCommand {
    /// Print the CA certificate in PEM form.
    Show,
    /// Write the CA certificate to a file.
    Export {
        /// Destination path.
        path: String,
    },
    /// Install the CA into the system trust store.
    Trust,
    /// Replace the CA (breaks clients that trusted the old one).
    Regenerate,
}

// ── Repeater ───────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct RepeaterArgs {
    #[command(subcommand)]
    pub command: RepeaterCommand,
}

#[derive(Subcommand, Debug)]
pub enum RepeaterCommand {
    /// Send a request built from flags, or from a raw HTTP file.
    Send {
        /// Raw HTTP request file: `METHOD URL HTTP/1.1`, headers, blank line, body.
        #[arg(long, value_name = "FILE")]
        raw: Option<String>,
        /// HTTP method (default GET).
        #[arg(short, long)]
        method: Option<String>,
        /// Target URL.
        #[arg(short, long)]
        url: Option<String>,
        /// Request header, repeatable: `-H "Accept: */*"`.
        #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
        headers: Vec<String>,
        /// Request body (curl-style `-d`).
        #[arg(short = 'd', long)]
        body: Option<String>,
    },
}

// ── Scan ───────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Host or IP to scan.
    pub target: String,
    /// Ports: `80`, `443,8000-8100`, `1-1024`.
    #[arg(short, long, default_value = "1-1024")]
    pub ports: String,
    /// Per-port connect timeout, milliseconds.
    #[arg(long, default_value_t = 500)]
    pub timeout_ms: u64,
    /// Parallel connections.
    #[arg(long, default_value_t = 100)]
    pub concurrency: u32,
    /// Read a service banner from open ports.
    #[arg(short, long)]
    pub banner: bool,
}

// ── Library ────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct LibraryArgs {
    /// Which saved-request table to list.
    #[arg(value_enum)]
    pub kind: LibraryKind,
    /// Row cap for `chronicle`.
    #[arg(short, long, default_value_t = 50)]
    pub limit: u32,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum LibraryKind {
    Collection,
    Stash,
    Endpoint,
    Context,
    Chronicle,
}

// ── Notes ──────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct NotesArgs {
    #[command(subcommand)]
    pub command: NotesCommand,
}

#[derive(Subcommand, Debug)]
pub enum NotesCommand {
    /// List notes.
    Ls,
    /// Delete one note.
    Rm { id: String },
    /// Import a JSON array of note records.
    Import { file: String },
}

// ── Storage ────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct StorageArgs {
    #[command(subcommand)]
    pub command: StorageCommand,
}

#[derive(Subcommand, Debug)]
pub enum StorageCommand {
    /// Show where data lives and how much space it takes.
    Info,
    /// Delete one artifact: database, regression_artifacts, log_file.
    Prune { artifact: String },
    /// Stop the proxy and wipe all local data.
    Reset,
}
