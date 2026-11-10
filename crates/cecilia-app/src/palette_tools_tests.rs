//! The palette's bar, repeat, ornament, slide, stroke, beam and stem tools: each is a command
//! that changes the score in one undo step, and the palette shows its state at the cursor.

use cecilia_core::*;

use crate::config::Mode;
use crate::{App, Command, Config, Key};

fn app() -> App {
    let mut app = App::new(Config::default(), None);
    app.run(Command::New);
    app
}

fn run(a: &mut App, line: &str) {
    a.run(Command::parse(line).unwrap_or_else(|e| panic!("{line}: {e}")));
}

fn keys(app: &mut App, s: &str) {
    for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
        app.handle_key(k);
    }
}

/// Fills bar `bar` of track 0 with `n` quarter notes on string 0 (frets 1, 2 …).
fn fill(a: &mut App, bar: usize, n: u8) {
    let b = a.pane().buffer;
    while a.buffers[b].score.master_bars.len() <= bar {
        a.buffers[b].score.master_bars.push(MasterBar::default());
        a.buffers[b].score.tracks[0].bars.push(Bar::default());
    }
    let beats = (0..n)
        .map(|i| Beat {
            duration: Duration::new(DurationValue::Eighth),
            notes: vec![Note::new(0, i + 1)],
            ..Beat::default()
        })
        .collect();
    a.buffers[b].score.tracks[0].bars[bar] = Bar {
        voices: vec![Voice { beats }],
        ..Default::default()
    };
}

fn status(a: &App) -> String {
    a.last_notification()
        .map(|m| m.text.clone())
        .unwrap_or_default()
}

fn score(a: &App) -> &Score {
    &a.buffer().score
}

fn beat(a: &App, bar: usize, i: usize) -> &Beat {
    &score(a).tracks[0].bars[bar].voices[0].beats[i]
}

fn undo_steps(a: &App) -> usize {
    let mut h = a.buffer().history.clone();
    let mut s = a.buffer().score.clone();
    let mut n = 0;
    while h.undo(&mut s).is_some() {
        n += 1;
    }
    n
}

fn row<'a>(
    rows: &'a [crate::palette::PaletteRow],
    command: &str,
) -> &'a crate::palette::PaletteRow {
    rows.iter()
        .find(|r| r.command == command)
        .unwrap_or_else(|| panic!("no palette item {command}"))
}

#[test]
fn every_requested_tool_is_in_the_palette() {
    let a = app();
    let rows = a.palette_rows();
    for command in [
        "key ",
        "time ",
        "free-time",
        "double-bar",
        "bar-repeat 1",
        "bar-repeat 2",
        "repeat-start",
        "repeat-end",
        "ending ",
        "direction ",
        "section ",
        "double-dot",
        "toggle fermata",
        "toggle crescendo",
        "toggle decrescendo",
        "toggle hammer",
        "toggle artificial-harmonic",
        "toggle arpeggio-down",
        "toggle arpeggio-up",
        "toggle rasgueado",
        "toggle pick-down",
        "toggle pick-up",
        "toggle slide-legato",
        "toggle slide",
        "toggle slide-in-above",
        "toggle slide-in-below",
        "toggle slide-out-down",
        "toggle slide-out-up",
        "toggle left-hand-tap",
        "toggle grace-before",
        "toggle grace-on",
        "toggle fade-in",
        "toggle fade-out",
        "toggle volume-swell",
        "beams auto",
        "beams force",
        "beams break",
        "beams break-secondary",
    ] {
        // The tab staff shows all but the clef, the stems and the accidentals.
        assert!(rows.iter().any(|r| r.command == command), "{command}");
    }
    // Standard notation adds the clef, the stems and the accidentals.
    let mut a = app();
    a.pane_mut().cursor.staff = 0;
    let rows = a.palette_rows();
    for command in [
        "clef ",
        "stems up",
        "stems down",
        "stems auto",
        "accidental double-sharp",
    ] {
        assert!(rows.iter().any(|r| r.command == command), "{command}");
    }
    assert!(!rows.iter().any(|r| r.command == "toggle fade-out"));
}

#[test]
fn argument_tools_open_the_command_line() {
    let mut a = app();
    let rows = a.palette_rows();
    let at = rows.iter().position(|r| r.command == "direction ").unwrap();
    assert_eq!(rows[at].name(), "Directions…");
    a.palette.selected = at;
    a.activate_palette();
    assert_eq!(a.mode, Mode::Command);
    assert_eq!(a.cmdline, "direction ");
    keys(&mut a, "da-capo<CR>");
    assert_eq!(a.mode, Mode::Normal);
    assert_eq!(score(&a).master_bars[0].directions, vec![Direction::DaCapo]);
}

#[test]
fn bar_tools_toggle_and_show_their_state() {
    let mut a = app();
    for (line, active) in [
        ("free-time", "free-time"),
        ("double-bar", "double-bar"),
        ("repeat-start", "repeat-start"),
        ("repeat-end", "repeat-end"),
    ] {
        assert!(!row(&a.palette_rows(), active).active, "{line}");
        run(&mut a, line);
        assert!(row(&a.palette_rows(), active).active, "{line}");
    }
    let mb = &score(&a).master_bars[0];
    assert!(mb.free_time && mb.double_bar && mb.repeat_start && mb.repeat_end.is_some());
    run(&mut a, "free-time");
    run(&mut a, "double-bar");
    assert!(!score(&a).master_bars[0].free_time && !score(&a).master_bars[0].double_bar);
    // Directions toggle; endings replace and clear.
    run(&mut a, "direction fine");
    run(&mut a, "direction dal-segno-al-coda");
    assert_eq!(
        score(&a).master_bars[0].directions,
        vec![Direction::Fine, Direction::DaSegnoAlCoda]
    );
    run(&mut a, "direction fine");
    assert_eq!(
        score(&a).master_bars[0].directions,
        vec![Direction::DaSegnoAlCoda]
    );
    run(&mut a, "ending 1 2");
    assert_eq!(score(&a).master_bars[0].alternate_endings, 0b11);
    run(&mut a, "ending 1 2");
    assert_eq!(score(&a).master_bars[0].alternate_endings, 0);
    assert!(Command::parse("ending 0x").is_err());
    run(&mut a, "section Intro");
    assert!(row(&a.palette_rows(), "section ").active);
}

#[test]
fn key_signatures_take_numbers_and_names() {
    assert_eq!(Command::parse("key 2"), Ok(Command::Key(2, None)));
    assert_eq!(Command::parse("key G"), Ok(Command::Key(1, Some(false))));
    assert_eq!(Command::parse("key Bb"), Ok(Command::Key(-2, Some(false))));
    assert_eq!(Command::parse("key Am"), Ok(Command::Key(0, Some(true))));
    assert_eq!(
        Command::parse("key C# minor"),
        Ok(Command::Key(4, Some(true)))
    );
    assert_eq!(Command::parse("key F#"), Ok(Command::Key(6, Some(false))));
    assert!(Command::parse("key H").is_err());
    assert!(Command::parse("key Fb").is_err());
    let mut a = app();
    run(&mut a, "key Dm");
    assert_eq!(
        score(&a).master_bars[0].key_signature,
        KeySignature {
            accidentals: -1,
            minor: true
        }
    );
    run(&mut a, "time 6/8");
    assert_eq!(score(&a).master_bars[0].time_signature.numerator, 6);
}

#[test]
fn repeat_bars_copy_the_bars_before_and_are_one_undo_step() {
    let mut a = app();
    fill(&mut a, 0, 3);
    fill(&mut a, 1, 2);
    fill(&mut a, 2, 1);
    fill(&mut a, 3, 1);
    a.pane_mut().cursor.bar = 2;
    let before = undo_steps(&a);
    run(&mut a, "bar-repeat 2");
    assert_eq!(undo_steps(&a), before + 1);
    let t = &score(&a).tracks[0];
    assert_eq!(t.bars[2].simile, Some(Simile::TwoBarsFirst));
    assert_eq!(t.bars[3].simile, Some(Simile::TwoBarsSecond));
    assert_eq!(t.bars[2].voices, t.bars[0].voices);
    assert_eq!(t.bars[3].voices, t.bars[1].voices);
    assert!(row(&a.palette_rows(), "bar-repeat 2").active);
    // Pressing it again takes both signs off, keeping the notes.
    run(&mut a, "bar-repeat 2");
    let t = &score(&a).tracks[0];
    assert!(t.bars[2].simile.is_none() && t.bars[3].simile.is_none());
    assert_eq!(t.bars[2].voices, t.bars[0].voices);
    // One bar.
    a.pane_mut().cursor.bar = 1;
    run(&mut a, "bar-repeat 1");
    assert_eq!(score(&a).tracks[0].bars[1].simile, Some(Simile::OneBar));
    assert_eq!(
        score(&a).tracks[0].bars[1].voices,
        score(&a).tracks[0].bars[0].voices
    );
    // Editing a repeated bar makes it an ordinary bar again.
    a.pane_mut().cursor.string = 0;
    a.pane_mut().cursor.beat = 0;
    run(&mut a, "toggle ghost");
    assert!(score(&a).tracks[0].bars[1].simile.is_none());
    // The first bar has nothing before it.
    a.pane_mut().cursor.bar = 0;
    run(&mut a, "bar-repeat 1");
    assert!(score(&a).tracks[0].bars[0].simile.is_none());
}

#[test]
fn beat_marks_toggle_and_show_their_state() {
    let mut a = app();
    fill(&mut a, 0, 2);
    a.pane_mut().cursor.string = 0;
    for (tool, check) in [
        (
            "fermata",
            (|e: &BeatEffects| e.fermata) as fn(&BeatEffects) -> bool,
        ),
        ("rasgueado", |e| e.rasgueado),
        ("fade-out", |e| e.fade_out),
        ("volume-swell", |e| e.volume_swell),
        ("pick-down", |e| e.pick_stroke == StrokeDirection::Down),
        ("pick-up", |e| e.pick_stroke == StrokeDirection::Up),
        ("arpeggio-down", |e| {
            e.stroke
                .is_some_and(|s| s.arpeggio && s.direction == StrokeDirection::Down)
        }),
        ("arpeggio-up", |e| {
            e.stroke
                .is_some_and(|s| s.arpeggio && s.direction == StrokeDirection::Up)
        }),
        ("crescendo", |e| e.hairpin == Some(Hairpin::Crescendo)),
        ("decrescendo", |e| e.hairpin == Some(Hairpin::Decrescendo)),
    ] {
        let line = format!("toggle {tool}");
        let steps = undo_steps(&a);
        run(&mut a, &line);
        assert!(check(&beat(&a, 0, 0).effects), "{tool}");
        assert_eq!(undo_steps(&a), steps + 1, "{tool} is one undo step");
        assert!(row(&a.palette_rows(), &line).active, "{tool} shows");
        run(&mut a, &line);
        assert!(!check(&beat(&a, 0, 0).effects), "{tool} off");
    }
    // The fades exclude one another, and fade-in too.
    run(&mut a, "toggle fade-in");
    run(&mut a, "toggle fade-out");
    let e = &beat(&a, 0, 0).effects;
    assert!(e.fade_out && !e.fade_in && !e.volume_swell);
    // Picking down and up exclude one another; a brush stroke is not an arpeggio.
    run(&mut a, "toggle pick-down");
    run(&mut a, "toggle pick-up");
    assert_eq!(beat(&a, 0, 0).effects.pick_stroke, StrokeDirection::Up);
    run(&mut a, "toggle stroke-down");
    assert!(!row(&a.palette_rows(), "toggle arpeggio-down").active);
    assert!(row(&a.palette_rows(), "toggle stroke-down").active);
    run(&mut a, "toggle arpeggio-down");
    assert!(beat(&a, 0, 0).effects.stroke.unwrap().arpeggio);
}

#[test]
fn double_dot_and_slides_and_note_marks() {
    let mut a = app();
    fill(&mut a, 0, 2);
    a.pane_mut().cursor.string = 0;
    run(&mut a, "double-dot");
    assert_eq!(beat(&a, 0, 0).duration.dots, 2);
    assert!(row(&a.palette_rows(), "double-dot").active);
    assert!(!row(&a.palette_rows(), "dot").active);
    run(&mut a, "double-dot");
    assert_eq!(beat(&a, 0, 0).duration.dots, 0);
    run(&mut a, "dot");
    run(&mut a, "double-dot");
    assert_eq!(beat(&a, 0, 0).duration.dots, 2);

    let slides = |a: &App| beat(a, 0, 0).notes[0].effects.slides.clone();
    run(&mut a, "toggle slide");
    run(&mut a, "toggle slide-legato");
    assert_eq!(
        slides(&a),
        vec![SlideKind::Legato],
        "shift and legato are alternatives"
    );
    run(&mut a, "toggle slide-in-below");
    run(&mut a, "toggle slide-out-up");
    assert_eq!(
        slides(&a),
        vec![
            SlideKind::Legato,
            SlideKind::IntoFromBelow,
            SlideKind::OutUp
        ]
    );
    run(&mut a, "toggle slide-in-above");
    run(&mut a, "toggle slide-out-down");
    assert_eq!(
        slides(&a),
        vec![
            SlideKind::Legato,
            SlideKind::IntoFromAbove,
            SlideKind::OutDown
        ]
    );
    assert!(row(&a.palette_rows(), "toggle slide-in-above").active);
    assert!(!row(&a.palette_rows(), "toggle slide-in-below").active);
    run(&mut a, "toggle slide-legato");
    run(&mut a, "toggle slide-in-above");
    run(&mut a, "toggle slide-out-down");
    assert!(slides(&a).is_empty());

    run(&mut a, "toggle artificial-harmonic");
    assert_eq!(
        beat(&a, 0, 0).notes[0].effects.harmonic,
        Some(Harmonic::Artificial { semitones: 12 })
    );
    assert!(row(&a.palette_rows(), "toggle artificial-harmonic").active);
    assert!(!row(&a.palette_rows(), "toggle harmonic").active);
    run(&mut a, "toggle artificial-harmonic");
    assert!(beat(&a, 0, 0).notes[0].effects.harmonic.is_none());

    run(&mut a, "toggle left-hand-tap");
    assert!(beat(&a, 0, 0).notes[0].effects.left_hand_tap);
    assert!(row(&a.palette_rows(), "toggle left-hand-tap").active);

    run(&mut a, "toggle grace-before");
    let g = beat(&a, 0, 0).notes[0].effects.grace.unwrap();
    assert!(!g.on_beat);
    run(&mut a, "toggle grace-on");
    assert!(beat(&a, 0, 0).notes[0].effects.grace.unwrap().on_beat);
    assert!(row(&a.palette_rows(), "toggle grace-on").active);
    assert!(!row(&a.palette_rows(), "toggle grace-before").active);
    run(&mut a, "toggle grace-on");
    assert!(beat(&a, 0, 0).notes[0].effects.grace.is_none());
}

#[test]
fn beams_and_stems_apply_to_the_beat_or_the_selection() {
    let mut a = app();
    fill(&mut a, 0, 4);
    a.pane_mut().cursor.staff = 0;
    a.pane_mut().cursor.beat = 1;
    run(&mut a, "beams break");
    assert_eq!(beat(&a, 0, 1).beam, BeamMode::Break);
    assert_eq!(beat(&a, 0, 0).beam, BeamMode::Auto);
    assert!(row(&a.palette_rows(), "beams break").active);
    run(&mut a, "beams auto");
    assert_eq!(beat(&a, 0, 1).beam, BeamMode::Auto);
    run(&mut a, "stems down");
    assert_eq!(beat(&a, 0, 1).stem, StemDir::Down);
    assert!(row(&a.palette_rows(), "stems down").active);
    // A visual selection: every beat in it; forcing leaves the first alone.
    a.pane_mut().cursor.beat = 0;
    keys(&mut a, "vll");
    assert_eq!(a.mode, Mode::Visual);
    let rows = a.palette_rows();
    assert!(rows.iter().any(|r| r.command == "beams force"));
    assert!(rows.iter().any(|r| r.command == "stems up"));
    let steps = undo_steps(&a);
    run(&mut a, "stems up");
    // The selection stays, so more can be applied to it.
    assert_eq!(a.mode, Mode::Visual);
    assert_eq!(undo_steps(&a), steps + 1);
    keys(&mut a, "<Esc>");
    for i in 0..3 {
        assert_eq!(beat(&a, 0, i).stem, StemDir::Up);
    }
    assert_eq!(beat(&a, 0, 3).stem, StemDir::Auto);
    a.pane_mut().cursor.beat = 0;
    keys(&mut a, "vll");
    run(&mut a, "beams force");
    assert_eq!(beat(&a, 0, 0).beam, BeamMode::Auto);
    assert_eq!(beat(&a, 0, 1).beam, BeamMode::Force);
    assert_eq!(beat(&a, 0, 2).beam, BeamMode::Force);
    assert!(Command::parse("beams sideways").is_err());
}

#[test]
fn double_accidentals_spell_the_note_and_are_undone() {
    let mut a = app();
    a.run(Command::Notation("standard".into()));
    a.pane_mut().cursor.staff = 0;
    // A C5 on the staff (diatonic 5 * 7).
    keys(&mut a, "i");
    run(&mut a, "note-letter c");
    keys(&mut a, "<Esc>");
    let c = a.pane().cursor;
    let pitch0 = {
        let t = &score(&a).tracks[0];
        t.pitch_of(&t.bars[c.bar].voices[0].beats[c.beat].notes[0])
    };
    run(&mut a, "accidental double-sharp");
    let t = &score(&a).tracks[0];
    let n = &t.bars[c.bar].voices[0].beats[c.beat].notes[0];
    assert_eq!(n.alter, Some(2));
    assert_eq!(t.pitch_of(n), pitch0 + 2);
    assert!(row(&a.palette_rows(), "accidental double-sharp").active);
    run(&mut a, "accidental double-flat");
    let t = &score(&a).tracks[0];
    let n = &t.bars[c.bar].voices[0].beats[c.beat].notes[0];
    assert_eq!(n.alter, Some(-2));
    assert_eq!(t.pitch_of(n), pitch0 - 2);
    run(&mut a, "accidental sharp");
    let t = &score(&a).tracks[0];
    assert_eq!(t.bars[c.bar].voices[0].beats[c.beat].notes[0].alter, None);
    run(&mut a, "undo");
    let t = &score(&a).tracks[0];
    assert_eq!(
        t.bars[c.bar].voices[0].beats[c.beat].notes[0].alter,
        Some(-2)
    );
}

#[test]
fn old_scores_without_the_new_fields_still_load() {
    // A beat, note and bar as the previous version saved them.
    let beat: Beat = serde_json::from_str(
        r#"{"duration":{"value":"Quarter","dots":0,"tuplet":null},"empty":false,"notes":[],
        "text":null,"chord":null,"effects":{"vibrato":false,"fade_in":false,"slap":"None",
        "stroke":null,"pick_stroke":"None","rasgueado":false,"tremolo_bar":null,
        "ottava":"None"},"lyrics":[]}"#,
    )
    .unwrap();
    assert_eq!(beat.beam, BeamMode::Auto);
    assert_eq!(beat.stem, StemDir::Auto);
    assert!(!beat.effects.fermata);
    let bar: Bar = serde_json::from_str(r#"{"voices":[]}"#).unwrap();
    assert!(bar.simile.is_none());
}

#[test]
fn search_jumps_as_typed_like_the_properties_pane() {
    let mut a = app();
    keys(&mut a, "<leader>E");
    assert_eq!(a.focused_kind(), crate::app::PaneKind::Palette);
    let name = |a: &App| a.palette_rows()[a.palette_selected()].label;
    let start = a.palette_selected();
    // `/` opens the field and jumps as it is typed: a tool by name, a section by title.
    keys(&mut a, "/fermata");
    assert!(a.palette.search.is_some());
    assert_eq!(name(&a), "Fermata");
    keys(&mut a, "<C-u>slides");
    assert_eq!(name(&a), "Slides");
    // Backspace takes characters off; Escape goes back and sets nothing.
    keys(&mut a, "<Esc>");
    assert!(a.palette.search.is_none() && a.palette.last_search.is_empty());
    assert_eq!(a.palette_selected(), start);
    // Enter keeps the jump and sets the register; `n` and `N` go on and back.
    keys(&mut a, "/slide<CR>");
    assert_eq!(a.palette.last_search, "slide");
    let first = a.palette_selected();
    keys(&mut a, "n");
    let second = a.palette_selected();
    assert_ne!(first, second);
    assert!(name(&a).to_lowercase().contains("slide") || name(&a) == "Shift slide");
    keys(&mut a, "N");
    assert_eq!(a.palette_selected(), first);
    // Enter did not apply anything.
    assert_eq!(undo_steps(&a), 0);
    // A match is found through the tooltip's keys and the section too (a capital is exact).
    keys(&mut a, "/Strokes<CR>");
    assert_eq!(name(&a), "Strokes");
    // A query nothing matches leaves the selection where the search began.
    let here = a.palette_selected();
    keys(&mut a, "/nosuchtool");
    assert_eq!(a.palette_selected(), here);
    keys(&mut a, "<Esc>");
}

#[test]
fn search_only_finds_the_tools_of_the_notation_and_mode() {
    let mut a = app();
    keys(&mut a, "<leader>E");
    let found = |a: &mut App, q: &str| {
        let before = a.palette_selected();
        keys(a, &format!("/{q}<Esc>"));
        let _ = before;
        a.palette.last_search = q.to_string();
        a.palette_rows().iter().any(|r| {
            r.kind == crate::palette::PaletteKind::Item && r.label.to_lowercase().contains(q)
        })
    };
    // On the tab staff there is no double sharp; on a standard staff there is.
    assert!(!found(&mut a, "double sharp"));
    keys(&mut a, "/double sharp");
    assert_ne!(a.palette_rows()[a.palette_selected()].label, "Double sharp");
    keys(&mut a, "<Esc><Esc>");
    a.pane_mut().cursor.staff = 0;
    keys(&mut a, "<leader>E/double sharp<CR>");
    assert_eq!(a.palette_rows()[a.palette_selected()].label, "Double sharp");
}

/// Selects the first `n` beats of bar 0 in Visual mode.
fn select(a: &mut App, n: usize) {
    a.pane_mut().cursor.bar = 0;
    a.pane_mut().cursor.beat = 0;
    keys(a, "v");
    for _ in 1..n {
        keys(a, "l");
    }
    assert_eq!(a.mode, Mode::Visual);
}

#[test]
fn one_hairpin_across_the_selection() {
    let mut a = app();
    fill(&mut a, 0, 6);
    select(&mut a, 4);
    let steps = undo_steps(&a);
    run(&mut a, "toggle crescendo");
    assert_eq!(undo_steps(&a), steps + 1, "one undo step");
    for i in 0..6 {
        assert_eq!(beat(&a, 0, i).effects.hairpin.is_some(), i < 4, "beat {i}");
    }
    assert!(row(&a.palette_rows(), "toggle crescendo").active);
    assert!(!row(&a.palette_rows(), "toggle decrescendo").active);
    // A decrescendo replaces it; the same again takes it off.
    run(&mut a, "toggle decrescendo");
    assert_eq!(beat(&a, 0, 2).effects.hairpin, Some(Hairpin::Decrescendo));
    run(&mut a, "toggle decrescendo");
    assert!((0..4).all(|i| beat(&a, 0, i).effects.hairpin.is_none()));
    // Typed on the command line from Visual mode it applies to the selection too.
    keys(&mut a, ":toggle crescendo<CR>");
    assert_eq!(a.mode, Mode::Visual);
    assert!((0..4).all(|i| beat(&a, 0, i).effects.hairpin == Some(Hairpin::Crescendo)));
    run(&mut a, "toggle crescendo");
    // A dynamic goes on every selected beat.
    run(&mut a, "dynamic p");
    assert!((0..4).all(|i| beat(&a, 0, i).notes[0].velocity == 47));
    assert_eq!(beat(&a, 0, 4).notes[0].velocity, DEFAULT_VELOCITY);
}

#[test]
fn plus_and_minus_step_every_duration_of_the_selection() {
    let mut a = app();
    fill(&mut a, 0, 4);
    {
        let b = a.pane().buffer;
        let beats = &mut a.buffers[b].score.tracks[0].bars[0].voices[0].beats;
        beats[0].duration = Duration::new(DurationValue::Sixteenth);
        beats[1].duration = Duration::new(DurationValue::Eighth);
        beats[2].duration = Duration {
            dots: 1,
            ..Duration::new(DurationValue::Quarter)
        };
        beats[3].duration = Duration::new(DurationValue::Whole);
    }
    select(&mut a, 4);
    let steps = undo_steps(&a);
    keys(&mut a, "+");
    assert_eq!(undo_steps(&a), steps + 1);
    let v = |a: &App, i| beat(a, 0, i).duration;
    assert_eq!(v(&a, 0).value, DurationValue::Eighth);
    assert_eq!(v(&a, 1).value, DurationValue::Quarter);
    assert_eq!(
        (v(&a, 2).value, v(&a, 2).dots),
        (DurationValue::Half, 1),
        "dots are kept"
    );
    assert_eq!(
        v(&a, 3).value,
        DurationValue::Whole,
        "a whole note is the longest"
    );
    assert_eq!(
        a.mode,
        Mode::Visual,
        "the selection stays for another press"
    );
    keys(&mut a, "--");
    assert_eq!(v(&a, 0).value, DurationValue::ThirtySecond);
    assert_eq!(v(&a, 1).value, DurationValue::Sixteenth);
    // A value from the note-value tools sets every beat; the bar then overflows or not.
    run(&mut a, "duration 4");
    assert!((0..4).all(|i| v(&a, i).value == DurationValue::Quarter));
    run(&mut a, "dot");
    assert!((0..4).all(|i| v(&a, i).dots == 1));
    run(&mut a, "dot");
    assert!((0..4).all(|i| v(&a, i).dots == 0));
    run(&mut a, "duration 2");
    assert!(status(&a).contains("over full"), "{:?}", status(&a));
    // A triplet over three selected beats.
    keys(&mut a, "<Esc>");
    run(&mut a, "duration 8");
    select(&mut a, 3);
    run(&mut a, "tuplet 3");
    assert!((0..3).all(|i| v(&a, i).tuplet.is_some()) && v(&a, 3).tuplet.is_none());
}

#[test]
fn staccato_and_other_marks_go_on_all_or_off_all() {
    let mut a = app();
    fill(&mut a, 0, 4);
    {
        let b = a.pane().buffer;
        a.buffers[b].score.tracks[0].bars[0].voices[0].beats[1].notes[0]
            .effects
            .staccato = true;
    }
    select(&mut a, 4);
    let staccato = |a: &App| -> Vec<bool> {
        (0..4)
            .map(|i| beat(a, 0, i).notes[0].effects.staccato)
            .collect()
    };
    let steps = undo_steps(&a);
    run(&mut a, "toggle staccato");
    assert_eq!(staccato(&a), vec![true; 4], "some had it: all get it");
    assert_eq!(undo_steps(&a), steps + 1);
    assert!(row(&a.palette_rows(), "toggle staccato").active);
    run(&mut a, "toggle staccato");
    assert_eq!(staccato(&a), vec![false; 4], "all had it: none keeps it");
    assert!(!row(&a.palette_rows(), "toggle staccato").active);
    // Accents, note effects and beat marks too.
    run(&mut a, "toggle accent");
    run(&mut a, "toggle palm-mute");
    run(&mut a, "toggle fermata");
    for i in 0..4 {
        let b = beat(&a, 0, i);
        assert!(b.notes[0].accent == Accent::Normal && b.notes[0].effects.palm_mute);
        assert!(b.effects.fermata);
    }
    // Rest-out turns the selection into rests.
    run(&mut a, "rest");
    assert!((0..4).all(|i| beat(&a, 0, i).is_rest()));
    run(&mut a, "undo");
    assert!((0..4).all(|i| !beat(&a, 0, i).is_rest()));
}

#[test]
fn legato_slide_and_tie_chains_link_each_beat_to_the_next() {
    let mut a = app();
    fill(&mut a, 0, 4);
    select(&mut a, 4);
    let steps = undo_steps(&a);
    run(&mut a, "toggle hammer");
    assert_eq!(undo_steps(&a), steps + 1);
    let hammer = |a: &App| -> Vec<bool> {
        (0..4)
            .map(|i| beat(a, 0, i).notes[0].effects.hammer_pull)
            .collect()
    };
    assert_eq!(
        hammer(&a),
        vec![true, true, true, false],
        "the last is not linked on"
    );
    assert!(row(&a.palette_rows(), "toggle hammer").active);
    run(&mut a, "toggle hammer");
    assert_eq!(hammer(&a), vec![false; 4]);

    run(&mut a, "toggle slide-legato");
    for i in 0..3 {
        assert_eq!(
            beat(&a, 0, i).notes[0].effects.slides,
            vec![SlideKind::Legato]
        );
    }
    assert!(beat(&a, 0, 3).notes[0].effects.slides.is_empty());
    run(&mut a, "toggle slide");
    assert_eq!(
        beat(&a, 0, 1).notes[0].effects.slides,
        vec![SlideKind::Shift]
    );
    assert!(!row(&a.palette_rows(), "toggle slide-legato").active);

    // Ties join each note to the one before it.
    run(&mut a, "toggle tie");
    let tied: Vec<bool> = (0..4).map(|i| beat(&a, 0, i).notes[0].tie).collect();
    assert_eq!(tied, vec![false, true, true, true]);
    run(&mut a, "undo");
    assert!((0..4).all(|i| !beat(&a, 0, i).notes[0].tie));
}

#[test]
fn pitch_tools_work_over_a_selection() {
    let mut a = app();
    fill(&mut a, 0, 3);
    select(&mut a, 3);
    run(&mut a, "pitch-up");
    let frets = |a: &App| -> Vec<u8> { (0..3).map(|i| beat(a, 0, i).notes[0].fret).collect() };
    assert_eq!(frets(&a), vec![2, 3, 4]);
    run(&mut a, "pitch-down");
    run(&mut a, "pitch-down");
    assert_eq!(frets(&a), vec![0, 1, 2]);
    // A note that would fall off the neck stays and is reported.
    run(&mut a, "pitch-down");
    assert_eq!(frets(&a), vec![0, 0, 1]);
    assert!(status(&a).contains("could not move"));
    // Accidentals on a standard staff.
    keys(&mut a, "<Esc>");
    a.pane_mut().cursor.staff = 0;
    a.pane_mut().cursor.beat = 0;
    keys(&mut a, "vll");
    let before: Vec<u8> = frets(&a);
    run(&mut a, "accidental double-sharp");
    for i in 0..3 {
        assert_eq!(beat(&a, 0, i).notes[0].alter, Some(2));
    }
    assert!((0..3).all(|i| beat(&a, 0, i).notes[0].fret >= before[i]));
}

#[test]
fn bar_selections_offer_the_bar_tools_for_their_bars() {
    let mut a = app();
    for b in 0..4 {
        fill(&mut a, b, 2);
    }
    a.pane_mut().cursor.bar = 1;
    keys(&mut a, "V");
    run(&mut a, "bar-next");
    run(&mut a, "bar-next");
    assert_eq!(a.selection().map(|s| s.2), Some(true));
    let rows = a.palette_rows();
    for c in [
        "repeat-start",
        "repeat-end",
        "section ",
        "ending ",
        "double-bar",
        "key ",
    ] {
        assert!(rows.iter().any(|r| r.command == c), "{c}");
    }
    assert!(
        !rows
            .iter()
            .any(|r| r.command == "clef " || r.command == "bar-repeat 1")
    );
    let steps = undo_steps(&a);
    run(&mut a, "repeat-start");
    run(&mut a, "repeat-end");
    assert!(score(&a).master_bars[1].repeat_start && score(&a).master_bars[3].repeat_end.is_some());
    assert!(
        !score(&a).master_bars[3].repeat_start && !score(&a).master_bars[1].repeat_end.is_some()
    );
    run(&mut a, "ending 1");
    assert!((1..=3).all(|b| score(&a).master_bars[b].alternate_endings == 1));
    assert!(row(&a.palette_rows(), "ending ").active);
    assert_eq!(undo_steps(&a), steps + 3);
    assert_eq!(a.mode, Mode::Visual);
    // A beat selection has none of them.
    keys(&mut a, "<Esc>v");
    assert!(!a.palette_rows().iter().any(|r| r.command == "repeat-start"));
}

/// Selects a palette row and applies it, as `<CR>` in the palette does.
fn apply(a: &mut App, command: &str) {
    let at = a
        .palette_rows()
        .iter()
        .position(|r| r.command == command)
        .unwrap_or_else(|| panic!("no palette item {command}"));
    a.palette.selected = at;
    a.activate_palette();
}

#[test]
fn dot_repeats_a_palette_toggle_on_another_beat() {
    let mut a = app();
    fill(&mut a, 0, 4);
    a.pane_mut().cursor.string = 0;
    a.pane_mut().cursor.beat = 0;
    apply(&mut a, "toggle staccato");
    assert!(beat(&a, 0, 0).notes[0].effects.staccato);
    keys(&mut a, "l");
    let steps = undo_steps(&a);
    keys(&mut a, ".");
    assert!(beat(&a, 0, 1).notes[0].effects.staccato);
    assert_eq!(undo_steps(&a), steps + 1, "one undo step");
    // A toggle repeats as a toggle: on a beat that has it, it takes it off.
    keys(&mut a, "h.");
    assert!(!beat(&a, 0, 0).notes[0].effects.staccato);
    // Navigating the palette does not replace it.
    keys(&mut a, "<leader>Ejj/dead<Esc><Esc>");
    keys(&mut a, "ll.");
    assert!(beat(&a, 0, 2).notes[0].effects.staccato);
    // An editing key replaces it again: `.` repeats that.
    keys(&mut a, "ta");
    keys(&mut a, "l.");
    assert_eq!(beat(&a, 0, 3).notes[0].accent, Accent::Normal);
    assert!(!beat(&a, 0, 3).notes[0].effects.staccato);
}

#[test]
fn dot_repeats_a_duration_bump_and_a_value() {
    let mut a = app();
    fill(&mut a, 0, 4);
    a.pane_mut().cursor.beat = 0;
    apply(&mut a, "duration-longer");
    assert_eq!(beat(&a, 0, 0).duration.value, DurationValue::Quarter);
    keys(&mut a, "l.");
    assert_eq!(beat(&a, 0, 1).duration.value, DurationValue::Quarter);
    apply(&mut a, "duration 16");
    keys(&mut a, "l.");
    assert_eq!(beat(&a, 0, 2).duration.value, DurationValue::Sixteenth);
}

#[test]
fn dot_repeats_a_range_tool_over_as_many_beats() {
    let mut a = app();
    fill(&mut a, 0, 8);
    select(&mut a, 3);
    apply(&mut a, "toggle staccato");
    assert!((0..3).all(|i| beat(&a, 0, i).notes[0].effects.staccato));
    keys(&mut a, "<Esc>");
    a.pane_mut().cursor.beat = 4;
    let steps = undo_steps(&a);
    keys(&mut a, ".");
    let staccato: Vec<bool> = (0..8)
        .map(|i| beat(&a, 0, i).notes[0].effects.staccato)
        .collect();
    assert_eq!(
        staccato,
        vec![true, true, true, false, true, true, true, false]
    );
    assert_eq!(a.mode, Mode::Normal);
    assert_eq!(undo_steps(&a), steps + 1);
    // Keys in Visual mode count too (`+`).
    select(&mut a, 2);
    keys(&mut a, "+<Esc>");
    a.pane_mut().cursor.beat = 5;
    keys(&mut a, ".");
    assert_eq!(beat(&a, 0, 5).duration.value, DurationValue::Quarter);
    assert_eq!(beat(&a, 0, 6).duration.value, DurationValue::Quarter);
    assert_eq!(beat(&a, 0, 7).duration.value, DurationValue::Eighth);
}

#[test]
fn dot_repeats_an_argument_tool_with_its_last_argument() {
    let mut a = app();
    for b in 0..3 {
        fill(&mut a, b, 2);
    }
    apply(&mut a, "direction ");
    keys(&mut a, "fine<CR>");
    assert_eq!(score(&a).master_bars[0].directions, vec![Direction::Fine]);
    a.pane_mut().cursor.bar = 2;
    keys(&mut a, ".");
    assert_eq!(score(&a).master_bars[2].directions, vec![Direction::Fine]);
}

#[test]
fn a_tool_that_changed_nothing_does_not_replace_the_last_change() {
    let mut a = app();
    fill(&mut a, 0, 3);
    a.pane_mut().cursor.beat = 0;
    apply(&mut a, "toggle ghost");
    assert!(beat(&a, 0, 0).notes[0].ghost);
    // Unavailable (a header folds; nothing changes): the last tool stays.
    a.palette.selected = 0;
    a.activate_palette();
    keys(&mut a, "l.");
    assert!(beat(&a, 0, 1).notes[0].ghost);
}

#[test]
fn hjkl_move_in_the_palette_while_visual_mode_stays() {
    let mut a = app();
    fill(&mut a, 0, 6);
    select(&mut a, 3);
    let (anchor, cursor) = (a.ed.visual_anchor, a.pane().cursor);
    keys(&mut a, "<leader>E");
    assert_eq!(a.focused_kind(), crate::app::PaneKind::Palette);
    assert_eq!(a.mode, Mode::Visual, "the selection stays");
    // The grid as the frontends lay it out: 5 buttons to a line.
    a.set_palette_cols(5);
    // The first row is a section title; the buttons start below it.
    keys(&mut a, "j");
    let sel = a.palette_selected();
    keys(&mut a, "l");
    assert_ne!(a.palette_selected(), sel, "l moves to the next button");
    keys(&mut a, "h");
    assert_eq!(a.palette_selected(), sel);
    keys(&mut a, "jj");
    assert_ne!(a.palette_selected(), sel, "j moves down the rows");
    keys(&mut a, "kk");
    assert_eq!(a.palette_selected(), sel);
    keys(&mut a, "<Down><Right><Left><Up>");
    assert_eq!(a.palette_selected(), sel);
    // The score selection and cursor did not move.
    assert_eq!(a.ed.visual_anchor, anchor);
    assert_eq!(a.pane().cursor, cursor);
    assert_eq!(a.mode, Mode::Visual);
    // Enter applies the tool to the selection.
    a.palette.selected = a
        .palette_rows()
        .iter()
        .position(|r| r.command == "toggle staccato")
        .unwrap();
    keys(&mut a, "<CR>");
    assert!((0..3).all(|i| beat(&a, 0, i).notes[0].effects.staccato));
    assert!(!beat(&a, 0, 3).notes[0].effects.staccato);
    // Escape goes back to the score and keeps the selection; there hjkl move it again.
    keys(&mut a, "<Esc>");
    assert_eq!(a.focused_kind(), crate::app::PaneKind::Score);
    assert_eq!(a.mode, Mode::Visual);
    assert!(a.selection().is_some());
    keys(&mut a, "l");
    assert_eq!(a.pane().cursor.beat, cursor.beat + 1);
    // The focus keys work from the palette too, and a search there keeps the selection.
    keys(&mut a, "<leader>E/fermata<CR>");
    assert_eq!(a.palette_rows()[a.palette_selected()].label, "Fermata");
    keys(&mut a, "<leader>E");
    assert_eq!(a.focused_kind(), crate::app::PaneKind::Score);
    assert!(a.selection().is_some());
}

fn set_velocities(a: &mut App, v: &[u8]) {
    let b = a.pane().buffer;
    let beats = &mut a.buffers[b].score.tracks[0].bars[0].voices[0].beats;
    for (beat, &vel) in beats.iter_mut().zip(v) {
        for n in &mut beat.notes {
            n.velocity = vel;
        }
    }
}

fn velocities(a: &App, n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| beat(a, 0, i).notes.first().map_or(0, |n| n.velocity))
        .collect()
}

#[test]
fn a_dynamic_over_a_selection_changes_the_whole_section() {
    let mut a = app();
    fill(&mut a, 0, 6);
    // mp, p, mf, f, then two beats outside the selection at mp.
    set_velocities(&mut a, &[63, 47, 79, 95, 63, 63]);
    select(&mut a, 4);
    let steps = undo_steps(&a);
    apply(&mut a, "dynamic ff");
    assert_eq!(velocities(&a, 6), vec![111, 111, 111, 111, 63, 63]);
    assert_eq!(undo_steps(&a), steps + 1, "one undo step");
    assert_eq!(a.mode, Mode::Visual);
    assert!(row(&a.palette_rows(), "dynamic ff").active);
    assert!(!row(&a.palette_rows(), "dynamic mp").active);
    // Rests in the selection are left alone.
    {
        let b = a.pane().buffer;
        a.buffers[b].score.tracks[0].bars[0].voices[0].beats[1]
            .notes
            .clear();
    }
    apply(&mut a, "dynamic p");
    assert_eq!(velocities(&a, 1), vec![47]);
    assert!(beat(&a, 0, 1).is_rest());
    assert_eq!(velocities(&a, 6)[2..], [47, 47, 63, 63]);
    // `.` applies it to as many beats from the cursor.
    keys(&mut a, "<Esc>");
    a.pane_mut().cursor.beat = 4;
    keys(&mut a, ".");
    assert_eq!(velocities(&a, 6)[4..], [47, 47]);
    run(&mut a, "undo");
    assert_eq!(velocities(&a, 6)[4..], [63, 63]);
}

#[test]
fn stepping_the_dynamics_keeps_the_differences_and_clamps() {
    let mut a = app();
    fill(&mut a, 0, 5);
    set_velocities(&mut a, &[63, 47, 79, 111, 127]);
    select(&mut a, 4);
    let steps = undo_steps(&a);
    keys(&mut a, "<A-+>");
    assert_eq!(velocities(&a, 5), vec![79, 63, 95, 127, 127]);
    assert_eq!(undo_steps(&a), steps + 1);
    keys(&mut a, "<A-->");
    keys(&mut a, "<A-->");
    assert_eq!(velocities(&a, 5), vec![47, 31, 63, 95, 127]);
    // Softer keeps going to ppp and stops.
    for _ in 0..6 {
        run(&mut a, "dynamic-step -1");
    }
    assert_eq!(velocities(&a, 4), vec![15; 4]);
    assert_eq!(
        velocities(&a, 5)[4],
        127,
        "outside the selection nothing changed"
    );
    // The palette has them.
    let rows = a.palette_rows();
    assert!(rows.iter().any(|r| r.command == "dynamic-step +1"));
    assert!(rows.iter().any(|r| r.command == "dynamic-step -1"));
    // `.` repeats a step over as many beats.
    keys(&mut a, "<Esc>");
    a.pane_mut().cursor.beat = 4;
    apply(&mut a, "dynamic-step +1");
    assert_eq!(velocities(&a, 5)[4], 127);
}

#[test]
fn a_dynamic_belongs_to_the_beat() {
    let mut a = app();
    fill(&mut a, 0, 3);
    // A chord whose notes differ in velocity, and a note added after the dynamic was set.
    {
        let b = a.pane().buffer;
        let beat = &mut a.buffers[b].score.tracks[0].bars[0].voices[0].beats[0];
        let mut second = Note::new(1, 2);
        second.velocity = 40;
        beat.notes.push(second);
    }
    a.pane_mut().cursor.beat = 0;
    run(&mut a, "dynamic mf");
    let b = beat(&a, 0, 0);
    assert_eq!(b.dynamic, Some(79));
    assert!(b.notes.iter().all(|n| n.velocity == 79), "the whole chord");
    assert!(row(&a.palette_rows(), "dynamic mf").active);
    assert_eq!(beat(&a, 0, 1).dynamic, None, "other beats keep theirs");
    // A range sets every beat's.
    select(&mut a, 3);
    run(&mut a, "dynamic ff");
    assert!((0..3).all(|i| beat(&a, 0, i).dynamic == Some(111)));
    // Playback follows the beat, whatever a note's own velocity says.
    {
        let b = a.pane().buffer;
        a.buffers[b].score.tracks[0].bars[0].voices[0].beats[0].notes[0].velocity = 20;
    }
    let perf = cecilia_core::Performance::build(score(&a), &[false]);
    assert!(perf.notes.iter().take(3).all(|n| n.velocity == 111));
}
