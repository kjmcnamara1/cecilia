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
| `<leader>l`               | loop selection (A–B)      |        |
| `<leader>c`               | toggle count-in           |        |
| `<leader>m`               | toggle metronome          |        |
| `<leader>-` / `<leader>=` | speed −5% / +5%           |        |
| `<leader>a`               | toggle audio track / MIDI |        |

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

### Editing (M4)

| keys                          | command                                     |
| ----------------------------- | ------------------------------------------- |
| `i` / `a`                     | insert before / after cursor beat           |
| `r`                           | replace note under cursor                   |
| `x`                           | delete note                                 |
| `d{motion}` / `dd`            | delete beats / bar                          |
| `y{motion}` / `yy`, `p` / `P` | yank / put                                  |
| `c{motion}`                   | change                                      |
| `.`                           | repeat last change                          |
| `u` / `<C-r>`                 | undo / redo; `g-` / `g+` walk the undo tree |
| `q{reg}` / `@{reg}`           | record / play macro                         |
| `+` / `-`                     | lengthen / shorten duration                 |
| `<C-a>` / `<C-x>`             | fret / pitch up / down                      |
| `v` / `V` / `<C-v>`           | visual beats / bars / block                 |

### Other

| keys        | command          |        |
| ----------- | ---------------- | ------ |
| `:`         | command line     | _(M1)_ |
| `<leader>?` | which-key / help |        |
| `ZZ`        | write and quit   |        |

## Insert mode (M4)

| keys                        | command                                                                          |
| --------------------------- | -------------------------------------------------------------------------------- |
| `0`–`9`                     | fret (two digits typed quickly = two-digit fret)                                 |
| `<Esc>`                     | back to Normal                                                                   |
| `h` `p` `b` `s` `~` `x` `m` | hammer/pull, bend, slide, vibrato, dead note, palm mute (toggle on current note) |
| `1`..`7` with `<A-…>`       | duration whole..64th                                                             |
| `.`                         | toggle dot                                                                       |
| `t`                         | tie                                                                              |
| `r`                         | rest                                                                             |

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
| `:tempo {percent}`                               | playback speed                 |
| `:audio {file}` / `:sync`                        | attach audio / enter Sync mode |
| `:export {file}`                                 | export by extension            |
