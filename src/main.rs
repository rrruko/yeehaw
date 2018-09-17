extern crate ggez;
extern crate rand;

use ggez::conf;
use ggez::event::{self, EventHandler, Keycode, Mod};
use ggez::graphics;
use ggez::graphics::{FilterMode, Point2, Vector2, set_default_filter};
use ggez::nalgebra as na;
use ggez::timer;
use ggez::{Context, ContextBuilder, GameResult};

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

#[derive(Debug)]
enum Facing {
    Left,
    Right,
}

impl Facing {
    fn to_f32(&self) -> f32 {
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

struct MainState {
    player: Actor,
    bullets: Bullets,
    hooks: Vec<Hook>,
    input: InputState,
    assets: Assets,
    screen_width: u32,
    screen_height: u32,
    global_time: f64,
    debug_data: graphics::Text,
}

impl MainState {
    fn new(ctx: &mut Context) -> GameResult<MainState> {
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

        let s = MainState {
            player,
            assets,
            hooks,
            bullets,
            screen_width,
            screen_height,
            global_time: now,
            input: InputState::default(),
            debug_data
        };

        Ok(s)
    }

    fn update_ui(&mut self, ctx: &mut Context) {
        let debug_str = format!("Debug: {}", self.player.pos.y);
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
        let left = self.input.keys.contains(&Input::LEFT) as i32 as f32;
        let right = self.input.keys.contains(&Input::RIGHT) as i32 as f32;
        self.input.xaxis = (-1.0 * left) + (1.0 * right);

        self.input.jump = self.input.keys.contains(&Input::JUMP);
        self.input.shoot = self.input.keys.contains(&Input::SHOOT);
        self.input.tool = self.input.keys.contains(&Input::TOOL);
    }

    fn register_keypress(&mut self, input: Input) {
        if !self.input.keys.contains(&input) {
            self.input.just_pressed.insert(input);
        }
        self.input.keys.insert(input);
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
}

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

fn shoot_a_bullet(actor: &Actor, bullets: &mut Bullets) {
    for bullet in &mut bullets.bullets {
        if !bullet.alive {
            bullet.alive = true;
            bullet.pos = actor.pos;
            bullet.vel = Vector2::new(600.0 * actor.facing.to_f32(), 0.0);
            break;
        }
    }
}

fn player_update_position(actor: &mut Actor, dt: f32, t: f64) {
    let mut sd = actor.swing_data.take();
    if let Some(ref mut swing_data) = sd {
        player_update_swing(actor, swing_data, t);
    } else {
        player_update_walk(actor, dt);
    }
    player_update_gun(actor, dt);
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

fn player_update_walk(actor: &mut Actor, dt: f32) {
    let dv = actor.vel * dt;
    actor.pos += dv;

    if actor.pos.y < -150.0 {
        actor.pos.y = -150.0;
        if actor.vel.y < 0.0 {
            actor.vel.y = 0.0;
            actor.jumping = false;
        }
    } else {
        actor.vel.y -= dt * 500.0;
    }
}

fn player_update_gun(actor: &mut Actor, dt: f32) {
    if actor.shoot_cooldown > 0.0 {
        actor.shoot_cooldown -= dt;
    }
    if actor.shoot_cooldown < 0.0 {
        actor.shoot_cooldown = 0.0;
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

impl EventHandler for MainState {
    fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
        const DESIRED_FPS: u32 = 60;
        while timer::check_update_time(ctx, DESIRED_FPS) {
            let seconds = 1.0 / (DESIRED_FPS as f32);

            player_handle_input(&mut self.player, &mut self.bullets, &self.hooks, &self.input, seconds, self.global_time);
            player_update_position(&mut self.player, seconds, self.global_time);
            bullets_update_position(&mut self.bullets, seconds);
            self.update_ui(ctx);
            self.update_key_flags();
            self.global_time = get_time(ctx);
        }
        self.input.just_pressed.clear();
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        graphics::clear(ctx);

        {
            let assets = &mut self.assets;
            let p = &self.player;
            draw_actor(assets, ctx, p, self.screen_width, self.screen_height)?;
            draw_bullets(assets, ctx, &self.bullets, self.screen_width, self.screen_height)?;
            for hook in &self.hooks {
                draw_hook(assets, ctx, *hook, self.screen_width, self.screen_height)?;
            }
        }

        let debug_data_pos = graphics::Point2::new(10.0, 10.0);
        graphics::draw(ctx, &self.debug_data, debug_data_pos, 0.0)?;

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
                self.input.keys.remove(&Input::LEFT);
            }
            Keycode::Right => {
                self.input.keys.remove(&Input::RIGHT);
            }
            Keycode::Z => {
                self.input.keys.remove(&Input::SHOOT);
            }
            Keycode::X => {
                self.input.keys.remove(&Input::TOOL);
            }
            Keycode::Up | Keycode::Space => {
                self.input.keys.remove(&Input::JUMP);
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

    let ctx = &mut cb.build().unwrap();
    set_default_filter(ctx, FilterMode::Nearest);


    match MainState::new(ctx) {
        Err(e) => {
            println!("Could not load game!");
            println!("Error: {}", e);
        }
        Ok(ref mut game) => {
            let result = event::run(ctx, game);
            if let Err(e) = result {
                println!("Error encountered running game: {}", e);
            } else {
                println!("Game exited cleanly.");
            }
        }
    }
}
