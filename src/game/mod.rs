//! Bevy presentation layer.
//!
//! Wires the pure [`crate::vn`] state machine to a window: title screen,
//! dialogue box with typewriter reveal, choice buttons, placeholder stage
//! chips (until real sprite art exists), scene background, and F5/F9 saves.
//!
//! Architecture: systems push engine state through the [`Present`] resource,
//! and [`sync_ui`] mirrors it into UI entities every frame. The story state
//! itself never touches bevy types, so hot-swapping the renderer stays
//! trivial.

pub mod ui;

use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::Path;

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use crate::vn::cast;
use crate::vn::save;
use crate::vn::script::{ChoiceOption, Script};
use crate::vn::state::{Flow, Playhead, StageActor, VnError};

/// Scripts live beside the crate during development (`cargo run`).
pub const SCRIPT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/scripts");

/// Asset root for art lookups (`backgrounds/`, `sprites/`, `title`).
const ASSETS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");

/// Where F5 saves go (gitignored).
pub const SAVE_PATH: &str = "saves/cuda-hearts.json";

/// Preferred entry script (file stem in `assets/scripts/`).
pub const ENTRY_SCRIPT: &str = "prologue";

/// Typewriter reveal speed in characters per second.
const CHARS_PER_SEC: f32 = 60.0;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scripts>()
            .init_resource::<Run>()
            .init_resource::<Present>()
            .init_resource::<Typewriter>()
            .add_plugins(
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            title:
                                "CUDA HEARTS — a leather-jacket love simulator (unofficial parody)"
                                    .into(),
                            ..default()
                        }),
                        ..default()
                    })
                    // Bevy resolves asset roots against the executable
                    // directory by default; point it at the crate's assets
                    // so `cargo run` works from anywhere. Swap to a relative
                    // "assets" when packaging for release.
                    .set(AssetPlugin {
                        file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
                        ..default()
                    }),
            )
            .add_systems(
                Startup,
                (spawn_camera, load_scripts, ui::spawn_title, ui::spawn_hud).chain(),
            )
            .add_systems(
                Update,
                (
                    handle_input,
                    run_typewriter,
                    sync_visibility,
                    sync_background,
                    sync_texts,
                    sync_stage_chips,
                    sync_choices,
                )
                    .chain(),
            );
    }
}

/// Run the game window.
pub fn run() {
    let mut app = App::new();
    app.add_plugins(GamePlugin);
    app.run();
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Parsed scripts by id (the `.vn` file stem).
#[derive(Resource, Default)]
pub struct Scripts(pub BTreeMap<String, Script>);

/// The active run, if the player left the title screen.
#[derive(Resource, Default)]
pub struct Run(pub Option<Playhead>);

/// What the UI should currently present. The single bridge between the
/// story engine and bevy entities.
#[derive(Resource, Default)]
pub struct Present {
    /// Display name of the speaker; `None` for narration.
    pub speaker: Option<String>,
    /// Speaker color as `(r, g, b)`.
    pub speaker_color: Option<(u8, u8, u8)>,
    /// Body text (typewriter reveals it over time).
    pub text: String,
    /// Current scene id, if any.
    pub scene: Option<String>,
    /// Characters on stage.
    pub stage: Vec<StageActor>,
    /// Current affinity score.
    pub affinity: i64,
    /// Active choice options; rendered while non-empty.
    pub choices: Vec<ChoiceOption>,
    /// Bumped whenever [`Present::choices`] changes identity, so [`sync_ui`]
    /// knows to rebuild the buttons even when the list is cleared.
    pub generation: u64,
    /// Ending id once reached.
    pub ended: Option<String>,
}

impl Present {
    /// Replace the choice list, bumping the generation counter.
    pub fn set_choices(&mut self, options: Vec<ChoiceOption>) {
        self.choices = options;
        self.generation += 1;
    }
}

/// Typewriter bookkeeping.
#[derive(Resource, Default)]
struct Typewriter {
    last: String,
    shown: usize,
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn load_scripts(mut scripts: ResMut<Scripts>) {
    let Ok(entries) = std::fs::read_dir(SCRIPT_DIR) else {
        error!("script directory {SCRIPT_DIR} is missing — no story to play");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "vn") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(e) => {
                error!("cannot read {}: {e}", path.display());
                continue;
            }
        };
        match Script::parse(&source) {
            Ok(script) => {
                info!("loaded script `{stem}` ({} steps)", script.steps.len());
                scripts.0.insert(stem.to_string(), script);
            }
            Err(e) => error!("{}: {e}", path.display()),
        }
    }
}

/// Pick the script a new run should start: `prologue` if present, else the
/// first script alphabetically.
pub fn entry_script(scripts: &Scripts) -> Option<String> {
    if scripts.0.contains_key(ENTRY_SCRIPT) {
        return Some(ENTRY_SCRIPT.to_string());
    }
    scripts.0.keys().next().cloned()
}

/// Find an art asset by stem, trying supported extensions. Returns the
/// path relative to `assets/` (what `AssetServer::load` expects), or
/// `None` when no art exists — callers then fall back to placeholders.
fn asset_art(rel_stem: &str) -> Option<String> {
    ["png", "jpg", "webp"]
        .into_iter()
        .map(|ext| format!("{rel_stem}.{ext}"))
        .find(|rel| std::path::Path::new(&format!("{ASSETS_DIR}/{rel}")).exists())
}

// ---------------------------------------------------------------------------
// Story plumbing
// ---------------------------------------------------------------------------

/// Advance the playhead one presentable step and mirror it into
/// [`Present`].
fn advance(
    playhead: &mut Playhead,
    scripts: &Scripts,
    present: &mut Present,
) -> Result<(), VnError> {
    match playhead.next(&scripts.0)? {
        Flow::Line { speaker, text } => {
            let character = cast::character(&speaker);
            present.speaker = character.map(|c| c.name.to_string());
            present.speaker_color = character.map(cast::Character::rgb);
            present.text = text;
        }
        Flow::Choice(options) => present.set_choices(options),
        Flow::Ending(name) => {
            present.ended = Some(name.clone());
            present.set_choices(Vec::new());
            present.text = format!("— ENDING: {name} —\nPress R to play again, Esc to quit.");
        }
        Flow::Done => {
            present.set_choices(Vec::new());
        }
    }
    // Cheap full sync of the non-text fields every step.
    present.scene = playhead.scene.clone();
    present.stage = playhead.stage.clone();
    present.affinity = playhead.affinity;
    Ok(())
}

/// Start a fresh run of `id` and present its first step.
fn start_run(id: String, scripts: &Scripts, run: &mut Run, present: &mut Present) {
    run.0 = Some(Playhead::start(id));
    let reset = Present::default();
    *present = reset;
    if let Some(playhead) = run.0.as_mut()
        && let Err(e) = advance(playhead, scripts, present)
    {
        error!("story error: {e}");
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scripts: Res<Scripts>,
    mut run: ResMut<Run>,
    mut present: ResMut<Present>,
    mut typewriter: ResMut<Typewriter>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
        return;
    }

    let Some(playhead) = run.0.as_mut() else {
        return;
    };

    // Saves.
    if keys.just_pressed(KeyCode::F5) {
        match save::save_to_path(Path::new(SAVE_PATH), &playhead.to_save()) {
            Ok(()) => info!("saved to {SAVE_PATH}"),
            Err(e) => error!("save failed: {e}"),
        }
        return;
    }
    if keys.just_pressed(KeyCode::F9) {
        match save::load_from_path(Path::new(SAVE_PATH)).map(Playhead::from_save) {
            Ok(restored) => {
                run.0 = Some(restored);
                if let Some(playhead) = run.0.as_mut()
                    && let Err(e) = advance(playhead, &scripts, &mut present)
                {
                    error!("story error after load: {e}");
                }
            }
            Err(e) => error!("load failed: {e}"),
        }
        return;
    }

    // Restart after an ending.
    if present.ended.is_some() && keys.just_pressed(KeyCode::KeyR) {
        match entry_script(&scripts) {
            Some(id) => start_run(id, &scripts, &mut run, &mut present),
            None => error!("cannot restart: no scripts loaded"),
        }
        return;
    }

    // Advance: click, Space, or Enter.
    let advance_pressed = mouse.just_pressed(MouseButton::Left)
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter);
    if !advance_pressed || present.ended.is_some() {
        return;
    }
    if playhead.pending_options().is_some() {
        return; // choice buttons own the input while a choice is pending
    }
    // First press during a typewriter reveal just completes the line.
    let total = present.text.chars().count();
    if typewriter.shown < total {
        typewriter.shown = usize::MAX; // clamped next frame
        return;
    }
    if let Err(e) = advance(playhead, &scripts, &mut present) {
        error!("story error: {e}");
    }
}

fn run_typewriter(
    time: Res<Time>,
    present: Res<Present>,
    mut typewriter: ResMut<Typewriter>,
    mut body: Query<&mut Text, With<ui::BodyText>>,
) {
    if present.text != typewriter.last {
        typewriter.last = present.text.clone();
        typewriter.shown = 0;
    }
    let total = present.text.chars().count();
    if typewriter.shown < total {
        let revealed = typewriter.shown.min(total) as f32 + CHARS_PER_SEC * time.delta_secs();
        typewriter.shown = (revealed as usize).min(total);
    }
    for mut text in &mut body {
        text.0 = present
            .text
            .chars()
            .take(typewriter.shown.min(total))
            .collect();
    }
}

/// Mirror [`Present`] into the text entities (name plate, body tint,
/// status labels). Runs every frame; writes are cheap no-ops when idle.
fn sync_texts(
    present: Res<Present>,
    mut text_query: Query<&mut Text>,
    name_entity: Query<Entity, With<ui::NameText>>,
    mut name_color: Query<&mut TextColor, With<ui::NameText>>,
    mut body_color: Query<&mut TextColor, (With<ui::BodyText>, Without<ui::NameText>)>,
    scene_entity: Query<Entity, With<ui::SceneLabel>>,
    affinity_entity: Query<Entity, With<ui::AffinityLabel>>,
) {
    // Name plate.
    if let Ok(entity) = name_entity.single()
        && let Ok(mut text) = text_query.get_mut(entity)
    {
        text.0 = present.speaker.clone().unwrap_or_default();
    }
    if let Ok(mut color) = name_color.single_mut() {
        color.0 = match present.speaker_color {
            Some((r, g, b)) => Color::srgb_u8(r, g, b),
            None => Color::srgb_u8(0xB0, 0xBE, 0xC5),
        };
    }
    // Narration is rendered in a muted tone; dialogue in near-white.
    if let Ok(mut color) = body_color.single_mut() {
        color.0 = if present.speaker.is_none() && present.ended.is_none() {
            Color::srgb_u8(0xB0, 0xBE, 0xC5)
        } else {
            Color::srgb_u8(0xF2, 0xF2, 0xF2)
        };
    }

    // Debug/status labels.
    if let Ok(entity) = scene_entity.single()
        && let Ok(mut text) = text_query.get_mut(entity)
    {
        text.0 = match &present.scene {
            Some(scene) => format!("scene: {scene}"),
            None => String::new(),
        };
    }
    if let Ok(entity) = affinity_entity.single()
        && let Ok(mut text) = text_query.get_mut(entity)
    {
        text.0 = format!("affinity: {:+}", present.affinity);
    }
}

/// Show the dialogue HUD only while a run is active.
fn sync_visibility(
    run: Res<Run>,
    mut dialogue_root: Query<&mut Visibility, With<ui::DialogueRoot>>,
) {
    if let Ok(mut visibility) = dialogue_root.single_mut() {
        *visibility = if run.0.is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Rebuild the stage whenever it changes: sprite portraits when art
/// exists, colored name chips otherwise.
fn sync_stage_chips(
    mut commands: Commands,
    present: Res<Present>,
    art: Res<AssetServer>,
    stage_row: Query<Entity, With<ui::StageRow>>,
    mut shown_chips: Local<Vec<Entity>>,
    mut shown_chip_stage: Local<Vec<(String, String)>>,
) {
    let stage_signature: Vec<(String, String)> = present
        .stage
        .iter()
        .map(|a| (a.cast.clone(), a.mood.clone()))
        .collect();
    if stage_signature != *shown_chip_stage {
        shown_chip_stage.clone_from(&stage_signature);
        for old in shown_chips.drain(..) {
            commands.entity(old).despawn();
        }
        if let Ok(row) = stage_row.single() {
            for actor in &present.stage {
                let (r, g, b) = cast::character(&actor.cast)
                    .map(cast::Character::rgb)
                    .unwrap_or((0xB0, 0xB0, 0xB0));

                // Sprite art wins over the placeholder chip: mood-specific
                // (`<cast>_<mood>`) first, then any art for the cast member.
                let sprite = asset_art(&format!("sprites/{}_{}", actor.cast, actor.mood))
                    .or_else(|| asset_art(&format!("sprites/{}", actor.cast)))
                    .map(|rel| art.load::<Image>(rel));
                if let Some(handle) = sprite {
                    let portrait = commands
                        .spawn((
                            ui::StagePortrait {
                                cast: actor.cast.clone(),
                            },
                            Node {
                                position_type: PositionType::Absolute,
                                right: Val::Px(48.0),
                                bottom: Val::Px(150.0),
                                width: percent(26),
                                height: percent(62),
                                border_radius: BorderRadius::all(Val::Px(14.0)),
                                ..default()
                            },
                            ImageNode {
                                image: handle,
                                ..default()
                            },
                            Visibility::Visible,
                        ))
                        .id();
                    shown_chips.push(portrait);
                    continue;
                }

                let chip = commands
                    .spawn((
                        ui::StageChip,
                        Node {
                            margin: UiRect {
                                left: Val::Px(6.0),
                                right: Val::Px(6.0),
                                ..default()
                            },
                            padding: ui::padding_px(8.0, 4.0),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(
                            f32::from(r) / 255.0,
                            f32::from(g) / 255.0,
                            f32::from(b) / 255.0,
                            0.22,
                        )),
                        Visibility::Visible,
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new(format!("{} · {}", actor.cast, actor.mood)),
                            TextFont {
                                font_size: FontSize::Px(18.0),
                                ..default()
                            },
                            TextColor(Color::srgb_u8(r, g, b)),
                        ));
                    })
                    .id();
                commands.entity(row).add_child(chip);
                shown_chips.push(chip);
            }
        }
    }
}

/// Rebuild the choice buttons whenever the choice generation changes.
fn sync_choices(
    mut commands: Commands,
    present: Res<Present>,
    mut shown_generation: Local<u64>,
    mut choice_root: Local<Option<Entity>>,
) {
    if *shown_generation != present.generation {
        *shown_generation = present.generation;
        if let Some(old) = choice_root.take() {
            commands.entity(old).despawn();
        }
        if !present.choices.is_empty() {
            let root = commands
                .spawn((
                    ui::ChoiceRoot,
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(72),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    Visibility::Visible,
                ))
                .id();
            for (index, option) in present.choices.iter().enumerate() {
                let button = commands
                    .spawn((
                        Button,
                        ui::ChoiceButton { index },
                        Node {
                            margin: UiRect::all(Val::Px(6.0)),
                            padding: ui::padding_px(18.0, 10.0),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.07, 0.16, 0.07)),
                        Visibility::Visible,
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new(option.text.clone()),
                            TextFont {
                                font_size: FontSize::Px(24.0),
                                ..default()
                            },
                            TextColor(Color::srgb_u8(0xE8, 0xF5, 0xE9)),
                        ));
                    })
                    .observe(
                        move |_: On<Activate>,
                              scripts: Res<Scripts>,
                              mut run: ResMut<Run>,
                              mut present: ResMut<Present>| {
                            let Some(playhead) = run.0.as_mut() else {
                                return;
                            };
                            if let Err(e) = playhead.pick(&scripts.0, index) {
                                error!("pick failed: {e}");
                                return;
                            }
                            present.set_choices(Vec::new());
                            if let Err(e) = advance(playhead, &scripts, &mut present) {
                                error!("story error: {e}");
                            }
                        },
                    )
                    .id();
                commands.entity(root).add_child(button);
            }
            *choice_root = Some(root);
        }
    }
}

/// Swap the scene backdrop whenever the scene id changes: full-screen
/// background art when `assets/backgrounds/<scene>.*` exists, otherwise
/// the tinted placeholder.
fn sync_background(
    mut commands: Commands,
    present: Res<Present>,
    art: Res<AssetServer>,
    mut backdrop: Query<(&mut Visibility, &mut BackgroundColor), With<ui::SceneBackdrop>>,
    mut shown_scene: Local<Option<String>>,
    mut image_entity: Local<Option<Entity>>,
) {
    let Some(scene) = present.scene.clone() else {
        return;
    };
    if shown_scene.as_ref() == Some(&scene) {
        return;
    }
    *shown_scene = Some(scene.clone());
    if let Some(old) = image_entity.take() {
        commands.entity(old).despawn();
    }
    match asset_art(&format!("backgrounds/{scene}")).map(|rel| art.load::<Image>(rel)) {
        Some(handle) => {
            if let Ok((mut visibility, _)) = backdrop.single_mut() {
                *visibility = Visibility::Hidden;
            }
            let entity = commands
                .spawn((
                    ui::SceneBackdropImage,
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    ImageNode {
                        image: handle,
                        ..default()
                    },
                    Visibility::Visible,
                ))
                .id();
            *image_entity = Some(entity);
        }
        None => {
            if let Ok((mut visibility, mut color)) = backdrop.single_mut() {
                *visibility = Visibility::Visible;
                color.0 = scene_color(&scene);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A stable pleasant background color derived from the scene id, so
/// distinct scenes read differently before real backgrounds exist.
pub fn scene_color(scene: &str) -> Color {
    let mut hasher = DefaultHasher::new();
    scene.hash(&mut hasher);
    let bits = hasher.finish();
    let channel = |shift: u32| 0.06 + f32::from((bits >> shift) as u8 & 0x1F) / 255.0;
    Color::srgb(channel(0) * 0.6, channel(8), channel(16) * 0.6)
}
