extern crate ggez;

use ggez::audio;
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

///
/// Actor
///

#[derive(Debug)]
enum Facing {
    Left,
    Right,
}

#[derive(Debug)]
struct Actor {
    is_player: bool,
    pos: Point2,
    vel: Vector2,
    jumping: bool
}

#[derive(Debug)]
struct Bullet {
    pos: Point2,
    vel: Vector2,
    alive: bool,
}

#[derive(Debug)]
struct Bullets {
    bullets: Vec<Bullet>,
}

fn quantize(pos: Point2) -> Point2 {
    Point2::new(pos.x.floor(), pos.y.floor())
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
    graphics::draw_ex(ctx, image, draw_params)
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

fn create_player() -> Actor {
    Actor {
        is_player: true,
        pos: Point2::origin(),
        vel: na::zero(),
        jumping: false,
    }
}

fn create_bullets(n: u32) -> Bullets {
    let mut bullets = Vec::new();
    for i in 0..n {
        bullets.push(Bullet {
            pos: Point2::new(0.0, 0.0),
            vel: Vector2::new(0.0, 0.0),
            alive: false,
        });
    }
    Bullets { bullets }
}

struct Assets {
    player_image: graphics::Image,
    bullet_image: graphics::Image,
    font: graphics::Font,
}

impl Assets {
    fn new(ctx: &mut Context) -> GameResult<Assets> {
        let player_image = graphics::Image::new(ctx, "/player.png")?;
        let bullet_image = graphics::Image::new(ctx, "/sixtyfour.png")?;
        let font = graphics::Font::new(ctx, "/Roboto-Regular.ttf", 18)?;

        Ok(Assets {
            player_image,
            bullet_image,
            font,
         })
    }

    fn actor_image(&mut self, actor: &Actor) -> &mut graphics::Image {
        &mut self.player_image
    }
}

#[derive(Debug)]
struct InputState {
    xaxis: f32,
    yaxis: f32,
    jump: bool,
    shoot: bool,
    keys: HashSet<Input>,
    shoot_pressed_last_frame: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Input {
    LEFT,
    RIGHT,
    JUMP,
    SHOOT,
}

impl Default for InputState {
    fn default() -> Self {
        InputState {
            xaxis: 0.0,
            yaxis: 0.0,
            jump: false,
            shoot: false,
            shoot_pressed_last_frame: false,
            keys: HashSet::new(),
        }
    }
}

struct MainState {
    player: Actor,
    bullets: Bullets,
    input: InputState,
    assets: Assets,
    screen_width: u32,
    screen_height: u32,
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
        let bullets = create_bullets(20);

        let s = MainState {
            player,
            assets,
            bullets,
            screen_width: ctx.conf.window_mode.width,
            screen_height: ctx.conf.window_mode.height,
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
    
    fn update_keys(&mut self) {
        let left = self.input.keys.contains(&Input::LEFT) as i32 as f32;
        let right = self.input.keys.contains(&Input::RIGHT) as i32 as f32;
        self.input.xaxis = (-1.0 * left) + (1.0 * right);
        self.input.jump = self.input.keys.contains(&Input::JUMP);

        if !self.input.shoot_pressed_last_frame && self.input.keys.contains(&Input::SHOOT) {
            self.input.shoot = true;
        } else {
            self.input.shoot = false;
        }

        self.input.shoot_pressed_last_frame = self.input.keys.contains(&Input::SHOOT);
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

fn player_handle_input(actor: &mut Actor, bullets: &mut Bullets, input: &InputState, dt: f32) {
    actor.vel.x = input.xaxis * 200.0;

    if input.jump && !actor.jumping {
        actor.jumping = true;
        actor.vel.y = 300.0;
    }

    if input.shoot {
        shoot_a_bullet(actor, bullets);
    }
}

fn shoot_a_bullet(actor: &Actor, bullets: &mut Bullets) {
    for bullet in &mut bullets.bullets {
        if !bullet.alive {
            bullet.alive = true;
            bullet.pos = actor.pos;
            bullet.vel = Vector2::new(800.0, 0.0);
            break;
        }
    }
}

fn player_update_position(actor: &mut Actor, dt: f32) {
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

fn bullets_update_position(bullets: &mut Bullets, dt: f32) {
    for bullet in &mut bullets.bullets {
        if bullet.alive {
            bullet.pos += bullet.vel * dt;
            if bullet.pos.x > 300.0 {
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

            player_handle_input(&mut self.player, &mut self.bullets, &self.input, seconds);
            player_update_position(&mut self.player, seconds);
            bullets_update_position(&mut self.bullets, seconds);
            self.update_ui(ctx);
            self.update_keys();
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        graphics::clear(ctx);

        {
            let assets = &mut self.assets;
            let p = &self.player;
            draw_actor(assets, ctx, p, self.screen_width, self.screen_height)?;
            draw_bullets(assets, ctx, &self.bullets, self.screen_width, self.screen_height)?;
            graphics::line(ctx, &[Point2::new(0.0, 2.0*self.screen_height as f32/3.0), Point2::new(self.screen_width as f32, 2.0*self.screen_height as f32/3.0)], 1.0);
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
                self.input.keys.insert(Input::LEFT);
            }
            Keycode::Right => {
                self.input.keys.insert(Input::RIGHT);
            }
            Keycode::Up | Keycode::Space => {
                self.input.keys.insert(Input::JUMP);
            }
            Keycode::Z => {
                self.input.keys.insert(Input::SHOOT);
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
