//! UI entity layout: components, the title screen, and the dialogue HUD.
//!
//! Everything here is placeholder-styled with bevy_ui nodes; swap in real
//! art (sprites/backgrounds) later without touching the story engine.

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::{Present, Run, Scripts, entry_script, scene_color, start_run};

// ---------------------------------------------------------------------------
// Marker components
// ---------------------------------------------------------------------------

/// Root of the title screen; despawned on Start.
#[derive(Component)]
pub struct TitleRoot;

/// Root of the in-game dialogue HUD.
#[derive(Component)]
pub struct DialogueRoot;

/// Full-screen scene background tint (fallback when no background art).
#[derive(Component)]
pub struct SceneBackdrop;

/// Full-screen scene background image (spawned when art exists).
#[derive(Component)]
pub struct SceneBackdropImage;

/// Full-screen title key art, if present.
#[derive(Component)]
pub struct TitleArt;

/// A stage actor shown as sprite art instead of a placeholder chip.
#[derive(Component)]
pub struct StagePortrait {
    pub cast: String,
}

/// Name plate text above the dialogue body.
#[derive(Component)]
pub struct NameText;

/// Dialogue body text (typewriter-driven).
#[derive(Component)]
pub struct BodyText;

/// Shows the current scene id (authoring aid).
#[derive(Component)]
pub struct SceneLabel;

/// Shows the current affinity score (authoring aid).
#[derive(Component)]
pub struct AffinityLabel;

/// Row that holds stage chips.
#[derive(Component)]
pub struct StageRow;

/// One placeholder actor chip on stage.
#[derive(Component)]
pub struct StageChip;

/// Overlay root that holds choice buttons.
#[derive(Component)]
pub struct ChoiceRoot;

/// A single choice button; `index` addresses `Playhead::pick`.
#[derive(Component)]
pub struct ChoiceButton {
    pub index: usize,
}

// ---------------------------------------------------------------------------
// Small helpers shared with systems in `super`
// ---------------------------------------------------------------------------

/// Rect helpers for `Node { margin, padding, .. }` fields.
pub fn padding_px(x: f32, y: f32) -> UiRect {
    UiRect {
        left: Val::Px(x),
        right: Val::Px(x),
        top: Val::Px(y),
        bottom: Val::Px(y),
    }
}

fn margin_px(x: f32, y: f32) -> UiRect {
    UiRect {
        left: Val::Px(x),
        right: Val::Px(x),
        top: Val::Px(y),
        bottom: Val::Px(y),
    }
}

// ---------------------------------------------------------------------------
// Title screen
// ---------------------------------------------------------------------------

pub fn spawn_title(mut commands: Commands, art: Res<AssetServer>) {
    // Title key art behind the text, when present.
    if let Some(rel) = super::asset_art("title") {
        commands.spawn((
            TitleArt,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode {
                image: art.load::<Image>(rel),
                ..default()
            },
            Visibility::Visible,
        ));
    }
    commands
        .spawn((
            TitleRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.05, 0.02, 0.55)),
            Visibility::Visible,
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    margin: margin_px(0.0, 8.0),
                    ..default()
                },
                Text::new("CUDA HEARTS"),
                TextFont {
                    font_size: FontSize::Px(64.0),
                    ..default()
                },
                TextColor(Color::srgb_u8(0x76, 0xB9, 0x00)),
            ));
            parent.spawn((
                Node {
                    margin: margin_px(0.0, 28.0),
                    ..default()
                },
                Text::new("a leather-jacket love simulator — unofficial parody"),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::srgb_u8(0xB0, 0xBE, 0xC5)),
            ));
            parent
                .spawn((
                    Button,
                    Node {
                        padding: padding_px(28.0, 12.0),
                        margin: margin_px(0.0, 10.0),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.07, 0.16, 0.07)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Start"),
                        TextFont {
                            font_size: FontSize::Px(28.0),
                            ..default()
                        },
                        TextColor(Color::srgb_u8(0xE8, 0xF5, 0xE9)),
                    ));
                })
                .observe(
                    |_: On<Activate>,
                     title: Query<Entity, With<TitleRoot>>,
                     scripts: Res<Scripts>,
                     mut run: ResMut<Run>,
                     mut present: ResMut<Present>,
                     mut commands: Commands| {
                        if let Ok(entity) = title.single() {
                            commands.entity(entity).despawn();
                        }
                        match entry_script(&scripts) {
                            Some(id) => start_run(id, &scripts, &mut run, &mut present),
                            None => error!("no scripts loaded from assets/scripts — cannot start"),
                        }
                    },
                );
            parent.spawn((
                Node {
                    margin: margin_px(0.0, 24.0),
                    ..default()
                },
                Text::new("click / Space: advance    F5: save    F9: load    Esc: quit"),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb_u8(0x78, 0x90, 0x9C)),
            ));
        });
}

// ---------------------------------------------------------------------------
// Dialogue HUD
// ---------------------------------------------------------------------------

pub fn spawn_hud(mut commands: Commands) {
    // Scene backdrop: tinted per scene id until real backgrounds exist.
    commands.spawn((
        SceneBackdrop,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(scene_color("")),
        Visibility::Hidden,
    ));

    commands
        .spawn((
            DialogueRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexEnd,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            // Top strip: scene + affinity + stage chips.
            parent
                .spawn((Node {
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },))
                .with_children(|top| {
                    top.spawn((
                        SceneLabel,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(16.0),
                            ..default()
                        },
                        TextColor(Color::srgb_u8(0x78, 0x90, 0x9C)),
                    ));
                    top.spawn((
                        AffinityLabel,
                        Text::new("affinity: +0"),
                        TextFont {
                            font_size: FontSize::Px(16.0),
                            ..default()
                        },
                        TextColor(Color::srgb_u8(0x76, 0xB9, 0x00)),
                    ));
                    top.spawn((
                        StageRow,
                        Node {
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            margin: margin_px(0.0, 6.0),
                            ..default()
                        },
                    ));
                });

            // Spacer pushes the dialogue box to the bottom.
            parent.spawn((Node {
                width: percent(100),
                flex_grow: 1.0,
                ..default()
            },));

            // Dialogue box.
            parent
                .spawn((
                    Node {
                        width: percent(92),
                        margin: margin_px(0.0, 18.0),
                        padding: padding_px(20.0, 14.0),
                        flex_direction: FlexDirection::Column,
                        border_radius: BorderRadius::all(Val::Px(12.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.06, 0.02, 0.92)),
                ))
                .with_children(|box_parent| {
                    box_parent.spawn((
                        NameText,
                        Node {
                            margin: margin_px(0.0, 6.0),
                            ..default()
                        },
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(22.0),
                            ..default()
                        },
                        TextColor(Color::srgb_u8(0x76, 0xB9, 0x00)),
                    ));
                    box_parent.spawn((
                        BodyText,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(26.0),
                            ..default()
                        },
                        // Default LineBreak already wraps at word boundaries.
                        TextColor(Color::srgb_u8(0xF2, 0xF2, 0xF2)),
                    ));
                });
        });
}
