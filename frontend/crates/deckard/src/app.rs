//! Window, event loop and the two modes: the menu and a running game
//! (with its quick menu).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use deckard_audio::AudioOutput;
use deckard_config::{Config, Paths};
use deckard_input::{Hotkeys, Input, MenuButtons, key_code};
use deckard_library::Library;
use deckard_system::SessionTarget;
use deckard_system::installer::{self as system_installer, InstallJob};
use deckard_system::wifi::{self as system_wifi, WifiWorker};
use deckard_ui::UiRenderer;
use deckard_ui::i18n::{Strings, fill, next_language};
use deckard_ui::installer::{
    DiskEntry, Installer, InstallerAction, InstallerEvent, installer_layer,
};
use deckard_ui::layout::{Hints, MenuView, QuickHints, Toast, game_overlay, layout};
use deckard_ui::nav::{Action, Menu, Setting, SettingsView};
use deckard_ui::quick::{QuickAction, QuickMenu};
use deckard_ui::repeat::{Held, Repeater};
use deckard_ui::wifi::{WifiAction, WifiNetwork, WifiPage, WifiUpdate, wifi_layers};
use deckard_video::{Presented, Renderer, Scaling};
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
    /// Where `config` is saved when a setting changes in the menu.
    pub config_path: PathBuf,
    pub paths: Paths,
    pub library: Library,
}

impl Context {
    fn settings_view(&self) -> SettingsView {
        SettingsView {
            integer_scale: self.config.video.integer_scale,
            smooth: self.config.video.smooth,
            shader: self.config.video.shader.as_str().into(),
            volume: self.config.audio.volume,
            session_switching: deckard_system::session_switching_available(),
            installer_available: system_installer::is_live_system(),
            wifi_available: system_wifi::available(),
        }
    }

    fn scaling(&self) -> Scaling {
        let shader = match self.config.video.shader {
            deckard_config::ShaderMode::None => deckard_video::ShaderMode::None,
            deckard_config::ShaderMode::Scanlines => deckard_video::ShaderMode::Scanlines,
            deckard_config::ShaderMode::Crt => deckard_video::ShaderMode::Crt,
        };
        Scaling {
            integer: self.config.video.integer_scale,
            smooth: self.config.video.smooth,
            shader,
        }
    }
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
    /// Quick menu over the running game (the game is paused while it is open).
    quick: Option<QuickMenu>,
    /// Short message over the running game.
    game_toast: Option<Toast>,
    /// "Install to disk" dialog over the menu, and the running installation.
    installer: Option<Installer>,
    install_job: Option<InstallJob>,
    /// Wi-Fi page over the menu, and the thread that talks to NetworkManager.
    wifi: Option<WifiPage>,
    wifi_worker: Option<WifiWorker>,
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
        Ok(mut a) => {
            a.set_volume(ctx.config.audio.volume as f32 / 100.0);
            Some(a)
        }
        Err(e) => {
            tracing::warn!("running without audio: {e}");
            None
        }
    };
    let strings = Strings::load(&ctx.config.language);
    let menu = Menu::build(
        &ctx.library,
        &strings,
        &ctx.paths.roms,
        &ctx.settings_view(),
    );
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
        quick: None,
        game_toast: None,
        installer: None,
        install_job: None,
        wifi: None,
        wifi_worker: None,
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

fn held(b: MenuButtons) -> Held {
    Held {
        up: b.up,
        down: b.down,
        left: b.left,
        right: b.right,
        confirm: b.confirm,
        back: b.back,
    }
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
        let mut renderer = Renderer::new(event_loop.owned_display_handle(), window.clone())?;
        renderer.set_scaling(self.ctx.scaling());
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
        let now = Instant::now();
        let dt = now - self.last_tick;
        self.last_tick = now;

        let ports = self.input.poll();
        let hotkeys = self.input.take_hotkeys();
        let presented = match self.mode {
            Mode::Game(_) => self.game_frame(event_loop, ports, hotkeys, dt),
            Mode::Menu => self.menu_frame(event_loop, dt),
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

    // ------------------------------------------------------------------ menu

    fn menu_frame(&mut self, event_loop: &ActiveEventLoop, dt: Duration) -> Presented {
        let buttons = self.input.menu_buttons();
        for nav in self.repeater.update(held(buttons), Instant::now()) {
            // The installer dialog, when open, takes all input.
            if let Some(installer) = self.installer.as_mut() {
                if let Some(action) = installer.handle(nav) {
                    self.perform_installer(action);
                }
                continue;
            }
            if let Some(page) = self.wifi.as_mut() {
                if let Some(action) = page.handle(nav, &self.strings) {
                    self.perform_wifi(action);
                }
                continue;
            }
            if let Some(action) = self.menu.handle(nav) {
                self.perform(action, event_loop);
                if !matches!(self.mode, Mode::Menu) {
                    return Presented::Yes;
                }
            }
        }
        self.poll_installer();
        self.poll_wifi();

        self.view.update(&self.menu, dt);
        let hints = self.hints();
        let Some(running) = self.running.as_mut() else {
            return Presented::No;
        };
        let gpu = running.renderer.gpu();
        let mut scene = layout(
            &self.menu,
            &self.view,
            &hints,
            gpu.width as f32,
            gpu.height as f32,
        );
        if let Some(page) = &self.wifi {
            scene.layers.extend(wifi_layers(
                page,
                &self.strings,
                gpu.width as f32,
                gpu.height as f32,
            ));
        }
        if let Some(installer) = &self.installer {
            scene.layers.push(installer_layer(
                installer,
                &self.strings,
                gpu.width as f32,
                gpu.height as f32,
            ));
        }
        running.ui.draw_background = true;
        running.ui.set_scene(scene);
        running.renderer.render(None, Some(&mut running.ui))
    }

    /// Opens the "Install to disk" dialog with the disks the installer accepts.
    fn open_installer(&mut self) {
        let disks = match system_installer::list_disks() {
            Ok(disks) => disks,
            Err(e) => {
                tracing::error!("cannot list disks: {e}");
                self.view.show_toast(e.to_string());
                return;
            }
        };
        let entries = disks
            .into_iter()
            .map(|d| DiskEntry {
                path: d.path,
                bytes: d.bytes,
                removable: d.removable,
                model: d.model,
            })
            .collect();
        self.installer = Some(Installer::new(entries));
    }

    fn perform_installer(&mut self, action: InstallerAction) {
        match action {
            InstallerAction::Close => {
                self.installer = None;
                self.install_job = None;
            }
            InstallerAction::Start(disk) => match InstallJob::start(&disk) {
                Ok(job) => self.install_job = Some(job),
                Err(e) => {
                    if let Some(installer) = &mut self.installer {
                        installer.update(InstallerEvent::Error(e.to_string()));
                    }
                }
            },
            InstallerAction::Reboot => {
                tracing::info!("rebooting after installation");
                if let Err(e) = std::process::Command::new("systemctl")
                    .arg("reboot")
                    .spawn()
                {
                    tracing::error!("cannot reboot: {e}");
                    self.view.show_toast(e.to_string());
                }
            }
        }
    }

    /// Opens the Wi-Fi page and starts a scan.
    fn open_wifi(&mut self) {
        let worker = self.wifi_worker.get_or_insert_with(WifiWorker::start);
        worker.request(system_wifi::Request::Scan);
        self.wifi = Some(WifiPage::new());
    }

    fn perform_wifi(&mut self, action: WifiAction) {
        let request = match action {
            WifiAction::Close => {
                self.wifi = None;
                return;
            }
            WifiAction::Scan => system_wifi::Request::Scan,
            WifiAction::Connect { ssid, password } => {
                tracing::info!(ssid, "connecting to Wi-Fi");
                system_wifi::Request::Connect { ssid, password }
            }
            WifiAction::Disconnect => system_wifi::Request::Disconnect,
        };
        if let Some(worker) = &self.wifi_worker {
            worker.request(request);
        }
    }

    /// Forwards Wi-Fi results to the page (and keeps the list fresh).
    fn poll_wifi(&mut self) {
        let Some(worker) = &self.wifi_worker else {
            return;
        };
        let replies = worker.poll();
        for reply in replies {
            let update = match reply {
                system_wifi::Reply::Networks(networks) => WifiUpdate::Networks(
                    networks
                        .into_iter()
                        .map(|n| WifiNetwork {
                            ssid: n.ssid,
                            signal: n.signal,
                            secured: n.secured,
                            active: n.active,
                        })
                        .collect(),
                ),
                system_wifi::Reply::Connected(ssid) => {
                    self.view
                        .show_toast(fill(&self.strings.wifi_connected, &[("ssid", &ssid)]));
                    WifiUpdate::Connected(ssid)
                }
                system_wifi::Reply::Disconnected => WifiUpdate::Disconnected,
                system_wifi::Reply::Error(error) => {
                    tracing::warn!("Wi-Fi: {error}");
                    WifiUpdate::Error(error)
                }
            };
            let follow_up = self
                .wifi
                .as_mut()
                .and_then(|page| page.update(update, &self.strings));
            if let Some(action) = follow_up {
                self.perform_wifi(action);
            }
        }
    }

    /// Physical keyboard while the on-screen keyboard is open: type into the
    /// password field instead of driving the menu. Returns true if consumed.
    fn wifi_keyboard_input(&mut self, event: &winit::event::KeyEvent) -> bool {
        let Some(page) = self.wifi.as_mut().filter(|p| p.keyboard_open()) else {
            return false;
        };
        if event.state != ElementState::Pressed {
            // Releases always reach the normal input, so no key stays "held".
            return false;
        }
        use winit::keyboard::{Key, NamedKey};
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => {
                if let Some(action) = page.submit() {
                    self.perform_wifi(action);
                }
            }
            Key::Named(NamedKey::Escape) => page.cancel_keyboard(),
            Key::Named(NamedKey::Backspace) => page.backspace(),
            // Arrows still move on the on-screen keyboard.
            Key::Named(
                NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight,
            ) => return false,
            _ => {
                if let Some(text) = &event.text {
                    for c in text.chars() {
                        page.type_char(c);
                    }
                }
            }
        }
        true
    }

    /// Forwards the running installation's progress to the dialog.
    fn poll_installer(&mut self) {
        let (Some(job), Some(installer)) = (self.install_job.as_mut(), self.installer.as_mut())
        else {
            return;
        };
        for event in job.poll() {
            installer.update(match event {
                system_installer::Event::Progress(percent, step) => {
                    InstallerEvent::Progress(percent, step)
                }
                system_installer::Event::Done => InstallerEvent::Done,
                system_installer::Event::Error(message) => InstallerEvent::Error(message),
            });
        }
    }

    fn perform(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        match action {
            Action::Quit => event_loop.exit(),
            Action::SwitchSession => self.switch_session(event_loop),
            Action::OpenInstaller => self.open_installer(),
            Action::OpenWifi => self.open_wifi(),
            Action::Notice(message) => self.view.show_toast(message),
            Action::Rescan => {
                self.ctx.library = Library::scan(&self.ctx.config, &self.ctx.paths);
                self.rebuild_menu();
                let count: usize = self.ctx.library.systems.iter().map(|s| s.games.len()).sum();
                let msg = fill(
                    &self.strings.library_updated,
                    &[("count", &count.to_string())],
                );
                self.view.show_toast(msg);
            }
            Action::Toggle(setting) => self.toggle(setting),
            Action::Launch { system, game } => self.launch(system, game),
        }
    }

    fn rebuild_menu(&mut self) {
        let menu = Menu::build(
            &self.ctx.library,
            &self.strings,
            &self.ctx.paths.roms,
            &self.ctx.settings_view(),
        );
        self.menu.replace(menu);
    }

    /// Changes a setting, applies it and saves the configuration.
    fn toggle(&mut self, setting: Setting) {
        match setting {
            Setting::IntegerScale => {
                self.ctx.config.video.integer_scale = !self.ctx.config.video.integer_scale
            }
            Setting::Smooth => self.ctx.config.video.smooth = !self.ctx.config.video.smooth,
            Setting::Shader => self.ctx.config.video.shader = self.ctx.config.video.shader.next(),
            Setting::Volume => {
                let next_vol = deckard_config::next_volume_step(self.ctx.config.audio.volume);
                self.ctx.config.audio.volume = next_vol;
                if let Some(audio) = &mut self.audio {
                    audio.set_volume(next_vol as f32 / 100.0);
                }
            }
            Setting::Language => {
                self.ctx.config.language = next_language(&self.ctx.config.language).to_owned();
                self.strings = Strings::load(&self.ctx.config.language);
            }
        }
        if let Some(running) = &mut self.running {
            running.renderer.set_scaling(self.ctx.scaling());
        }
        self.rebuild_menu();
        if let Err(e) = self.ctx.config.save(&self.ctx.config_path) {
            tracing::error!("cannot save the configuration: {e}");
            self.view.show_toast(e.to_string());
        }
    }

    /// Writes the `next-session` file to request Steam and exits.
    fn switch_session(&mut self, event_loop: &ActiveEventLoop) {
        let state_dir = deckard_system::state_dir();
        if let Err(e) = deckard_system::request_session_switch(SessionTarget::Steam, &state_dir) {
            tracing::error!("cannot request session switch: {e}");
            self.view.show_toast(e.to_string());
            return;
        }
        event_loop.exit();
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
        self.close_quick_menu();
        self.game_toast = None;
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
        let buttons = self.input.menu_buttons();
        self.repeater.reset(held(buttons));
    }

    // ------------------------------------------------------------------ game

    fn open_quick_menu(&mut self) {
        let Mode::Game(game) = &mut self.mode else {
            return;
        };
        let disc = game.core.disk_info().and_then(|d| {
            if d.count > 1 {
                Some(deckard_ui::quick::DiscState {
                    current: d.current,
                    count: d.count,
                    label: d.label,
                })
            } else {
                None
            }
        });
        self.quick = Some(QuickMenu::new(
            game.title.clone(),
            game.slots.current(),
            self.ctx.config.video.shader.as_str(),
            self.ctx.config.audio.volume,
            disc,
            game.option_entries(),
        ));
        if let Some(audio) = &mut self.audio {
            audio.set_paused(true);
        }
        let buttons = self.input.menu_buttons();
        self.repeater.reset(held(buttons));
    }

    fn close_quick_menu(&mut self) {
        if self.quick.take().is_some()
            && let Some(audio) = &mut self.audio
        {
            audio.set_paused(false);
        }
    }

    fn game_toast(&mut self, text: String) {
        self.game_toast = Some(Toast::new(text));
    }

    fn save_state(&mut self) {
        let Mode::Game(game) = &mut self.mode else {
            return;
        };
        let slot = game.slots.current();
        let msg = match game.core.save_state_to(&game.state_path(slot)) {
            Ok(()) => fill(&self.strings.state_saved, &[("slot", &slot.to_string())]),
            Err(e) => {
                tracing::error!("cannot save state: {e}");
                fill(&self.strings.state_error, &[("error", &e.to_string())])
            }
        };
        self.game_toast(msg);
    }

    fn load_state(&mut self) {
        let Mode::Game(game) = &mut self.mode else {
            return;
        };
        let slot = game.slots.current();
        let msg = match game.core.load_state_from(&game.state_path(slot)) {
            Ok(()) => fill(&self.strings.state_loaded, &[("slot", &slot.to_string())]),
            Err(e) => {
                tracing::error!("cannot load state: {e}");
                fill(&self.strings.state_error, &[("error", &e.to_string())])
            }
        };
        self.game_toast(msg);
    }

    fn change_slot(&mut self, forward: bool) {
        let Mode::Game(game) = &mut self.mode else {
            return;
        };
        let slot = if forward {
            game.slots.select_next()
        } else {
            game.slots.select_previous()
        };
        if let Some(quick) = &mut self.quick {
            quick.set_slot(slot);
        } else {
            let msg = fill(&self.strings.state_slot, &[("slot", &slot.to_string())]);
            self.game_toast(msg);
        }
    }

    fn change_disc(&mut self, forward: bool) {
        let Mode::Game(game) = &mut self.mode else {
            return;
        };
        let Some(info) = game.core.disk_info() else {
            return;
        };
        if info.count <= 1 {
            return;
        }
        let target = if forward {
            (info.current + 1) % info.count
        } else {
            (info.current + info.count - 1) % info.count
        };
        if game.core.change_disk(target) {
            if let Some(new_info) = game.core.disk_info() {
                let label = new_info.label.clone().unwrap_or_else(|| {
                    fill(
                        &self.strings.disc_count,
                        &[
                            ("current", &(new_info.current + 1).to_string()),
                            ("count", &new_info.count.to_string()),
                        ],
                    )
                });
                let toast_msg = fill(&self.strings.disc_inserted, &[("disc", &label)]);
                self.game_toast(toast_msg);
                if let Some(quick) = &mut self.quick {
                    quick.set_disc(Some(deckard_ui::quick::DiscState {
                        current: new_info.current,
                        count: new_info.count,
                        label: new_info.label,
                    }));
                }
            }
        } else {
            tracing::error!(target, "core failed to change disk");
        }
    }

    fn perform_quick(&mut self, action: QuickAction, event_loop: &ActiveEventLoop) {
        match action {
            QuickAction::Resume => self.close_quick_menu(),
            QuickAction::SaveState => self.save_state(),
            QuickAction::LoadState => {
                self.load_state();
                self.close_quick_menu();
            }
            QuickAction::NextSlot => self.change_slot(true),
            QuickAction::PreviousSlot => self.change_slot(false),
            QuickAction::NextDisc => self.change_disc(true),
            QuickAction::PreviousDisc => self.change_disc(false),
            QuickAction::CycleShader => {
                self.ctx.config.video.shader = self.ctx.config.video.shader.next();
                if let Some(running) = &mut self.running {
                    running.renderer.set_scaling(self.ctx.scaling());
                }
                if let Some(quick) = &mut self.quick {
                    quick.set_shader(self.ctx.config.video.shader.as_str());
                }
                if let Err(e) = self.ctx.config.save(&self.ctx.config_path) {
                    tracing::error!("cannot save the configuration: {e}");
                }
            }
            QuickAction::ChangeVolume(delta) => {
                let current = self.ctx.config.audio.volume as i16;
                let new_vol = (current + delta as i16).clamp(0, 100) as u8;
                self.ctx.config.audio.volume = new_vol;
                if let Some(audio) = &mut self.audio {
                    audio.set_volume(new_vol as f32 / 100.0);
                }
                if let Some(quick) = &mut self.quick {
                    quick.set_volume(new_vol);
                }
                if let Err(e) = self.ctx.config.save(&self.ctx.config_path) {
                    tracing::error!("cannot save the configuration: {e}");
                }
            }
            QuickAction::Reset => {
                if let Mode::Game(game) = &mut self.mode {
                    game.core.reset();
                }
                self.close_quick_menu();
            }
            QuickAction::CloseGame => self.leave_game(event_loop),
            QuickAction::SetOption { key, value } => {
                if let Mode::Game(game) = &mut self.mode
                    && !game.set_option(&key, &value)
                {
                    tracing::warn!(key, value, "core rejected option value");
                }
            }
        }
    }

    fn game_frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        ports: [deckard_core::PadState; deckard_core::MAX_PORTS],
        hotkeys: Hotkeys,
        dt: Duration,
    ) -> Presented {
        if hotkeys.menu {
            if self.quick.is_some() {
                self.close_quick_menu();
            } else {
                self.open_quick_menu();
            }
        } else if hotkeys.quit && self.quick.is_none() {
            // Esc closes the game; inside the quick menu it means "back".
            self.leave_game(event_loop);
            return Presented::Yes;
        }

        if self.quick.is_some() {
            let buttons = self.input.menu_buttons();
            for nav in self.repeater.update(held(buttons), Instant::now()) {
                let action = self.quick.as_mut().and_then(|q| q.handle(nav));
                if let Some(action) = action {
                    self.perform_quick(action, event_loop);
                }
                if !matches!(self.mode, Mode::Game(_)) {
                    return Presented::Yes;
                }
            }
        } else {
            if hotkeys.next_slot {
                self.change_slot(true);
            }
            if hotkeys.previous_slot {
                self.change_slot(false);
            }
            if hotkeys.save_state {
                self.save_state();
            }
            if hotkeys.load_state {
                self.load_state();
            }
        }
        if self.game_toast.as_mut().is_some_and(|t| !t.update(dt)) {
            self.game_toast = None;
        }

        let Mode::Game(game) = &mut self.mode else {
            return Presented::No;
        };
        let paused = self.quick.is_some();
        let mut due = 0;
        if !paused {
            for (port, pad) in ports.into_iter().enumerate() {
                game.core.set_pad(port, pad);
            }
            due = game.pacer.frames_due(Instant::now());
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
        } else {
            // Don't count the paused time as emulation time when resuming.
            game.pacer.frames_due(Instant::now());
            if game.started.is_some() {
                game.paused += dt;
            }
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
                let gpu = running.renderer.gpu();
                let hints = QuickHints {
                    confirm: self.strings.hint_confirm.clone(),
                    change: self.strings.hint_change.clone(),
                    back: self.strings.hint_back.clone(),
                };
                let quick = self.quick.as_ref().map(|q| (q, &self.strings));
                let scene = game_overlay(
                    quick,
                    self.game_toast.as_ref(),
                    &hints,
                    gpu.width as f32,
                    gpu.height as f32,
                );
                if quick.is_some() || self.game_toast.is_some() {
                    running.ui.draw_background = false;
                    running.ui.set_scene(scene);
                    running.renderer.render(Some(aspect), Some(&mut running.ui))
                } else {
                    running.renderer.render(Some(aspect), None)
                }
            }
            None => Presented::No,
        };
        if shutdown {
            self.leave_game(event_loop);
        }
        presented
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
                if self.wifi_keyboard_input(&event) {
                    return;
                }
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
