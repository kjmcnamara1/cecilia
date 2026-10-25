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
    /// Open in a window (the graphical interface) instead of the terminal.
    #[arg(long)]
    gui: bool,
    /// The terminal interface (the default; kept for scripts that ask for it).
    #[arg(long, hide = true, conflicts_with = "gui")]
    tui: bool,
    /// Render one GUI frame to this PNG file and exit (keys from --keys are applied first).
    #[arg(long, hide = true)]
    screenshot: Option<PathBuf>,
    /// Display scale factor for --screenshot (e.g. 1.5 for a 150% scaled monitor); the image is
    /// 1400×900 logical pixels times this.
    #[arg(long, hide = true, default_value_t = 1.0)]
    scale_factor: f64,
    /// Workspace to use (by name, or a path to its .toml file); created if new. Default: the
    /// workspace used last, or "default".
    #[arg(long, short = 'W')]
    workspace: Option<String>,
    /// Mouse position (logical "X,Y") for --screenshot, to show hover effects.
    #[arg(long, hide = true)]
    hover: Option<String>,
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
        /// Track to engrave (1-based) for .pdf/.svg/.png, or to turn into chant for .gabc.
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
            if matches!(ext.as_str(), "wav" | "flac") {
                return export_audio(&score, &input, &output, &ext, &config);
            }
            if ext == "gabc" && score.tracks.iter().all(|t| t.chant.is_none()) {
                let index = track
                    .saturating_sub(1)
                    .min(score.tracks.len().saturating_sub(1));
                let r = muse_convert::chant::modern_to_chant(&score, index)
                    .map_err(anyhow::Error::msg)?;
                std::fs::write(
                    &output,
                    muse_formats::gabc::write(&r.chant, &score.meta.title),
                )?;
                eprintln!(
                    "wrote {} (chant from track {}; lossy: transposed {:+} semitones, {} notes rounded, {} chords reduced)",
                    output.display(),
                    index + 1,
                    r.transpose,
                    r.rounded,
                    r.chords
                );
                return Ok(());
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
            // There is always a workspace: the one asked for, else the last one used, else
            // "default". (Screenshots only use one when asked.)
            let interactive = cli.screenshot.is_none();
            if interactive || cli.workspace.is_some() {
                app.remember_workspace = interactive;
                let ws = cli
                    .workspace
                    .clone()
                    .or_else(muse_app::workspace::last_workspace)
                    .unwrap_or_else(|| muse_app::workspace::DEFAULT_WORKSPACE.into());
                app.use_workspace(&ws);
            }
            // Files given open as buffer tabs in it; the first one is shown.
            for f in &cli.files {
                app.run(Command::Edit(f.clone()));
            }
            if let Some(first) = cli.files.first().filter(|_| cli.files.len() > 1) {
                app.run(Command::Edit(first.clone()));
            }
            if let Some(out) = &cli.screenshot {
                if let Some(keys) = &cli.keys {
                    for k in muse_app::keys::parse_keys(keys, &app.config.leader.clone())
                        .map_err(anyhow::Error::msg)?
                    {
                        app.handle_key(k);
                    }
                }
                // No audio in a screenshot, but show a started song as playing.
                if app
                    .take_effects()
                    .iter()
                    .any(|e| matches!(e, muse_app::Effect::Play { .. }))
                {
                    app.set_playing(true);
                }
                let f = cli.scale_factor.max(0.1);
                let (w, h) = ((1400.0 * f).round() as u32, (900.0 * f).round() as u32);
                let hover = cli.hover.as_deref().and_then(|h| {
                    let (x, y) = h.split_once(',')?;
                    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
                });
                std::fs::write(out, muse_gui::screenshot(app, w, h, f, hover)?)?;
                return Ok(());
            }
            // The terminal interface by default; --gui opens a window.
            if !cli.gui {
                return muse_tui::run(app);
            }
            let has_display = std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var_os("DISPLAY").is_some()
                || cfg!(not(target_os = "linux"));
            if !has_display {
                anyhow::bail!("--gui: no display (WAYLAND_DISPLAY or DISPLAY is not set)");
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

/// Decodes a score's attached audio track (embedded, or a file relative to the score).
fn load_audio(score: &muse_core::Score, score_path: &Path) -> Option<muse_audio::AudioClip> {
    let audio = score.audio.as_ref()?;
    let ext = audio.file.as_deref().and_then(|f| f.rsplit('.').next());
    let result = match (&audio.embedded, &audio.file) {
        (Some(bytes), _) => muse_audio::AudioClip::decode((**bytes).clone(), ext),
        (None, Some(f)) => {
            muse_audio::AudioClip::load(&score_path.parent().unwrap_or(Path::new(".")).join(f))
        }
        _ => return None,
    };
    match result {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("warning: {e}");
            None
        }
    }
}

/// Renders the MIDI performance (mixed with the audio track, if any) to WAV or FLAC.
fn export_audio(
    score: &muse_core::Score,
    input: &Path,
    output: &Path,
    ext: &str,
    config: &Config,
) -> Result<()> {
    let rate = 44_100;
    let seq = Arc::new(Sequence::build(
        score,
        &SequenceOptions {
            audio_sync: score.audio.is_some(),
            ..Default::default()
        },
    ));
    let clip = load_audio(score, input).map(|c| Arc::new(c.resampled(rate)));
    let soundfont = config
        .file
        .audio
        .soundfont
        .clone()
        .or_else(muse_app::config::find_system_soundfont);
    let (synth, name, _) = muse_audio::make_synth(soundfont.as_deref(), rate);
    let length = seq
        .duration
        .max(clip.as_ref().map_or(0.0, |c| c.duration()))
        + 2.0;
    let mix = muse_audio::Mix {
        midi: if clip.is_some() { 0.6 } else { 1.0 },
        ..Default::default()
    };
    let samples = muse_audio::render_offline_with(
        seq,
        synth,
        rate,
        length,
        &muse_audio::RenderOptions {
            audio: clip,
            mix,
            metronome: false,
        },
    );
    let bytes = if ext == "wav" {
        muse_audio::encode_wav(&samples, rate)?
    } else {
        muse_audio::encode_flac(&samples, rate)?
    };
    std::fs::write(output, bytes)?;
    eprintln!("wrote {} ({:.0} s, {name})", output.display(), length);
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
    let seq = Arc::new(Sequence::build(
        &score,
        &SequenceOptions {
            audio_sync: score.audio.is_some(),
            ..Default::default()
        },
    ));
    let clip = load_audio(&score, path);
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
    if let Some(clip) = clip {
        eprintln!("with audio track ({:.0} s)", clip.duration());
        player.set_audio(Some(Arc::new(clip.resampled(player.sample_rate))));
        player.set_mix(muse_audio::Mix {
            midi: 0.6,
            ..Default::default()
        });
    }
    player.play(false);
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
