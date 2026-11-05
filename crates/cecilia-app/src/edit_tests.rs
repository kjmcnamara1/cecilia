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
fn enter_in_a_tab_completes_the_fret_and_goes_on() {
    let mut a = app();
    a.pane_mut().cursor.string = 1;
    keys(&mut a, "i12<CR>");
    assert_eq!(frets(&a, 0), vec![vec![12], vec![]]);
    assert_eq!(at(&a), (0, 1));
    // The next digit starts a new fret on the new beat.
    keys(&mut a, "3<CR>");
    assert_eq!(frets(&a, 0)[1], vec![3]);
    assert_eq!(at(&a), (0, 2));
    // With nothing typed, Enter leaves a rest and moves on.
    keys(&mut a, "<CR>");
    assert!(bar_beats(&a, 0)[2].is_rest());
    assert_eq!(at(&a), (0, 3));
    keys(&mut a, "<Esc>");
    assert_eq!(undo_steps(&a), 1);
}

#[test]
fn enter_jumps_to_the_next_bar_after_the_last_complete_beat() {
    let mut a = app();
    // A 4/4 bar of quarter beats: the fourth Enter lands on bar 2.
    keys(&mut a, "i1<CR>2<CR>3<CR>4<CR>");
    assert_eq!(frets(&a, 0), vec![vec![1], vec![2], vec![3], vec![4]]);
    assert_eq!(at(&a), (1, 0));
    keys(&mut a, "5<CR>");
    assert_eq!(frets(&a, 1), vec![vec![5], vec![]]);
    assert_eq!(at(&a), (1, 1));
    // At the end of the score a bar is added.
    set_bar(&mut a, 7, &[&[(0, 1, false)][..]; 3]);
    a.pane_mut().cursor.bar = 7;
    a.pane_mut().cursor.beat = 2;
    keys(&mut a, "<Esc>gi");
    a.run(Command::GotoBar(8));
    a.pane_mut().cursor.beat = 2;
    keys(&mut a, "<CR>");
    assert_eq!(at(&a), (7, 3), "room for a fourth beat");
    keys(&mut a, "<CR>");
    assert_eq!(a.buffer().score.master_bars.len(), 9);
    assert_eq!(at(&a), (8, 0));
}

#[test]
fn enter_uses_the_duration_tool() {
    let mut a = app();
    a.ed.insert_duration = Some(Duration::new(DurationValue::Eighth));
    keys(&mut a, "i2<CR>3<CR>");
    assert_eq!(bar_beats(&a, 0)[0].duration.value, DurationValue::Eighth);
    assert_eq!(bar_beats(&a, 0)[1].duration.value, DurationValue::Eighth);
    // Eight eighths fill the bar: the eighth Enter jumps to the next bar.
    keys(&mut a, "1<CR>1<CR>1<CR>1<CR>1<CR>1<CR>");
    assert_eq!(at(&a), (1, 0));
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
    assert_eq!(at(&a), (0, 1));
    // A line up, then Enter again: a second note on the new beat.
    keys(&mut a, "k<CR>");
    let t = track(&a);
    let written = t.pitch_of(&bar_beats(&a, 0)[1].notes[0]) as i32 + t.staff_clef().transpose();
    assert_eq!(staff::diatonic(written, 0), 33);
    // A note typed by letter is not doubled by Enter.
    keys(&mut a, "c<CR>");
    assert_eq!(bar_beats(&a, 0)[2].notes.len(), 1);
    keys(&mut a, "<Esc>");
    assert_eq!(undo_steps(&a), 1);
}

#[test]
fn enter_on_a_slash_staff_puts_a_slash() {
    let mut a = app();
    a.run(Command::StaffAdd(Some("slash".into())));
    assert_eq!(a.insert_kind(), InsertKind::Slash);
    keys(&mut a, "i<CR><CR>");
    assert!(!bar_beats(&a, 0)[0].notes.is_empty());
    assert!(!bar_beats(&a, 0)[1].notes.is_empty());
    assert_eq!(at(&a), (0, 2));
}

#[test]
fn chant_insert_moves_like_standard_and_keeps_its_letters() {
    let mut a = app();
    a.run(Command::StaffAdd(Some("chant".into())));
    assert_eq!(a.insert_kind(), InsertKind::Chant);
    let (c, _) = Config::parse("").unwrap();
    let chant = c.insert_keymap(InsertKind::Chant);
    // h j k l are not GABC pitches in insert mode: they fall through to the cursor movement.
    for k in ["h", "j", "k", "l"] {
        let keys = crate::keys::parse_keys(k, &[]).unwrap();
        assert!(!chant.bindings.contains_key(&keys), "{k}");
    }
    let names = |k: &str| chant.names[&crate::keys::parse_keys(k, &[]).unwrap()].clone();
    assert_eq!(names("a"), vec!["chant-note a".to_string()]);
    assert_eq!(names("<CR>"), vec!["chant-syllable".to_string()]);
    assert_eq!(names("<Space>"), vec!["insert-next".to_string()]);
}

#[test]
fn enter_and_space_on_a_chant_staff_of_an_ordinary_track() {
    let mut a = app();
    a.run(Command::StaffAdd(Some("chant".into())));
    a.pane_mut().cursor.line = 6;
    keys(&mut a, "i<CR>");
    // A punctum at the cursor's position in the beat, then on to the next beat.
    assert_eq!(bar_beats(&a, 0)[0].notes.len(), 1);
    assert!(bar_beats(&a, 0)[0].notes[0].chant.as_ref().unwrap().punctum);
    assert_eq!(at(&a), (0, 1));
    // k moves the cursor up a position; Enter puts the next punctum there.
    keys(&mut a, "k");
    let line = a.pane().cursor.line;
    assert_eq!(a.pane().cursor.beat, 1);
    keys(&mut a, "<CR>");
    assert_eq!(bar_beats(&a, 0)[1].notes.len(), 1);
    assert_eq!(at(&a), (0, 2));
    assert_ne!(line, 6);
    // Space moves on without a note.
    keys(&mut a, "<Space>");
    assert!(bar_beats(&a, 0)[2].notes.is_empty());
    assert_eq!(at(&a), (0, 3));
    // h goes back a beat.
    keys(&mut a, "h");
    assert_eq!(at(&a), (0, 2));
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
    // Chant: note values and the clef; sections with nothing left are gone.
    a.run(Command::StaffAdd(Some("chant".into())));
    let chant = commands(&a);
    assert!(chant.iter().all(|c| c.starts_with("duration")
        || matches!(*c, "dot" | "double-dot" | "clef ")
        || c.starts_with("tuplet")));
    assert_eq!(titles(&a), vec!["Duration", "Bar"]);
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
fn folding_survives_a_notation_that_hides_sections() {
    let mut a = app();
    // Section 1 is Duration (0 is the voice selector).
    a.palette.folded.insert(1);
    a.pane_mut().cursor.staff = 0;
    a.run(Command::StaffAdd(Some("chant".into())));
    // Only the Duration and Bar sections are left (no voices), and Duration is still folded.
    let rows = a.palette_rows();
    assert_eq!(rows.len(), 3);
    a.run(Command::parse("palette-fold open").unwrap());
    assert!(a.palette_rows().len() > 1);
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
    keys(&mut a, "<CR>");
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
