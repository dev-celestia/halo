//! `halo-cli` — every backend operation, from a terminal.
//!
//! The binary is a thin argument parser over [`halo_backend::commands`]. It owns
//! no logic of its own: if a command is missing here, the fix is to add it to
//! the backend command layer, not to this crate.

mod cli;
mod cmd;
mod context;
mod output;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{
    CertCommand, Cli, Command, HistoryCommand, NotesCommand, ProxyCommand, RepeaterCommand,
    SessionCommand, StorageCommand,
};
use crate::context::Ctx;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("halo-cli: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    // `__serve` is the detached proxy child; it must not re-parse as a normal
    // command, and it boots before the context so it can report bind failures.
    if let Command::Serve(args) = &cli.command {
        return cmd::proxy::serve(args.port, args.tls_port).await;
    }

    let ctx = Ctx::boot(cli.json)?;

    match cli.command {
        Command::Proxy(args) => match args.command {
            ProxyCommand::Start { port, tls_port, detach } => {
                cmd::proxy::start(&ctx, port, tls_port, detach).await
            }
            ProxyCommand::Stop => cmd::proxy::stop(&ctx).await,
            ProxyCommand::Status => cmd::proxy::status(&ctx).await,
        },
        Command::History(args) => match args.command {
            HistoryCommand::Ls {
                limit,
                method,
                status,
                host,
                search,
            } => cmd::history::ls(&ctx, limit, method, status, host, search).await,
            HistoryCommand::Show { id } => cmd::history::show(&ctx, &id).await,
            HistoryCommand::Rm { id } => cmd::history::remove(&ctx, &id).await,
            HistoryCommand::Clear { all, before, date } => {
                cmd::history::clear(&ctx, all, before.as_deref(), date.as_deref()).await
            }
        },
        Command::Session(args) => match args.command {
            SessionCommand::Ls => cmd::session::list(&ctx).await,
            SessionCommand::New { name } => cmd::session::create(&ctx, &name).await,
            SessionCommand::Use { id } => cmd::session::activate(&ctx, &id).await,
            SessionCommand::Clear { id } => cmd::session::clear(&ctx, &id).await,
        },
        Command::Cert(args) => match args.command {
            CertCommand::Show => cmd::cert::show(&ctx),
            CertCommand::Export { path } => cmd::cert::export(&ctx, &path),
            CertCommand::Trust => cmd::cert::trust(&ctx),
            CertCommand::Regenerate => cmd::cert::regenerate(&ctx),
        },
        Command::Storage(args) => match args.command {
            StorageCommand::Info => cmd::storage::info(&ctx),
            StorageCommand::Prune { artifact } => cmd::storage::prune(&ctx, &artifact),
            StorageCommand::Reset => cmd::storage::reset(&ctx).await,
        },
        Command::Repeater(args) => match args.command {
            RepeaterCommand::Send {
                raw,
                method,
                url,
                headers,
                body,
            } => cmd::repeater::send(&ctx, raw, method, url, headers, body).await,
        },
        Command::Scan(args) => {
            cmd::scan::run(
                &ctx,
                args.target,
                args.ports,
                args.timeout_ms,
                args.concurrency,
                args.banner,
            )
            .await
        }
        Command::Library(args) => cmd::library::list(&ctx, args.kind, args.limit).await,
        Command::Notes(args) => match args.command {
            NotesCommand::Ls => cmd::notes::list(&ctx).await,
            NotesCommand::Rm { id } => cmd::notes::remove(&ctx, &id).await,
            NotesCommand::Import { file } => cmd::notes::import(&ctx, &file).await,
        },
        Command::Serve(_) => unreachable!("handled above"),
    }
}
