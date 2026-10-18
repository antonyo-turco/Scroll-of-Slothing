//! `sleuth` — scriptable access to the service layer. Every command prints a
//! human summary, or JSON with `--json` (stable, for scripts and agents).

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{Map, Value};
use sleuth_core::settings::SttEngine;
use sleuth_core::stt::{Correction, ExportFormat};
use sleuth_service::{
    AssetPatch, AssetQuery, AssetSort, CloudKeyStatus, ExportWhat, ImportFile, JobState,
    LegacyImportOptions, Service, TranscribeOptions, TranscribeResult,
};

#[derive(Parser)]
#[command(name = "sleuth", version, about = "Kastanomata's Scroll of Sleuthing CLI")]
struct Cli {
    /// Data directory (default: $SLEUTH_DATA_DIR or %APPDATA%\sleuth).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Import data from Orb of Many Functions (read-only on the source; idempotent).
    ImportLegacy {
        /// Path to the old db/creatures.db
        db: PathBuf,
        /// Old app_settings.json
        #[arg(long)]
        settings: Option<PathBuf>,
        /// Old aoe_presets.json
        #[arg(long)]
        aoe: Option<PathBuf>,
    },
    /// Asset library.
    #[command(subcommand)]
    Library(LibraryCmd),
    /// Tags.
    #[command(subcommand)]
    Tags(TagsCmd),
    /// Settings.
    #[command(subcommand)]
    Settings(SettingsCmd),
    /// Speech-to-text models.
    #[command(subcommand)]
    Stt(SttCmd),
    /// Transcribe an audio file with the local engine.
    Transcribe {
        file: PathBuf,
        /// local | cloud (default: the `stt_engine` setting).
        #[arg(long)]
        engine: Option<String>,
        /// Local: a model id (see `sleuth stt models`). Cloud: an OpenRouter model id.
        #[arg(long)]
        model: Option<String>,
        /// Language code, or `auto`.
        #[arg(long, default_value = "it")]
        language: String,
        /// Output format: txt | md | srt | vtt.
        #[arg(long, default_value = "txt")]
        format: String,
        /// Write here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// A term to spell right (repeatable), e.g. `--glossary "hit points"`.
        #[arg(long = "glossary")]
        glossary: Vec<String>,
        /// A correction applied afterwards (repeatable), e.g. `--correct "bioldar=beholder"`.
        #[arg(long = "correct")]
        corrections: Vec<String>,
        /// Beam width; 1 is greedy (faster).
        #[arg(long)]
        beam: Option<u32>,
        /// CPU threads (default: the engine's choice).
        #[arg(long)]
        threads: Option<u32>,
    },
    /// Print the data directory in use.
    Info,
}

#[derive(Subcommand)]
enum SttCmd {
    /// List models and whether they are installed.
    Models,
    /// Download a model.
    Download { id: String },
    /// Delete a downloaded model.
    Delete { id: String },
    /// The OpenRouter API key for cloud transcription.
    #[command(subcommand)]
    Key(KeyCmd),
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Whether a key is available (the key itself is never printed).
    Status,
    /// Save a key in the Windows Credential Manager. Reads it from stdin, so it
    /// stays out of the shell history.
    Set,
    /// Remove the saved key.
    Clear,
    /// Check the key against the API.
    Test,
}

#[derive(Subcommand)]
enum LibraryCmd {
    /// Search assets.
    Search {
        query: Option<String>,
        #[arg(long = "kind")]
        kinds: Vec<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long)]
        pinned: bool,
        #[arg(long, default_value = "newest")]
        sort: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Show one asset.
    Get { id: i64 },
    /// Import image files as assets.
    Import {
        files: Vec<PathBuf>,
        #[arg(long, default_value = "token")]
        kind: String,
        #[arg(long)]
        size: Option<String>,
    },
    /// Edit metadata.
    Update {
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        size: Option<String>,
        #[arg(long)]
        pinned: Option<bool>,
        /// Replace tags (comma separated).
        #[arg(long)]
        tags: Option<String>,
    },
    /// Export a file of the asset (original | rendered | pdf).
    Export {
        id: i64,
        #[arg(long, default_value = "rendered")]
        what: String,
        out: PathBuf,
    },
    /// Delete assets.
    Delete { ids: Vec<i64> },
}

#[derive(Subcommand)]
enum TagsCmd {
    List,
    Rename { id: i64, name: String },
    Merge { src: i64, dst: i64 },
    Delete { id: i64 },
}

#[derive(Subcommand)]
enum SettingsCmd {
    Get,
    /// Set a value, e.g. `sleuth settings set map_square_in 1.5`.
    Set {
        key: String,
        value: String,
    },
    Reset,
}

fn print<T: Serialize + std::fmt::Debug>(json: bool, v: &T, human: impl FnOnce(&T)) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(v)?);
    } else {
        human(v);
    }
    Ok(())
}

/// Waits for a job, drawing its progress on stderr. Returns the job result.
async fn wait_job(svc: &Service, id: &str) -> Result<Value> {
    let mut last = String::new();
    loop {
        let st = svc.job_status(id)?;
        let line = format!("{:>3.0}% {}", st.progress * 100.0, st.message);
        if line != last {
            eprint!("\r{line:<60}");
            last = line;
        }
        match st.state {
            JobState::Running => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            JobState::Done => {
                eprintln!();
                return Ok(st.result.unwrap_or(Value::Null));
            }
            JobState::Cancelled => anyhow::bail!("cancelled"),
            JobState::Failed => {
                eprintln!();
                anyhow::bail!("{}", st.error.unwrap_or_default())
            }
        }
    }
}

fn parse_sort(s: &str) -> Result<AssetSort> {
    serde_json::from_value(Value::String(s.replace('-', "_")))
        .with_context(|| format!("unknown sort {s:?} (newest|oldest|name_asc|name_desc|kind|size)"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let cli = Cli::parse();
    let dir = cli.data_dir.clone().unwrap_or_else(sleuth_service::default_data_dir);
    let svc = Service::open(&dir).with_context(|| format!("opening data dir {}", dir.display()))?;
    let j = cli.json;

    match cli.cmd {
        Cmd::Info => {
            print(j, &serde_json::json!({"data_dir": dir}), |_| println!("{}", dir.display()))?
        }
        Cmd::ImportLegacy { db, settings, aoe } => {
            eprintln!("Importing from {} (read-only)…", db.display());
            let r = svc
                .legacy_import_blocking(LegacyImportOptions {
                    db_path: db,
                    settings_path: settings,
                    aoe_presets_path: aoe,
                })
                .await?;
            print(j, &r, |r| {
                println!("Assets imported: {:?}", r.assets);
                println!("Already present (skipped): {}", r.skipped_existing);
                println!(
                    "Campaigns: {}  Sessions: {}  Tag links: {}",
                    r.campaigns, r.sessions, r.tag_links
                );
                println!(
                    "Links: {} campaign, {} session",
                    r.asset_campaign_links, r.asset_session_links
                );
                if let Some(p) = &r.music_json {
                    println!("Music data preserved in {}", p.display());
                }
                for f in &r.failed {
                    println!("FAILED #{} {}: {}", f.legacy_id, f.name, f.error);
                }
            })?;
        }
        Cmd::Stt(c) => match c {
            SttCmd::Models => print(j, &svc.stt_models_list().await?, |ms| {
                for m in ms {
                    let mark = if m.installed { "installed" } else { "" };
                    println!("{:<22} {:>6} MB  {:<42} {mark}", m.id, m.size / 1_048_576, m.label);
                }
            })?,
            SttCmd::Download { id } => {
                let job = svc.stt_model_download(id)?;
                let r = wait_job(&svc, &job).await?;
                print(j, &r, |r| println!("Saved {}", r["path"].as_str().unwrap_or_default()))?;
            }
            SttCmd::Delete { id } => {
                svc.stt_model_delete(id).await?;
                print(j, &Value::Null, |_| println!("Deleted"))?;
            }
            SttCmd::Key(k) => {
                let human = |s: &CloudKeyStatus| match (&s.hint, s.source) {
                    (Some(h), Some(src)) => println!("Key {h} ({src})"),
                    _ => println!("No key"),
                };
                match k {
                    KeyCmd::Status => print(j, &svc.stt_cloud_key_status().await?, human)?,
                    KeyCmd::Set => {
                        eprint!("Paste the OpenRouter API key and press Enter: ");
                        let mut key = String::new();
                        std::io::stdin().read_line(&mut key)?;
                        print(j, &svc.stt_cloud_key_set(key).await?, human)?;
                    }
                    KeyCmd::Clear => print(j, &svc.stt_cloud_key_clear().await?, human)?,
                    KeyCmd::Test => print(j, &svc.stt_cloud_key_test().await?, |k| {
                        let usd =
                            |v: Option<f64>| v.map_or("unlimited".into(), |x| format!("${x:.2}"));
                        println!(
                            "OK: {} · used {} · limit {}",
                            k.label.as_deref().unwrap_or("key"),
                            usd(k.usage),
                            usd(k.limit)
                        );
                    })?,
                }
            }
        },
        Cmd::Transcribe {
            file,
            engine,
            model,
            language,
            format,
            out,
            glossary,
            corrections,
            beam,
            threads,
        } => {
            let corrections = corrections
                .iter()
                .map(|c| match c.split_once('=') {
                    Some((from, to)) => Ok(Correction { from: from.into(), to: to.into() }),
                    None => anyhow::bail!("--correct needs from=to, got {c:?}"),
                })
                .collect::<Result<Vec<_>>>()?;
            let fmt: ExportFormat = serde_json::from_value(Value::String(format.clone()))
                .with_context(|| format!("unknown format {format:?} (txt|md|srt|vtt)"))?;
            let title = file.file_stem().map(|s| s.to_string_lossy().into_owned());
            let engine = match engine.as_deref() {
                None => None,
                Some("local") => Some(SttEngine::Local),
                Some("cloud") => Some(SttEngine::Cloud),
                Some(e) => anyhow::bail!("unknown engine {e:?} (local|cloud)"),
            };
            let opts = TranscribeOptions {
                engine,
                model,
                language: Some(language),
                glossary,
                corrections,
                beam_size: beam,
                threads,
            };
            let job = svc.transcribe_file(file, opts)?;
            let r: TranscribeResult = serde_json::from_value(wait_job(&svc, &job).await?)?;
            if let Some(c) = r.cost {
                eprintln!("Cost: ${c:.4}");
            }
            if j {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                let text = svc.transcript_export(r.segments, fmt, title, out.clone()).await?;
                match out {
                    Some(p) => eprintln!("Wrote {}", p.display()),
                    None => print!("{text}"),
                }
            }
        }
        Cmd::Library(c) => match c {
            LibraryCmd::Search { query, kinds, tags, pinned, sort, limit } => {
                let page = svc
                    .library_search(AssetQuery {
                        q: query,
                        kinds,
                        tags,
                        campaign_id: None,
                        session_id: None,
                        pinned_only: pinned,
                        sort: parse_sort(&sort)?,
                        limit: Some(limit),
                        offset: None,
                    })
                    .await?;
                print(j, &page, |p| {
                    for a in &p.items {
                        let pin = if a.pinned { "★" } else { " " };
                        println!(
                            "{pin} {:>5}  {:<7} {:<10} {}  [{}]",
                            a.id,
                            a.kind,
                            a.size_label,
                            a.name,
                            a.tags.join(", ")
                        );
                    }
                    println!("{} of {} shown", p.items.len(), p.total);
                })?;
            }
            LibraryCmd::Get { id } => {
                print(j, &svc.library_get(id).await?, |a| println!("{a:#?}"))?
            }
            LibraryCmd::Import { files, kind, size } => {
                let r = svc
                    .library_import(
                        files
                            .into_iter()
                            .map(|path| ImportFile {
                                path,
                                name: None,
                                kind: kind.clone(),
                                size: size.clone(),
                            })
                            .collect(),
                    )
                    .await?;
                print(j, &r, |r| {
                    println!("Imported {} asset(s): {:?}", r.imported.len(), r.imported);
                    for f in &r.failed {
                        println!("FAILED {}: {}", f.path.display(), f.error);
                    }
                })?;
            }
            LibraryCmd::Update { id, name, size, pinned, tags } => {
                let mut a = svc
                    .library_update(
                        id,
                        AssetPatch { name, aliases: None, size_label: size, pinned },
                    )
                    .await?;
                if let Some(t) = tags {
                    let list = t
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    a = svc.library_set_tags(id, list).await?;
                }
                print(j, &a, |a| println!("Updated #{} {}", a.id, a.name))?;
            }
            LibraryCmd::Export { id, what, out } => {
                let what: ExportWhat = serde_json::from_value(Value::String(what.clone()))
                    .with_context(|| format!("unknown export {what:?} (original|rendered|pdf)"))?;
                let p = svc.library_export(id, what, out).await?;
                print(j, &p, |p| println!("Wrote {}", p.display()))?;
            }
            LibraryCmd::Delete { ids } => {
                let n = svc.library_delete(ids).await?;
                print(j, &n, |n| println!("Deleted {n} asset(s)"))?;
            }
        },
        Cmd::Tags(c) => match c {
            TagsCmd::List => print(j, &svc.tags_list().await?, |ts| {
                for t in ts {
                    println!("{:>5}  {:<24} {}", t.id, t.name, t.count);
                }
            })?,
            TagsCmd::Rename { id, name } => {
                print(j, &svc.tags_rename(id, name).await?, |r| match r.merged_into {
                    Some(d) => println!("Merged into tag #{d}"),
                    None => println!("Renamed"),
                })?
            }
            TagsCmd::Merge { src, dst } => {
                svc.tags_merge(src, dst).await?;
                print(j, &true, |_| println!("Merged #{src} into #{dst}"))?;
            }
            TagsCmd::Delete { id } => {
                svc.tags_delete(id).await?;
                print(j, &true, |_| println!("Deleted tag #{id}"))?;
            }
        },
        Cmd::Settings(c) => match c {
            SettingsCmd::Get => print(j, &svc.settings_get().await?, |s| println!("{s:#?}"))?,
            SettingsCmd::Reset => {
                print(j, &svc.settings_reset().await?, |_| println!("Settings reset"))?
            }
            SettingsCmd::Set { key, value } => {
                // Accept JSON literals (numbers, booleans) and fall back to a plain string.
                let v = serde_json::from_str::<Value>(&value).unwrap_or(Value::String(value));
                let mut patch = Map::new();
                patch.insert(key, v);
                let up = svc.settings_set(patch).await?;
                if !up.errors.is_empty() {
                    anyhow::bail!("invalid setting: {:?}", up.errors);
                }
                print(j, &up.settings, |_| println!("Saved"))?;
            }
        },
    }
    Ok(())
}
