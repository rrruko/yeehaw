extern crate ggez;
extern crate rand;
extern crate specs;
#[macro_use]
extern crate specs_derive;

use ggez::conf;
use ggez::event::{self, EventHandler, Keycode, Mod};
use ggez::graphics;
use ggez::graphics::{FilterMode, Point2, Vector2, set_default_filter};
use ggez::nalgebra as na;
use ggez::timer;
use ggez::{Context, ContextBuilder, GameResult};

use specs::prelude::*;
use std::collections::HashSet;
use std::env;
use std::path;

// Point2 already implements an equivalent trait but rust won't let me import
// it
trait Dist {
    fn distance(&self, other: &Point2) -> f32;
}

impl Dist for Point2 {
    fn distance(&self, other: &Point2) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Component, Debug)]
struct Vel(Vector2);

#[derive(Component, Clone, Copy, Debug)]
struct Pos(Point2);

#[derive(Component, Debug)]
struct IsPlayer;

#[derive(Component, Debug)]
struct DeltaTime(f32);

#[derive(Component, Debug)]
struct GlobalTime(f64);

#[derive(Component, Debug)]
struct HasGravity;

#[derive(Component, Debug)]
struct IsJumping(bool);

impl Default for DeltaTime {
    fn default() -> Self {
        DeltaTime(0.0)
    }
}

impl Default for GlobalTime {
    fn default() -> Self {
        GlobalTime(0.0)
    }
}

struct RigidBodyPhysics;

impl<'a> System<'a> for RigidBodyPhysics {
    type SystemData = (Read<'a, DeltaTime>,
                       Entities<'a>,
                       WriteStorage<'a, Pos>,
                       WriteStorage<'a, Vel>,
                       ReadStorage<'a, HasGravity>);

    fn run(&mut self, (dt, entities, mut pos, mut vel, has_gravity): Self::SystemData) {
        let dt = dt.0;
        for (ent, pos, vel) in (&*entities, &mut pos, &mut vel).join() {
            pos.0 += vel.0 * dt; // update pos

            if has_gravity.get(ent).is_some() {
                vel.0.y -= 500.0 * dt; // gravity
            }
        }
    }
}

struct PlayerControl;

impl<'a> System<'a> for PlayerControl {
    type SystemData = (Read<'a, InputState>,
                       Read<'a, DeltaTime>,
                       WriteStorage<'a, Pos>,
                       WriteStorage<'a, Vel>,
                       WriteStorage<'a, Facing>,
                       WriteStorage<'a, ShootCooldown>,
                       WriteStorage<'a, IsJumping>,
                       ReadStorage<'a, IsPlayer>);
    fn run(&mut self, (input, dt, mut pos, mut vel, mut facing, mut cooldown, mut is_jumping, is_player): Self::SystemData) {
        let dt = dt.0;
        for (pos, vel, facing, cooldown, is_jumping, _) in (&mut pos, &mut vel, &mut facing, &mut cooldown, &mut is_jumping, &is_player).join() {
            pos.0.x += input.xaxis * dt * 100.0;
            
            if pos.0.y < -150.0 {
                pos.0.y = -150.0;
                vel.0.y = 0.0;
                is_jumping.0 = false;
            }

            if input.keys.contains(&Input::JUMP) && !is_jumping.0 {
                vel.0.y = 300.0;
                is_jumping.0 = true;
            }

            if cooldown.0 > 0.0 {
                cooldown.0 -= dt;
            }
            if cooldown.0 < 0.0 {
                cooldown.0 = 0.0;
            }

            if input.xaxis < 0.0 {
                std::mem::replace(facing, Facing::Left);
            } else if input.xaxis > 0.0 {
                std::mem::replace(facing, Facing::Right);
            }
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
enum BulletStatus {
    Alive,
    Dead,
}

#[derive(Component, Debug)]
struct ShootCooldown(f32);

struct ShootBullets;

impl<'a> System<'a> for ShootBullets {
    type SystemData = (Read<'a, InputState>,
                       WriteStorage<'a, Pos>,
                       WriteStorage<'a, Vel>,
                       ReadStorage<'a, IsPlayer>,
                       ReadStorage<'a, Facing>,
                       WriteStorage<'a, ShootCooldown>,
                       WriteStorage<'a, BulletStatus>,
                       Read<'a, DeltaTime>);
    fn run(&mut self, (input, mut pos, mut vel, is_player, facing, mut cooldown, mut bullet, dt): Self::SystemData) {
        let dt = dt.0;
        let shoot = input.shoot;

        let mut player_pos = None;
        let mut player_facing = None;
        let mut player_cooldown = std::f32::INFINITY;
        {
            for (pos, facing, mut cooldown, _) in (&pos, &facing, &mut cooldown, &is_player).join() {
                player_pos = Some(*pos);
                player_facing = Some(*facing);
                if cooldown.0 > 0.0 {
                    cooldown.0 -= dt;
                }
                if cooldown.0 < 0.0 {
                    cooldown.0 = 0.0;
                }
                player_cooldown = cooldown.0;
                if cooldown.0 == 0.0 && input.shoot {
                    cooldown.0 = 0.035;
                }
            }
        }

        if let (Some(player_pos), Some(facing)) = (player_pos, player_facing) {
            if input.shoot && player_cooldown == 0.0 {
                for (mut pos, mut vel, mut bullet) in (&mut pos, &mut vel, &mut bullet).join() {
                    if let BulletStatus::Dead = bullet {
                        std::mem::replace(bullet, BulletStatus::Alive);
                        pos.0 = player_pos.0;
                        vel.0 = Vector2::new(600.0 * facing.to_f32(), 0.0);
                        break;
                    }
                }
            }
        }

        for (mut pos, mut bullet) in (&mut pos, &mut bullet).join() {
            if pos.0.x.abs() > 400.0 || pos.0.y.abs() > 400.0 {
                std::mem::replace(bullet, BulletStatus::Dead);
            }
        }
    }
}

#[derive(Component, Debug)]
struct IsHook;

#[derive(Component, Debug)]
struct IsSwingTarget;

#[derive(Component, Clone, Copy, Debug)]
struct SwingData_ {
    theta0: f32,
    theta: f32,
    dist: f32,
    start_time: f64,
}

struct DoHook;

impl<'a> System<'a> for DoHook {
    type SystemData = (Read<'a, InputState>,
                       Entities<'a>,
                       WriteStorage<'a, Pos>,
                       ReadStorage<'a, IsPlayer>,
                       WriteStorage<'a, SwingData_>,
                       ReadStorage<'a, IsHook>,
                       WriteStorage<'a, IsSwingTarget>,
                       Read<'a, DeltaTime>,
                       Read<'a, GlobalTime>);
    fn run(&mut self, (input, entities, mut pos, is_player, mut swing_data, is_hook, mut is_target, dt, t): Self::SystemData) {
        if input.just_pressed.contains(&Input::TOOL) {
            for (mut player_entity, _) in (&*entities, &is_player).join() {
                match swing_data.get(player_entity).cloned() {
                    Some(sd) => {
                        swing_data.remove(player_entity);
                    }
                    None => {
                        let hooks = (&*entities, &is_hook).join().map(|(e, h)| e).collect();
                        self.try_hook(&mut pos, &mut is_target, &mut player_entity, &mut swing_data, hooks, t.0);
                    }
                }
            }
        }
    }
}

impl<'a> DoHook {
    fn try_hook(&mut self, 
                pos: &mut WriteStorage<'a, Pos>,
                is_target: &mut WriteStorage<'a, IsSwingTarget>,
                player: &mut Entity,
                swing_data: &mut WriteStorage<'a, SwingData_>,
                hooks: Vec<Entity>,
                t: f64
    ) {
        if let Some(player_pos) = pos.get(*player) {
            let (ent, hook_pos, nearest_dist) = hooks.iter()
                .map(|entity| {
                    let hook_pos = pos.get(*entity).unwrap();
                    let d = hook_pos.0.distance(&player_pos.0);
                    (entity, hook_pos, d)
                })
                .min_by(|x, y| {
                    PartialOrd::partial_cmp(&x.2, &y.2).unwrap()
                })
                .unwrap();
            
            is_target.insert(*ent, IsSwingTarget);
            if nearest_dist < 100.0 {
                let dx = player_pos.0.x - hook_pos.0.x;
                let dy = player_pos.0.y - hook_pos.0.y;
                let theta0 = dx.atan2(-dy);
                let dist = (dx * dx + dy * dy).sqrt();

                swing_data.insert(*player, SwingData_ {
                    theta0,
                    theta: theta0,
                    start_time: t,
                    dist,
                });
                println!("Inserted swing data at dist {}", dist);
            } 
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
enum Facing {
    Left,
    Right,
}

impl Facing {
    fn to_f32(self) -> f32 {
        match self {
            Facing::Left => -1.0,
            Facing::Right => 1.0
        }
    }
}

#[derive(Debug)]
struct Actor {
    is_player: bool, // Currently useless since there's only one Actor
    pos: Point2,
    vel: Vector2,
    facing: Facing,
    jumping: bool, // Set on jump, cleared on landing
    shoot_cooldown: f32, // Little timer so the gun doesn't fire every frame
    swing_data: Option<SwingData>, // If this is Some, the player is swinging
}

enum BossPhase {
    Attack,
    Evade,
}

struct Boss {
    pos: Point2,
    vel: Vector2,
    hp: f32,
    facing: Facing,
    jumping: bool,
    phase: BossPhase,
    phase_timer: f32,
}

fn get_time(ctx: &Context) -> f64 {
    timer::duration_to_f64(
        timer::get_time_since_start(ctx)
    )
}

#[derive(Debug)]
struct SwingData {
    theta0: f32,
    theta: f32,
    dist: f32,
    start_time: f64,
    target: Hook,
}

#[derive(Debug)]
struct Bullet {
    pos: Point2,
    vel: Vector2,
    alive: bool,
}

#[derive(Copy, Clone, Debug)]
struct Hook {
    pos: Point2
}

#[derive(Debug)]
struct Bullets {
    bullets: Vec<Bullet>,
}

// Why does this function floor and add 0.5?
// ggez (or perhaps gfx) has a bug that causes sprites to be sampled
// incorrectly when drawn at whole number floating point coords in the Nearest
// filter mode. (The whole top row of pixels in the sprite disappears.)
//
// As far as I can tell, this happens *only* at whole number coordinates, so we
// could just as well add 0.1 or 0.9.
fn quantize(pos: Point2) -> Point2 {
    Point2::new(pos.x.floor() + 0.5, pos.y.floor() + 0.5)
}

fn draw_actor(
    assets: &mut Assets,
    ctx: &mut Context,
    actor: &Actor,
    screen_width: u32,
    screen_height: u32,
) -> GameResult<()> {
    let pos = world_to_screen_coords(screen_width, screen_height, actor.pos);
    let image = assets.actor_image(actor);
    let draw_params = graphics::DrawParam {
        dest: quantize(pos),
        rotation: 0.0,
        offset: graphics::Point2::new(0.5, 0.5),
        ..Default::default()
    };
    graphics::draw_ex(ctx, image, draw_params)?;

    // Draw lasso
    if let Some(ref sd) = actor.swing_data {
        let target_pos = world_to_screen_coords(screen_width, screen_height, sd.target.pos);
        graphics::line(ctx, &[pos, target_pos], 1.0)?;
    }
    Ok(())
}

fn draw_debug_sprite(
    assets: &mut Assets,
    ctx: &mut Context,
    pos: Pos,
    screen_width: u32,
    screen_height: u32,
) -> GameResult<()> {
    let pos = world_to_screen_coords(screen_width, screen_height, pos.0);
    let image = &assets.player_image;
    let draw_params = graphics::DrawParam {
        dest: quantize(pos),
        offset: graphics::Point2::new(0.5, 0.5),
        ..Default::default()
    };
    graphics::draw_ex(ctx, image, draw_params)?;
    Ok(())
}

fn draw_bullet_sprite(
    assets: &mut Assets,
    ctx: &mut Context,
    pos: Pos,
    screen_width: u32,
    screen_height: u32,
) -> GameResult<()> {
    let pos = world_to_screen_coords(screen_width, screen_height, pos.0);
    let image = &assets.bullet_image;
    let draw_params = graphics::DrawParam {
        dest: quantize(pos),
        offset: graphics::Point2::new(0.5, 0.5),
        ..Default::default()
    };
    graphics::draw_ex(ctx, image, draw_params)?;
    Ok(())
}

fn draw_boss(
    assets: &mut Assets,
    ctx: &mut Context,
    boss: &Boss,
    screen_width: u32,
    screen_height: u32,
) -> GameResult<()> {
    let pos = world_to_screen_coords(screen_width, screen_height, boss.pos);
    let image = &assets.player_image;
    let draw_params = graphics::DrawParam {
        dest: quantize(pos),
        rotation: 0.0,
        offset: graphics::Point2::new(0.5, 0.5),
        ..Default::default()
    };
    graphics::draw_ex(ctx, image, draw_params)?;

    Ok(())
}

fn draw_bullets(
    assets: &mut Assets,
    ctx: &mut Context,
    bullets: &Bullets,
    screen_width: u32,
    screen_height: u32
) -> GameResult<()> {
    let image = &assets.bullet_image;
    for bullet in &bullets.bullets {
        if bullet.alive {
            let pos = world_to_screen_coords(screen_width, screen_height, bullet.pos);
            let draw_params = graphics::DrawParam {
                dest: quantize(pos),
                rotation: 0.0,
                offset: graphics::Point2::new(0.5, 0.5),
                ..Default::default()
            };
            graphics::draw_ex(ctx, image, draw_params)?;
        }
    }
    Ok(())
}

fn draw_hook(
    assets: &mut Assets,
    ctx: &mut Context,
    hook: Hook,
    screen_width: u32,
    screen_height: u32
) -> GameResult<()> {
    let image = &assets.hook_image;
    let pos = world_to_screen_coords(screen_width, screen_height, hook.pos);
    let draw_params = graphics::DrawParam {
        dest: quantize(pos),
        rotation: 0.0,
        offset: graphics::Point2::new(0.5, 0.5),
        ..Default::default()
    };
    graphics::draw_ex(ctx, image, draw_params)
}

fn create_player() -> Actor {
    Actor {
        is_player: true,
        pos: Point2::origin(),
        vel: na::zero(),
        facing: Facing::Right,
        jumping: false,
        shoot_cooldown: 0.0,
        swing_data: None,
    }
}

fn create_bullets(n: u32) -> Bullets {
    let mut bullets = Vec::new();
    for _ in 0..n {
        bullets.push(Bullet {
            pos: Point2::new(0.0, 0.0),
            vel: Vector2::new(0.0, 0.0),
            alive: false,
        });
    }
    Bullets { bullets }
}

fn create_boss() -> Boss {
    Boss {
        pos: Point2::origin(),
        vel: na::zero(),
        hp: 50.0,
        facing: Facing::Left,
        jumping: false,
        phase: BossPhase::Attack,
        phase_timer: 0.0,
    }
}

fn create_hook(pos: Point2) -> Hook {
    Hook {
        pos
    }
}

struct Assets {
    player_image: graphics::Image,
    bullet_image: graphics::Image,
    hook_image: graphics::Image,
    font: graphics::Font,
}

impl Assets {
    fn new(ctx: &mut Context) -> GameResult<Assets> {
        let player_image = graphics::Image::new(ctx, "/player.png")?;
        let bullet_image = graphics::Image::new(ctx, "/big_bullet.png")?;
        let hook_image = graphics::Image::new(ctx, "/big_bullet.png")?;
        let font = graphics::Font::new(ctx, "/Roboto-Regular.ttf", 18)?;

        Ok(Assets {
            player_image,
            bullet_image,
            hook_image,
            font,
         })
    }

    fn actor_image(&mut self, _: &Actor) -> &mut graphics::Image {
        &mut self.player_image
    }
}

#[derive(Debug)]
struct InputState {
    xaxis: f32,
    yaxis: f32,
    jump: bool,
    shoot: bool,
    tool: bool,
    keys: HashSet<Input>,
    just_pressed: HashSet<Input>,
}

impl InputState {
    fn register_keypress(&mut self, input: Input) {
        if !self.keys.contains(&input) {
            self.just_pressed.insert(input);
        }
        self.keys.insert(input);
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Input {
    LEFT,
    RIGHT,
    JUMP,
    SHOOT,
    TOOL,
}

impl Default for InputState {
    fn default() -> Self {
        InputState {
            xaxis: 0.0,
            yaxis: 0.0,
            jump: false,
            shoot: false,
            tool: false,
            keys: HashSet::new(),
            just_pressed: HashSet::new(),
        }
    }
}

struct MainState<'a, 'b> {
    player: Actor,
    bullets: Bullets,
    boss: Boss,
    hooks: Vec<Hook>,
    assets: Assets,
    screen_width: u32,
    screen_height: u32,
    global_time: f64,
    debug_data: graphics::Text,
    world: World,
    dispatcher: Dispatcher<'a, 'b>
}

impl<'a, 'b> MainState<'a, 'b> {
    fn new(ctx: &mut Context) -> GameResult<MainState<'a, 'b>> {
        ctx.print_resource_stats();
        graphics::set_background_color(ctx, (0, 0, 0, 255).into());

        println!("Game resource path: {:?}", ctx.filesystem);

        let assets = Assets::new(ctx)?;
        let debug_data = graphics::Text::new(ctx, "debug", &assets.font)?;

        let player = create_player();
        let bullets = create_bullets(100);

        let screen_width = ctx.conf.window_mode.width;
        let screen_height = ctx.conf.window_mode.height;

        let mut hooks = vec![];
        for i in 0..3 {
            let hook = create_hook(Point2::new(-150.0 + 150.0 * i as f32, 0.0));
            hooks.push(hook);
        }

        let now = get_time(ctx);

        let boss = create_boss();

        let mut world = World::new();
        world.register::<Pos>();
        world.register::<Vel>();
        world.register::<IsPlayer>();
        world.register::<BulletStatus>();
        world.register::<Facing>();
        world.register::<HasGravity>();
        world.register::<ShootCooldown>();
        world.register::<IsJumping>();
        world.register::<IsHook>();

        // The player
        world.create_entity()
            .with(Vel(na::zero()))
            .with(Pos(Point2::new(0.0, 0.0)))
            .with(Facing::Right)
            .with(IsPlayer)
            .with(HasGravity)
            .with(IsJumping(false))
            .with(ShootCooldown(0.035))
            .build();

        for _ in 0..100 {
            world.create_entity()
                .with(Vel(na::zero()))
                .with(Pos(Point2::new(0.0, 0.0)))
                .with(BulletStatus::Dead)
                .build();
        }

        for i in 0..3 {
            world.create_entity()
                .with(Pos(Point2::new(-150.0 + 150.0 * i as f32, 0.0)))
                .with(IsHook)
                .build();
        }

        world.add_resource(DeltaTime(0.0));
        world.add_resource(InputState::default());

        let mut dispatcher = DispatcherBuilder::new()
            .with(RigidBodyPhysics, "rigid-body-physics", &[])
            .with(PlayerControl, "player-control", &[])
            .with(ShootBullets, "shoot-bullets", &[])
            .with(DoHook, "do-hook", &[])
            .build();
        dispatcher.setup(&mut world.res);

        let s = MainState {
            player,
            assets,
            boss,
            hooks,
            bullets,
            screen_width,
            screen_height,
            global_time: now,
            debug_data,
            world,
            dispatcher
        };

        Ok(s)
    }

    fn update_ui(&mut self, ctx: &mut Context) {
        let debug_str = format!("Debug: {}", timer::get_fps(ctx) as i32);
        let debug_text = graphics::Text::new(ctx, &debug_str, &self.assets.font).unwrap();

        self.debug_data = debug_text;
    }

    /// The input state contains useful (but strictly redundant) flags that
    ///   area easier to use than just checking what inputs are pressed. This
    ///   function updates them.
    fn update_key_flags(&mut self) {
        // true  as i32 as f32 = 1.0
        // false as i32 as f32 = 0.0
        // This way, simultaneously pressing both left and right does nothing.
        //   It might be better to give precedence to whichever input was
        //   pressed latest, e.g. if you were holding right, then began to hold
        //   left while still holding right, the character would turn around.
        //   Instead we just require the player to release right if they want
        //   to turn around.
        
        let mut input_state = self.world.write_resource::<InputState>();

        let left = input_state.keys.contains(&Input::LEFT) as i32 as f32;
        let right = input_state.keys.contains(&Input::RIGHT) as i32 as f32;
        input_state.xaxis = (-1.0 * left) + (1.0 * right);

        input_state.jump = input_state.keys.contains(&Input::JUMP);
        input_state.shoot = input_state.keys.contains(&Input::SHOOT);
        input_state.tool = input_state.keys.contains(&Input::TOOL);
    }

    fn register_keypress(&mut self, input: Input) {
        let mut input_state = self.world.write_resource::<InputState>();
        if !input_state.keys.contains(&input) {
            input_state.just_pressed.insert(input);
        }
        input_state.keys.insert(input);
    }

    fn unregister_keypress(&mut self, input: Input) {
        let mut input_state = self.world.write_resource::<InputState>();
        input_state.keys.remove(&input);
    }
}

/// Translates the world coordinate system, which
/// has Y pointing up and the origin at the center,
/// to the screen coordinate system, which has Y
/// pointing downward and the origin at the top-left,
fn world_to_screen_coords(screen_width: u32, screen_height: u32, point: Point2) -> Point2 {
    let width = screen_width as f32;
    let height = screen_height as f32;
    let x = point.x + width / 2.0;
    let y = height - (point.y + height / 2.0);
    Point2::new(x, y)
}

/*
fn player_handle_input(actor: &mut Actor, bullets: &mut Bullets, hooks: &[Hook], input: &InputState, dt: f32, t: f64) {
    actor.vel.x = input.xaxis * 200.0;

    if input.xaxis < 0.0 && !input.shoot {
        actor.facing = Facing::Left;
    } else if input.xaxis > 0.0 && !input.shoot {
        actor.facing = Facing::Right;
    }

    if let Some(ref mut sd) = actor.swing_data {
        sd.theta0 += input.xaxis * dt;
    }

    if input.jump && !actor.jumping {
        actor.jumping = true;
        actor.vel.y = 300.0;
        actor.swing_data = None;
    }

    if input.just_pressed.contains(&Input::TOOL) {
        // Later we should switch on the kind of tool equipped.
        actor.swing_data = try_hook(&actor, hooks, t);
        if actor.swing_data.is_some() {
            actor.jumping = false;
        }
    }

    if input.shoot && actor.shoot_cooldown == 0.0 {
        actor.shoot_cooldown = 0.035;
        shoot_a_bullet(actor, bullets);
    }
}*/

fn try_hook(actor: &Actor, hooks: &[Hook], t: f64) -> Option<SwingData> {
    // Try to attach if we aren't hooked
    // by finding the closest hook and checking if it's within 100 pixels
    let (hook, nearest_dist) = hooks.iter()
        .map(|hook| {
            let d = hook.pos.distance(&actor.pos);
            (hook, d)
        })
        .min_by(|x, y| {
            PartialOrd::partial_cmp(&x.1, &y.1).unwrap()
        })
        .unwrap();
    if nearest_dist < 100.0 {
        let dx = actor.pos.x - hook.pos.x;
        let dy = actor.pos.y - hook.pos.y;
        let theta0 = dx.atan2(-dy);
        let dist = (dx * dx + dy * dy).sqrt();

        Some(SwingData {
            theta0,
            theta: theta0,
            start_time: t,
            target: *hook,
            dist,
        })
    } else {
        None
    }
}

fn player_update_position(actor: &mut Actor, dt: f32, t: f64) {
    let mut sd = actor.swing_data.take();
    if let Some(ref mut swing_data) = sd {
        player_update_swing(actor, swing_data, t);
    } else {
        //player_update_walk(actor, dt);
    }
    //player_update_gun(actor, dt);
    actor.swing_data = sd;
}

fn player_update_swing(actor: &mut Actor, swing_data: &mut SwingData, t: f64) {
    let theta0 = swing_data.theta0;
    let elapsed = t - swing_data.start_time;
    let dist = swing_data.dist;
    let k = 10.0 * 6.28 / f64::from(dist); // 2pi / period in seconds
    let theta = theta0 * (k * elapsed).cos() as f32;
    let target = swing_data.target;
    swing_data.theta = theta;

    actor.pos.x = target.pos.x + dist * theta.sin();
    actor.pos.y = target.pos.y - dist * theta.cos();

    actor.vel.y = 0.0;
}

fn boss_update(boss: &mut Boss, player: &mut Actor, bullets: &mut Bullets, dt: f32) {
    let dv = boss.vel * dt;
    boss.pos += dv;

    let dist = player.pos.x - boss.pos.x;

    if dist.abs() > 5.0 { // Chase player
        let direction = dist.signum();
        if direction > 0.0 {
            boss.facing = Facing::Right;
        } else {
            boss.facing = Facing::Left;
        }
        boss.pos.x += direction * 100.0 * dt;
    }

    if boss.pos.y < -150.0 {
        boss.pos.y = -150.0;
        if boss.vel.y < 0.0 {
            boss.vel.y = 0.0;
            boss.jumping = false;
        }
    } else {
        boss.vel.y -= dt * 500.0;
    }

    boss.vel.x -= 10.0 * dt * boss.vel.x;

    boss.phase_timer += dt;
    match boss.phase {
        BossPhase::Attack if boss.phase_timer > 10.0 => {
            boss.phase_timer = 0.0;
            boss.phase = BossPhase::Evade;

        }
        BossPhase::Evade if boss.phase_timer > 10.0 => {
            boss.phase_timer = 0.0;
            boss.phase = BossPhase::Attack;
        }
        _ => ()
    }

    match boss.phase {
        BossPhase::Attack => {
            boss_update_attack(boss, player, bullets, dt);
        }
        BossPhase::Evade => {
            boss_update_evade(boss, player, dt);
        }
    }
}

fn boss_update_attack(boss: &mut Boss, player: &mut Actor, bullets: &mut Bullets, dt: f32) {
    // Unimplemented   
}

fn boss_update_evade(boss: &mut Boss, player: &mut Actor, dt: f32) {
    // Unimplemented
}

fn handle_intersection(boss: &mut Boss, bullets: &mut Bullets, dt: f32) {
    for bullet in &mut bullets.bullets {
        if bullet.alive && Disc::new(bullet.pos, 5.0).intersects(&Disc::new(boss.pos, 10.0)) {
            bullet.alive = false;
            boss.hp -= 10.0;
            boss.vel.x += bullet.vel.x / 2.0;
        }
    }
}

struct Disc {
    pos: Point2,
    radius: f32
}

impl Disc {
    fn new(pos: Point2, radius: f32) -> Self {
        Disc { pos, radius }
    }

    fn intersects(&self, other: &Disc) -> bool {
        let d = self.pos.distance(&other.pos);
        d < self.radius || d < other.radius
    }
}

fn bullets_update_position(bullets: &mut Bullets, dt: f32) {
    for bullet in &mut bullets.bullets {
        if bullet.alive {
            bullet.pos += bullet.vel * dt;
            if bullet.pos.x > 400.0 || bullet.pos.x < -400.0 {
                bullet.alive = false;
            }
        }
    }
}

impl<'a, 'b> EventHandler for MainState<'a, 'b> {
    fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
        const DESIRED_FPS: u32 = 60;
        while timer::check_update_time(ctx, DESIRED_FPS) {
            let seconds = 1.0 / (DESIRED_FPS as f32);

            {
                let mut delta = self.world.write_resource::<DeltaTime>();
                *delta = DeltaTime(seconds);
            }

            //player_handle_input(&mut self.player, &mut self.bullets, &self.hooks, &self.input, seconds, self.global_time);
            //player_update_position(&mut self.player, seconds, self.global_time);
            //bullets_update_position(&mut self.bullets, seconds);
            //boss_update(&mut self.boss, &mut self.player, &mut self.bullets, seconds);
            //handle_intersection(&mut self.boss, &mut self.bullets, seconds);
            self.update_ui(ctx);
            self.update_key_flags();
            self.global_time = get_time(ctx);
            self.dispatcher.dispatch(&self.world.res);
        }
        let mut input_state = self.world.write_resource::<InputState>();
        input_state.just_pressed.clear();
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        graphics::clear(ctx);

        /*{
            let assets = &mut self.assets;
            let p = &self.player;
            draw_actor(assets, ctx, p, self.screen_width, self.screen_height)?;
            draw_boss(assets, ctx, &self.boss, self.screen_width, self.screen_height)?;
            draw_bullets(assets, ctx, &self.bullets, self.screen_width, self.screen_height)?;
            for hook in &self.hooks {
                draw_hook(assets, ctx, *hook, self.screen_width, self.screen_height)?;
            }
        }*/

        let debug_data_pos = graphics::Point2::new(10.0, 10.0);
        graphics::draw(ctx, &self.debug_data, debug_data_pos, 0.0)?;

        use specs::Join;

        let entities = self.world.entities();
        let positions = self.world.read_storage::<Pos>();
        let bullets = self.world.read_storage::<BulletStatus>();
        let hooks = self.world.read_storage::<IsHook>();

        for (ent, pos, bullet) in (&*entities, &positions, &bullets).join() {
            if let BulletStatus::Alive = bullet {
                draw_bullet_sprite(&mut self.assets, ctx, *pos, self.screen_width, self.screen_height)?;
            }
        }

        for (ent, pos, not_bullet, not_hook) in (&*entities, &positions, !&bullets, !&hooks).join() {
            draw_debug_sprite(&mut self.assets, ctx, *pos, self.screen_width, self.screen_height)?;
        }

        for (pos, hook) in (&positions, &hooks).join() {
            draw_bullet_sprite(&mut self.assets, ctx, *pos, self.screen_width, self.screen_height)?;
        }

        graphics::present(ctx);

        timer::yield_now();

        Ok(())
    }

    fn key_down_event(&mut self, ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        match keycode {
            Keycode::Left => {
                self.register_keypress(Input::LEFT);
            }
            Keycode::Right => {
                self.register_keypress(Input::RIGHT);
            }
            Keycode::Up | Keycode::Space => {
                self.register_keypress(Input::JUMP);
            }
            Keycode::Z => {
                self.register_keypress(Input::SHOOT);
            }
            Keycode::X => {
                self.register_keypress(Input::TOOL);
            }
            Keycode::Escape => ctx.quit().unwrap(),
            _ => (), // Do nothing
        }
    }

    fn key_up_event(&mut self, _ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        match keycode {
            Keycode::Left => {
                self.unregister_keypress(Input::LEFT);
            }
            Keycode::Right => {
                self.unregister_keypress(Input::RIGHT);
            }
            Keycode::Z => {
                self.unregister_keypress(Input::SHOOT);
            }
            Keycode::X => {
                self.unregister_keypress(Input::TOOL);
            }
            Keycode::Up | Keycode::Space => {
                self.unregister_keypress(Input::JUMP);
            }
            _ => (), // Do nothing
        }
    }
}

///
/// Main
///

pub fn main() {
    let mut cb = ContextBuilder::new("YEEHAW", "ggez")
        .window_setup(conf::WindowSetup::default().title("YEEHAW"))
        .window_mode(conf::WindowMode::default().dimensions(640, 360));

    // We add the CARGO_MANIFEST_DIR/resources to the filesystems paths so
    // we we look in the cargo project for files.
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        println!("Adding path {:?}", path);
        // We need this re-assignment alas, see
        // https://aturon.github.io/ownership/builders.html
        // under "Consuming builders"
        cb = cb.add_resource_path(path);
    } else {
        unimplemented!();
    }

    let mut ctx = &mut cb.build().unwrap();
    set_default_filter(ctx, FilterMode::Nearest);

    match MainState::new(&mut ctx) {
        Err(e) => {
            println!("Could not load game!");
            println!("Error: {}", e);
        }
        Ok(ref mut game) => {
            let result = event::run(&mut ctx, game);
            if let Err(e) = result {
                println!("Error encountered running game: {}", e);
            } else {
                println!("Game exited cleanly.");
            }
        }
    }
}
