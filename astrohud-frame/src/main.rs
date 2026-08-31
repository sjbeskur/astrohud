use astrohud_frame::BoxError;
use astrohud_frame::cache::CacheStore;
use astrohud_frame::config::{Config, usage};
use astrohud_frame::model::Playlist;
use astrohud_frame::sync::{ServerClient, sync_once};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use image::{DynamicImage, ImageDecoder, ImageReader};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const FRAME_MARGIN: u32 = 24;
const FRAME_GAP: i32 = 8;
const FRAME_RAIL: u32 = 5;
const HUD_LINE: Color = Color::RGB(48, 58, 75);
const HUD_SURFACE: Color = Color::RGB(16, 21, 32);
const HUD_TEXT: Color = Color::RGB(237, 242, 247);
const HUD_AMBER: Color = Color::RGB(239, 180, 106);
const HUD_SALMON: Color = Color::RGB(233, 140, 119);
const HUD_LAVENDER: Color = Color::RGB(173, 150, 216);
const HUD_BLUE: Color = Color::RGB(112, 169, 214);
const HUD_ACCENTS: [Color; 4] = [HUD_AMBER, HUD_SALMON, HUD_LAVENDER, HUD_BLUE];

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
        let device_credential = loop {
            match load_device_credential(config.device_credential_file.as_deref()) {
                Ok(credential) => break credential,
                Err(error) => {
                    eprintln!("frame credential unavailable; waiting: {error}");
                    thread::sleep(config.sync_interval);
                }
            }
        };
        let client = match ServerClient::new(&config.server_url, config.frame_id, device_credential)
        {
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

fn load_device_credential(path: Option<&Path>) -> Result<Option<String>, BoxError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let credential = fs::read_to_string(path)?;
    let credential = credential.trim();
    if credential.len() < 32 || credential.chars().any(char::is_whitespace) {
        return Err(format!("invalid device credential in {}", path.display()).into());
    }
    Ok(Some(credential.to_owned()))
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
    let mut next_setup_check = Instant::now();
    let mut setup_revision = None;
    let mut setup_active = false;
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

        let now = Instant::now();
        if now >= next_setup_check {
            let detected_revision = setup_screen_revision(&config.setup_screen);
            match detected_revision {
                Some(revision) if setup_revision.as_ref() != Some(&revision) => {
                    match render_image(&mut canvas, &config.setup_screen, None, None) {
                        Ok(()) => {
                            setup_revision = Some(revision);
                            setup_active = true;
                        }
                        Err(error) => eprintln!("could not display setup screen: {error}"),
                    }
                }
                None => {
                    setup_revision = None;
                    if setup_active {
                        setup_active = false;
                        current = None;
                        next_slide = now;
                    }
                }
                Some(_) => {}
            }
            next_setup_check = now + Duration::from_millis(500);
        }

        if !setup_active && !playlist.photos.is_empty() && now >= next_slide {
            current = show_next(&mut canvas, &playlist, current);
            next_slide = now + config.slide_interval;
        }

        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SetupScreenRevision {
    length: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

fn setup_screen_revision(path: &Path) -> Option<SetupScreenRevision> {
    let metadata = fs::metadata(path).ok()?;
    Some(SetupScreenRevision {
        length: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        device: std::os::unix::fs::MetadataExt::dev(&metadata),
        #[cfg(unix)]
        inode: std::os::unix::fs::MetadataExt::ino(&metadata),
    })
}

fn show_next(
    canvas: &mut Canvas<Window>,
    playlist: &Playlist,
    current: Option<usize>,
) -> Option<usize> {
    for offset in 1..=playlist.photos.len() {
        let index = (current.unwrap_or(playlist.photos.len() - 1) + offset) % playlist.photos.len();
        let photo = &playlist.photos[index];
        match render_image(
            canvas,
            &photo.path,
            Some(&photo.id),
            photo.location_label.as_deref(),
        ) {
            Ok(()) => return Some(index),
            Err(error) => eprintln!(
                "could not display {}: {error}",
                playlist.photos[index].path.display()
            ),
        }
    }
    None
}

fn render_image(
    canvas: &mut Canvas<Window>,
    path: &Path,
    chrome_seed: Option<&str>,
    location_label: Option<&str>,
) -> Result<(), BoxError> {
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
    let destination = if chrome_seed.is_some() {
        framed_contain_rect(source_width, source_height, output_width, output_height)
    } else {
        contain_rect(source_width, source_height, output_width, output_height)
    };
    canvas.set_draw_color(Color::RGB(5, 7, 12));
    canvas.clear();
    if let Some(seed) = chrome_seed {
        render_frame_chrome(canvas, destination, seed)?;
    }
    canvas.copy(&texture, None, destination).map_err(other)?;
    if let (Some(seed), Some(location_label)) = (chrome_seed, location_label) {
        render_location_tab(canvas, destination, seed, location_label)?;
    }
    canvas.present();
    Ok(())
}

fn framed_contain_rect(
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
) -> Rect {
    let margin = FRAME_MARGIN
        .min(output_width.saturating_sub(1) / 2)
        .min(output_height.saturating_sub(1) / 2);
    let available_width = output_width.saturating_sub(margin * 2).max(1);
    let available_height = output_height.saturating_sub(margin * 2).max(1);
    let contained = contain_rect(
        source_width,
        source_height,
        available_width,
        available_height,
    );
    Rect::new(
        contained.x() + margin as i32,
        contained.y() + margin as i32,
        contained.width(),
        contained.height(),
    )
}

fn render_frame_chrome(
    canvas: &mut Canvas<Window>,
    image: Rect,
    seed: &str,
) -> Result<(), BoxError> {
    let chrome = frame_chrome(image, seed);
    canvas.set_draw_color(HUD_LINE);
    canvas.draw_rect(chrome.outline).map_err(other)?;
    canvas.set_draw_color(HUD_ACCENTS[chrome.rail_color]);
    canvas.fill_rect(chrome.rail).map_err(other)?;
    canvas.set_draw_color(HUD_ACCENTS[chrome.first_signal_color]);
    canvas.fill_rect(chrome.first_signal).map_err(other)?;
    canvas.set_draw_color(HUD_ACCENTS[chrome.second_signal_color]);
    canvas.fill_rect(chrome.second_signal).map_err(other)?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct FrameChrome {
    outline: Rect,
    rail: Rect,
    first_signal: Rect,
    second_signal: Rect,
    rail_color: usize,
    first_signal_color: usize,
    second_signal_color: usize,
}

fn frame_chrome(image: Rect, seed: &str) -> FrameChrome {
    let outline = Rect::new(
        image.x() - FRAME_GAP,
        image.y() - FRAME_GAP,
        image.width() + (FRAME_GAP as u32 * 2),
        image.height() + (FRAME_GAP as u32 * 2),
    );
    let hash = stable_hash(seed);
    let rail_on_right = hash & 1 != 0;
    let first_on_bottom = hash & 2 != 0;
    let first_at_end = hash & 4 != 0;
    let second_at_end = hash & 8 == 0;
    let length_variation = ((hash >> 8) % 81) as u32;
    let signal_length = (outline.width() / 5 + length_variation).clamp(72, 260);
    let signal_x = |at_end| {
        if at_end {
            outline.x() + outline.width() as i32 - signal_length as i32
        } else {
            outline.x()
        }
    };
    let signal_y = |on_bottom| {
        if on_bottom {
            outline.y() + outline.height() as i32 - FRAME_RAIL as i32
        } else {
            outline.y()
        }
    };
    let palette_offset = ((hash >> 16) as usize) % HUD_ACCENTS.len();
    FrameChrome {
        outline,
        rail: Rect::new(
            if rail_on_right {
                outline.x() + outline.width() as i32 - FRAME_RAIL as i32
            } else {
                outline.x()
            },
            outline.y(),
            FRAME_RAIL,
            outline.height(),
        ),
        first_signal: Rect::new(
            signal_x(first_at_end),
            signal_y(first_on_bottom),
            signal_length,
            FRAME_RAIL,
        ),
        second_signal: Rect::new(
            signal_x(second_at_end),
            signal_y(!first_on_bottom),
            signal_length,
            FRAME_RAIL,
        ),
        rail_color: palette_offset,
        first_signal_color: (palette_offset + 1) % HUD_ACCENTS.len(),
        second_signal_color: (palette_offset + 2) % HUD_ACCENTS.len(),
    }
}

/// FNV-1a gives a stable visual identity across processes, builds, and reboots.
fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn render_location_tab(
    canvas: &mut Canvas<Window>,
    image: Rect,
    seed: &str,
    location_label: &str,
) -> Result<(), BoxError> {
    let (output_width, _) = canvas.output_size().map_err(other)?;
    let tab = location_tab(image, output_width, seed, location_label);

    canvas.set_draw_color(HUD_SURFACE);
    canvas.fill_rect(tab.body).map_err(other)?;
    canvas.fill_rect(tab.cap_inner).map_err(other)?;
    canvas.fill_rect(tab.cap_outer).map_err(other)?;
    canvas.set_draw_color(HUD_LINE);
    canvas.draw_rect(tab.body).map_err(other)?;
    canvas.set_draw_color(HUD_ACCENTS[tab.accent_color]);
    canvas.fill_rect(tab.rail).map_err(other)?;
    canvas.fill_rect(tab.signal).map_err(other)?;
    draw_bitmap_text(
        canvas,
        tab.text_x,
        tab.text_y,
        tab.text_scale,
        &tab.text,
        HUD_TEXT,
    )
}

#[derive(Debug, PartialEq, Eq)]
struct LocationTab {
    body: Rect,
    cap_inner: Rect,
    cap_outer: Rect,
    rail: Rect,
    signal: Rect,
    text_x: i32,
    text_y: i32,
    text_scale: u32,
    text: String,
    accent_color: usize,
}

fn location_tab(image: Rect, output_width: u32, seed: &str, label: &str) -> LocationTab {
    let outline = frame_chrome(image, seed).outline;
    let available_width = outline.width().min(output_width.saturating_sub(32)).max(80);
    let text_scale = if available_width >= 620 { 2 } else { 1 };
    let horizontal_padding = 18_u32;
    let cap_width = 10_u32;
    let max_chars =
        available_width.saturating_sub(horizontal_padding * 2 + cap_width) / (9 * text_scale);
    let text = location_tab_text(label, max_chars as usize);
    let text_width = text.chars().count() as u32 * 9 * text_scale;
    let height = 8 * text_scale + 16;
    let width = (text_width + horizontal_padding * 2 + cap_width).min(available_width);
    let align_right = stable_hash(seed) & 0x20 != 0;
    let x = if align_right {
        outline.right() - width as i32
    } else {
        outline.x()
    }
    .max(0);
    let y = (outline.bottom() - height as i32 + 1).max(0);
    let body_width = width.saturating_sub(cap_width);
    let accent_color = ((stable_hash(seed) >> 20) as usize) % HUD_ACCENTS.len();

    LocationTab {
        body: Rect::new(x, y, body_width, height),
        cap_inner: Rect::new(x + body_width as i32, y + 3, 5, height.saturating_sub(6)),
        cap_outer: Rect::new(
            x + body_width as i32 + 5,
            y + 7,
            5,
            height.saturating_sub(14),
        ),
        rail: Rect::new(x, y, 5, height),
        signal: Rect::new(x, y, (width / 4).clamp(38, 110), 3),
        text_x: x + horizontal_padding as i32,
        text_y: y + 8,
        text_scale,
        text,
        accent_color,
    }
}

fn location_tab_text(label: &str, max_chars: usize) -> String {
    let sanitized = label
        .trim()
        .to_uppercase()
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '?'
            }
        })
        .collect::<String>();
    let text = format!("LOCATION // {sanitized}");
    if text.chars().count() <= max_chars {
        return text;
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    let mut truncated = text.chars().take(max_chars - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn draw_bitmap_text(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    scale: u32,
    text: &str,
    color: Color,
) -> Result<(), BoxError> {
    canvas.set_draw_color(color);
    let mut cursor = x;
    for character in text.chars() {
        if let Some(glyph) = BASIC_FONTS.get(character) {
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..8 {
                    if bits & (1 << column) != 0 {
                        canvas
                            .fill_rect(Rect::new(
                                cursor + column * scale as i32,
                                y + row as i32 * scale as i32,
                                scale,
                                scale,
                            ))
                            .map_err(other)?;
                    }
                }
            }
        }
        cursor += 9 * scale as i32;
    }
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
    fn setup_screen_revision_detects_atomic_replacement() {
        let path = std::env::temp_dir().join(format!(
            "astrohud-frame-setup-revision-{}",
            std::process::id()
        ));
        let replacement = path.with_extension("replacement");
        fs::write(&path, b"alpha").expect("write first screen");
        let first = setup_screen_revision(&path).expect("first revision");

        fs::write(&replacement, b"bravo").expect("write replacement screen");
        fs::rename(&replacement, &path).expect("replace screen atomically");
        let second = setup_screen_revision(&path).expect("second revision");

        assert_ne!(first, second);
        fs::remove_file(path).expect("remove test screen");
    }

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

    #[test]
    fn framed_landscape_leaves_room_for_chrome() {
        assert_eq!(
            framed_contain_rect(1600, 900, 1024, 768),
            Rect::new(24, 109, 976, 549)
        );
    }

    #[test]
    fn framed_portrait_leaves_room_for_chrome() {
        assert_eq!(
            framed_contain_rect(900, 1600, 1920, 1080),
            Rect::new(670, 24, 580, 1032)
        );
    }

    #[test]
    fn chrome_stays_outside_the_photo() {
        let chrome = frame_chrome(Rect::new(100, 80, 800, 450), "photo-1");
        assert_eq!(chrome.outline, Rect::new(92, 72, 816, 466));
        assert!(chrome.rail.x() == 92 || chrome.rail.x() == 903);
        for signal in [chrome.first_signal, chrome.second_signal] {
            assert!(signal.x() >= chrome.outline.x());
            assert!(signal.right() <= chrome.outline.right());
            assert!(signal.y() == 72 || signal.y() == 533);
        }
    }

    #[test]
    fn chrome_is_stable_for_a_photo() {
        let image = Rect::new(100, 80, 800, 450);
        assert_eq!(
            frame_chrome(image, "photo-42"),
            frame_chrome(image, "photo-42")
        );
    }

    #[test]
    fn chrome_varies_between_photos() {
        let image = Rect::new(100, 80, 800, 450);
        let variants =
            ["photo-1", "photo-2", "photo-3", "photo-4"].map(|seed| frame_chrome(image, seed));
        assert!(variants.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn location_tab_is_attached_to_the_lower_photo_edge() {
        let image = Rect::new(100, 80, 800, 450);
        let tab = location_tab(image, 1024, "photo-1", "40.0 N / 105.3 W");
        let outline = frame_chrome(image, "photo-1").outline;
        assert_eq!(
            tab.body.y() + tab.body.height() as i32 - 1,
            outline.bottom()
        );
        assert!(tab.body.x() >= outline.x());
        assert!(tab.cap_outer.right() <= outline.right());
        assert_eq!(tab.text, "LOCATION // 40.0 N / 105.3 W");
    }

    #[test]
    fn long_location_labels_are_safely_truncated() {
        assert_eq!(
            location_tab_text("A very long location name", 16),
            "LOCATION // A..."
        );
    }
}
