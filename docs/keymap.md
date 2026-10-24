# Default keymap

All bindings can be overridden in `config.toml` under `[keymap.<mode>]`. `<leader>` defaults to `<Space>`.
Items marked _(M1)_ are implemented in the first milestone; the rest are the planned defaults.

## Normal mode

### Cursor movement

| keys                | command                                         |        |
| ------------------- | ----------------------------------------------- | ------ |
| `h` / `l`           | previous / next beat                            | _(M1)_ |
| `j` / `k`           | next / previous string (lower / higher)         | _(M1)_ |
| `w` / `b`           | next / previous beat with notes                 | _(M1)_ |
| `}` / `{`           | next / previous bar                             | _(M1)_ |
| `0` / `$`           | first / last beat of bar                        | _(M1)_ |
| `gg` / `G`          | first bar / last bar (`[count]G` = bar _count_) | _(M1)_ |
| `J` / `K`           | next / previous track                           | _(M1)_ |
| `]s` / `[s`         | next / previous section marker                  |        |
| `m{a-z}` / `'{a-z}` | set / jump to mark                              |        |
| `<C-d>` / `<C-u>`   | half page down / up                             | _(M1)_ |

### Playback

| keys                      | command                   |        |
| ------------------------- | ------------------------- | ------ |
| `<leader><leader>`        | play / pause from cursor  | _(M1)_ |
| `<leader>s`               | stop                      | _(M1)_ |
| `<leader>f`               | toggle follow-cursor      | _(M1)_ |
| `<leader>l`               | loop the selected bars (or the cursor bar) / loop off | _(M5)_ |
| `<leader>c`               | toggle count-in           | _(M5)_ |
| `<leader>m`               | toggle metronome          | _(M5)_ |
| `<leader>-` / `<leader>=` | speed −5% / +5%           | _(M5)_ |
| `<leader>a`               | cycle mix: audio + MIDI / audio only / MIDI only | _(M5)_ |
| `<leader>t`               | sync tap: place the next bar at the current audio time | _(M5)_ |
| `<leader>w`               | show / hide the waveform strip (GUI) | _(M5)_ |
| `<leader>v`               | cycle score layout (vertical / page / horizontal) | _(M3)_ |
| `<C-=>` / `<C-->` / `<C-0>` | zoom in / out / reset (GUI; also Ctrl+wheel) | _(M3)_ |

### Panes, tabs, buffers

| keys                | command                         |        |
| ------------------- | ------------------------------- | ------ |
| `<C-w>s` / `<C-w>v` | split horizontally / vertically | _(M1)_ |
| `<C-w>h/j/k/l`      | focus pane left/down/up/right   | _(M1)_ |
| `<C-w>q`            | close pane                      | _(M1)_ |
| `<C-w>=`            | equalise pane sizes             |        |
| `gt` / `gT`         | next / previous tab             | _(M1)_ |
| `<leader>b`         | buffer list                     |        |
| `<leader>e`         | file tree                       |        |

### Editing (M4, done)

| keys | command |
| --- | --- |
| `i` / `a` | insert at the cursor beat / append a beat and insert |
| `x` | delete the note on the cursor string (`[count]x`) |
| `d{motion}` / `dd` | delete beats / bars (`[count]dd`) |
| `y{motion}` / `yy`, `p` / `P` | yank beats or bars / put after / before |
| `c{motion}` / `cc` | change (delete, then insert) |
| `"{reg}` | use register for the next yank/delete/put |
| `.` | repeat the last change (including an insert session) |
| `u` / `<C-r>` / `g-` / `g+` | undo / redo / older / newer state across undo branches |
| `q{reg}` … `q` / `[count]@{reg}` / `@@` | record / play / replay a macro |
| `v` / `V` | visual beats / visual bars (then `d`, `y`, `c`, `x`) |
| `o` / `O` | new bar after / before |
| `+` / `-` | longer / shorter note value |
| `<C-a>` / `<C-x>` | pitch up / down a semitone (moves string when the fret runs out) |
| `r` | make the beat a rest |
| `th` `tb` `ts` `tv` `tx` `tm` `tl` `tg` `ta` `tn` `tt` | toggle hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |
| `t.` / `t3` | toggle dot / triplet |
| `ZZ` | write and close |

### Other

| keys        | command          |        |
| ----------- | ---------------- | ------ |
| `:`         | command line     | _(M1)_ |
| `<leader>?` | which-key / help |        |
| `ZZ`        | write and quit   |        |

## Insert mode (M4, done)

| keys | command |
| --- | --- |
| `0`–`9` | fret on the cursor string (two quick digits = two-digit fret; MIDI key on drum/pitched tracks) |
| `<Space>` / `<Right>` | next beat, appending one (or moving to the next bar) at the end |
| `<Left>` / `<Up>` / `<Down>` | previous beat / string up / string down |
| `<CR>` | new beat after the cursor |
| `<BS>` / `<Del>` | delete the note |
| `h` `b` `s` `~` `x` `m` `l` `g` `a` `n` `t` | hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |
| `r` / `.` / `<C-t>` | rest / dot / triplet |
| `+` / `-` / `<A-1>`…`<A-7>` | longer / shorter / whole … 64th |
| `<Esc>` | back to Normal (the whole session is one undo step) |

## Command mode (selected)

| command                                          |                                |
| ------------------------------------------------ | ------------------------------ |
| `:e {file}` / `:w [file]` / `:q` / `:wq` / `:q!` | _(M2)_ `:w file.ext` exports by extension |
| `:tabnew [file]` / `:tabclose`                   | _(M1)_                         |
| `:split` / `:vsplit`                             | _(M1)_                         |
| `:track {n}`                                     | _(M1)_                         |
| `:bar {n}` / `:{n}`                              | _(M1)_                         |
| `:set {option}[=value]`                          |                                |
| `:source`                                        | reload config _(M1)_           |
| `:time 3/4` / `:key -2` / `:tempo 96` | time / key signature from the cursor bar, tempo at the cursor bar |
| `:repeat-start` / `:repeat-end [n]` / `:section NAME` / `:text TEXT` | bar marks and beat text |
| `:note C#4` / `:voice 2` | set the cursor note's pitch / edit another voice |
| `:tuning drop-d` or `:tuning D2 A2 D3 G3 B3 E4` / `:capo 2` | retune keeping pitches |
| `:refinger` / `:transpose 2` | re-finger the track to minimise hand movement / transpose |
| `:track-add bass` / `:track-delete` / `:new` | tracks and new scores |
| `:speed 75` | playback speed in percent |

### Chant _(M6)_

On a chant track the cursor moves note by note; `<C-a>`/`<C-x>` move a note up or down (adding a
flat or natural when needed), `x` deletes it and `u` undoes, as on any track.

| command | |
| --- | --- |
| `:gabc` / `:gabc fgh` / `:gabc Glo(fg)ri(h)` | show the cursor syllable as GABC; replace its notes, or the syllable itself |
| `:interpretation [equalist\|solesmes]` | rhythm used for playback and the modern transcription |
| `:clef c3` | change the chant clef (`c1`–`c4`, `f1`–`f4`, `cb3` for a flat clef); notes keep their staff positions |
| `:chant-view` | square notation / modern transcription of chant tracks |
| `:modern-to-chant` | open the cursor track's melody as chant in a new buffer (syllables from lyrics) |
| `:chant-to-modern` | turn the chant track into an ordinary staff track |

### Play-along _(M5)_

| command | |
| --- | --- |
| `:audio [file]` / `:audio off` | attach a recording (mp3, m4a/aac, flac, ogg, wav; relative to the score). Without a file, the best name match next to the score or in its `mp3/`/`audio/` folder |
| `:sync` | start (or stop) sync tapping from the cursor bar: play, then `<leader>t` on each downbeat |
| `:sync-tap` / `:sync-clear` | add a sync point at the playing bar / remove all sync points |
| `:sync-detect` | beat-detection assist: fill in sync points from the last existing one (tap the first downbeat first for best results) |
| `:mix [both\|audio\|midi]` | what plays when a recording is attached (no argument cycles) |
| `:volume midi\|audio\|metronome {percent}` | source levels |
| `:mute [n]` / `:solo [n]` | toggle a track's mute / solo (cursor track by default) |
| `:loop [a b]` / `:loop-toggle` | loop bars a–b (1-based), or the selection / cursor bar |
| `:trainer start target step` | speed trainer: start at `start`%, add `step`% each loop up to `target`% (e.g. `:trainer 60 100 5`) |
| `:pitch {semitones}` | shift the recording's pitch (−12…12, keeps tempo) |
| `:metronome` / `:count-in` / `:waveform` | toggles |
| `:export {file}`                                 | export by extension            |
| `:layout [page\|vertical\|horizontal]`            | GUI score layout _(M3)_        |
