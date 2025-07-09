mod simd_utils;

use dxgi_capture_rs::{CaptureError, DXGIManager};
use eframe::egui;
use egui::{ColorImage, TextureHandle};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const CAPTURE_TIMEOUT: Duration = Duration::from_millis(0);

struct RenderableFrame {
    image: ColorImage,
    capture_fps: f32,
}

struct StreamApp {
    screen_texture: Option<TextureHandle>,
    frame_receiver: mpsc::Receiver<RenderableFrame>,
    _capture_thread: thread::JoinHandle<()>,
    last_frame_size: (usize, usize),
    capture_fps: f32,
    render_fps: f32,
    last_render_time: Instant,
    render_frame_count: u32,
}

impl StreamApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let egui_context = cc.egui_ctx.clone();

        let capture_thread = thread::spawn(move || {
            capture_thread_main(sender, egui_context);
        });

        Self {
            screen_texture: None,
            frame_receiver: receiver,
            _capture_thread: capture_thread,
            last_frame_size: (0, 0),
            capture_fps: 0.0,
            render_fps: 0.0,
            last_render_time: Instant::now(),
            render_frame_count: 0,
        }
    }

    fn calculate_scaled_size(&self, available_size: egui::Vec2) -> (f32, f32) {
        if self.last_frame_size.0 == 0 || self.last_frame_size.1 == 0 {
            return (0.0, 0.0);
        }

        let aspect_ratio = self.last_frame_size.0 as f32 / self.last_frame_size.1 as f32;
        let mut scaled_width = available_size.x;
        let mut scaled_height = scaled_width / aspect_ratio;

        if scaled_height > available_size.y {
            scaled_height = available_size.y;
            scaled_width = scaled_height * aspect_ratio;
        }

        (scaled_width, scaled_height)
    }
}

impl eframe::App for StreamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(latest_frame) = self.frame_receiver.try_iter().last() {
            self.last_frame_size = (latest_frame.image.width(), latest_frame.image.height());
            self.capture_fps = latest_frame.capture_fps;

            if let Some(texture) = &mut self.screen_texture {
                texture.set(latest_frame.image, egui::TextureOptions::NEAREST);
            } else {
                self.screen_texture = Some(ctx.load_texture(
                    "screen_capture",
                    latest_frame.image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }

        self.render_frame_count += 1;
        let elapsed = self.last_render_time.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.render_fps = self.render_frame_count as f32 / elapsed.as_secs_f32();
            self.render_frame_count = 0;
            self.last_render_time = Instant::now();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Capture: {:.1} FPS", self.capture_fps));
                ui.separator();
                ui.label(format!("Render: {:.1} FPS", self.render_fps));
                ui.separator();
                ui.label(format!(
                    "Resolution: {}x{}",
                    self.last_frame_size.0, self.last_frame_size.1
                ));
            });
            ui.separator();

            if let Some(ref texture) = self.screen_texture {
                let available_size = ui.available_size_before_wrap();
                let (scaled_width, scaled_height) = self.calculate_scaled_size(available_size);

                let (response_rect, _response) =
                    ui.allocate_at_least(available_size, egui::Sense::hover());
                let image_rect = egui::Rect::from_center_size(
                    response_rect.center(),
                    egui::vec2(scaled_width, scaled_height),
                );
                ui.put(
                    image_rect,
                    egui::Image::new(egui::ImageSource::Texture(egui::load::SizedTexture::new(
                        texture.id(),
                        egui::vec2(scaled_width, scaled_height),
                    ))),
                );
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Waiting for first frame...");
                });
            }
        });

        ctx.request_repaint();
    }
}

fn capture_thread_main(sender: mpsc::Sender<RenderableFrame>, ctx: egui::Context) {
    let mut manager = match DXGIManager::new(CAPTURE_TIMEOUT.as_millis() as u32) {
        Ok(manager) => manager,
        Err(e) => {
            eprintln!("Failed to create DXGIManager: {e:?}");
            return;
        }
    };

    let mut frame_count: u32 = 0;
    let mut last_fps_time = Instant::now();
    let mut last_reported_fps = 0.0;

    loop {
        match manager.capture_frame_components() {
            Ok((components, (width, height))) => {
                let mut rgba_pixels = components;
                simd_utils::bgra_to_rgba(&mut rgba_pixels);

                let image = ColorImage::from_rgba_unmultiplied([width, height], &rgba_pixels);

                frame_count += 1;
                let elapsed = last_fps_time.elapsed();
                if elapsed >= Duration::from_secs(1) {
                    last_reported_fps = frame_count as f32 / elapsed.as_secs_f32();
                    frame_count = 0;
                    last_fps_time = Instant::now();
                }

                let frame = RenderableFrame {
                    image,
                    capture_fps: last_reported_fps,
                };

                if sender.send(frame).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
            Err(CaptureError::Timeout) => {
                // No new frame available
            }
            Err(e) => {
                eprintln!("Capture error: {e:?}");
                if let CaptureError::AccessLost = e {
                    break;
                }
            }
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("DXGI Desktop Capture Stream"),
        ..Default::default()
    };

    eframe::run_native(
        "Desktop Capture Stream",
        options,
        Box::new(|cc| Ok(Box::new(StreamApp::new(cc)))),
    )
}
