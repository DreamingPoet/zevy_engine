mod levels;

use bevy::prelude::*;

use crate::app::{LaunchMode, StartupMode};

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DefaultLevel(LevelId::PerformanceLab))
            .insert_resource(CurrentLevel(None))
            .add_event::<OpenLevel>()
            .add_systems(Startup, load_default_level)
            .add_systems(Update, (open_level, levels::animate_orbiting_lights));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelId {
    PerformanceLab,
    #[allow(dead_code)]
    Empty,
}

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefaultLevel(pub LevelId);

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentLevel(pub Option<LevelId>);

#[derive(Event, Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenLevel(pub LevelId);

#[derive(Component)]
pub(super) struct LevelEntity;

#[derive(Component)]
pub struct MirrorCamera;

fn load_default_level(
    default_level: Res<DefaultLevel>,
    startup_mode: Res<StartupMode>,
    mut current_level: ResMut<CurrentLevel>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_level(
        default_level.0,
        startup_mode.0,
        &mut current_level,
        &mut commands,
        &mut meshes,
        &mut materials,
    );
}

fn open_level(
    mut events: EventReader<OpenLevel>,
    level_entities: Query<Entity, With<LevelEntity>>,
    startup_mode: Res<StartupMode>,
    mut current_level: ResMut<CurrentLevel>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(level) = events.read().last().map(|event| event.0) else {
        return;
    };

    if current_level.0 == Some(level) {
        return;
    }

    for entity in &level_entities {
        commands.entity(entity).despawn();
    }

    spawn_level(
        level,
        startup_mode.0,
        &mut current_level,
        &mut commands,
        &mut meshes,
        &mut materials,
    );
}

fn spawn_level(
    level: LevelId,
    launch_mode: LaunchMode,
    current_level: &mut ResMut<CurrentLevel>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    match level {
        LevelId::PerformanceLab => {
            levels::spawn_performance_lab(launch_mode, commands, meshes, materials);
        }
        LevelId::Empty => {
            levels::spawn_empty(launch_mode, commands);
        }
    }

    current_level.0 = Some(level);
    info!("Opened level: {level:?}");
}
