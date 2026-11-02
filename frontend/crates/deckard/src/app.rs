//! Window, event loop and the two modes: the menu and a running game.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use deckard_audio::AudioOutput;
use deckard_config::{Config, Paths};
use deckard_input::{Hotkeys, Input, key_code};
use deckard_library::Library;
use deckard_ui::UiRenderer;
use deckard_ui::i18n::{Strings, fill};
use deckard_ui::layout::{Hints, MenuView, layout};
use deckard_ui::nav::{Action, Menu};
use deckard_ui::repeat::{Held, Repeater};
use deckard_video::{Presented, Renderer};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

use crate::launcher::{Game, LaunchRequest};

/// Audio buffering target.
const AUDIO_LATENCY_MS: u32 = 64;

pub struct Options {
    pub fullscreen: bool,
    /// Quit after this many frames (emulated in a game, presented in the menu),
    /// for automated checks.
    pub max_frames: Option<u64>,
    /// Quit instead of returning to the menu when the game ends
    /// (games started from the command line).
    pub quit_after_game: bool,
}

/// Configuration and library shared by both modes.
pub struct Context {
    pub config: Config,
    pub paths: Paths,
    pub library: Library,
}

enum Mode {
    Menu,
    Game(Box<Game>),
}

struct Running {
    window: Arc<Window>,
    renderer: Renderer,
    ui: UiRenderer,
}

struct App {
    ctx: Context,
    options: Options,
    strings: Strings,
    menu: Menu,
    view: MenuView,
    repeater: Repeater,
    last_tick: Instant,
    mode: Mode,
    input: Input,
    audio: Option<AudioOutput>,
    running: Option<Running>,
    presented: u64,
    error: Option<anyhow::Error>,
}

/// Runs until the user quits. Starts in `game` if given, otherwise in the menu.
pub fn run(ctx: Context, game: Option<Game>, options: Options) -> anyhow::Result<()> {
    let event_loop = EventLoop::new().context("cannot create the event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let audio = match AudioOutput::new(AUDIO_LATENCY_MS) {
        Ok(a) => Some(a),
        Err(e) => {
            tracing::warn!("running without audio: {e}");
            None
        }
    };
    let strings = Strings::load(&ctx.config.language);
    let menu = Menu::build(&ctx.library, &strings, &ctx.paths.roms);
    let view = MenuView::new(&menu);
    let mut app = App {
        ctx,
        options,
        strings,
        menu,
        view,
        repeater: Repeater::new(),
        last_tick: Instant::now(),
        mode: game.map_or(Mode::Menu, |g| Mode::Game(Box::new(g))),
        input: Input::new(),
        audio,
        running: None,
        presented: 0,
        error: None,
    };
    event_loop.run_app(&mut app).context("event loop failed")?;
    if let Mode::Game(game) = std::mem::replace(&mut app.mode, Mode::Menu) {
        let underruns = app.audio.as_ref().map_or(0, AudioOutput::underruns);
        game.finish(underruns);
    }
    app.error.map_or(Ok(()), Err)
}

fn display_hz(window: &Window) -> Option<f64> {
    window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz())
        .map(|mhz| f64::from(mhz) / 1000.0)
}

impl App {
    fn start(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let size = match &self.mode {
            Mode::Game(game) => {
                let av = game.core.av_info();
                let scale = 3.0;
                LogicalSize::new(
                    f64::from(av.base_height) * f64::from(av.aspect_ratio) * scale,
                    f64::from(av.base_height) * scale,
                )
            }
            Mode::Menu => LogicalSize::new(1280.0, 800.0),
        };
        let mut attributes = Window::default_attributes()
            .with_title(self.title())
            .with_inner_size(size);
        if self.options.fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .context("cannot create the window")?,
        );
        let renderer = Renderer::new(event_loop.owned_display_handle(), window.clone())?;
        let ui = UiRenderer::new(&renderer.gpu());
        if let Mode::Game(game) = &mut self.mode {
            Self::sync_game_timing(game, &window, self.audio.as_ref());
        }
        self.running = Some(Running {
            window,
            renderer,
            ui,
        });
        Ok(())
    }

    fn title(&self) -> String {
        match &self.mode {
            Mode::Game(game) => format!("Deckard — {}", game.title),
            Mode::Menu => "Deckard".into(),
        }
    }

    /// Points the game's pacer and the core at the display refresh rate.
    fn sync_game_timing(game: &mut Game, window: &Window, audio: Option<&AudioOutput>) {
        let hz = display_hz(window);
        if let Some(hz) = hz {
            game.core.set_target_refresh_rate(hz as f32);
        }
        game.pacer.update(game.core.av_info().fps, hz);
        game.underruns_at_start = audio.map_or(0, AudioOutput::underruns);
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn hints(&self) -> Hints {
        Hints {
            confirm: self.strings.hint_confirm.clone(),
            back: self.strings.hint_back.clone(),
        }
    }

    /// One iteration of the main loop.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let ports = self.input.poll();
        let hotkeys = self.input.take_hotkeys();
        let presented = match &mut self.mode {
            Mode::Game(_) => self.game_frame(event_loop, ports, hotkeys),
            Mode::Menu => self.menu_frame(event_loop),
        };
        if presented == Presented::No {
            // No vsync to wait on (hidden window): don't spin.
            std::thread::sleep(Duration::from_millis(16));
        } else {
            self.presented += 1;
        }
        let frames = match &self.mode {
            Mode::Game(game) => game.frames,
            Mode::Menu => self.presented,
        };
        if self.options.max_frames.is_some_and(|max| frames >= max) {
            event_loop.exit();
        }
    }

    fn menu_frame(&mut self, event_loop: &ActiveEventLoop) -> Presented {
        let now = Instant::now();
        let dt = now - self.last_tick;
        self.last_tick = now;

        let buttons = self.input.menu_buttons();
        let held = Held {
            up: buttons.up,
            down: buttons.down,
            left: buttons.left,
            right: buttons.right,
            confirm: buttons.confirm,
            back: buttons.back,
        };
        for nav in self.repeater.update(held, now) {
            if let Some(action) = self.menu.handle(nav) {
                self.perform(action, event_loop);
                if !matches!(self.mode, Mode::Menu) {
                    return Presented::Yes;
                }
            }
        }

        self.view.update(&self.menu, dt);
        let hints = self.hints();
        let Some(running) = self.running.as_mut() else {
            return Presented::No;
        };
        let gpu = running.renderer.gpu();
        let scene = layout(
            &self.menu,
            &self.view,
            &hints,
            gpu.width as f32,
            gpu.height as f32,
        );
        running.ui.draw_background = true;
        running.ui.set_scene(scene);
        running.renderer.render(None, Some(&mut running.ui))
    }

    fn perform(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        match action {
            Action::Quit => event_loop.exit(),
            Action::Notice(message) => self.view.show_toast(message),
            Action::Rescan => {
                self.ctx.library = Library::scan(&self.ctx.config, &self.ctx.paths);
                self.menu.replace(Menu::build(
                    &self.ctx.library,
                    &self.strings,
                    &self.ctx.paths.roms,
                ));
                let count: usize = self.ctx.library.systems.iter().map(|s| s.games.len()).sum();
                let msg = fill(
                    &self.strings.library_updated,
                    &[("count", &count.to_string())],
                );
                self.view.show_toast(msg);
            }
            Action::Launch { system, game } => self.launch(system, game),
        }
    }

    fn launch(&mut self, system: usize, game: usize) {
        let Some(entry) = self.ctx.library.systems.get(system) else {
            return;
        };
        let Some(item) = entry.games.get(game) else {
            return;
        };
        let request = LaunchRequest {
            core_path: entry.core_path.clone(),
            rom: Some(item.path.clone()),
            system_id: Some(entry.id.clone()),
            cli_options: Vec::new(),
        };
        tracing::info!(game = item.title, system = entry.id, "launching");
        match Game::start(request, &self.ctx.config, &self.ctx.paths) {
            Ok(mut game) => {
                if let Some(running) = &self.running {
                    Self::sync_game_timing(&mut game, &running.window, self.audio.as_ref());
                }
                self.mode = Mode::Game(Box::new(game));
                if let Some(running) = &self.running {
                    running.window.set_title(&self.title());
                }
            }
            Err(e) => {
                tracing::error!("cannot start the game: {e:#}");
                let msg = fill(&self.strings.launch_failed, &[("error", &format!("{e}"))]);
                self.view.show_toast(msg);
            }
        }
    }

    /// Closes the game and goes back to the menu (or quits).
    fn leave_game(&mut self, event_loop: &ActiveEventLoop) {
        if self.options.quit_after_game {
            event_loop.exit();
            return;
        }
        if let Mode::Game(game) = std::mem::replace(&mut self.mode, Mode::Menu) {
            let underruns = self.audio.as_ref().map_or(0, AudioOutput::underruns);
            game.finish(underruns);
        }
        let title = self.title();
        if let Some(running) = &mut self.running {
            running.renderer.clear_frame();
            running.window.set_title(&title);
        }
        // The button that closed the game is still held: don't let it act in the menu.
        let b = self.input.menu_buttons();
        self.repeater.reset(Held {
            up: b.up,
            down: b.down,
            left: b.left,
            right: b.right,
            confirm: b.confirm,
            back: b.back,
        });
        self.last_tick = Instant::now();
    }

    fn game_frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        ports: [deckard_core::PadState; deckard_core::MAX_PORTS],
        hotkeys: Hotkeys,
    ) -> Presented {
        if hotkeys.quit || hotkeys.menu {
            self.leave_game(event_loop);
            return Presented::Yes;
        }
        let Mode::Game(game) = &mut self.mode else {
            return Presented::No;
        };
        for (port, pad) in ports.into_iter().enumerate() {
            game.core.set_pad(port, pad);
        }
        handle_state_hotkeys(game, hotkeys);

        let due = game.pacer.frames_due(Instant::now());
        for _ in 0..due {
            game.core.run_frame();
            game.frames += 1;
            game.started.get_or_insert_with(Instant::now);
            if let Some(audio) = self.audio.as_mut() {
                let rate = game
                    .pacer
                    .audio_source_rate(game.core.av_info().sample_rate);
                audio.push(game.core.audio(), rate);
            }
        }
        if game.core.take_av_changed() {
            let hz = self.running.as_ref().and_then(|r| display_hz(&r.window));
            game.pacer.update(game.core.av_info().fps, hz);
        }
        let shutdown = game.core.shutdown_requested();
        let aspect = game.core.av_info().aspect_ratio;

        let presented = match self.running.as_mut() {
            Some(running) => {
                if due > 0
                    && let Some(frame) = game.core.frame()
                {
                    running.renderer.upload(&frame);
                }
                running.renderer.render(Some(aspect), None)
            }
            None => Presented::No,
        };
        if shutdown {
            self.leave_game(event_loop);
        }
        presented
    }
}

/// Save state keys. Failures are logged, never fatal: the game keeps running.
fn handle_state_hotkeys(game: &mut Game, hotkeys: Hotkeys) {
    if hotkeys.next_slot {
        tracing::info!(slot = game.slots.select_next(), "state slot");
    }
    if hotkeys.previous_slot {
        tracing::info!(slot = game.slots.select_previous(), "state slot");
    }
    let path = game.state_path(game.slots.current());
    if hotkeys.save_state
        && let Err(e) = game.core.save_state_to(&path)
    {
        tracing::error!("cannot save state: {e}");
    }
    if hotkeys.load_state
        && let Err(e) = game.core.load_state_from(&path)
    {
        tracing::error!("cannot load state: {e}");
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
                if let Some(code) = key_code(event.physical_key, &event.logical_key)
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
