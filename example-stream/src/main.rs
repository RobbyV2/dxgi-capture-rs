use dxgi_capture_rs::{CaptureError, DXGIManager};
use eframe::egui;
use egui::{ColorImage, TextureHandle};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const CAPTURE_TIMEOUT: Duration = Duration::from_millis(0); // Timeout for capture attempts

/// A processed frame, ready for display.
struct RenderableFrame {
    image: ColorImage,
    capture_fps: f32,
}

struct StreamApp {
    /// The texture displayed on screen.
    screen_texture: Option<TextureHandle>,
    /// Receives processed frames from the capture thread.
    frame_receiver: mpsc::Receiver<RenderableFrame>,
    /// A handle to the capture thread.
    _capture_thread: thread::JoinHandle<()>,
    /// Last known size of the captured frame.
    last_frame_size: (usize, usize),
    /// Frame rate of the capture thread.
    capture_fps: f32,
    /// Frame rate of the UI thread.
    render_fps: f32,
    /// For calculating render FPS.
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

    /// Calculate the scaled size to display the texture while maintaining aspect ratio.
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
        // Grab the latest frame from the capture thread, discarding any older ones.
        if let Some(latest_frame) = self.frame_receiver.try_iter().last() {
            self.last_frame_size = (latest_frame.image.width(), latest_frame.image.height());
            self.capture_fps = latest_frame.capture_fps;

            // Update the texture
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

        // Calculate rendering FPS
        self.render_frame_count += 1;
        let elapsed = self.last_render_time.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.render_fps = self.render_frame_count as f32 / elapsed.as_secs_f32();
            self.render_frame_count = 0;
            self.last_render_time = Instant::now();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // --- Top Stats Bar ---
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

            // --- Image Display Area ---
            if let Some(ref texture) = self.screen_texture {
                let available_size = ui.available_size_before_wrap();
                let (scaled_width, scaled_height) = self.calculate_scaled_size(available_size);

                // Allocate the space for the image and center it.
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

        // Request a repaint to get the next frame
        ctx.request_repaint();
    }
}

/// The main function for the capture thread.
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
    let mut log_timer = Instant::now();

    loop {
        let loop_start = Instant::now();
        match manager.capture_frame_components() {
            Ok((components, (width, height))) => {
                let capture_time = loop_start.elapsed();

                let convert_start = Instant::now();
                let mut pixels = Vec::with_capacity(width * height);

                // Ultra-fast bulk conversion using memory operations
                unsafe {
                    let pixel_count = width * height;
                    pixels.set_len(pixel_count);

                    // Copy memory in bulk, then modify in place
                    std::ptr::copy_nonoverlapping(
                        components.as_ptr() as *const u32,
                        pixels.as_mut_ptr(),
                        pixel_count,
                    );

                    // Now modify in place to swap B and R channels
                    let dst_u32 = pixels.as_mut_ptr();
                    for i in 0..pixel_count {
                        let bgra = *dst_u32.add(i);
                        // BGRA: 0xAABBGGRR -> RGBA: 0xAAGGBBRR
                        *dst_u32.add(i) =
                            (bgra & 0xFF00FF00) | ((bgra & 0xFF) << 16) | ((bgra & 0xFF0000) >> 16);
                    }
                }

                let image = ColorImage {
                    size: [width, height],
                    pixels: pixels
                        .into_iter()
                        .map(|p| unsafe { std::mem::transmute(p) })
                        .collect(),
                };
                let convert_time = convert_start.elapsed();

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
                    // Receiver has been dropped, so the main window was closed.
                    break;
                }
                ctx.request_repaint(); // Wake up UI thread

                if log_timer.elapsed() >= Duration::from_secs(1) {
                    println!(
                        "Capture Thread: Capture: {:.2}ms, Convert: {:.2}ms",
                        capture_time.as_secs_f32() * 1000.0,
                        convert_time.as_secs_f32() * 1000.0
                    );
                    log_timer = Instant::now();
                }
            }
            Err(CaptureError::Timeout) => {
                // This is expected if the screen is not updating. Yield to avoid pegging CPU.
                thread::yield_now();
            }
            Err(e) => {
                eprintln!("Capture error: {e:?}");
                break;
            }
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DXGI Stream Example",
        options,
        Box::new(|cc| Ok(Box::new(StreamApp::new(cc)))),
    )
}
