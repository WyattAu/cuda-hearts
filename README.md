# CUDA HEARTS

**A leather-jacket love simulator.** An unofficial parody visual novel built
with a custom bevy VN engine. You, two backstage passes, and the man who
insists the more you buy, the more you save.

> ### ⚠️ Parody disclaimer
>
> This is a **fan-made satire**. It is a work of **fiction** created for
> humor. It is not affiliated with, endorsed by, or connected to NVIDIA or
> Jensen Huang in any way. All dialogue, events, and romantic outcomes are
> invented and played for affectionate, good-natured comedy. Do not treat
> anything in this game as a factual claim about any real person.

## Status: scaffold

The engine is playable end-to-end; the story is one scene. Everything below
is real and working:

- [x] Line-based `.vn` script format (parser with line-numbered errors)
- [x] Branching via labels + choices, `affinity` stat, story flags
- [x] Multiple endings, scene/stage direction directives
- [x] Typewriter dialogue box, choice buttons, title screen (bevy 0.20)
- [x] Save/load (F5 / F9) as JSON snapshots
- [ ] More chapters (`assets/scripts/*.vn` — just add files)
- [ ] Real sprite/background art (stage chips are colored placeholders)
- [ ] BGM + SFX
- [ ] Endings gallery, flag-gated dialogue, affinity-ranked endings

## Run it

```sh
cargo run --features gui
```

Linux build deps (GUI only — CI builds the engine without them): audio is
disabled (no alsa) and Wayland goes through XWayland (no libwayland-dev
needed), so a Vulkan-capable runtime is all that's required. On Arch:

```sh
pacman -S --needed vulkan-radeon libx11 libxkbcommon-x11 # as appropriate for your GPU
```

### Controls

| Key | Action |
| --- | ------ |
| Click / Space / Enter | Advance (or complete the typewriter line) |
| 1–9 | *(planned)* pick choice by number |
| F5 / F9 | Save / load |
| R | Restart after an ending |
| Esc | Quit |

## Writing the story

Everything is `assets/scripts/*.vn` — one file per chapter, picked up
automatically at boot. `prologue.vn` doubles as the format reference:

```text
# comment (also: blank lines are ignored)
~ scene gtc_backstage              # background switch
~ show jensen happy                # cast member on stage, with mood
~ hide jensen                      # leave the stage
~ label flirt                      # branch target
~ jump encore                      # go to label
~ affinity +2                      # relationship score (int, +/−)
~ flag cuda_rizz                   # set a story flag
~ ending first_keynote             # ending; subsequent next() is Done
jensen: The more you love, the more you save.
you: Are you a data center? Because my heart just doubled its throughput.
narrator: He says it to the espresso machine. It still works on you.
* Flirt with silicon metaphors -> flirt
* Ask about the jacket -> jacket
```

Rules:

- Speakers must exist in `src/vn/cast.rs` (`CAST`), or be `narrator`/`you`.
  Unknown speakers fail the parse with a line number — add your cast there.
- Ids (`scene`, `label`, `flag`, `cast`, `mood`) are `[a-z_][a-z0-9_]*`.
- Consecutive `*` lines form one choice block; each option targets a label.
- Labels are checked at parse time; jumps/choices to missing labels are
  runtime errors. The integration tests play **every** choice path to an
  ending, so a broken branch fails CI.

### Adding a character

1. Add an entry to `CAST` in `src/vn/cast.rs` (id, display name, hex color).
2. Use them in any script: `~ show <id> <mood>` and `<id>: <line>`.

## Architecture

```
src/
├── vn/               # pure story engine — no bevy, no GPU, fully tested
│   ├── script.rs     #   .vn parser: Step model, labels, line-numbered errors
│   ├── state.rs      #   Playhead: advance/pick/jump, affinity, flags, stage
│   ├── cast.rs       #   character registry (names, colors)
│   └── save.rs       #   JSON save snapshots
├── game/             # bevy presentation layer (feature: gui)
│   ├── mod.rs        #   plugin, systems, Present bridge, saves
│   └── ui.rs         #   title + dialogue HUD layout
└── main.rs
assets/scripts/       # the story itself
```

The `vn` crate is dependency-light (`serde` only) so CI runs the full gate
without graphics system libraries; the `gui` feature is the only thing
pulling bevy, and bevy's feature set excludes audio/3d/gamepad/wayland for
the same reason (XWayland covers Wayland desktops). `Playhead` is the whole state machine — if you ever want a different
renderer (web, TUI), it ports unchanged.

### Design notes

- `Present` (`src/game/mod.rs`) is the single bridge: systems drive
  `Playhead` → `Present`, and one `sync_ui` system mirrors it into entities.
- Stage "sprites" are colored name chips until real art exists; the stage
  model (`StageActor { cast, mood }`) is already sprite-shaped.
- Choices block the playhead (`VnError::ChoicePending`) instead of
  desynchronizing it — the UI cannot accidentally skip one.

## Development

```sh
export CARGO_TARGET_DIR="$HOME/.cache/cargo-target"   # estate shared target
cargo test                                            # story engine (fast)
cargo clippy --features gui --all-targets             # full lint pass
cargo run --features gui                              # play
```

CI reuses the estate `rust-kit.yml` workflow at tier c (onboarding),
`all-features: false` — the `gui` feature needs a display stack that CI
runners do not have; `tests/vn.rs` still plays every branch of every
checked-in script, headlessly.

## License

MIT OR Apache-2.0. Parody content only; see the disclaimer above.
