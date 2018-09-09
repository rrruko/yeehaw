extern crate ggez;

use ggez::audio;
use ggez::conf;
use ggez::event::{self, EventHandler, Keycode, Mod};
use ggez::graphics;
use ggez::graphics::{Point2, Vector2};
use ggez::nalgebra as na;
use ggez::timer;
use ggez::{Context, ContextBuilder, GameResult};

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
    vel: Vector2
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
        dest: pos,
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
    }
}

struct Assets {
    player_image: graphics::Image,
    font: graphics::Font,
}

impl Assets {
    fn new(ctx: &mut Context) -> GameResult<Assets> {
        let player_image = graphics::Image::new(ctx, "/player.png")?;
        let font = graphics::Font::new(ctx, "/Roboto-Regular.ttf", 18)?;

        Ok(Assets {
            player_image,
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
}

impl Default for InputState {
    fn default() -> Self {
        InputState {
            xaxis: 0.0,
            yaxis: 0.0,
            jump: false,
        }
    }
}

struct MainState {
    player: Actor,
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

        let s = MainState {
            player,
            assets,
            screen_width: ctx.conf.window_mode.width,
            screen_height: ctx.conf.window_mode.height,
            input: InputState::default(),
            debug_data
        };

        Ok(s)
    }

    fn update_ui(&mut self, ctx: &mut Context) {
        let debug_str = format!("Debug: {}", 0.0);
        let debug_text = graphics::Text::new(ctx, &debug_str, &self.assets.font).unwrap();

        self.debug_data = debug_text;
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

fn player_handle_input(actor: &mut Actor, input: &InputState, dt: f32) {
    actor.vel.x += dt * input.xaxis;
    actor.vel.y += dt * input.yaxis;
}

fn player_update_position(actor: &mut Actor, dt: f32) {
    let norm_sq = actor.vel.norm_squared();
    if norm_sq > 10.0 {
        actor.vel = actor.vel / norm_sq.sqrt() * 10.0;
    }
    let dv = actor.vel * dt;
    actor.pos += dv;
}

impl EventHandler for MainState {
    fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
        const DESIRED_FPS: u32 = 60;
        while timer::check_update_time(ctx, DESIRED_FPS) {
            let seconds = 1.0 / (DESIRED_FPS as f32);

            player_handle_input(&mut self.player, &self.input, seconds);
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        graphics::clear(ctx);

        {
            let assets = &mut self.assets;
            let p = &self.player;
            draw_actor(assets, ctx, p, self.screen_width, self.screen_height)?;
        }

        let debug_data_pos = graphics::Point2::new(10.0, 10.0);
        graphics::draw(ctx, &self.debug_data, debug_data_pos, 0.0)?;

        graphics::present(ctx);

        timer::yield_now();

        Ok(())
    }

    fn key_down_event(&mut self, ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        match keycode {
            Keycode::Up => {
                self.input.yaxis = 1.0;
            }
            Keycode::Left => {
                self.input.xaxis = -1.0;
            }
            Keycode::Right => {
                self.input.xaxis = 1.0;
            }
            Keycode::Space => {
                self.input.jump = true;
            }
            Keycode::Escape => ctx.quit().unwrap(),
            _ => (), // Do nothing
        }
    }

    fn key_up_event(&mut self, _ctx: &mut Context, keycode: Keycode, _keymod: Mod, _repeat: bool) {
        match keycode {
            Keycode::Up => {
                self.input.yaxis = 0.0;
            }
            Keycode::Left | Keycode::Right => {
                self.input.xaxis = 0.0;
            }
            Keycode::Space => {
                self.input.jump = false;
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
        .window_mode(conf::WindowMode::default().dimensions(640, 480));

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
    }

    let ctx = &mut cb.build().unwrap();

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
