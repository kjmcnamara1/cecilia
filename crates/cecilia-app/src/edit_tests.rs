//! The vim-style editing workflow: motions on a string / line, beats and bars as the units of
//! `d` `c` `y`, the insert commands (`i a I A o O`), visual `i` / `a`, Enter in insert mode,
//! and the palette following the notation. Every edit is one undo step.

use cecilia_core::*;

use crate::config::{InsertKind, Mode};
use crate::{App, Command, Config, Key};

fn app() -> App {
    let mut app = App::new(Config::default(), None);
    app.run(Command::New);
    app
}

fn keys(app: &mut App, s: &str) {
    for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
        app.handle_key(k);
    }
}

/// One beat: (string, fret, tied from the note before) per note; none: a rest.
type Spec<'a> = &'a [(u8, u8, bool)];

/// Fills a bar of track 0 with quarter beats.
fn set_bar(a: &mut App, bar: usize, beats: &[Spec]) {
    let beats = beats
        .iter()
        .map(|notes| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: notes
                .iter()
                .map(|&(string, fret, tie)| Note {
                    tie,
                    ..Note::new(string, fret)
                })
                .collect(),
            ..Beat::default()
        })
        .collect();
    let b = a.pane().buffer;
    a.buffers[b].score.tracks[0].bars[bar] = Bar {
        voices: vec![Voice { beats }],
        ..Default::default()
    };
}

fn track(a: &App) -> &Track {
    &a.buffer().score.tracks[0]
}

fn bar_beats(a: &App, bar: usize) -> &[Beat] {
    &track(a).bars[bar].voices[0].beats
}

/// Per beat of a bar, the frets (by string order of the notes).
fn frets(a: &App, bar: usize) -> Vec<Vec<u8>> {
    bar_beats(a, bar)
        .iter()
        .map(|b| b.notes.iter().map(|n| n.fret).collect())
        .collect()
}

fn at(a: &App) -> (usize, usize) {
    let c = a.pane().cursor;
    (c.bar, c.beat)
}

fn undo_steps(a: &App) -> usize {
    // Undo until nothing is left, counting the steps.
    let mut a2 = a.buffer().history.clone();
    let mut score = a.buffer().score.clone();
    let mut n = 0;
    while a2.undo(&mut score).is_some() {
        n += 1;
    }
    n
}

/// The tab staff is the home staff of a new score; the standard staff is staff 0.
fn to_standard(a: &mut App) {
    a.pane_mut().cursor.staff = 0;
    let c = a.pane().cursor;
    // The line of the first note's pitch, as the staff shows it.
    let t = track(a);
    let n = &t.bars[c.bar].voices[0].beats[c.beat].notes[0];
    let written = t.pitch_of(n) as i32 + t.staff_clef().transpose();
    let line = staff::diatonic(written, 0);
    a.pane_mut().cursor.line = line;
}

#[test]
fn normal_hjkl_keep_the_row() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[], &[(2, 7, false)]]);
    a.pane_mut().cursor.string = 2;
    keys(&mut a, "l");
    assert_eq!((at(&a), a.pane().cursor.string), ((0, 1), 2));
    keys(&mut a, "h");
    assert_eq!((at(&a), a.pane().cursor.string), ((0, 0), 2));
    keys(&mut a, "j");
    assert_eq!(a.pane().cursor.string, 3);
    keys(&mut a, "kk");
    assert_eq!(a.pane().cursor.string, 1);
}

#[test]
fn w_e_b_follow_the_string_and_ties_on_a_tab() {
    let mut a = app();
    // string 0: 1 . 3~3 . 7     string 1 has a note at beat 4
    set_bar(
        &mut a,
        0,
        &[
            &[(0, 1, false)],
            &[],
            &[(0, 3, false)],
            &[(0, 3, true)],
            &[(1, 5, false)],
            &[(0, 7, false)],
        ],
    );
    a.pane_mut().cursor.string = 0;
    keys(&mut a, "w");
    assert_eq!(at(&a), (0, 2), "next note on the string");
    keys(&mut a, "w");
    assert_eq!(at(&a), (0, 5), "skips the tied note and the other string");
    keys(&mut a, "w");
    assert_eq!(at(&a), (0, 5), "no more notes: stays");
    keys(&mut a, "b");
    assert_eq!(at(&a), (0, 2), "start of the tie, passing the tied note");
    keys(&mut a, "b");
    assert_eq!(at(&a), (0, 0));
    keys(&mut a, "e");
    assert_eq!(at(&a), (0, 3), "end of the next note's tie");
    keys(&mut a, "e");
    assert_eq!(at(&a), (0, 5));
    // In the middle of a tie: `e` goes to its end, `b` to its start.
    a.pane_mut().cursor.beat = 2;
    keys(&mut a, "e");
    assert_eq!(at(&a), (0, 3));
    keys(&mut a, "b");
    assert_eq!(at(&a), (0, 2));
    // Another string finds its own notes.
    a.pane_mut().cursor.beat = 0;
    a.pane_mut().cursor.string = 1;
    keys(&mut a, "w");
    assert_eq!(at(&a), (0, 4));
}

#[test]
fn w_and_b_cross_bars_and_take_counts() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false)], &[]]);
    set_bar(&mut a, 1, &[&[], &[(0, 2, false)]]);
    set_bar(&mut a, 2, &[&[(0, 3, true)], &[(0, 4, false)]]);
    a.pane_mut().cursor.string = 0;
    keys(&mut a, "w");
    assert_eq!(at(&a), (1, 1));
    keys(&mut a, "w");
    assert_eq!(at(&a), (2, 1), "bar 3's first note is tied from bar 2");
    keys(&mut a, "gg2w");
    assert_eq!(at(&a), (2, 1));
    keys(&mut a, "$");
    keys(&mut a, "b");
    assert_eq!(at(&a), (1, 1), "b from the only note of the bar goes back");
    keys(&mut a, "b");
    assert_eq!(at(&a), (0, 0));
}

#[test]
fn w_e_b_on_a_standard_staff_use_the_line() {
    let mut a = app();
    // Same pitch (string 1, fret 5 = E4 / string 0 fret 0 = E4) on different strings share a
    // line; another pitch has another.
    set_bar(
        &mut a,
        0,
        &[
            &[(1, 5, false)],
            &[(0, 3, false)],
            &[(0, 0, false)],
            &[(0, 0, true)],
            &[(1, 5, false)],
        ],
    );
    a.pane_mut().cursor.beat = 0;
    to_standard(&mut a);
    keys(&mut a, "w");
    assert_eq!(at(&a), (0, 2), "the next note on the same line");
    keys(&mut a, "e");
    assert_eq!(at(&a), (0, 3), "end of the tie");
    keys(&mut a, "e");
    assert_eq!(at(&a), (0, 4));
    keys(&mut a, "bb");
    assert_eq!(at(&a), (0, 0));
}

#[test]
fn bar_first_and_last_note() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[&[], &[(0, 1, false)], &[], &[(3, 2, false)], &[]],
    );
    keys(&mut a, "$");
    assert_eq!(at(&a), (0, 3));
    keys(&mut a, "^");
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "gl");
    assert_eq!(at(&a), (0, 3));
    keys(&mut a, "gh");
    assert_eq!(at(&a), (0, 1));
    // `0` and <End> are the first and last beat; `W` and `B` the old w and b.
    keys(&mut a, "0");
    assert_eq!(at(&a), (0, 0));
    keys(&mut a, "<End>");
    assert_eq!(at(&a), (0, 4));
    keys(&mut a, "0W");
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "WB");
    assert_eq!(at(&a), (0, 1));
    // A bar with no notes: the first and last beats.
    set_bar(&mut a, 1, &[&[], &[], &[]]);
    keys(&mut a, "}$");
    assert_eq!(at(&a), (1, 2));
    keys(&mut a, "^");
    assert_eq!(at(&a), (1, 0));
}

#[test]
fn x_deletes_one_note_on_each_notation() {
    // Tab: the note on the cursor's string.
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false), (2, 5, false)]]);
    a.pane_mut().cursor.string = 2;
    keys(&mut a, "x");
    assert_eq!(frets(&a, 0), vec![vec![1]]);
    keys(&mut a, "u");
    assert_eq!(frets(&a, 0), vec![vec![1, 5]]);
    // Standard: the note at the cursor's line, not the chord.
    a.pane_mut().cursor.string = 0;
    to_standard(&mut a);
    keys(&mut a, "x");
    assert_eq!(frets(&a, 0), vec![vec![5]]);
    // Slash: the beat's slash is one thing; all its notes go.
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false), (2, 5, false)]]);
    a.run(Command::StaffAdd(Some("slash".into())));
    keys(&mut a, "x");
    assert_eq!(frets(&a, 0), vec![Vec::<u8>::new()]);
}

#[test]
fn x_on_a_pitched_track_removes_only_the_cursor_note() {
    let mut a = app();
    a.run(Command::TrackAdd("piano".into()));
    let b = a.pane().buffer;
    let beat = Beat {
        duration: Duration::new(DurationValue::Quarter),
        notes: vec![Note::new(0, 72), Note::new(1, 64)],
        ..Beat::default()
    };
    a.buffers[b].score.tracks[1].bars[0].voices[0].beats = vec![beat];
    let t = 1;
    a.pane_mut().cursor.track = t;
    a.pane_mut().cursor.bar = 0;
    a.pane_mut().cursor.beat = 0;
    let track = &a.buffer().score.tracks[t];
    let staff = track
        .staves()
        .iter()
        .position(|s| matches!(s, Staff::Standard(_)))
        .unwrap();
    let written = track.pitch_of(&Note::new(0, 72)) as i32 + track.staff_clef().transpose();
    a.pane_mut().cursor.staff = staff;
    a.pane_mut().cursor.line = staff::diatonic(written, 0);
    keys(&mut a, "x");
    let notes: Vec<u8> = a.buffer().score.tracks[t].bars[0].voices[0].beats[0]
        .notes
        .iter()
        .map(|n| n.fret)
        .collect();
    assert_eq!(notes, vec![64]);
}

#[test]
fn delete_beat_under_and_before_the_cursor() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[
            &[(0, 1, false)],
            &[(0, 2, false)],
            &[(0, 3, false)],
            &[(0, 4, false)],
        ],
    );
    keys(&mut a, "ldl");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![3], vec![4]]);
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "dh");
    assert_eq!(frets(&a, 0), vec![vec![3], vec![4]]);
    assert_eq!(at(&a), (0, 0));
    // The last beat of the score: `l` cannot move, `dl` still deletes the beat.
    keys(&mut a, "G$dl");
    assert_eq!(a.buffer().score.master_bars.len(), 8);
    set_bar(&mut a, 7, &[&[(0, 8, false)]]);
    keys(&mut a, "dl");
    assert_eq!(bar_beats(&a, 7).len(), 1);
    assert!(bar_beats(&a, 7)[0].empty, "the bar keeps a placeholder");
    // One undo each.
    keys(&mut a, "u");
    assert_eq!(frets(&a, 7), vec![vec![8]]);
    keys(&mut a, "gguu");
    assert_eq!(frets(&a, 0).len(), 4);
}

#[test]
fn dd_deletes_the_bar_and_yy_p_pastes_it() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false)]]);
    set_bar(&mut a, 1, &[&[(0, 2, false)]]);
    keys(&mut a, "yy");
    keys(&mut a, "p");
    assert_eq!(frets(&a, 1), vec![vec![1]], "pasted after the bar");
    assert_eq!(a.buffer().score.master_bars.len(), 9);
    keys(&mut a, "gg");
    keys(&mut a, "P");
    assert_eq!(frets(&a, 0), vec![vec![1]], "pasted before the bar");
    assert_eq!(a.buffer().score.master_bars.len(), 10);
    keys(&mut a, "dd");
    assert_eq!(a.buffer().score.master_bars.len(), 9);
    keys(&mut a, "u");
    assert_eq!(a.buffer().score.master_bars.len(), 10);
}

#[test]
fn yank_and_put_beats_before_and_after() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[&[(0, 1, false)], &[(0, 2, false)], &[(0, 3, false)]],
    );
    keys(&mut a, "yl");
    keys(&mut a, "ll");
    keys(&mut a, "p");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3], vec![1]]);
    keys(&mut a, "gg");
    keys(&mut a, "P");
    assert_eq!(
        frets(&a, 0),
        vec![vec![1], vec![1], vec![2], vec![3], vec![1]]
    );
    keys(&mut a, "u");
    assert_eq!(frets(&a, 0).len(), 4);
}

#[test]
fn change_beat_under_and_before_the_cursor_leaves_a_rest() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[&[(0, 1, false)], &[(0, 2, false)], &[(0, 3, false)]],
    );
    keys(&mut a, "l");
    keys(&mut a, "cl");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(frets(&a, 0), vec![vec![1], vec![], vec![3]]);
    assert!(bar_beats(&a, 0)[1].is_rest());
    assert_eq!(bar_beats(&a, 0)[1].duration.value, DurationValue::Quarter);
    assert_eq!(at(&a), (0, 1));
    // Type into it: still one undo step for the clear and the typing.
    keys(&mut a, "9<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![9], vec![3]]);
    keys(&mut a, "u");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3]]);
    // Before the cursor.
    keys(&mut a, "l");
    keys(&mut a, "ch");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(frets(&a, 0), vec![vec![1], vec![], vec![3]]);
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "<Esc>u");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3]]);
    // `cl` on the very last beat of the score.
    set_bar(&mut a, 7, &[&[(0, 8, false)]]);
    keys(&mut a, "G$cl");
    assert_eq!(a.mode, Mode::Insert);
    assert!(bar_beats(&a, 7)[0].is_rest());
}

#[test]
fn cc_clears_the_bar_of_the_cursor_track_and_inserts() {
    let mut a = app();
    a.run(Command::TrackAdd("bass".into()));
    set_bar(&mut a, 0, &[&[(0, 1, false)], &[(0, 2, false)]]);
    let b = a.pane().buffer;
    a.buffers[b].score.tracks[1].bars[0].voices[0].beats =
        vec![Beat::rest(Duration::new(DurationValue::Half)); 2];
    a.pane_mut().cursor.track = 0;
    keys(&mut a, "cc");
    assert_eq!(a.mode, Mode::Insert);
    assert!(bar_beats(&a, 0)[0].empty);
    assert_eq!(a.buffer().score.tracks[1].bars[0].voices[0].beats.len(), 2);
    keys(&mut a, "4<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![4]]);
    keys(&mut a, "u");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2]]);
}

#[test]
fn d_and_c_take_the_new_motions() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[
            &[(0, 1, false)],
            &[(0, 2, false)],
            &[(0, 3, false)],
            &[(0, 3, true)],
            &[(0, 5, false)],
        ],
    );
    // dw: up to (not including) the next note on the string.
    keys(&mut a, "dw");
    assert_eq!(frets(&a, 0).len(), 4);
    keys(&mut a, "u");
    // de: through the end of the next note's tie... from a lone note, the next note's tie.
    keys(&mut a, "lde");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![5]]);
    keys(&mut a, "u");
    // d$ is inclusive; d^ takes what is before the cursor from the first note.
    keys(&mut a, "d$");
    assert_eq!(frets(&a, 0), vec![vec![1]]);
    keys(&mut a, "u");
    keys(&mut a, "$d^");
    assert_eq!(frets(&a, 0), vec![vec![5]]);
}

#[test]
fn i_a_big_i_big_a_insert_a_beat_and_start_typing() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[&[(0, 1, false)], &[(0, 2, false)], &[(0, 3, false)]],
    );
    keys(&mut a, "l");
    keys(&mut a, "i");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(frets(&a, 0), vec![vec![1], vec![], vec![2], vec![3]]);
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "7<Esc>");
    assert_eq!(frets(&a, 0)[1], vec![7]);
    assert_eq!(undo_steps(&a), 1, "the beat and the fret are one step");
    keys(&mut a, "u");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3]]);

    keys(&mut a, "gg");
    keys(&mut a, "a8<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![8], vec![2], vec![3]]);
    keys(&mut a, "u");

    keys(&mut a, "lI6<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![6], vec![1], vec![2], vec![3]]);
    keys(&mut a, "u");

    keys(&mut a, "A5<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3], vec![5]]);
    assert_eq!(at(&a), (0, 3));
    keys(&mut a, "u");
    assert_eq!(frets(&a, 0).len(), 3);
}

#[test]
fn i_in_an_empty_bar_types_into_the_bar() {
    let mut a = app();
    keys(&mut a, "i3<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![3]]);
    assert!(!bar_beats(&a, 0)[0].empty);
}

#[test]
fn big_a_and_big_i_use_the_insert_duration() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false)]]);
    a.ed.insert_duration = Some(Duration::new(DurationValue::Eighth));
    keys(&mut a, "A<Esc>");
    assert_eq!(bar_beats(&a, 0)[1].duration.value, DurationValue::Eighth);
    assert!(bar_beats(&a, 0)[1].is_rest());
}

#[test]
fn o_and_big_o_open_a_bar_and_insert() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false)]]);
    keys(&mut a, "o");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(a.buffer().score.master_bars.len(), 9);
    assert_eq!(at(&a), (1, 0));
    keys(&mut a, "5<Esc>");
    assert_eq!(frets(&a, 1), vec![vec![5]]);
    assert_eq!(undo_steps(&a), 1, "the bar and the fret are one step");
    keys(&mut a, "u");
    assert_eq!(a.buffer().score.master_bars.len(), 8);
    assert_eq!(frets(&a, 0), vec![vec![1]]);

    keys(&mut a, "O4<Esc>");
    assert_eq!(a.buffer().score.master_bars.len(), 9);
    assert_eq!(at(&a), (0, 0));
    assert_eq!(frets(&a, 0), vec![vec![4]]);
    assert_eq!(frets(&a, 1), vec![vec![1]]);
    keys(&mut a, "u");
    assert_eq!(a.buffer().score.master_bars.len(), 8);
    // Redo brings both back.
    keys(&mut a, "U");
    assert_eq!(a.buffer().score.master_bars.len(), 9);
    assert_eq!(frets(&a, 0), vec![vec![4]]);
}

#[test]
fn visual_i_and_a_insert_around_the_selection() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[
            &[(0, 1, false)],
            &[(0, 2, false)],
            &[(0, 3, false)],
            &[(0, 4, false)],
        ],
    );
    keys(&mut a, "lvli");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(
        frets(&a, 0),
        vec![vec![1], vec![], vec![2], vec![3], vec![4]]
    );
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "<Esc>u");
    assert_eq!(frets(&a, 0).len(), 4);
    // After the selection, even when it was made backwards.
    keys(&mut a, "gglvha");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(
        frets(&a, 0),
        vec![vec![1], vec![2], vec![], vec![3], vec![4]]
    );
    assert_eq!(at(&a), (0, 2));
    keys(&mut a, "<Esc>u");
    // Bars: a beat at the start of the first / end of the last selected bar.
    set_bar(&mut a, 1, &[&[(0, 9, false)]]);
    keys(&mut a, "ggV}i");
    assert_eq!(at(&a), (0, 0));
    assert_eq!(frets(&a, 0).len(), 5);
    keys(&mut a, "<Esc>u");
    keys(&mut a, "ggV}a");
    assert_eq!(at(&a), (1, 1));
    assert_eq!(frets(&a, 1).len(), 2);
    keys(&mut a, "<Esc>");
    assert_eq!(a.mode, Mode::Normal);
}

#[test]
fn insert_mode_hjkl_move_and_capitals_toggle_techniques() {
    let mut a = app();
    set_bar(
        &mut a,
        0,
        &[&[(0, 1, false)], &[(1, 2, false)], &[(2, 3, false)]],
    );
    keys(&mut a, "gi");
    assert_eq!(a.mode, Mode::Insert);
    keys(&mut a, "l");
    assert_eq!(at(&a), (0, 1));
    keys(&mut a, "j");
    assert_eq!(a.pane().cursor.string, 1);
    keys(&mut a, "k");
    assert_eq!(a.pane().cursor.string, 0);
    keys(&mut a, "h");
    assert_eq!(at(&a), (0, 0));
    // H hammer-on / pull-off, L let ring.
    keys(&mut a, "HL");
    let n = &bar_beats(&a, 0)[0].notes[0];
    assert!(n.effects.hammer_pull);
    assert!(n.effects.let_ring);
    keys(&mut a, "<Esc>");
    // `l` in insert mode does not append a beat at the end of the bar.
    keys(&mut a, "gi");
    keys(&mut a, "lllll");
    assert_eq!(bar_beats(&a, 0).len(), 3);
}

#[test]
fn enter_in_a_tab_completes_the_fret_and_adds_an_empty_beat_after_it() {
    let mut a = app();
    a.pane_mut().cursor.string = 1;
    keys(&mut a, "i12<CR>");
    // The cursor stays on the beat; an empty beat follows it.
    assert_eq!(frets(&a, 0), vec![vec![12], vec![]]);
    assert_eq!(at(&a), (0, 0));
    // Enter again adds no second empty beat.
    keys(&mut a, "<CR>");
    assert_eq!(frets(&a, 0), vec![vec![12], vec![]]);
    // `l` moves on, and the next digit starts a new fret there.
    keys(&mut a, "l3<CR>");
    assert_eq!(frets(&a, 0)[1], vec![3]);
    assert_eq!(at(&a), (0, 1));
    assert_eq!(bar_beats(&a, 0).len(), 3);
    // Leaving takes away the empty beat nothing was entered in.
    keys(&mut a, "<Esc>");
    assert_eq!(bar_beats(&a, 0).len(), 2);
    assert_eq!(undo_steps(&a), 1);
}

#[test]
fn space_does_what_enter_does_and_the_bar_end_adds_no_beat() {
    let mut a = app();
    keys(&mut a, "i1<Space>");
    assert_eq!(at(&a), (0, 0));
    assert_eq!(frets(&a, 0), vec![vec![1], vec![]]);
    // A 4/4 bar of quarter beats: with the bar full there is no room for an empty one.
    keys(&mut a, "l2<Space>l3<Space>l4<Space>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3], vec![4]]);
    assert_eq!(at(&a), (0, 3));
}

#[test]
fn replace_mode_on_a_modern_score_adds_no_empty_beats() {
    let mut a = app();
    keys(&mut a, "i1<CR>l2<CR><Esc>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2]]);
    keys(&mut a, "0R");
    assert_eq!(a.mode, Mode::Insert);
    assert_eq!(a.focus_ring_color(), a.config.theme.error);
    keys(&mut a, "9<CR>");
    assert_eq!(frets(&a, 0), vec![vec![9], vec![2]]);
    assert_eq!(at(&a), (0, 0));
    keys(&mut a, "<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![9], vec![2]]);
}

#[test]
fn enter_on_a_standard_staff_creates_a_note_at_the_cursor() {
    let mut a = app();
    a.pane_mut().cursor.staff = 0;
    a.pane_mut().cursor.line = 32;
    a.pane_mut().cursor.string = 0;
    keys(&mut a, "i");
    keys(&mut a, "<CR>");
    assert_eq!(bar_beats(&a, 0)[0].notes.len(), 1);
    let t = track(&a);
    let written = t.pitch_of(&bar_beats(&a, 0)[0].notes[0]) as i32 + t.staff_clef().transpose();
    assert_eq!(staff::diatonic(written, 0), 32);
    assert_eq!(at(&a), (0, 0));
    // Next beat, a line up, then Enter again: a second note on the new beat.
    keys(&mut a, "lk<CR>");
    let t = track(&a);
    let written = t.pitch_of(&bar_beats(&a, 0)[1].notes[0]) as i32 + t.staff_clef().transpose();
    assert_eq!(staff::diatonic(written, 0), 33);
    // A note typed by letter is not doubled by Enter.
    keys(&mut a, "lc<CR>");
    assert_eq!(bar_beats(&a, 0)[2].notes.len(), 1);
    keys(&mut a, "<Esc>");
    assert_eq!(undo_steps(&a), 1);
}

#[test]
fn enter_on_a_slash_staff_puts_a_slash() {
    let mut a = app();
    a.run(Command::StaffAdd(Some("slash".into())));
    assert_eq!(a.insert_kind(), InsertKind::Slash);
    keys(&mut a, "i<CR>l<CR>");
    assert!(!bar_beats(&a, 0)[0].notes.is_empty());
    assert!(!bar_beats(&a, 0)[1].notes.is_empty());
    assert_eq!(at(&a), (0, 1));
}

/// An app showing a chant track (a score read from GABC).
fn chant_app() -> App {
    let mut a = app();
    let chant =
        cecilia_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n").unwrap();
    let b = a.pane().buffer;
    a.buffers[b].score = cecilia_formats::gabc::chant_score(chant);
    a.pane_mut().cursor = Cursor0::default();
    a
}

#[test]
fn chant_insert_moves_like_standard_and_keeps_its_letters() {
    let a = chant_app();
    assert_eq!(a.insert_kind(), InsertKind::Chant);
    let (c, _) = Config::parse("").unwrap();
    let chant = c.insert_keymap(InsertKind::Chant);
    // h j k l are not GABC pitches in insert mode: they fall through to the cursor movement.
    for k in ["h", "j", "k", "l"] {
        let keys = crate::keys::parse_keys(k, &[]).unwrap();
        assert!(!chant.bindings.contains_key(&keys), "{k}");
    }
    let names = |k: &str| chant.names[&crate::keys::parse_keys(k, &[]).unwrap()].clone();
    // The pitch letters are gone: the cursor's position and Enter set the pitch.
    let a_key = crate::keys::parse_keys("a", &[]).unwrap();
    assert!(!chant.bindings.contains_key(&a_key));
    assert_eq!(names("v"), vec!["chant-mod v".to_string()]);
    // Enter is the general Insert key (a note on the cursor's row, on to a new beat).
    assert!(
        !chant
            .bindings
            .contains_key(&crate::keys::parse_keys("<CR>", &[]).unwrap())
    );
    assert_eq!(names("<Space>"), vec!["insert-next".to_string()]);
}

#[test]
fn the_normal_keys_follow_the_notation() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false)], &[(1, 2, false)]]);
    // Tab: bend is a technique key.
    keys(&mut a, "tb");
    assert!(bar_beats(&a, 0)[0].notes[0].effects.bend.is_some());
    // Standard: no bend key; `t#` is the sharp.
    to_standard(&mut a);
    keys(&mut a, "tb");
    assert!(a.message.is_none() || bar_beats(&a, 0)[0].notes[0].effects.bend.is_some());
    let pitch = |a: &App| track(a).pitch_of(&bar_beats(a, 0)[0].notes[0]);
    let before = pitch(&a);
    keys(&mut a, "t#");
    assert_eq!(pitch(&a), before + 1);
    // Help lists the layer of the staff.
    let rows = a.help_view().1;
    assert!(rows.iter().any(|(k, _)| k == "t#"));
    a.pane_mut().cursor.staff = Cursor0::HOME;
    let rows = a.help_view().1;
    assert!(!rows.iter().any(|(k, _)| k == "t#"));
    assert!(rows.iter().any(|(k, _)| k == "tb"));
}

use crate::Cursor as Cursor0;

#[test]
fn the_palette_lists_the_tools_of_the_notation_and_mode() {
    let mut a = app();
    let commands = |a: &App| -> Vec<&'static str> {
        a.palette_rows()
            .iter()
            .filter(|r| !r.command.is_empty())
            .map(|r| r.command)
            .collect()
    };
    let titles = |a: &App| -> Vec<&'static str> {
        a.palette_rows()
            .iter()
            .filter(|r| r.command.is_empty())
            .map(|r| r.label)
            .collect()
    };
    // Tab.
    assert_eq!(a.insert_kind(), InsertKind::Tab);
    let tab = commands(&a);
    assert!(tab.contains(&"toggle bend"));
    assert!(tab.contains(&"toggle palm-mute"));
    assert!(!tab.contains(&"accidental sharp"));
    // Standard.
    a.pane_mut().cursor.staff = 0;
    let standard = commands(&a);
    assert!(standard.contains(&"accidental sharp"));
    assert!(!standard.contains(&"toggle bend"));
    assert!(!standard.contains(&"toggle palm-mute"));
    assert!(standard.contains(&"toggle tie"));
    assert!(standard.contains(&"duration 8"));
    // Slash.
    a.run(Command::StaffAdd(Some("slash".into())));
    let slash = commands(&a);
    assert!(slash.contains(&"duration 4"));
    assert!(slash.contains(&"toggle tie"));
    assert!(slash.contains(&"toggle accent"));
    assert!(!slash.contains(&"toggle bend"));
    assert!(!slash.contains(&"accidental sharp"));
    assert!(!slash.contains(&"octave-up"));
    // A chant score has its own palette: no durations, but clefs, divisions, heads and signs.
    {
        let c = chant_app();
        let chant = commands(&c);
        assert!(chant.contains(&"chant-clef c3") && chant.contains(&"chant-division ::"));
        assert!(chant.contains(&"chant-shape quilisma") && chant.contains(&"chant-mora"));
        assert!(
            chant
                .iter()
                .all(|c| c.starts_with("chant-") || c.starts_with("pitch-") || *c == "lyric-mode")
        );
        assert_eq!(
            titles(&c),
            vec![
                "Clef",
                "Divisions",
                "Shapes",
                "Liquescence",
                "Rhythm",
                "Alteration",
                "Pitch",
                "Text"
            ]
        );
    }
    // Visual mode: the range tools, on any staff.
    a.pane_mut().cursor.staff = Cursor0::HOME;
    assert!(commands(&a).contains(&"toggle bend"));
    keys(&mut a, "v");
    assert_eq!(a.mode, Mode::Visual);
    // Every tool that works over a range; none of the bar tools.
    let titles = titles(&a);
    assert!(titles.contains(&"Dynamics") && titles.contains(&"Slides"));
    assert!(!titles.contains(&"Bar") && !titles.contains(&"Repeats"));
    assert!(commands(&a).contains(&"rest"));
    assert!(commands(&a).contains(&"duration-longer"));
    assert!(!commands(&a).contains(&"key "));
    keys(&mut a, "<Esc>");
    assert!(commands(&a).contains(&"toggle bend"));
}

#[test]
fn palette_folds_are_remembered_per_kind_of_score() {
    let mut a = app();
    // Modern: section 1 is Duration (0 is the voice selector). Fold it.
    a.palette.folded.insert(1);
    let modern_rows = a.palette_rows().len();
    // A chant buffer has its own sections, none folded, and its own selection.
    let chant = chant_app();
    let score = chant.buffer().score.clone();
    assert!(score.is_chant());
    a.buffers.push(crate::app::Buffer {
        path: None,
        name: "t.gabc".into(),
        score,
        modified: false,
        history: Default::default(),
        version: 0,
        disk: None,
    });
    let b = a.buffers.len() - 1;
    a.show_buffer(b);
    assert!(a.buffer().score.is_chant());
    assert!(a.palette.folded.is_empty());
    let chant_rows = a.palette_rows().len();
    a.palette.selected = 0;
    a.run(Command::parse("palette-fold close").unwrap());
    assert!(a.palette_rows().len() < chant_rows);
    // Back on the modern score its fold is still there and the chant's is not shared.
    a.show_buffer(0);
    assert_eq!(a.palette_rows().len(), modern_rows);
    assert!(a.palette.folded.contains(&1));
    a.show_buffer(b);
    assert!(a.palette.folded.contains(&0));
    assert!(a.palette_rows().len() < chant_rows);
}

#[test]
fn the_example_config_documents_the_new_layers() {
    let text = include_str!("../../../docs/config.example.toml");
    for layer in [
        "normal-tab",
        "normal-standard",
        "normal-slash",
        "normal-chant",
    ] {
        assert!(text.contains(layer), "{layer}");
    }
}

#[test]
fn dot_repeats_the_new_insert_and_change_commands() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(0, 1, false)], &[(0, 2, false)]]);
    keys(&mut a, "A5<Esc>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![5]]);
    keys(&mut a, ".");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![5], vec![5]]);
    keys(&mut a, "gg");
    keys(&mut a, "cl7<Esc>");
    assert_eq!(frets(&a, 0)[0], vec![7]);
    keys(&mut a, "l.");
    assert_eq!(frets(&a, 0)[1], vec![7]);
    keys(&mut a, "3o");
    assert_eq!(a.buffer().score.master_bars.len(), 11);
}

#[test]
fn pitch_change_moves_the_cursor_with_the_note_on_a_standard_staff() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)]]);
    to_standard(&mut a);
    let line = a.pane().cursor.line;
    keys(&mut a, "<C-a><C-a>");
    let up = a.pane().cursor.line;
    assert!(up > line, "a tone up moves to the next line or space");
    // The cursor is still on the note: a further shift acts on it.
    let before = track(&a).bars[0].voices[0].beats[0].notes.len();
    keys(&mut a, "<C-x><C-x>");
    assert_eq!(a.pane().cursor.line, line);
    assert_eq!(track(&a).bars[0].voices[0].beats[0].notes.len(), before);
}

fn voices_of(a: &App, bar: usize) -> &[Voice] {
    &track(a).bars[bar].voices
}

#[test]
fn a_new_voice_is_empty_at_the_same_time_position() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[(2, 7, false)], &[], &[]]);
    keys(&mut a, "l");
    assert_eq!(at(&a), (0, 1));
    a.run(Command::parse("voice 2").unwrap());
    let c = a.pane().cursor;
    assert_eq!((c.voice, c.beat), (1, 0));
    let v = &voices_of(&a, 0)[1];
    assert_eq!(v.beats.len(), 1);
    assert!(v.beats[0].empty, "no rest is created");
    assert_eq!(v.ticks(), 0);
    // Back to the first voice: the cursor is on the beat sounding at the same time (the start).
    a.run(Command::parse("voice 1").unwrap());
    assert_eq!((a.pane().cursor.voice, a.pane().cursor.beat), (0, 0));
}

#[test]
fn editing_one_voice_leaves_the_others_alone() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[(2, 7, false)], &[], &[]]);
    a.run(Command::parse("voice 2").unwrap());
    keys(&mut a, "i");
    keys(&mut a, "3");
    keys(&mut a, "<Esc>");
    let bar = &track(&a).bars[0];
    assert_eq!(bar.voices[0].beats.len(), 4);
    assert_eq!(bar.voices[0].beats[0].notes[0].fret, 5);
    assert_eq!(bar.voices[1].beats[0].notes[0].fret, 3);
    // Undo covers the voice edit and its creation.
    keys(&mut a, "u");
    keys(&mut a, "u");
    assert_eq!(voices_of(&a, 0).len(), 1);
}

#[test]
fn voice_next_goes_round_through_the_bar_voices_and_one_more() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[], &[], &[]]);
    let v = |a: &App| a.pane().cursor.voice;
    a.run(Command::VoiceNext);
    assert_eq!(v(&a), 1);
    // The second voice is only a rest: going on from it wraps round to the first.
    a.run(Command::VoiceNext);
    assert_eq!(v(&a), 0);
    a.run(Command::parse("voice 4").unwrap());
    assert_eq!(v(&a), 3);
    assert_eq!(voices_of(&a, 0).len(), 4);
    a.run(Command::VoiceNext);
    assert_eq!(v(&a), 0);
    // Back from the first voice: the one more than those with notes.
    a.run(Command::VoicePrev);
    assert_eq!(v(&a), 1);
    a.run(Command::parse("voice 9").unwrap());
    assert_eq!(v(&a), 3);
}

#[test]
fn deleting_a_voice_content_gives_back_an_empty_voice() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[], &[], &[]]);
    a.run(Command::parse("voice 2").unwrap());
    keys(&mut a, "i");
    keys(&mut a, "3");
    keys(&mut a, "<Esc>");
    keys(&mut a, "cc");
    keys(&mut a, "<Esc>");
    let v = &voices_of(&a, 0)[1];
    assert!(v.is_empty());
    assert_eq!(voices_of(&a, 0)[0].beats.len(), 4);
}

#[test]
fn del_in_insert_mode_deletes_the_whole_beat() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[], &[], &[]]);
    let before = voices_of(&a, 0)[0].beats.len();
    keys(&mut a, "gi");
    keys(&mut a, "<Del>");
    assert_eq!(voices_of(&a, 0)[0].beats.len(), before - 1);
    assert!(
        voices_of(&a, 0)[0].beats[0].notes.is_empty(),
        "the noted beat is gone"
    );
}

#[test]
fn the_status_line_names_the_voice() {
    let mut a = app();
    let tag = |a: &App| {
        a.statusline(None, false)
            .right
            .iter()
            .find(|s| s.text.starts_with('V'))
            .map(|s| s.text.clone())
    };
    assert_eq!(tag(&a), None);
    a.run(Command::parse("voice 2").unwrap());
    assert_eq!(tag(&a), Some("V2".into()));
    keys(&mut a, "<A-F1>");
    assert_eq!(a.pane().cursor.voice, 0);
    keys(&mut a, "]v");
    assert_eq!(a.pane().cursor.voice, 1);
}

#[test]
fn rest_beats_fill_any_length() {
    for ticks in [1920, 1440, 2400, 3360, 960 + 240, 720] {
        let beats = rest_beats(ticks);
        assert_eq!(beats.iter().map(Beat::ticks).sum::<u32>(), ticks);
    }
    assert_eq!(rest_beats(1920).len(), 1);
}

#[test]
fn typing_into_a_new_voice_fills_it() {
    let mut a = app();
    set_bar(&mut a, 0, &[&[(2, 5, false)], &[], &[], &[]]);
    a.run(Command::parse("voice 2").unwrap());
    keys(&mut a, "i3<Esc>");
    let v = &voices_of(&a, 0)[1];
    assert_eq!(v.beats.len(), 1);
    assert_eq!(v.beats[0].notes[0].fret, 3);
}

/// The first syllable of each beat of bar 0.
fn syllables(a: &App) -> Vec<String> {
    bar_beats(a, 0)
        .iter()
        .map(|b| b.lyrics.first().cloned().unwrap_or_default())
        .collect()
}

fn lyric_bar(a: &mut App, text: &[&str]) {
    let spec: Vec<Vec<(u8, u8, bool)>> = text.iter().map(|_| vec![(0, 3, false)]).collect();
    let refs: Vec<Spec> = spec.iter().map(|s| &s[..]).collect();
    set_bar(a, 0, &refs);
    let b = a.pane().buffer;
    for (i, t) in text.iter().enumerate() {
        if !t.is_empty() {
            a.buffers[b].score.tracks[0].bars[0].voices[0].beats[i].lyrics = vec![t.to_string()];
        }
    }
}

#[test]
fn deleting_a_beat_shifts_the_syllables_on() {
    let mut a = app();
    lyric_bar(&mut a, &["A", "le", "lu", "ia", ""]);
    // Delete the second beat: "le" moves to "lu"'s note, and so on, into the free note.
    a.pane_mut().cursor.beat = 1;
    a.mode = Mode::Insert;
    keys(&mut a, "<Del><Esc>");
    assert_eq!(syllables(&a), ["A", "le", "lu", "ia"]);
    keys(&mut a, "u");
    assert_eq!(syllables(&a), ["A", "le", "lu", "ia", ""]);
}

#[test]
fn deleting_the_last_note_of_a_beat_shifts_its_syllable_on() {
    let mut a = app();
    lyric_bar(&mut a, &["A", "le", "lu", "ia", ""]);
    a.pane_mut().cursor.beat = 1;
    a.pane_mut().cursor.string = 0;
    a.mode = Mode::Insert;
    keys(&mut a, "<BS><Esc>");
    // The beat is a rest now with no syllable; the text goes on over the notes.
    assert_eq!(syllables(&a), ["A", "", "le", "lu", "ia"]);
}

#[test]
fn syllables_past_the_last_note_are_kept_and_return_with_new_notes() {
    let mut a = app();
    lyric_bar(&mut a, &["A", "le", "lu", "ia"]);
    a.pane_mut().cursor.beat = 1;
    a.mode = Mode::Insert;
    keys(&mut a, "<Del><Esc>");
    // Three notes, three syllables; "ia" is not shown but the track's text keeps it.
    assert_eq!(syllables(&a), ["A", "le", "lu"]);
    let kept = a.buffer().score.tracks[0].lyrics_text.clone();
    assert!(kept.ends_with("ia"), "{kept:?}");
    // A note added takes its place in the text again: no syllable stacks on a beat.
    keys(&mut a, "a3<Esc>");
    assert_eq!(syllables(&a), ["A", "le", "lu", "ia"]);
    keys(&mut a, "u");
    assert_eq!(syllables(&a), ["A", "le", "lu"]);
}

/// The lengths in quarters of the beats of bar `b` of track 0's first voice.
fn quarters(a: &App, b: usize) -> Vec<f64> {
    bar_beats(a, b)
        .iter()
        .filter(|x| !x.empty)
        .map(|x| x.ticks() as f64 / 960.0)
        .collect()
}

#[test]
fn reflow_moves_the_excess_into_the_next_bar_and_fills_the_gaps() {
    let mut a = app();
    let q: Spec = &[(0, 1, false)];
    // 4/4: five quarters, then two.
    set_bar(&mut a, 0, &[q, q, q, q, q]);
    set_bar(&mut a, 1, &[q, q]);
    keys(&mut a, ":reflow<CR>");
    assert_eq!(quarters(&a, 0), [1.0; 4]);
    assert_eq!(quarters(&a, 1), [1.0; 3]);
    assert_eq!(undo_steps(&a), 1);
    // A short bar takes notes from the next.
    set_bar(&mut a, 0, &[q, q, q]);
    set_bar(&mut a, 1, &[q, q, q, q]);
    keys(&mut a, ":reflow<CR>");
    assert_eq!(quarters(&a, 0), [1.0; 4]);
    assert_eq!(quarters(&a, 1), [1.0; 3]);
}

#[test]
fn reflow_splits_a_beat_over_the_barline_and_ties_it() {
    let mut a = app();
    let q: Spec = &[(0, 1, false)];
    // 3 quarters and a half note: the half note straddles the barline.
    set_bar(&mut a, 0, &[q, q, q]);
    let b = a.pane().buffer;
    let half = Beat {
        duration: Duration::new(DurationValue::Half),
        notes: vec![Note::new(1, 7)],
        ..Beat::default()
    };
    a.buffers[b].score.tracks[0].bars[0].voices[0]
        .beats
        .push(half);
    set_bar(&mut a, 1, &[]);
    keys(&mut a, ":reflow<CR>");
    assert_eq!(quarters(&a, 0), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(quarters(&a, 1), [1.0]);
    let (head, tail) = (&bar_beats(&a, 0)[3], &bar_beats(&a, 1)[0]);
    assert_eq!((head.notes[0].string, head.notes[0].fret), (1, 7));
    assert_eq!((tail.notes[0].string, tail.notes[0].fret), (1, 7));
    assert!(!head.notes[0].tie);
    assert!(tail.notes[0].tie);
}

#[test]
fn reflow_in_visual_mode_only_touches_the_selection() {
    let mut a = app();
    let q: Spec = &[(0, 1, false)];
    set_bar(&mut a, 0, &[q, q, q, q, q]);
    set_bar(&mut a, 1, &[q, q, q, q, q, q, q]);
    set_bar(&mut a, 2, &[q, q, q, q, q]);
    // Bars 2 and 3 selected: the excess of bar 2 shifts on, the last bar keeps what is left.
    a.pane_mut().cursor.bar = 1;
    a.run(Command::Visual { bars: true });
    a.run(Command::BarNext);
    keys(&mut a, ":reflow<CR>");
    assert_eq!(a.mode, Mode::Normal);
    assert_eq!(quarters(&a, 0), [1.0; 5]);
    assert_eq!(quarters(&a, 1), [1.0; 4]);
    assert_eq!(quarters(&a, 2), [1.0; 8]);
    // A selection of beats: only those are repacked, the rest of their bars stays.
    set_bar(&mut a, 0, &[q, q, q, q, q]);
    set_bar(&mut a, 1, &[q, q]);
    a.pane_mut().cursor.bar = 0;
    a.pane_mut().cursor.beat = 3;
    a.run(Command::Visual { bars: false });
    a.run(Command::BarNext);
    keys(&mut a, ":reflow<CR>");
    // Beats 4-5 of bar 1 and beat 1 of bar 2 repacked around the beats left out: bar 1 is
    // filled to four, bar 2 keeps the rest and the unselected beat after the area.
    assert_eq!(quarters(&a, 0), [1.0; 4]);
    assert_eq!(quarters(&a, 1), [1.0; 3]);
}

#[test]
fn reflow_only_changes_the_cursor_voice() {
    let mut a = app();
    let q: Spec = &[(0, 1, false)];
    set_bar(&mut a, 0, &[q, q, q, q, q]);
    let b = a.pane().buffer;
    let second = Voice {
        beats: vec![Beat::rest(Duration::new(DurationValue::Whole))],
    };
    a.buffers[b].score.tracks[0].bars[0]
        .voices
        .push(second.clone());
    keys(&mut a, ":reflow<CR>");
    assert_eq!(a.buffers[b].score.tracks[0].bars[0].voices[1], second);
    assert_eq!(quarters(&a, 0), [1.0; 4]);
}

/// A modern app with a chant buffer added; returns the chant buffer's index (buffer 0 is modern).
fn add_chant_buffer(a: &mut App) -> usize {
    let score = chant_app().buffer().score.clone();
    a.buffers.push(crate::app::Buffer {
        path: Some("t.gabc".into()),
        name: "t.gabc".into(),
        score,
        modified: false,
        history: Default::default(),
        version: 0,
        disk: None,
    });
    a.buffers.len() - 1
}

#[test]
fn the_status_line_pane_title_and_tab_name_the_kind_of_score() {
    use crate::statusline::SegKind;
    let mut a = app();
    let chant = add_chant_buffer(&mut a);
    let chip = |a: &App| {
        let s = a.statusline(None, false);
        s.left
            .iter()
            .find(|s| matches!(s.kind, SegKind::ScoreModern | SegKind::ScoreChant))
            .map(|s| (s.text.clone(), s.kind))
            .unwrap()
    };
    assert_eq!(chip(&a), ("MODERN".to_string(), SegKind::ScoreModern));
    assert_eq!(a.score_kind_name(0), "modern");
    a.show_buffer(chant);
    assert_eq!(chip(&a), ("CHANT".to_string(), SegKind::ScoreChant));
    assert_eq!(a.score_kind_name(chant), "chant");
    // The tab of a chant score has the chant icon whatever its file.
    let tabs = a.bufferline();
    assert_eq!(
        tabs.iter().find(|t| t.buffer == chant).unwrap().kind,
        "chant"
    );
    assert_ne!(tabs.iter().find(|t| t.buffer == 0).unwrap().kind, "chant");
    // Back to the modern buffer: the chip follows.
    a.show_buffer(0);
    assert_eq!(chip(&a).0, "MODERN");
}

#[test]
fn property_rows_follow_the_kind_of_score() {
    use crate::properties::{Field, PropTab, RowKind};
    let fields = |a: &App| -> Vec<Field> {
        a.property_rows()
            .iter()
            .filter_map(|r| match r.kind {
                RowKind::Field { field, .. } => Some(field),
                _ => None,
            })
            .collect()
    };
    let mut a = app();
    let chant = add_chant_buffer(&mut a);
    for tab in [PropTab::Score, PropTab::Tracks, PropTab::Lyrics] {
        a.props.tab = tab;
        let modern = fields(&a);
        a.show_buffer(chant);
        a.pane_mut().cursor = Cursor0::default();
        let chants = fields(&a);
        assert_ne!(modern, chants, "{tab:?}");
        match tab {
            PropTab::Score => {
                assert!(modern.contains(&Field::Title) && !chants.contains(&Field::Title));
                assert!(chants.contains(&Field::ChantHeader(0)));
                assert!(chants.contains(&Field::ChantInterpretation));
                assert!(chants.contains(&Field::ChantStaffLines));
                // The page section is shared.
                assert!(chants.contains(&Field::PageWidth));
            }
            PropTab::Tracks => {
                for f in [
                    Field::Capo,
                    Field::FretCount,
                    Field::Tuning,
                    Field::StaffAt(0),
                ] {
                    assert!(!chants.contains(&f), "{f:?}");
                }
                assert!(chants.contains(&Field::ChantClef) && chants.contains(&Field::Volume));
                assert!(chants.contains(&Field::TrackName) && chants.contains(&Field::Mute));
                assert!(!modern.contains(&Field::ChantClef));
            }
            PropTab::Lyrics => {}
        }
        a.show_buffer(0);
    }
}

#[test]
fn chant_score_properties_edit_the_chant() {
    use crate::properties::{Field, PropTab};
    let mut a = app();
    let chant = add_chant_buffer(&mut a);
    a.show_buffer(chant);
    a.pane_mut().cursor = Cursor0::default();
    a.props.tab = PropTab::Score;
    let chant_of = |a: &App| a.buffer().score.tracks[0].chant.as_deref().unwrap().clone();
    // A header (mode) is added, then changed: one undo step each.
    let mode = crate::properties::chant_header_keys(&chant_of(&a))
        .iter()
        .position(|k| k == "mode")
        .unwrap();
    a.commit_property(Field::ChantHeader(mode), "8", false);
    assert!(
        chant_of(&a)
            .headers
            .contains(&("mode".to_string(), "8".to_string()))
    );
    a.commit_property(Field::ChantStaffLines, "5", false);
    assert_eq!(chant_of(&a).staff_lines, 5);
    a.commit_property(Field::ChantInterpretation, "solesmes", false);
    assert_eq!(
        chant_of(&a).interpretation,
        cecilia_core::chant::Interpretation::Solesmes
    );
    a.run(Command::Undo);
    assert_eq!(
        chant_of(&a).interpretation,
        cecilia_core::chant::Interpretation::Equalist
    );
    a.run(Command::Undo);
    assert_eq!(chant_of(&a).staff_lines, 4);
    a.run(Command::Undo);
    assert!(!chant_of(&a).headers.iter().any(|(k, _)| k == "mode"));
    // The header that names the score is its title on the page.
    let name = crate::properties::chant_header_keys(&chant_of(&a))
        .iter()
        .position(|k| k == "name")
        .unwrap();
    a.commit_property(Field::ChantHeader(name), "New name", false);
    assert_eq!(a.buffer().score.meta.title, "New name");
}

#[test]
fn a_command_of_the_other_kind_errors_naming_the_kind() {
    let mut a = app();
    let chant = add_chant_buffer(&mut a);
    // Modern: a chant command parses but is refused with the kind.
    assert_eq!(
        a.parse_for_score("chant-mora").unwrap_err(),
        "chant-mora: not in a modern score"
    );
    assert!(a.parse_for_score("capo 2").is_ok());
    a.show_buffer(chant);
    assert_eq!(
        a.parse_for_score("capo 2").unwrap_err(),
        "capo: not in a chant score"
    );
    assert_eq!(
        a.parse_for_score(":tuning guitar").unwrap_err(),
        "tuning: not in a chant score"
    );
    assert!(a.parse_for_score("chant-mora").is_ok() && a.parse_for_score("write").is_ok());
    // An unknown command is still unknown.
    assert!(
        a.parse_for_score("nonsense")
            .unwrap_err()
            .contains("unknown command")
    );
    // Typed on the command line, the refusal shows as the error.
    keys(&mut a, ":capo 2<CR>");
    let m = a.message.as_ref().unwrap();
    assert!(
        m.error && m.text == "capo: not in a chant score",
        "{}",
        m.text
    );
}

#[test]
fn completion_help_and_which_key_follow_the_kind() {
    let mut a = app();
    let chant = add_chant_buffer(&mut a);
    let listed = |a: &mut App| -> Vec<String> {
        a.open_command_line("");
        let p = a.palette().unwrap();
        a.mode = crate::config::Mode::Normal;
        p.items.into_iter().map(|i| i.text).collect()
    };
    let modern = listed(&mut a);
    assert!(modern.contains(&"capo".to_string()) && !modern.contains(&"chant-mora".to_string()));
    a.show_buffer(chant);
    let chants = listed(&mut a);
    assert!(chants.contains(&"chant-mora".to_string()) && !chants.contains(&"capo".to_string()));
    // The chant layer binds the t keys to chant commands; they are not on a modern score.
    let which = |a: &mut App| {
        a.pending = crate::keys::parse_keys("t", &[]).unwrap();
        a.which_key().map(|(_, rows)| rows).unwrap_or_default()
    };
    let rows = which(&mut a);
    assert!(
        rows.iter().any(|(_, c)| c.contains("chant-mora")),
        "{rows:?}"
    );
    a.show_buffer(0);
    let rows = which(&mut a);
    assert!(!rows.iter().any(|(_, c)| c.contains("chant-")), "{rows:?}");
}
