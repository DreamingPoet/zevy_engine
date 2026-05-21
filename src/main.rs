use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins) // 加载默认插件（包含渲染器）
        .add_systems(Startup, setup) // 启动时运行 setup 函数
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 1. 创建一个具有金属质感的 PBR 球体
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(5).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9), // 漂亮的科技蓝
            metallic: 0.85,                        // 高金属度
            perceptual_roughness: 0.15,            // 低粗糙度（像镜面一样反射）
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    // 2. 创建一个会投射阴影的高亮光源
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    // 3. 创建观察相机的视野
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}