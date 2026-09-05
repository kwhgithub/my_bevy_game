use bevy::prelude::*;

/// 玩家方块大小
const PLAYER_SIZE: f32 = 40.0;
/// 金币大小
const COIN_SIZE: f32 = 20.0;
/// 玩家移动速度（像素/秒）
const PLAYER_SPEED: f32 = 300.0;
/// 场上同时存在的金币数量
const COIN_COUNT: usize = 5;
/// 游戏区域边界（以世界原点为中心，半宽/半高）
const FIELD_HALF_X: f32 = 360.0;
const FIELD_HALF_Y: f32 = 260.0;
/// 吃到金币时的判定半径
const PICKUP_RADIUS: f32 = (PLAYER_SIZE + COIN_SIZE) * 0.5;

/// 标记玩家实体
#[derive(Component)]
struct Player;

/// 标记金币实体
#[derive(Component)]
struct Coin;

/// 计分板资源
#[derive(Resource, Default)]
struct Score(u32);

/// 计分文本实体
#[derive(Component)]
struct ScoreText;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .init_resource::<Score>()
        .insert_resource(CoinRng::new())
        .add_systems(Startup, (setup, spawn_coins.after(setup)))
        .add_systems(
            Update,
            (player_movement, coin_collision, update_score_ui).chain(),
        )
        .run();
}

/// 初始设置：相机、玩家、计分 UI
fn setup(mut commands: Commands) {
    // 2D 相机
    commands.spawn(Camera2d);

    // 玩家：蓝色方块
    commands.spawn((
        Player,
        Sprite::from_color(Color::srgb(0.25, 0.55, 1.0), Vec2::splat(PLAYER_SIZE)),
        Transform::default(),
    ));

    // 计分文本：左上角
    commands.spawn((
        ScoreText,
        Text::new("Score: 0"),
        TextFont {
            font_size: FontSize::Px(32.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(16.0),
            ..default()
        },
    ));
}

/// 生成一枚金币，位置在游戏区域内随机（避开边缘）
fn spawn_one_coin(commands: &mut Commands, rng: &mut CoinRng) {
    let x = rng.range(-FIELD_HALF_X + COIN_SIZE, FIELD_HALF_X - COIN_SIZE);
    let y = rng.range(-FIELD_HALF_Y + COIN_SIZE, FIELD_HALF_Y - COIN_SIZE);

    commands.spawn((
        Coin,
        Sprite::from_color(Color::srgb(1.0, 0.8, 0.1), Vec2::splat(COIN_SIZE)),
        Transform::from_xyz(x, y, 0.0),
    ));
}

/// 简单的伪随机数生成器（xorshift32），
/// 避免为了一个小游戏引入额外的 rand 依赖。
#[derive(Resource)]
struct CoinRng(u32);

impl CoinRng {
    fn new() -> Self {
        // 注意：wasm32-unknown-unknown 上 std::time::SystemTime 未实现，
        // 调用会直接 panic（曾导致 Web 版白屏），wasm 上改用 web_time。
        #[cfg(target_arch = "wasm32")]
        let nanos = web_time::SystemTime::now()
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        #[cfg(not(target_arch = "wasm32"))]
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        // xorshift32 要求种子非零
        Self(nanos | 1)
    }

    /// 返回 0.0..1.0 的伪随机浮点数
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// 返回 min..max 范围内的伪随机浮点数
    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
}

/// 启动时生成若干金币
fn spawn_coins(mut commands: Commands, mut rng: ResMut<CoinRng>) {
    for _ in 0..COIN_COUNT {
        spawn_one_coin(&mut commands, &mut rng);
    }
}

/// 键盘控制玩家移动，并限制在游戏区域内
fn player_movement(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<&mut Transform, With<Player>>,
) {
    let Ok(mut transform) = query.single_mut() else {
        return;
    };

    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    if direction != Vec2::ZERO {
        direction = direction.normalize_or_zero();
        let delta = direction * PLAYER_SPEED * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;

        // 限制在边界内
        transform.translation.x = transform
            .translation
            .x
            .clamp(-FIELD_HALF_X, FIELD_HALF_X);
        transform.translation.y = transform
            .translation
            .y
            .clamp(-FIELD_HALF_Y, FIELD_HALF_Y);
    }
}

/// 碰撞检测：玩家吃到金币则加分，并补充新金币
fn coin_collision(
    mut commands: Commands,
    mut score: ResMut<Score>,
    mut rng: ResMut<CoinRng>,
    player_query: Query<&Transform, With<Player>>,
    coin_query: Query<(Entity, &Transform), With<Coin>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    for (entity, coin_transform) in &coin_query {
        let distance = player_transform
            .translation
            .xy()
            .distance(coin_transform.translation.xy());
        if distance < PICKUP_RADIUS {
            // 吃掉金币、加分、补充一枚新金币
            commands.entity(entity).despawn();
            spawn_one_coin(&mut commands, &mut rng);
            score.0 += 1;
            info!("Score: {}", score.0);
        }
    }
}

/// 更新计分文本
fn update_score_ui(score: Res<Score>, mut query: Query<&mut Text, With<ScoreText>>) {
    if score.is_changed() {
        if let Ok(mut text) = query.single_mut() {
            **text = format!("Score: {}", score.0);
        }
    }
}
