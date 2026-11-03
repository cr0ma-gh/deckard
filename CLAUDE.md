# CLAUDE.md — Deckard

> Istruzioni per agenti IA (Claude Code) che lavorano su questo repository.
> **GEMINI.md contiene le stesse regole condivise**: se modifichi una sezione
> marcata `[CONDIVISO]`, applica la stessa modifica anche in GEMINI.md.

---

## 1. Cos'è Deckard `[CONDIVISO]`

Deckard è una distribuzione Linux specializzata per **Steam Deck LCD (Jupiter)**,
basata su **Arch Linux** e costruita con **archiso (`mkarchiso`)**.

Integra un **frontend libretro nativo scritto in Rust**, con interfaccia a menu
orizzontale in stile XMB (simile a Lakka/RetroArch), che carica direttamente i
core libretro (`.so`) senza passare da RetroArch.

Funziona in **dual mode**:
- **Modalità Deckard** (predefinita all'avvio): il frontend Rust a schermo intero.
- **Modalità Steam**: sessione Steam Big Picture sotto gamescope.
Dal menu di ciascuna modalità si passa all'altra (come "Passa al desktop" su SteamOS).

## 2. Decisioni prese `[CONDIVISO]`

| Tema | Decisione | Motivo |
|---|---|---|
| Base OS | Arch Linux + archiso | Leggera, stessi pacchetti/driver usati da SteamOS, build riproducibile, nessun rootfs immutabile |
| Hardware | Solo Steam Deck **LCD** (Jupiter) | Un solo target da testare. **Non** aggiungere codice per OLED (Galileo) senza richiesta esplicita |
| Frontend | Rust nativo, API libretro via FFI | Massimo controllo su latenza, UI e integrazione |
| Grafica | `wgpu` (backend Vulkan) + `winit` | Rust puro, Vulkan su RADV |
| Steam | Dual mode con Steam (gamescope) | L'utente vuole entrambe le esperienze |
| Lingua | **Documentazione in italiano, codice e commenti in inglese** | Vedi §8 |
| Licenza | GPL-3.0-or-later | Coerente con l'ecosistema libretro/Lakka |
| Nome | Deckard — binario `deckard`, immagine `deckard-os` | |

Per cambiare una di queste decisioni serve l'approvazione esplicita dell'utente.
Registra ogni nuova decisione architetturale in `docs/decisions/` (formato ADR breve).

## 3. Struttura del repository (prevista) `[CONDIVISO]`

Esistono già (Fasi 0–4): tutti i crate (incluso `deckard-system`), `os/`,
`packages/deckard/`, `session/` (`deckard-session`, `switch-to-deckard.sh`),
`scripts/` (build ISO, QEMU), la CI (`.github/workflows/ci.yml`: Rust + shellcheck;
`iso.yml`: immagine) e `docs/decisions/`. `docs/hardware.md` è previsto: crealo
nella fase in cui serve, non prima. Aggiorna questa sezione quando la
struttura cambia.

Il repository è pubblico: **github.com/cr0ma-gh/deckard** (remote `origin`).
I commit usano l'identità `cr0ma-gh` con l'email noreply di GitHub, configurata
nel repository: non usare altre email.

```
Deckard/
├── CLAUDE.md / GEMINI.md      # istruzioni per agenti IA (tenere sincronizzate)
├── README.md                  # panoramica per umani (italiano)
├── LICENSE                    # GPL-3.0
├── frontend/                  # Cargo workspace del frontend Rust
│   ├── Cargo.toml
│   └── crates/
│       ├── libretro-sys/      # binding FFI di libretro.h (unico posto con tipi C grezzi)
│       ├── deckard-core/      # caricamento core, callback, run loop, SRAM, save state
│       ├── deckard-video/     # wgpu: upload framebuffer, scaling, shader, presentazione
│       ├── deckard-audio/     # output audio (cpal), resampling, controllo dinamico del rate
│       ├── deckard-input/     # gamepad (gilrs) + tastiera → RetroPad, hotkey
│       ├── deckard-test-core/ # core libretro minimale in Rust, usato solo nei test
│       ├── deckard-ui/        # menu XMB: nav, layout, traduzioni, renderer (assets/: font, i18n)
│       ├── deckard-library/   # scansione roms/<sistema>/, titoli, copertine, nomi dei save state
│       ├── deckard-config/    # config.toml (cartelle, sistemi, video) e opzioni salvate dei core
│       ├── deckard-system/    # integrazione OS: batteria, luminosità, Wi-Fi, cambio sessione
│       └── deckard/           # binario principale
├── os/
│   ├── README.md              # come si costruisce e prova l'immagine
│   └── archiso/               # aggiunte al profilo releng: packages.x86_64,
│                              #   profiledef.overrides.sh, airootfs/ (greetd, utente, sudo)
├── packages/deckard/          # PKGBUILD che compila Deckard da questo checkout
├── installer/                 # deckard-install: installazione su disco intero
├── session/                   # deckard-session (gamescope/cage); cambio sessione in Fase 4
├── scripts/                   # build-iso.sh, run-qemu.sh, boot-test.sh, install-test.sh, lib/
└── docs/
    ├── decisions/             # ADR
    └── hardware.md            # note su Steam Deck LCD
```

## 4. Architettura del frontend Rust `[CONDIVISO]`

### 4.1 Flusso principale
1. `deckard` carica `config.toml` (`deckard-config`), risolve le cartelle,
   scansiona la libreria (`deckard-library`) e mostra il menu XMB
   (`deckard-ui`). Con `--rom`/`--core` salta il menu e avvia subito il gioco,
   uscendo quando il gioco finisce.
2. Dal menu il gioco porta con sé sistema e core; da riga di comando il sistema
   si ricava dalla cartella sotto `roms/`, poi dall'estensione
   (`Config::system_for`), e da lì il core (`snes9x` → `<cores>/snes9x_libretro.so`).
   L'avvio passa da `deckard/src/launcher.rs` (`Game::start` / `Game::finish`).
3. `deckard-core` (`Core::load`) carica il core con `libloading`, verifica
   `retro_api_version() == 1`, registra le callback, chiama `retro_init`;
   `Core::load_game` chiama `retro_load_game` e carica la SRAM.
4. Ciclo principale (`deckard/src/app.rs`, thread principale dentro winit), due
   modalità: **Menu** (input → `Repeater` → `Menu::handle` → `layout` →
   `UiRenderer`) e **Gioco** (input → `Core::run_frame` quante volte dice
   `pacing.rs` → audio a `deckard-audio` → frame a `deckard-video`). Entrambe
   presentano con vsync; il menu è un `Overlay` disegnato nel pass del video.
5. Hotkey in gioco: tasto Guide/Steam o `F1` aprono/chiudono il **menu rapido**
   (il gioco e l'audio vanno in pausa); `Esc` chiude il gioco e torna al menu
   (dentro il menu rapido vale "indietro"); `F2`/`F4` salvano/caricano lo stato,
   `F6`/`F7` cambiano slot (con un messaggio a comparsa).
6. All'uscita la SRAM viene salvata (`Drop` di `Core`) e le opzioni del core
   diverse dal default vanno in `<core-options>/<core>.toml`; i valori passati
   con `-o` valgono solo per quella sessione e non vengono salvati, a meno che
   l'utente li cambi dal menu rapido (`Game::set_option`).

### 4.1b Configurazione e libreria (ADR 0003)
- `config.toml` in `<config dir>/deckard/` (Linux: `~/.config/deckard/`); se
  manca viene creato con i default. I percorsi relativi sono dentro
  `<data dir>/deckard/` (Linux: `~/.local/share/deckard/`): `roms/`, `cores/`,
  `system/`, `saves/`, `states/`, `thumbnails/`, `core-options/`.
- Core: cercati in `cores/` dell'utente e poi, su Linux, in `/usr/lib/libretro`
  (`Paths::core_dirs`, `find_core`).
- Sistemi: tabelle `[[system]]` con `id`, `name`, `core`, `extensions`,
  `folder` opzionale (default = `id`). Se il file ne definisce, sostituiscono i
  default (NES, SNES, GB, GBC, GBA, Mega Drive, PlayStation).
- Giochi: `roms/<cartella>/` fino a 3 livelli di profondità, file nascosti
  esclusi, ordinati per titolo; titolo = nome del file senza estensione.
- Copertine: `thumbnails/<id>/<nome file>.png` oppure
  `thumbnails/<id>/Named_Boxarts/<titolo>.png` (convenzione libretro-thumbnails).
- Save state: `states/<id sistema>/<nome file>.state` (slot 0) o `.stateN`;
  SRAM: `saves/<nome file>.srm`. Scritture atomiche (file temporaneo + rename).
- Lingua dell'interfaccia: `language = "it"` o `"en"` (default `"en"`).
- Le impostazioni cambiate dal menu (scala intera, filtro, lingua) vengono
  salvate subito in `config.toml` riscrivendo il file: i commenti scritti a mano
  non si conservano.

### 4.1c Menu XMB (ADR 0004)
- `deckard-ui` separa la logica dal disegno: `nav.rs` (modello e navigazione,
  pura), `repeat.rs` (tasti tenuti → passi, con ripetizione), `layout.rs`
  (stato + animazioni → `Scene` di rettangoli, testi e immagini; geometria per
  1280×800 scalata sull'altezza), `i18n.rs` (testi), `render/` (wgpu + glyphon).
  Le modifiche al comportamento vanno in `nav`/`layout` e si testano lì; il
  renderer si verifica con screenshot.
- Categorie: Impostazioni (Scala intera, Filtro morbido, Shader, Volume,
  Lingua, Wi-Fi, Ricarica libreria, Esci, Passa a Steam, Installa su disco,
  Riavvia, Spegni: le ultime compaiono solo dove servono) poi un sistema per ogni cartella con giochi; senza giochi
  compare una voce che indica la cartella ROM. Un gioco senza core mostra
  "Core mancante" invece di avviarsi. Ogni voce ha un `id` stabile: la
  selezione resta al suo posto anche quando cambia l'etichetta.
- **Menu rapido** (ADR 0005): modello in `quick.rs` (pagina principale:
  Riprendi, Salva stato, Carica stato, Slot, Opzioni del core, Riavvia, Chiudi
  gioco; pagina delle opzioni del core con valori che si cambiano con
  sinistra/destra), disegno in `layout::game_overlay` come pannello a sinistra
  sopra il gioco in pausa. I messaggi a comparsa (stato salvato/caricato, slot)
  usano `Toast`, anche fuori dal menu rapido.
- Colori: **tutti** in `deckard-ui/src/theme.rs` (tema Steam Deck, ADR 0012:
  blu Steam `#1A9FFF` per l'elemento selezionato, superfici blu-grigio scuro);
  lo sfondo animato in `render/ui.wgsl` usa le stesse tinte. Non scrivere
  colori letterali nei moduli di layout.
- Font: **Inter** (licenza OFL, `deckard-ui/assets/fonts/`, con `OFL.txt`),
  unico font caricato: aspetto identico ovunque. Icone disegnate con forme e
  sigle, nessun asset di terzi.
- Copertine: decodificate in un thread separato (`render/thumbs.rs`), al
  massimo 24 sulla GPU.
- Testi: ogni stringa visibile sta in `deckard-ui/assets/i18n/{en,it}.toml`; un
  test verifica che i file abbiano le stesse chiavi e gli stessi segnaposto.

### 4.2 Regole FFI libretro (importanti)
- Le callback libretro sono `extern "C"` senza puntatore di contesto: usa **un
  contesto thread-local** impostato prima di ogni chiamata al core e rimosso dopo.
  Niente variabili `static mut`.
- Un core va chiamato **sempre dallo stesso thread** (il thread di emulazione).
- **Nessun panic può attraversare il confine FFI**: ogni callback è avvolta in
  `std::panic::catch_unwind`; in caso di errore si registra il problema e si
  restituisce un valore neutro.
- Un solo core caricato per processo alla volta (molti core hanno stato globale).
  Per cambiare core: `retro_unload_game` → `retro_deinit` → drop della libreria.
- `unsafe` è ammesso **solo** in `libretro-sys`, `deckard-core` e
  `deckard-test-core` (più, in futuro, l'interop GPU in `deckard-video`).
  Ogni blocco `unsafe` ha un commento `// SAFETY:`
  (imposto dal lint clippy `undocumented_unsafe_blocks`).
- `libretro-sys` è scritto a mano (ADR 0001): nomi identici all'header C, enum C
  come `c_int` + costanti. **Ogni costante aggiunta va inserita anche in
  `tests/header_consistency.rs`**, che la confronta con `include/libretro.h`.
- La callback di log libretro è variadica: è implementata in
  `deckard-core/src/log_shim.c` (compilato con `cc` in `build.rs`), che formatta
  il messaggio e lo passa a Rust; Rust stable non può definire funzioni variadiche.
- Implementazione: `deckard-core/src/callbacks.rs` (stato condiviso `Shared`,
  puntatore thread-local, tutte le callback ed environment) e `lib.rs` (`Core`,
  che passa lo stato alle callback con `with_core`).

### 4.3 Environment callback
Implementati (Fase 1, in `callbacks.rs::handle_environment`): directory
(system, save, core assets, libretro path), `SET_PIXEL_FORMAT` (0RGB1555,
XRGB8888, RGB565), log, opzioni dei core in **tutte** le versioni
(`SET_VARIABLES`, v1, v1 INTL, v2, v2 INTL, `GET_VARIABLE`,
`GET_VARIABLE_UPDATE`, `SET_VARIABLE`; versione dichiarata: 2), `GET_CAN_DUPE`,
`SET_GEOMETRY`, `SET_SYSTEM_AV_INFO`, `SET_SUPPORT_NO_GAME`, `SHUTDOWN`,
`SET_MESSAGE`, `GET_INPUT_BITMASKS`, `GET_TARGET_REFRESH_RATE`, dischi multipli
(`GET_DISK_CONTROL_INTERFACE_VERSION` = 1, `SET_DISK_CONTROL_INTERFACE` ed
`_EXT_INTERFACE`, `deckard-core/src/disk.rs`; le funzioni dei dischi si
chiamano sempre dentro `with_core`), più alcuni
comandi informativi accettati e ignorati.
Fase 5: `SET_HW_RENDER` con **Vulkan** (`RETRO_HW_CONTEXT_VULKAN`, condividendo il
device di wgpu tramite `wgpu-hal`), poi OpenGL/GLES tramite EGL + import dmabuf.
Comandi non gestiti: restituire `false` e loggare una volta sola a livello `debug`.

### 4.4 Video, audio, sincronizzazione
- Il frame del core viene convertito in BGRA8 su CPU, caricato in una texture e
  disegnato all'aspect ratio del core (letterbox/pillarbox) con uno shader WGSL.
  `Renderer::set_scaling(Scaling { integer, smooth })`: scala intera sui
  multipli dell'altezza del frame (`fit_integer`, ripiega sul normale se non
  entra nemmeno 1×) e filtro nearest o bilineare. Stretch e shader CRT sono
  fasi successive.
- Superficie sRGB: la trasparenza si fonde in spazio lineare, quindi i pannelli
  scuri sembrano più trasparenti del loro alpha; per pannelli leggibili usare
  alpha ≥ 0,95.
- La GPU scelta è quella ad alte prestazioni (portatili ibridi);
  `WGPU_POWER_PREF=high|low` e `WGPU_BACKEND=vulkan|dx12|gl` la cambiano.
- Pannello LCD: nativamente **800×1280 verticale**, usato ruotato a 1280×800,
  refresh 60 Hz (range utile ~40–60 Hz). La rotazione va gestita (gamescope o
  trasformazione in wgpu) — non dare per scontato l'orientamento.
- Sincronizzazione (ADR 0002, `deckard/src/pacing.rs`): modalità **Vsync** (un
  frame del core per refresh, audio scalato) se display e core differiscono al
  massimo del 5%, altrimenti **Timed** (frame schedulati sull'orologio).
- Audio: ricampionatore Hermite proprio + **controllo dinamico del rate**
  (±0,5%) che tiene il ring buffer pieno a metà; buffer pre-riempito di silenzio
  quando è vuoto. A fine sessione il log riporta FPS reali e underrun: devono
  essere ~60 e 0.
- Budget di latenza: input letto subito prima di `retro_run()`; niente buffering
  video oltre il necessario.

### 4.5 Input
- Senza Steam in esecuzione, i controlli del Deck sono esposti dal driver kernel
  `hid-steam` come gamepad evdev. Con Steam attivo, Steam li gestisce.
- Porta 0 = tastiera + primo gamepad uniti; porte 1–3 = gamepad successivi.
  Layout RetroPad stile Nintendo (B = tasto in basso, A = a destra).
- Menu: conferma = tasto in basso (A sul Deck, RetroPad B), indietro = tasto a
  destra; croce, stick sinistro o frecce per muoversi. Tastiera: `Invio`/`Z`
  conferma, `Backspace`/`Esc`/`X` indietro. Le pressioni sono "memorizzate"
  fino al frame successivo: anche un tocco brevissimo conta.
- I tasti di navigazione si leggono dal loro significato (`key_code` in
  `deckard-input`), gli altri dalla posizione fisica: così le frecce del
  tastierino numerico funzionano e i tasti dei giochi non dipendono dal layout.
- Tastiera (sviluppo su desktop): frecce, `X`=A, `Z`=B, `S`=X, `A`=Y, `Q`/`W`=L/R,
  `1`/`2`=L2/R2, `Invio`=Start, `Shift destro`=Select, `F1`=menu rapido,
  `Esc`=chiudi gioco, `F2`/`F4` salva/carica stato, `F6`/`F7` slot
  precedente/successivo (come RetroArch).
- Le mappature configurabili andranno in `deckard-config` (fase successiva); i trackpad
  (mouse/lightgun) sono una fase successiva.

## 5. Sistema operativo (archiso) `[CONDIVISO]`

- **Implementazione (Fase 3, ADR 0006)**: `scripts/build-iso.sh` copia il
  profilo `releng` di archiso e ci applica `os/archiso/` (pacchetti in più,
  nome/permessi, overlay `airootfs/`), compila il pacchetto `deckard` dal
  checkout in un repository pacman locale, abilita i servizi con symlink
  (greetd come display manager, Bluetooth, PipeWire utente) e lancia
  `mkarchiso`. Non mettere symlink nel repository: si creano nello script
  (il repository si modifica anche da Windows).
- Utente `deck` (uid 1000) creato da `airootfs/etc/sysusers.d/deckard.conf`,
  senza password, `sudo` senza password (immagine live).
- greetd `initial_session` → `deckard-session` (`session/`, esporta
  `DECKARD_SESSION=1`), che avvia
  `gamescope -f -- deckard --fullscreen` sull'hardware e `cage -s -- deckard`
  nelle macchine virtuali (`systemd-detect-virt`); log in
  `~/.local/state/deckard/session.log`; override in
  `~/.config/deckard/session.env` (`DECKARD_COMPOSITOR`, `DECKARD_GAMESCOPE_ARGS`).
- Core: pacchetti Arch `libretro-*` in `/usr/lib/libretro`
  (`deckard_config::SYSTEM_CORE_DIR`), cercati dopo `cores/` dell'utente.
  I sistemi predefiniti usano solo core pacchettizzati da Arch.
- **Kernel**: partire con il kernel Arch `linux` mainline + `linux-firmware` +
  `sof-firmware` + `alsa-ucm-conf` (il Deck LCD è supportato upstream). Valutare il
  kernel Valve (`linux-neptune`) solo se emergono problemi hardware concreti, e
  documentarlo in un ADR. **Verifica sempre** nomi di pacchetti e repository
  prima di aggiungerli: non inventare pacchetti.
- Grafica: `mesa`, `vulkan-radeon` (RADV). Audio: PipeWire + WirePlumber.
- Steam: pacchetto `steam` dal repo `multilib` (scarica il client a runtime) +
  `gamescope`. La sessione Steam usa `gamescope` con `steam -gamepadui`.
- Avvio: autologin (es. `greetd`) su un utente non-root `deck` → lancia il
  **session manager** di `session/`.
- **Sessione Steam** (ADR 0012): `gamescope -e -- deckard-steam-session`, che
  riavvia Steam quando si chiude da solo (aggiornamenti) finché non arriva un
  `next-session`; `steamos-session-select` (chiamato da "Passa al desktop" di
  Steam) torna a Deckard. Riavvia/Spegni nelle Impostazioni solo con
  `DECKARD_SESSION` (`deckard-system/src/power.rs`).
- **Cambio sessione**: il frontend scrive la sessione successiva in un file di
  stato (es. `~/.local/state/deckard/next-session`) ed esce; il session manager
  legge il file e avvia l'altra sessione. Lo stesso meccanismo va reso
  disponibile dalla modalità Steam (voce/scorciatoia non-Steam).
- Ventola, TDP, luminosità, batteria: tramite sysfs/strumenti standard. Nessuna
  modifica a firmware/BIOS del Deck.
- Immagine live ISO avviabile da USB e **installer su disco intero** (ADR 0010):
  `installer/deckard-install` (`list`, `install <disco> --yes`, righe
  `PROGRESS`/`DONE`/`ERROR`), voce "Installa su disco" nelle Impostazioni solo
  in live, due conferme obbligatorie prima di cancellare. Non scrivere mai su un
  disco reale durante lo sviluppo: si prova solo in QEMU (`scripts/install-test.sh`).
  Elenco dei file solo-live rimossi in `live_only` dentro lo script: aggiornalo
  se cambia il profilo releng.
- **Rete** (ADR 0011): NetworkManager (non networkd/iwd), comandato con `nmcli`
  dall'utente `deck` grazie alla regola polkit per `wheel`. Pagina Wi-Fi in
  `deckard-ui/src/wifi.rs`, tastiera a schermo riusabile in stile SteamOS
  (touch + scorciatoie X/Y/L1/R1/L2/R2, `Input::extra_buttons`) in
  `deckard-ui/src/keyboard.rs`, backend in `deckard-system/src/wifi.rs`
  (`DECKARD_FAKE_WIFI=1` per provarla senza Wi-Fi).

## 6. Ambiente di sviluppo e comandi `[CONDIVISO]`

Lo sviluppatore lavora su **Windows 11**. Quindi:
- Il **frontend** deve restare compilabile anche su Windows/Linux desktop per lo
  sviluppo (winit/wgpu/cpal sono cross-platform; su Windows i core sono `.dll`).
  Il codice specifico Linux (evdev, sysfs, sessioni) sta dietro
  `#[cfg(target_os = "linux")]` in `deckard-input`/`deckard-system`.
- La **build dell'ISO** richiede Arch Linux: il workflow `ISO` di GitHub Actions
  (container `archlinux` privilegiato) oppure WSL2 con Arch. Sul PC di sviluppo
  WSL non è ancora installato (`wsl --install archlinux`, lo fa l'utente) e non
  c'è Docker.

Comandi (da `frontend/`):
```bash
cargo build                      # build debug
cargo run -p deckard                            # apre il menu
cargo run -p deckard -- --list                  # mostra configurazione e libreria
cargo run -p deckard -- --rom <percorso_rom>     # core scelto dalla configurazione
cargo run -p deckard -- --core <percorso_core> --rom <percorso_rom>   # core esplicito
cargo build -p deckard-test-core && cargo run -p deckard -- --core target/debug/deckard_test_core.dll   # prova senza ROM (su Linux: libdeckard_test_core.so)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```
Opzioni utili di `deckard`: `--config <file>` e `--data-dir <dir>` (utili per
prove in cartelle temporanee), `-o chiave=valore` (opzione del core solo per
quella sessione, ripetibile), `--fullscreen` / `--windowed`, `--system-dir`,
`--save-dir`, `--max-frames N` (nascosta: esce dopo N frame — emulati nel
gioco, presentati nel menu — per verifiche automatiche). Log: `RUST_LOG=debug`;
i messaggi dei core hanno target `core`.

Verifica visiva su Windows: avviare `deckard.exe` con `Start-Process`, portare la
finestra in primo piano (`SetForegroundWindow`), inviare tasti con
`System.Windows.Forms.SendKeys` e catturare la finestra con
`Graphics.CopyFromScreen`; usare una cartella temporanea con `--config` e
`--data-dir`. Attenzione: in PowerShell le variabili non distinguono
maiuscole/minuscole (`$d` e `$D` sono la stessa variabile).
Per provare il core di test via configurazione, copialo in `cores/` come
`deckard_test_core_libretro.dll` (convenzione `<nome>_libretro`).

Su Linux la build richiede `libasound2-dev` (ALSA) e `libudev-dev` (gilrs).
Sul portatile di sviluppo il driver Vulkan della Intel UHD va in crash in
`vkCreateDevice`: per questo si usa la GPU NVIDIA (o `WGPU_BACKEND=dx12`).
Build e prova dell'ISO (in ambiente Arch, dalla root del repo; vedi `os/README.md`):
```bash
sudo pacman -S --needed archiso base-devel rust pkgconf git
sudo scripts/build-iso.sh            # ISO in out/
scripts/run-qemu.sh                  # avvio UEFI in QEMU (qemu-desktop, edk2-ovmf)
gh workflow run ISO                  # build in GitHub Actions (artifact "deckard-iso")
DECKARD_BOOT_DIAG=1 scripts/boot-test.sh out/*.iso shots 120   # avvio senza finestra + screenshot + controllo
```
Se aggiungi o cambi comandi, aggiorna questa sezione.

## 7. Regole di codice `[CONDIVISO]`

- Rust stable, **edition 2024**. Versione minima supportata (MSRV) **1.89** (richiesta da cosmic-text, usato da glyphon),
  dichiarata in `rust-version` di `frontend/Cargo.toml` e verificata in CI.
  Lint condivisi in `[workspace.lints]`: ogni crate deve avere `[lints] workspace = true`.
- `cargo fmt` e `cargo clippy -D warnings` devono passare prima di considerare
  finito un lavoro.
- Errori: `thiserror` nei crate di libreria, `anyhow` solo nel binario `deckard`.
  Nessun `unwrap()`/`expect()` nel codice di produzione, salvo invarianti
  documentate.
- Logging con `tracing`. `println!` solo per l'output richiesto da un comando
  (es. `--list`), mai per log.
- Dipendenze: preferire crate maturi e mantenuti; ogni nuova dipendenza va
  motivata nel messaggio di commit. Verificare la compatibilità di licenza con GPL-3.0.
- Test: unità per parsing/config/library; integrazione con il core di test
  `deckard-test-core`, che `deckard-core/tests/test_core.rs` compila e carica
  davvero (mai binari scaricati). Una nuova funzione del frontend che tocca i
  core va resa osservabile nel core di test e coperta lì. `unwrap` è ammesso nei
  test (`clippy.toml`; nei file di test di integrazione serve
  `#![allow(clippy::unwrap_used)]`).
- Script shell: `bash` con `set -euo pipefail`, verificati con `shellcheck` prima del
  push (è installato in WSL): `shellcheck -x --source-path=SCRIPTDIR scripts/*.sh
  session/deckard-session session/deckard-steam-session
  session/steamos-session-select session/*.sh installer/deckard-install
  os/archiso/profiledef.overrides.sh` (lo stesso comando della CI).

## 8. Lingua `[CONDIVISO]`

- **Italiano**: README, `docs/`, ADR, questo file, messaggi all'utente.
- **Inglese**: identificatori, commenti nel codice, messaggi di log, messaggi di commit.
- Testi dell'interfaccia: tramite file di traduzione (inglese di base + italiano),
  mai stringhe cablate nel codice UI.

## 9. Limiti legali e di sicurezza `[CONDIVISO]`

- **Mai** includere ROM, BIOS, firmware proprietari di console o chiavi nel repo
  o nell'immagine.
- Non copiare codice di RetroArch/Lakka: il frontend è scritto da zero
  (consultare `libretro.h` e la documentazione libretro va bene).
- Stile XMB "ispirato", senza asset, icone, font o suoni di Sony o altri marchi.
- Non ridistribuire componenti proprietari Valve; Steam viene installato dal
  pacchetto Arch che lo scarica a runtime.
- Nessuna operazione distruttiva su dischi (partizionamento, `dd`) senza
  conferma esplicita.

## 10. Roadmap `[CONDIVISO]`

1. **Fase 0** ✅ — Scheletro: workspace Cargo, `libretro-sys`, CI (fmt/clippy/test), LICENSE, README.
2. **Fase 1** ✅ — Avvio di un core software da riga di comando (video+audio+input), su desktop.
   Verificata su Windows con il core di test; su Linux la verifica è affidata alla CI,
   e manca ancora la prova con un core reale (es. snes9x).
3. **Fase 2** — Menu XMB, libreria ROM, configurazione, save state. In tre tappe:
   - **2a** ✅ configurazione (`deckard-config`), libreria (`deckard-library`),
     save state su file con slot, scelta del core dalla ROM, `--list`;
   - **2b** ✅ menu XMB disegnato con wgpu + glyphon (font Inter, OFL), navigazione
     come macchina a stati testata, copertine, avvio di un gioco e ritorno al menu;
   - **2c** ✅ menu rapido in gioco (Guide/Steam o `F1`) con save state e opzioni
     del core, impostazioni video (scala intera, filtro) e lingua dal menu.
     Copertine solo locali.
   Fase 2 verificata su Windows con il core di test e la tastiera; restano da
   provare un gamepad vero, un core reale e Linux.
4. **Fase 3** — Profilo archiso avviabile su Steam Deck LCD con Deckard in autostart.
   Build verificata in CI e in WSL; l'ISO si avvia in QEMU fino al menu di Deckard
   (test automatico `scripts/boot-test.sh` nel workflow ISO, con diagnostica).
   Da verificare sul Deck: pannello, controlli, audio, Wi-Fi.
5. **Fase 4** ✅ — Dual mode con Steam/gamescope e cambio sessione (`deckard-system`,
   protocollo `next-session`, session manager a loop, `switch-to-deckard.sh`,
   voce "Passa a Steam" nel menu Impostazioni).
6. **Fase 5** — Core hardware (Vulkan, poi GL), shader, menu rapido completo:
   - **5a** ✅ Shader CRT e scanline post-processing in WGSL con commutazione dinamica;
   - **5b** ✅ Supporto dischi multipli (libretro disk control interface) e controllo volume in gioco/impostazioni;
   - **5c** Core hardware (Vulkan `RETRO_HW_CONTEXT_VULKAN`, poi GL/GLES via dmabuf/EGL).
7. **Fase 6** — Installazione e trasferimento dei contenuti (ordine scelto dall'utente):
   - **6a** ✅ Installer su disco intero (SSD o microSD, ext4, systemd-boot) dal menu
     con doppia conferma; testato end-to-end in QEMU (ADR 0010);
   - **6b** ✅ Wi-Fi dal menu con NetworkManager e tastiera a schermo (ADR 0011);
   - **6c** Cartella condivisa Samba attivabile dalle Impostazioni;
   - **6d** Caricamento di giochi/BIOS/core da browser (piccolo server web in Deckard).
   La 5c (core hardware) è rimandata a dopo la Fase 6, su richiesta dell'utente.

Lavora una fase alla volta; non anticipare funzionalità di fasi successive senza richiesta.

## 11. Come lavorare su questo repo `[CONDIVISO]`

- Prima di modifiche ampie, proponi un piano breve e attendi conferma.
- Modifiche piccole e verificabili; esegui fmt/clippy/test prima di dichiarare finito.
- Se qualcosa non è verificabile qui (es. richiede l'hardware Deck), dillo
  esplicitamente invece di darlo per funzionante.
- Commit solo su richiesta dell'utente; messaggi in inglese, stile conventional
  commits (`feat(core): ...`, `fix(os): ...`).
- Mantieni aggiornati questo file e GEMINI.md quando cambiano struttura,
  comandi o decisioni.

---

## 12. Note specifiche per Claude Code

- Questo file è letto automaticamente da Claude Code. GEMINI.md è il suo gemello
  per Gemini CLI: le sezioni `[CONDIVISO]` devono restare identiche.
- Ambiente host: Windows 11, shell PowerShell e Git Bash. Per la build ISO serve
  un ambiente Arch (container o WSL2): non tentare `mkarchiso` direttamente su Windows.
