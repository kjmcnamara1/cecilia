//! The `cecilia` command line, shared by the two executables: `cecilia` (the terminal UI by
//! default, `--gui` for the window) and `ceciliaw` (always the window; on Windows a program of
//! the GUI subsystem, so a Start Menu shortcut opens no console window next to it).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use cecilia_app::{App, Command, Config};
use cecilia_audio::{Player, Sequence, SequenceOptions};
use clap::{Parser, Subcommand};

/// Tablature, notation and chant editor and player.
#[derive(Parser)]
#[command(name = "cecilia", version)]
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
    /// Config file (default: $XDG_CONFIG_HOME/cecilia/config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Set an option for this run, overriding the config file (repeatable; e.g. -o
    /// ui.tab_width=4 tabline.wrap=true). Use -- before file names that follow.
    #[arg(
        short = 'o',
        long = "option",
        value_name = "NAME=VALUE",
        num_args = 1..,
        global = true,
        value_parser = parse_assignment
    )]
    options: Vec<(String, String)>,
    #[command(subcommand)]
    command: Option<Cmd>,
}

/// Split `NAME=VALUE` on the first `=`; the value may be empty.
fn parse_assignment(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((name, value)) if !name.is_empty() => Ok((name.to_string(), value.to_string())),
        _ => Err("expected NAME=VALUE (file names go after --)".into()),
    }
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

/// Runs cecilia with the process's arguments; `ceciliaw` passes `force_gui` (it always opens
/// the window). Velopack's install, update and uninstall hooks run (and exit) before anything
/// else, and an update downloaded while running is handed to its updater on the way out.
pub fn main(force_gui: bool) -> Result<()> {
    cecilia_app::update::startup();
    let mut cli = if force_gui {
        // No console to print a usage error to: it becomes the error ceciliaw shows.
        match Cli::try_parse() {
            Ok(cli) => cli,
            Err(e) if e.use_stderr() => bail!("{e}"),
            Err(e) => e.exit(),
        }
    } else {
        Cli::parse()
    };
    if force_gui {
        cli.gui = true;
        cli.tui = false;
    }
    let result = run(cli);
    cecilia_app::update::apply_on_exit();
    result
}

/// Shows an error of `ceciliaw`, which has no console on Windows: in a message box there, on
/// stderr elsewhere.
pub fn show_error(error: &anyhow::Error) {
    let text = format!("{error:#}");
    #[cfg(windows)]
    cecilia_app::dialog::error_box("cecilia", &text);
    #[cfg(not(windows))]
    eprintln!("Error: {text}");
}

fn run(cli: Cli) -> Result<()> {
    // `info` needs no settings, so a broken config file does not stop it.
    if let Some(Cmd::Info { file, json }) = &cli.command {
        return info(file, *json);
    }
    let config_path = cli
        .config
        .clone()
        .or_else(cecilia_app::config::default_config_path);
    let (config, warnings) = match &config_path {
        Some(p) => Config::load(p, &cli.options).map_err(anyhow::Error::msg)?,
        None => Config::parse("", &cli.options).map_err(anyhow::Error::msg)?,
    };
    match cli.command {
        Some(Cmd::Info { .. }) => unreachable!("handled above"),
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
                return export_engraving(&score, &output, &ext, track, &layout, scale, &config);
            }
            if matches!(ext.as_str(), "wav" | "flac") {
                return export_audio(&score, &input, &output, &ext, &config);
            }
            if ext == "gabc" && !score.is_chant() {
                let index = track
                    .saturating_sub(1)
                    .min(score.tracks.len().saturating_sub(1));
                let r = cecilia_convert::chant::modern_to_chant(
                    &score,
                    index,
                    cecilia_core::melody::ChordPolicy::parse(&config.file.ui.chant_chords)
                        .unwrap_or_default(),
                )
                .map_err(anyhow::Error::msg)?;
                std::fs::write(
                    &output,
                    cecilia_formats::gabc::write(&r.chant, &score.meta.title),
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
            // Modern formats hold staff tracks: a chant score goes through the same conversion as
            // `:chant-to-modern` (its cached bars as a pitched track, syllables as lyrics).
            let score =
                if score.is_chant() && ext != "gabc" && !matches!(ext.as_str(), "ccc" | "ccm") {
                    score.chant_to_modern().unwrap_or(score)
                } else {
                    score
                };
            cecilia_formats::write_file(&score, &output)
                .with_context(|| format!("writing {}", output.display()))?;
            eprintln!("wrote {}", output.display());
            Ok(())
        }
        None => {
            let restore_last = config.file.ui.workspace_restore_last;
            let mut app = App::new(config, config_path);
            if cli.screenshot.is_none() {
                app.keep_notification_history();
                app.keep_command_history();
                app.keep_frecency();
            }
            if !warnings.is_empty() {
                app.error(warnings.join("; "));
            }
            // `cecilia` alone opens the workspace used last (else creates "default"; with
            // `ui.workspace_restore_last` off it shows the dashboard, with no workspace). Files
            // named on the command line open with no workspace, so one-off edits leave the
            // workspaces alone (`:workspace-save NAME` keeps such a session). `--workspace`
            // always uses that one. (Screenshots only use one when asked.)
            let interactive = cli.screenshot.is_none();
            app.remember_workspace = interactive;
            if interactive {
                app.load_recent();
            }
            if let Some(ws) = cli.workspace.clone() {
                app.open_workspace(&ws, true);
            } else if interactive && cli.files.is_empty() {
                if restore_last {
                    let ws = cecilia_app::workspace::last_workspace()
                        .unwrap_or_else(|| cecilia_app::workspace::DEFAULT_WORKSPACE.into());
                    app.open_workspace(&ws, true);
                } else {
                    app.show_dashboard();
                }
            }
            // Opening a workspace at startup is expected, not news (the notification stays in
            // the history).
            app.notifications
                .dismiss_where(|c| c.text.starts_with("workspace opened"));
            // Files given open as buffer tabs in it; the first one is shown.
            for f in &cli.files {
                app.run(Command::Edit(f.clone()));
            }
            if let Some(first) = cli.files.first().filter(|_| cli.files.len() > 1) {
                app.run(Command::Edit(first.clone()));
            }
            if let Some(out) = &cli.screenshot {
                if let Some(keys) = &cli.keys {
                    for k in cecilia_app::keys::parse_keys(keys, &app.config.leader.clone())
                        .map_err(anyhow::Error::msg)?
                    {
                        app.handle_key(k);
                    }
                }
                // No audio in a screenshot, but show a started song as playing.
                if app
                    .take_effects()
                    .iter()
                    .any(|e| matches!(e, cecilia_app::Effect::Play { .. }))
                {
                    app.set_playing(true);
                }
                let f = cli.scale_factor.max(0.1);
                let (w, h) = ((1400.0 * f).round() as u32, (900.0 * f).round() as u32);
                let hover = cli.hover.as_deref().and_then(|h| {
                    let (x, y) = h.split_once(',')?;
                    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
                });
                std::fs::write(out, cecilia_gui::screenshot(app, w, h, f, hover)?)?;
                return Ok(());
            }
            // At most once a day, as `ui.auto_update` says (off: no network at all).
            app.check_for_updates_on_start();
            // The terminal interface by default; --gui opens a window.
            if !cli.gui {
                return cecilia_tui::run(app);
            }
            let has_display = std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var_os("DISPLAY").is_some()
                || cfg!(not(target_os = "linux"));
            if !has_display {
                anyhow::bail!("--gui: no display (WAYLAND_DISPLAY or DISPLAY is not set)");
            }
            cecilia_gui::run(app)
        }
    }
}

/// Writes engraved notation; multi-page SVG/PNG output gets one file per page (`name-N.ext`).
fn export_engraving(
    score: &cecilia_core::Score,
    output: &Path,
    ext: &str,
    track: usize,
    layout: &str,
    scale: f64,
    config: &Config,
) -> Result<()> {
    let index = track.checked_sub(1).filter(|&t| t < score.tracks.len());
    let Some(index) = index else {
        bail!(
            "track {track} does not exist ({} tracks)",
            score.tracks.len()
        )
    };
    let mode = match layout {
        "page" => cecilia_engrave::Mode::Page {
            width: 595.0,
            height: 842.0,
        },
        "vertical" => cecilia_engrave::Mode::Vertical { width: 842.0 },
        "horizontal" => cecilia_engrave::Mode::Horizontal,
        other => bail!("unknown layout '{other}' (page, vertical, horizontal)"),
    };
    let mut options = cecilia_engrave::Options {
        mode,
        chant_font: cecilia_engrave::display::TextFamily::parse(&config.file.ui.chant_text_font)
            .unwrap_or_default(),
        ..Default::default()
    };
    // The page layout is the score's page setup: its size and margins.
    if layout == "page" {
        options = options.for_page(&score.page);
    }
    let engraved = cecilia_engrave::engrave(score, index, &options);
    let pages = &engraved.pages;
    let path_for = |i: usize| -> PathBuf {
        if pages.len() == 1 {
            return output.to_path_buf();
        }
        let stem = output.file_stem().unwrap_or_default().to_string_lossy();
        output.with_file_name(format!("{stem}-{}.{ext}", i + 1))
    };
    match ext {
        "pdf" => std::fs::write(output, cecilia_engrave::output::to_pdf(pages))?,
        "svg" => {
            for (i, p) in pages.iter().enumerate() {
                std::fs::write(path_for(i), cecilia_engrave::output::to_svg(p))?;
            }
        }
        _ => {
            for (i, p) in pages.iter().enumerate() {
                let png = cecilia_engrave::output::to_png(p, scale).map_err(anyhow::Error::msg)?;
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

/// The audio file a score names, if it is inside the score's folder: an absolute path or one
/// climbing out with `..` (which a downloaded score could use to read any file) is refused.
/// Windows-style separators in the stored name are accepted.
fn audio_path(score_path: &Path, name: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel: PathBuf = name.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if name.starts_with(['/', '\\'])
        || name.contains(':')
        || rel
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    Some(score_path.parent().unwrap_or(Path::new(".")).join(rel))
}

/// Decodes a score's attached audio track (embedded, or a file relative to the score).
fn load_audio(score: &cecilia_core::Score, score_path: &Path) -> Option<cecilia_audio::AudioClip> {
    let audio = score.audio.as_ref()?;
    let ext = audio.file.as_deref().and_then(|f| f.rsplit('.').next());
    let result = match (&audio.embedded, &audio.file) {
        (Some(bytes), _) => cecilia_audio::AudioClip::decode((**bytes).clone(), ext),
        (None, Some(f)) => match audio_path(score_path, f) {
            Some(p) => cecilia_audio::AudioClip::load(&p),
            None => {
                eprintln!("warning: audio file {f} is outside the score's folder; not loaded");
                return None;
            }
        },
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
    score: &cecilia_core::Score,
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
        .or_else(cecilia_app::config::find_system_soundfont);
    let (synth, name, warning) = cecilia_audio::make_synth(soundfont.as_deref(), rate);
    if let Some(w) = warning {
        eprintln!("warning: {w}");
    }
    let length = seq
        .duration
        .max(clip.as_ref().map_or(0.0, |c| c.duration()))
        + 2.0;
    let mix = cecilia_audio::Mix {
        midi: if clip.is_some() { 0.6 } else { 1.0 },
        ..Default::default()
    };
    let samples = cecilia_audio::render_offline_with(
        seq,
        synth,
        rate,
        length,
        &cecilia_audio::RenderOptions {
            audio: clip,
            mix,
            metronome: false,
        },
    );
    let bytes = if ext == "wav" {
        cecilia_audio::encode_wav(&samples, rate)?
    } else {
        cecilia_audio::encode_flac(&samples, rate)?
    };
    std::fs::write(output, bytes)?;
    eprintln!("wrote {} ({:.0} s, {name})", output.display(), length);
    Ok(())
}

fn load(path: &Path) -> Result<cecilia_core::Score> {
    cecilia_formats::read_file(path).with_context(|| format!("reading {}", path.display()))
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
            cecilia_core::TrackKind::Drums => "drums".to_string(),
            cecilia_core::TrackKind::Pitched => "pitched".to_string(),
            cecilia_core::TrackKind::Chant => "chant".to_string(),
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
        .or_else(cecilia_app::config::find_system_soundfont);
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
        player.set_mix(cecilia_audio::Mix {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_files_stay_inside_the_score_folder() {
        let score = Path::new("/music/song.gp");
        assert_eq!(
            audio_path(score, "take.mp3"),
            Some(PathBuf::from("/music/take.mp3"))
        );
        assert_eq!(
            audio_path(score, "audio\\take.mp3"),
            Some(PathBuf::from("/music/audio/take.mp3"))
        );
        assert_eq!(audio_path(score, "/etc/passwd"), None);
        assert_eq!(audio_path(score, "../secret.wav"), None);
        assert_eq!(audio_path(score, "a/../../secret.wav"), None);
        assert_eq!(audio_path(score, "C:\\x.wav"), None);
    }

    #[test]
    fn options_then_double_dash_then_files() {
        let cli = Cli::try_parse_from([
            "cecilia",
            "-o",
            "tabline.wrap=true",
            "ui.tab_width=4",
            "-W",
            "foo",
            "--",
            "song.cc",
            "tab.gp",
        ])
        .unwrap();
        assert_eq!(
            cli.options,
            vec![
                ("tabline.wrap".to_string(), "true".to_string()),
                ("ui.tab_width".to_string(), "4".to_string())
            ]
        );
        assert_eq!(cli.workspace.as_deref(), Some("foo"));
        assert_eq!(
            cli.files,
            vec![PathBuf::from("song.cc"), PathBuf::from("tab.gp")]
        );
    }

    #[test]
    fn option_without_equals_is_an_error() {
        let err = Cli::try_parse_from(["cecilia", "-o", "song.gp"])
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("NAME=VALUE") && err.contains("--"), "{err}");
    }

    #[test]
    fn dash_file_after_double_dash() {
        let cli = Cli::try_parse_from(["cecilia", "--", "-odd.gp"]).unwrap();
        assert!(cli.options.is_empty());
        assert_eq!(cli.files, vec![PathBuf::from("-odd.gp")]);
    }
}
