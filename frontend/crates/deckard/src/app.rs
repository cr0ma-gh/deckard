//! Window, event loop and the emulation main loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use deckard_audio::AudioOutput;
use deckard_core::Core;
use deckard_input::Input;
use deckard_video::{Presented, Renderer};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window, WindowId};

use crate::pacing::Pacer;

/// Audio buffering target.
const AUDIO_LATENCY_MS: u32 = 64;

pub struct Options {
    pub fullscreen: bool,
    /// Quit after this many core frames (for automated checks).
    pub max_frames: Option<u64>,
}

struct Running {
    window: Arc<Window>,
    renderer: Renderer,
}

struct App {
    core: Core,
    options: Options,
    input: Input,
    audio: Option<AudioOutput>,
    pacer: Pacer,
    running: Option<Running>,
    frames: u64,
    /// Time of the first emulated frame.
    started: Option<Instant>,
    error: Option<anyhow::Error>,
}

/// Runs the core in a window until the user quits.
pub fn run(core: Core, options: Options) -> anyhow::Result<()> {
    let event_loop = EventLoop::new().context("cannot create the event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let audio = match AudioOutput::new(AUDIO_LATENCY_MS) {
        Ok(a) => Some(a),
        Err(e) => {
            tracing::warn!("running without audio: {e}");
            None
        }
    };
    let pacer = Pacer::new(core.av_info().fps, None);
    let mut app = App {
        core,
        options,
        input: Input::new(),
        audio,
        pacer,
        running: None,
        frames: 0,
        started: None,
        error: None,
    };
    event_loop.run_app(&mut app).context("event loop failed")?;
    if let Some(started) = app.started {
        let seconds = started.elapsed().as_secs_f64();
        tracing::info!(
            frames = app.frames,
            seconds = format!("{seconds:.2}"),
            fps = format!("{:.2}", app.frames as f64 / seconds.max(f64::EPSILON)),
            audio_underruns = app.audio.as_ref().map_or(0, AudioOutput::underruns),
            "session ended"
        );
    }
    app.error.map_or(Ok(()), Err)
}

impl App {
    fn start(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let av = self.core.av_info();
        let title = format!("Deckard — {}", self.core.system_info().library_name);
        let scale = 3.0;
        let mut attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(
                f64::from(av.base_height) * f64::from(av.aspect_ratio) * scale,
                f64::from(av.base_height) * scale,
            ));
        if self.options.fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .context("cannot create the window")?,
        );
        let renderer = Renderer::new(event_loop.owned_display_handle(), window.clone())?;

        let display_hz = window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| f64::from(mhz) / 1000.0);
        if let Some(hz) = display_hz {
            self.core.set_target_refresh_rate(hz as f32);
        }
        self.pacer.update(av.fps, display_hz);
        self.running = Some(Running { window, renderer });
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    /// One iteration of the main loop: input, emulation, audio, video.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        for (port, pad) in self.input.poll().into_iter().enumerate() {
            self.core.set_pad(port, pad);
        }
        if self.input.take_hotkeys().quit {
            event_loop.exit();
            return;
        }

        let due = self.pacer.frames_due(Instant::now());
        for _ in 0..due {
            self.core.run_frame();
            self.frames += 1;
            self.started.get_or_insert_with(Instant::now);
            if let Some(audio) = self.audio.as_mut() {
                let rate = self
                    .pacer
                    .audio_source_rate(self.core.av_info().sample_rate);
                audio.push(self.core.audio(), rate);
            }
        }
        if self.core.take_av_changed() {
            let display_hz = self
                .running
                .as_ref()
                .and_then(|r| r.window.current_monitor())
                .and_then(|m| m.refresh_rate_millihertz())
                .map(|mhz| f64::from(mhz) / 1000.0);
            self.pacer.update(self.core.av_info().fps, display_hz);
        }

        let Some(running) = self.running.as_mut() else {
            return;
        };
        if due > 0
            && let Some(frame) = self.core.frame()
        {
            running.renderer.upload(&frame);
        }
        if running.renderer.render(self.core.av_info().aspect_ratio) == Presented::No {
            // No vsync to wait on (hidden window): don't spin.
            std::thread::sleep(Duration::from_secs_f64(
                1.0 / self.core.av_info().fps.max(1.0),
            ));
        }

        if self.core.shutdown_requested()
            || self
                .options
                .max_frames
                .is_some_and(|max| self.frames >= max)
        {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_none()
            && let Err(e) = self.start(event_loop)
        {
            self.fail(event_loop, e);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(running) = self.running.as_mut() {
                    running.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => self.input.clear_keyboard(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key
                    && !event.repeat
                {
                    self.input
                        .handle_key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(running) = &self.running {
            running.window.request_redraw();
        }
    }
}
