use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use muse_app::{App, Command, Config};
use muse_audio::{Player, Sequence, SequenceOptions};

/// Tablature, notation and chant editor and player.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Files to open.
    files: Vec<PathBuf>,
    /// Use the terminal interface instead of the GUI.
    #[arg(long)]
    tui: bool,
    /// Render one GUI frame to this PNG file and exit (keys from --keys are applied first).
    #[arg(long, hide = true)]
    screenshot: Option<PathBuf>,
    /// Keys to feed before a --screenshot, in key notation (e.g. "J4}<C-w>v").
    #[arg(long, hide = true)]
    keys: Option<String>,
    /// Config file (default: $XDG_CONFIG_HOME/muse/config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print tracks, tunings, tempo and bar count.
    Info {
        file: PathBuf,
        /// Dump the full score model as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Convert between formats, chosen by the output file's extension. Engraved output
    /// (.pdf, .svg, .png) shows one track.
    Convert {
        input: PathBuf,
        output: PathBuf,
        /// Track to engrave (1-based) for .pdf/.svg/.png.
        #[arg(long, default_value_t = 1)]
        track: usize,
        /// Engraving layout: page, vertical or horizontal.
        #[arg(long, default_value = "page")]
        layout: String,
        /// Pixels per point for .png output.
        #[arg(long, default_value_t = 2.0)]
        scale: f64,
    },
    /// Play a file without the interface.
    Play {
        file: PathBuf,
        /// Playback speed multiplier.
        #[arg(long)]
        speed: Option<f64>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli
        .config
        .clone()
        .or_else(muse_app::config::default_config_path);
    let (config, warnings) = match &config_path {
        Some(p) => Config::load(p).map_err(anyhow::Error::msg)?,
        None => (Config::default(), vec![]),
    };
    match cli.command {
        Some(Cmd::Info { file, json }) => info(&file, json),
        Some(Cmd::Play { file, speed }) => play(&file, &config, speed),
        Some(Cmd::Convert {
            input,
            output,
            track,
            layout,
            scale,
        }) => {
            let score = load(&input)?;
            let ext = output
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "pdf" | "svg" | "png") {
                return export_engraving(&score, &output, &ext, track, &layout, scale);
            }
            muse_formats::write_file(&score, &output)
                .with_context(|| format!("writing {}", output.display()))?;
            eprintln!("wrote {}", output.display());
            Ok(())
        }
        None => {
            let mut app = App::new(config, config_path);
            if !warnings.is_empty() {
                app.error(warnings.join("; "));
            }
            for (i, f) in cli.files.iter().enumerate() {
                app.run(if i == 0 {
                    Command::Edit(f.clone())
                } else {
                    Command::TabNew(Some(f.clone()))
                });
            }
            if cli.files.len() > 1 {
                app.run(Command::TabNext);
            }
            if let Some(out) = &cli.screenshot {
                if let Some(keys) = &cli.keys {
                    for k in muse_app::keys::parse_keys(keys, &app.config.leader.clone())
                        .map_err(anyhow::Error::msg)?
                    {
                        app.handle_key(k);
                    }
                }
                std::fs::write(out, muse_gui::screenshot(app, 1400, 900)?)?;
                return Ok(());
            }
            let has_display = std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var_os("DISPLAY").is_some()
                || cfg!(not(target_os = "linux"));
            if cli.tui || !has_display {
                return muse_tui::run(app);
            }
            muse_gui::run(app)
        }
    }
}

/// Writes engraved notation; multi-page SVG/PNG output gets one file per page (`name-N.ext`).
fn export_engraving(
    score: &muse_core::Score,
    output: &Path,
    ext: &str,
    track: usize,
    layout: &str,
    scale: f64,
) -> Result<()> {
    let index = track.checked_sub(1).filter(|&t| t < score.tracks.len());
    let Some(index) = index else {
        bail!(
            "track {track} does not exist ({} tracks)",
            score.tracks.len()
        )
    };
    let mode = match layout {
        "page" => muse_engrave::Mode::Page {
            width: 595.0,
            height: 842.0,
        },
        "vertical" => muse_engrave::Mode::Vertical { width: 842.0 },
        "horizontal" => muse_engrave::Mode::Horizontal,
        other => bail!("unknown layout '{other}' (page, vertical, horizontal)"),
    };
    let engraved = muse_engrave::engrave(
        score,
        index,
        &muse_engrave::Options {
            mode,
            ..Default::default()
        },
    );
    let pages = &engraved.pages;
    let path_for = |i: usize| -> PathBuf {
        if pages.len() == 1 {
            return output.to_path_buf();
        }
        let stem = output.file_stem().unwrap_or_default().to_string_lossy();
        output.with_file_name(format!("{stem}-{}.{ext}", i + 1))
    };
    match ext {
        "pdf" => std::fs::write(output, muse_engrave::output::to_pdf(pages))?,
        "svg" => {
            for (i, p) in pages.iter().enumerate() {
                std::fs::write(path_for(i), muse_engrave::output::to_svg(p))?;
            }
        }
        _ => {
            for (i, p) in pages.iter().enumerate() {
                let png = muse_engrave::output::to_png(p, scale).map_err(anyhow::Error::msg)?;
                std::fs::write(path_for(i), png)?;
            }
        }
    }
    eprintln!(
        "wrote {} ({} page{})",
        output.display(),
        pages.len(),
        if pages.len() == 1 { "" } else { "s" }
    );
    Ok(())
}

fn load(path: &Path) -> Result<muse_core::Score> {
    muse_formats::read_file(path).with_context(|| format!("reading {}", path.display()))
}

fn info(path: &Path, json: bool) -> Result<()> {
    let score = load(path)?;
    if json {
        let mut out = std::io::stdout().lock();
        return match serde_json::to_writer_pretty(&mut out, &score) {
            Err(e) if e.io_error_kind() == Some(std::io::ErrorKind::BrokenPipe) => Ok(()),
            other => Ok(other?),
        };
    }
    let mut out = String::new();
    let m = &score.meta;
    let seq = Sequence::build(&score, &SequenceOptions::default());
    for (label, value) in [
        ("Title", &m.title),
        ("Subtitle", &m.subtitle),
        ("Artist", &m.artist),
        ("Album", &m.album),
        ("Tab", &m.tab_author),
    ] {
        if !value.is_empty() {
            writeln!(out, "{label:<9} {value}")?;
        }
    }
    if let Some(v) = score.extras.get("gp.version") {
        writeln!(
            out,
            "{:<9} Guitar Pro {}.{:02}",
            "Format",
            v.parse::<u32>().unwrap_or(0) / 100,
            v.parse::<u32>().unwrap_or(0) % 100
        )?;
    }
    writeln!(out, "{:<9} {}", "Tempo", score.tempo)?;
    writeln!(
        out,
        "{:<9} {} ({} played with repeats)",
        "Bars",
        score.master_bars.len(),
        seq.bars().len()
    )?;
    let secs = seq.duration as u64;
    writeln!(out, "{:<9} {}:{:02}", "Length", secs / 60, secs % 60)?;
    writeln!(out, "Tracks:")?;
    for (i, t) in score.tracks.iter().enumerate() {
        let tuning: Vec<String> = t.strings.iter().rev().map(|&p| note_name(p)).collect();
        let kind = match t.kind {
            muse_core::TrackKind::Drums => "drums".to_string(),
            muse_core::TrackKind::Pitched => "pitched".to_string(),
            muse_core::TrackKind::Chant => "chant".to_string(),
            _ => format!("{} strings [{}]", t.strings.len(), tuning.join(" ")),
        };
        let capo = if t.capo > 0 {
            format!(" capo {}", t.capo)
        } else {
            String::new()
        };
        writeln!(
            out,
            "  {:>2}. {:<24} {kind}{capo}  program {}",
            i + 1,
            t.name.trim(),
            t.midi.program
        )?;
    }
    write_stdout(&out)
}

/// Writes to stdout, treating a closed pipe (e.g. `| head`) as success.
fn write_stdout(text: &str) -> Result<()> {
    use std::io::Write;
    match std::io::stdout().lock().write_all(text.as_bytes()) {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

fn note_name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NAMES[midi as usize % 12], midi as i32 / 12 - 1)
}

fn play(path: &Path, config: &Config, speed: Option<f64>) -> Result<()> {
    let score = load(path)?;
    if score.tracks.is_empty() {
        bail!("nothing to play");
    }
    let seq = Arc::new(Sequence::build(&score, &SequenceOptions::default()));
    let soundfont = config
        .file
        .audio
        .soundfont
        .clone()
        .or_else(muse_app::config::find_system_soundfont);
    let player = Player::new(soundfont.as_deref())?;
    if let Some(w) = &player.warning {
        eprintln!("warning: {w}");
    }
    eprintln!(
        "playing {} with {} (Ctrl-C to stop)",
        path.display(),
        player.synth_name
    );
    player.load(seq.clone());
    player.set_speed(speed.unwrap_or(config.file.audio.speed));
    player.play();
    // Give the audio thread a moment to start before polling its state.
    std::thread::sleep(std::time::Duration::from_millis(200));
    while player.is_playing() {
        let t = player.position();
        let bar = seq.location_at(t).map_or(0, |l| l.bar + 1);
        eprint!(
            "\r{}:{:02} / {}:{:02}  bar {bar}   ",
            t as u64 / 60,
            t as u64 % 60,
            seq.duration as u64 / 60,
            seq.duration as u64 % 60
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!();
    Ok(())
}
