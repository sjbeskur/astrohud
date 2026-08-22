use astrohud_frame::BoxError;
use astrohud_frame::cache::CacheStore;
use astrohud_frame::config::{Config, usage};
use astrohud_frame::model::Playlist;
use astrohud_frame::sync::{ServerClient, sync_once};
use image::{DynamicImage, ImageDecoder, ImageReader};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::env;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), BoxError> {
    if env::args().any(|argument| matches!(argument.as_str(), "--help" | "-h")) {
        println!("{}", usage());
        return Ok(());
    }
    let config = Config::from_env_and_args()?;
    let initial =
        CacheStore::open(&config.cache_dir, config.cache_limit_bytes)?.cached_playlist()?;
    let updates = spawn_sync(config.clone());
    run_viewer(&config, initial, updates)
}

fn spawn_sync(config: Config) -> Receiver<Playlist> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let cache = match CacheStore::open(&config.cache_dir, config.cache_limit_bytes) {
            Ok(cache) => cache,
            Err(error) => {
                eprintln!("frame cache failed to start: {error}");
                return;
            }
        };
        let client = match ServerClient::new(&config.server_url, config.frame_id) {
            Ok(client) => client,
            Err(error) => {
                eprintln!("frame sync failed to start: {error}");
                return;
            }
        };

        loop {
            match sync_once(&client, &cache) {
                Ok(result) => {
                    for warning in result.warnings {
                        eprintln!("frame sync warning: {warning}");
                    }
                    let _ = sender.try_send(result.playlist);
                }
                Err(error) => eprintln!("frame sync unavailable; using cache: {error}"),
            }
            thread::sleep(config.sync_interval);
        }
    });
    receiver
}

fn run_viewer(
    config: &Config,
    mut playlist: Playlist,
    updates: Receiver<Playlist>,
) -> Result<(), BoxError> {
    let sdl = sdl2::init().map_err(other)?;
    let video = sdl.video().map_err(other)?;
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "best");

    let mut window_builder = video.window("AstroHUD", 960, 540);
    if config.windowed {
        window_builder.position_centered().resizable();
    } else {
        window_builder.fullscreen_desktop().borderless();
    }
    let window = window_builder.build().map_err(other)?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(other)?;
    canvas.set_draw_color(Color::RGB(5, 7, 12));
    canvas.clear();
    canvas.present();

    let mut events = sdl.event_pump().map_err(other)?;
    let mut current = None;
    let mut next_slide = Instant::now();
    let mut running = true;

    while running {
        for event in events.poll_iter() {
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    }
            ) {
                running = false;
            }
        }

        loop {
            match updates.try_recv() {
                Ok(update) => {
                    let old_first = playlist.photos.first().map(|photo| &photo.id);
                    let new_first = update.photos.first().map(|photo| &photo.id);
                    if old_first != new_first {
                        current = None;
                        next_slide = Instant::now();
                    }
                    playlist = update;
                    let _ = canvas
                        .window_mut()
                        .set_title(&format!("AstroHUD — {}", playlist.place_name));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if !playlist.photos.is_empty() && Instant::now() >= next_slide {
            current = show_next(&mut canvas, &playlist, current);
            next_slide = Instant::now() + config.slide_interval;
        }

        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn show_next(
    canvas: &mut Canvas<Window>,
    playlist: &Playlist,
    current: Option<usize>,
) -> Option<usize> {
    for offset in 1..=playlist.photos.len() {
        let index = (current.unwrap_or(playlist.photos.len() - 1) + offset) % playlist.photos.len();
        match render_photo(canvas, &playlist.photos[index].path) {
            Ok(()) => return Some(index),
            Err(error) => eprintln!(
                "could not display {}: {error}",
                playlist.photos[index].path.display()
            ),
        }
    }
    None
}

fn render_photo(canvas: &mut Canvas<Window>, path: &Path) -> Result<(), BoxError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let rgba = image.into_rgba8();
    let (source_width, source_height) = rgba.dimensions();

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, source_width, source_height)
        .map_err(other)?;
    texture
        .update(None, rgba.as_raw(), source_width as usize * 4)
        .map_err(other)?;

    let (output_width, output_height) = canvas.output_size().map_err(other)?;
    let destination = contain_rect(source_width, source_height, output_width, output_height);
    canvas.set_draw_color(Color::RGB(5, 7, 12));
    canvas.clear();
    canvas.copy(&texture, None, destination).map_err(other)?;
    canvas.present();
    Ok(())
}

fn contain_rect(
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
) -> Rect {
    let width_limited = u64::from(output_width) * u64::from(source_height)
        <= u64::from(output_height) * u64::from(source_width);
    let (width, height) = if width_limited {
        (
            output_width,
            (u64::from(source_height) * u64::from(output_width) / u64::from(source_width)) as u32,
        )
    } else {
        (
            (u64::from(source_width) * u64::from(output_height) / u64::from(source_height)) as u32,
            output_height,
        )
    };
    Rect::new(
        ((output_width - width) / 2) as i32,
        ((output_height - height) / 2) as i32,
        width,
        height,
    )
}

fn other(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_image_is_letterboxed_vertically() {
        assert_eq!(
            contain_rect(1600, 900, 1024, 768),
            Rect::new(0, 96, 1024, 576)
        );
    }

    #[test]
    fn portrait_image_is_letterboxed_horizontally() {
        assert_eq!(
            contain_rect(900, 1600, 1920, 1080),
            Rect::new(656, 0, 607, 1080)
        );
    }
}
