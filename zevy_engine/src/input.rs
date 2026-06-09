use bevy::{input::mouse::MouseMotion, prelude::*};
use bevy_xr_utils::xr_utils_actions::{
    ActiveSet, XRUtilsAction, XRUtilsActionSet, XRUtilsActionState, XRUtilsBinding,
};

const XR_CONTROLLER_PROFILES: &[&str] = &[
    "/interaction_profiles/oculus/touch_controller",
    "/interaction_profiles/valve/index_controller",
    "/interaction_profiles/bytedance/pico4s_controller",
];

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum InputSource {
    Keyboard,
    Mouse,
    XrController,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum InputButton {
    PrimaryAction,
    SecondaryAction,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum InputAxis2 {
    Move,
}

#[derive(Event, Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum EngineInputEvent {
    Button {
        source: InputSource,
        button: InputButton,
        pressed: bool,
    },
    Axis2 {
        source: InputSource,
        axis: InputAxis2,
        value: Vec2,
    },
    MouseMotion {
        delta: Vec2,
    },
}

#[derive(Resource, Debug, Default)]
pub struct EngineInputState {
    buttons: std::collections::HashMap<(InputSource, InputButton), bool>,
    axes2: std::collections::HashMap<(InputSource, InputAxis2), Vec2>,
    mouse_delta: Vec2,
}

impl EngineInputState {
    #[allow(dead_code)]
    pub fn button_pressed(&self, button: InputButton) -> bool {
        self.buttons
            .iter()
            .any(|((_, stored_button), pressed)| *stored_button == button && *pressed)
    }

    pub fn axis2(&self, axis: InputAxis2) -> Vec2 {
        self.axes2
            .iter()
            .filter_map(|((_, stored_axis), value)| (*stored_axis == axis).then_some(*value))
            .fold(Vec2::ZERO, |accumulated, value| accumulated + value)
            .clamp_length_max(1.0)
    }

    #[allow(dead_code)]
    pub fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }
}

#[derive(Component)]
pub struct XrMoveAction;

#[derive(Component)]
pub struct XrTriggerAction;

pub struct EngineInputPlugin;

#[derive(SystemSet, Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum EngineInputSet {
    Reset,
    Collect,
    React,
}

impl Plugin for EngineInputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EngineInputState::default())
            .add_event::<EngineInputEvent>()
            .configure_sets(
                Update,
                (
                    EngineInputSet::Reset,
                    EngineInputSet::Collect,
                    EngineInputSet::React,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                reset_frame_input_state.in_set(EngineInputSet::Reset),
            )
            .add_systems(
                Update,
                (collect_keyboard_mouse_input, collect_xr_controller_input)
                    .chain()
                    .in_set(EngineInputSet::Collect),
            )
            .add_systems(
                Update,
                log_primary_action_input.in_set(EngineInputSet::React),
            );
    }
}

pub fn setup_xr_actions(mut commands: Commands) {
    let locomotion_set = commands
        .spawn((
            XRUtilsActionSet {
                name: "locomotion".into(),
                pretty_name: "Locomotion".into(),
                priority: 0,
            },
            ActiveSet,
        ))
        .id();

    let move_action = commands
        .spawn((
            XRUtilsAction {
                action_name: "move".into(),
                localized_name: "Move".into(),
                action_type: bevy_mod_xr::actions::ActionType::Vector,
            },
            XrMoveAction,
        ))
        .id();

    for binding in XR_CONTROLLER_PROFILES {
        let binding_entity = commands
            .spawn(XRUtilsBinding {
                profile: (*binding).into(),
                binding: "/user/hand/right/input/thumbstick".into(),
            })
            .id();
        commands.entity(move_action).add_child(binding_entity);
    }
    commands.entity(locomotion_set).add_child(move_action);

    let input_set = commands
        .spawn((
            XRUtilsActionSet {
                name: "input".into(),
                pretty_name: "Input".into(),
                priority: 1,
            },
            ActiveSet,
        ))
        .id();

    let trigger_action = commands
        .spawn((
            XRUtilsAction {
                action_name: "trigger_click".into(),
                localized_name: "Trigger Click".into(),
                action_type: bevy_mod_xr::actions::ActionType::Bool,
            },
            XrTriggerAction,
        ))
        .id();

    for binding in XR_CONTROLLER_PROFILES {
        let binding_entity = commands
            .spawn(XRUtilsBinding {
                profile: (*binding).into(),
                binding: "/user/hand/right/input/trigger/click".into(),
            })
            .id();
        commands.entity(trigger_action).add_child(binding_entity);
    }
    commands.entity(input_set).add_child(trigger_action);
}

fn reset_frame_input_state(mut state: ResMut<EngineInputState>) {
    state.mouse_delta = Vec2::ZERO;
}

fn collect_keyboard_mouse_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut state: ResMut<EngineInputState>,
    mut events: EventWriter<EngineInputEvent>,
) {
    let keyboard_move = keyboard_axis(&keyboard);
    state
        .axes2
        .insert((InputSource::Keyboard, InputAxis2::Move), keyboard_move);
    events.write(EngineInputEvent::Axis2 {
        source: InputSource::Keyboard,
        axis: InputAxis2::Move,
        value: keyboard_move,
    });

    collect_button(
        InputSource::Keyboard,
        InputButton::PrimaryAction,
        keyboard.just_pressed(KeyCode::Space),
        keyboard.just_released(KeyCode::Space),
        &mut state,
        &mut events,
    );
    collect_button(
        InputSource::Mouse,
        InputButton::PrimaryAction,
        mouse_buttons.just_pressed(MouseButton::Left),
        mouse_buttons.just_released(MouseButton::Left),
        &mut state,
        &mut events,
    );
    collect_button(
        InputSource::Mouse,
        InputButton::SecondaryAction,
        mouse_buttons.just_pressed(MouseButton::Right),
        mouse_buttons.just_released(MouseButton::Right),
        &mut state,
        &mut events,
    );

    let delta = mouse_motion
        .read()
        .fold(Vec2::ZERO, |accumulated, event| accumulated + event.delta);
    if delta != Vec2::ZERO {
        state.mouse_delta = delta;
        events.write(EngineInputEvent::MouseMotion { delta });
    }
}

fn collect_xr_controller_input(
    time: Res<Time>,
    move_query: Query<&XRUtilsActionState, With<XrMoveAction>>,
    trigger_query: Query<&XRUtilsActionState, With<XrTriggerAction>>,
    mut state: ResMut<EngineInputState>,
    mut events: EventWriter<EngineInputEvent>,
    mut last_logged_move: Local<Option<Vec2>>,
    mut seconds_since_move_log: Local<f32>,
) {
    let mut xr_move = Vec2::ZERO;
    let mut saw_active_move = false;
    for action_state in &move_query {
        let XRUtilsActionState::Vector(vector_state) = action_state else {
            continue;
        };

        if !vector_state.is_active {
            continue;
        }

        saw_active_move = true;
        xr_move = Vec2::new(vector_state.current_state[0], vector_state.current_state[1]);
        state
            .axes2
            .insert((InputSource::XrController, InputAxis2::Move), xr_move);
        events.write(EngineInputEvent::Axis2 {
            source: InputSource::XrController,
            axis: InputAxis2::Move,
            value: xr_move,
        });
    }
    if xr_move == Vec2::ZERO {
        state
            .axes2
            .insert((InputSource::XrController, InputAxis2::Move), Vec2::ZERO);
    }
    *seconds_since_move_log += time.delta_secs();
    let should_log_move = xr_move.length_squared() > 0.01
        && last_logged_move.is_none_or(|last| last.distance_squared(xr_move) > 0.04)
        || (saw_active_move && xr_move.length_squared() > 0.01 && *seconds_since_move_log >= 1.0);
    if should_log_move {
        info!("XR controller move axis: {xr_move:?}");
        *last_logged_move = Some(xr_move);
        *seconds_since_move_log = 0.0;
    }

    for action_state in &trigger_query {
        let XRUtilsActionState::Bool(button_state) = action_state else {
            continue;
        };

        if !button_state.is_active || !button_state.changed_since_last_sync {
            continue;
        }

        state.buttons.insert(
            (InputSource::XrController, InputButton::PrimaryAction),
            button_state.current_state,
        );
        events.write(EngineInputEvent::Button {
            source: InputSource::XrController,
            button: InputButton::PrimaryAction,
            pressed: button_state.current_state,
        });
        info!("XR controller trigger click: {}", button_state.current_state);
    }
}

fn log_primary_action_input(mut events: EventReader<EngineInputEvent>) {
    for event in events.read() {
        if let EngineInputEvent::Button {
            source,
            button: InputButton::PrimaryAction,
            pressed: true,
        } = event
        {
            info!("Primary action pressed from {source:?}");
        }
    }
}

fn keyboard_axis(keyboard: &ButtonInput<KeyCode>) -> Vec2 {
    let mut axis = Vec2::ZERO;

    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        axis.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        axis.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        axis.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        axis.y -= 1.0;
    }

    axis.normalize_or_zero()
}

fn collect_button(
    source: InputSource,
    button: InputButton,
    just_pressed: bool,
    just_released: bool,
    state: &mut EngineInputState,
    events: &mut EventWriter<EngineInputEvent>,
) {
    if just_pressed {
        state.buttons.insert((source, button), true);
        events.write(EngineInputEvent::Button {
            source,
            button,
            pressed: true,
        });
    }
    if just_released {
        state.buttons.insert((source, button), false);
        events.write(EngineInputEvent::Button {
            source,
            button,
            pressed: false,
        });
    }
}
