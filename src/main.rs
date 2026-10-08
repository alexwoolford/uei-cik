use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use uei_cik::db::{last_run, lookup, open, open_work};
use uei_cik::http::{LiveSources, DEFAULT_SLEEP_SECS};
use uei_cik::resolve::{apply_matches, record_error, resolve};
use uei_cik::sec_ua::validate_user_agent;
use uei_cik::time::utc_date;

#[derive(Parser)]
#[command(
    name = "uei-cik",
    about = "Exact USAspending recipient UEI to listed SEC CIK. A map, not a score.",
    version
)]
struct Cli {
    #[arg(
        long,
        global = true,
        default_value = "data/uei-cik.sqlite",
        env = "UEI_CIK_SQLITE"
    )]
    db: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Match listed CIKs to recipient UEIs on name and business-address state
    Match {
        #[arg(long, env = "EDGAR_SLEEP_SECS", default_value_t = DEFAULT_SLEEP_SECS)]
        sleep: f64,
        #[arg(long, env = "SEC_USER_AGENT")]
        sec_user_agent: Option<String>,
        /// Successful SEC and USAspending JSON. A rerun reads these files.
        #[arg(long, default_value = "data/http-cache")]
        cache_dir: PathBuf,
    },
    /// Last match_runs row
    Status,
    /// Live rows by UEI or CIK
    Lookup { query: String },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match run() {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("{err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Match {
            sleep,
            sec_user_agent,
            cache_dir,
        } => {
            let ua = validate_user_agent(sec_user_agent.as_deref().unwrap_or(""))?;
            let started = Utc::now();
            let as_of = utc_date(started);
            let db = open_work(&cli.db)?;
            let sources = LiveSources::new(&ua, sleep, cache_dir)?;
            match resolve(&sources) {
                Ok((matches, ciks_seen, recipients_seen)) => {
                    let finished = Utc::now();
                    let stats = apply_matches(
                        &db,
                        &as_of,
                        started,
                        finished,
                        ciks_seen,
                        recipients_seen,
                        &matches,
                    )?;
                    tracing::info!(
                        as_of_date = %as_of,
                        ciks_seen = stats.ciks_seen,
                        recipients_seen = stats.recipients_seen,
                        upserted = stats.matches_upserted,
                        retracted = stats.matches_retracted,
                        "match finished"
                    );
                    Ok(ExitCode::SUCCESS)
                }
                Err(err) => {
                    let finished = Utc::now();
                    record_error(&db, &as_of, started, finished)?;
                    Err(err)
                }
            }
        }
        Command::Status => {
            let conn = open(&cli.db)?;
            match last_run(&conn)? {
                None => println!("no match_runs"),
                Some(r) => println!(
                    "as_of_date={} status={} ciks_seen={} recipients_seen={} upserted={} retracted={} started_at={} finished_at={}",
                    r.as_of_date,
                    r.status,
                    r.ciks_seen,
                    r.recipients_seen,
                    r.matches_upserted,
                    r.matches_retracted,
                    r.started_at,
                    r.finished_at
                ),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Lookup { query } => {
            let conn = open(&cli.db)?;
            let rows = lookup(&conn, &query)?;
            if rows.is_empty() {
                println!("no matches");
            }
            for row in rows {
                println!(
                    "uei={} cik={} state={} sec_name={} recipient_name={}",
                    row.uei, row.cik, row.state, row.sec_name, row.recipient_name
                );
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
