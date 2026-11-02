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

Esistono già (Fasi 0–2a): `libretro-sys`, `deckard-core`, `deckard-video`,
`deckard-audio`, `deckard-input`, `deckard-config`, `deckard-library`,
`deckard-test-core`, `deckard`, la CI in `.github/workflows/ci.yml` e
`docs/decisions/`. Gli altri crate e cartelle (`deckard-ui`, `deckard-system`,
`os/`, `packages/`, `session/`, `scripts/`) sono previsti: creali nella fase in
cui servono, non prima. Aggiorna questa sezione quando la struttura cambia.

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
│       ├── deckard-ui/        # menu XMB, temi, font, animazioni
│       ├── deckard-library/   # scansione roms/<sistema>/, titoli, copertine, nomi dei save state
│       ├── deckard-config/    # config.toml (cartelle, sistemi, video) e opzioni salvate dei core
│       ├── deckard-system/    # integrazione OS: batteria, luminosità, Wi-Fi, cambio sessione
│       └── deckard/           # binario principale
├── os/
│   └── archiso/               # profilo archiso (profiledef.sh, packages.x86_64, airootfs/)
├── packages/                  # PKGBUILD: deckard, core libretro, file di sessione
├── session/                   # script/unità per avvio e cambio sessione Deckard <-> Steam
├── scripts/                   # build ISO, container di build, utilità
└── docs/
    ├── decisions/             # ADR
    └── hardware.md            # note su Steam Deck LCD
```

## 4. Architettura del frontend Rust `[CONDIVISO]`

### 4.1 Flusso principale
1. `deckard` carica `config.toml` (`deckard-config`), risolve le cartelle e
   mostra il menu XMB (`deckard-ui`, Fase 2b). Oggi si avvia da riga di comando.
2. Il gioco scelto (oggi `--rom`) determina il sistema — prima dalla cartella
   sotto `roms/`, poi dall'estensione (`Config::system_for`) — e quindi il core
   (`snes9x` → `<cores>/snes9x_libretro.so`). `deckard-library` elenca i giochi.
3. `deckard-core` (`Core::load`) carica il core con `libloading`, verifica
   `retro_api_version() == 1`, registra le callback, chiama `retro_init`;
   `Core::load_game` chiama `retro_load_game` e carica la SRAM.
4. Ciclo principale (`deckard/src/app.rs`, thread principale dentro winit):
   input → `Core::run_frame` (quante volte dice `pacing.rs`) → audio a
   `deckard-audio` → ultimo frame a `deckard-video` → presentazione con vsync.
5. Hotkey (es. `…` + `A`) apre il menu rapido in sovrimpressione: salva/carica stato,
   opzioni del core, esci (Fase 2c). Oggi: `Esc` o il tasto Guide/Steam escono,
   `F2`/`F4` salvano/caricano lo stato, `F6`/`F7` cambiano slot.
6. All'uscita la SRAM viene salvata (`Drop` di `Core`) e le opzioni del core
   diverse dal default vanno in `<core-options>/<core>.toml`; i valori passati
   con `-o` valgono solo per quella sessione e non vengono salvati.

### 4.1b Configurazione e libreria (ADR 0003)
- `config.toml` in `<config dir>/deckard/` (Linux: `~/.config/deckard/`); se
  manca viene creato con i default. I percorsi relativi sono dentro
  `<data dir>/deckard/` (Linux: `~/.local/share/deckard/`): `roms/`, `cores/`,
  `system/`, `saves/`, `states/`, `thumbnails/`, `core-options/`.
- Sistemi: tabelle `[[system]]` con `id`, `name`, `core`, `extensions`,
  `folder` opzionale (default = `id`). Se il file ne definisce, sostituiscono i
  default (NES, SNES, GB, GBC, GBA, Mega Drive, PlayStation).
- Giochi: `roms/<cartella>/` fino a 3 livelli di profondità, file nascosti
  esclusi, ordinati per titolo; titolo = nome del file senza estensione.
- Copertine: `thumbnails/<id>/<nome file>.png` oppure
  `thumbnails/<id>/Named_Boxarts/<titolo>.png` (convenzione libretro-thumbnails).
- Save state: `states/<id sistema>/<nome file>.state` (slot 0) o `.stateN`;
  SRAM: `saves/<nome file>.srm`. Scritture atomiche (file temporaneo + rename).

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
`SET_MESSAGE`, `GET_INPUT_BITMASKS`, `GET_TARGET_REFRESH_RATE`, più alcuni
comandi informativi accettati e ignorati.
Fase 5: `SET_HW_RENDER` con **Vulkan** (`RETRO_HW_CONTEXT_VULKAN`, condividendo il
device di wgpu tramite `wgpu-hal`), poi OpenGL/GLES tramite EGL + import dmabuf.
Comandi non gestiti: restituire `false` e loggare una volta sola a livello `debug`.

### 4.4 Video, audio, sincronizzazione
- Il frame del core viene convertito in BGRA8 su CPU, caricato in una texture e
  disegnato all'aspect ratio del core (letterbox/pillarbox, filtro nearest) con
  uno shader WGSL. Integer scale, stretch e shader CRT sono fasi successive.
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
- Tastiera (sviluppo su desktop): frecce, `X`=A, `Z`=B, `S`=X, `A`=Y, `Q`/`W`=L/R,
  `1`/`2`=L2/R2, `Invio`=Start, `Shift destro`=Select, `Esc`=esci, `F2`/`F4`
  salva/carica stato, `F6`/`F7` slot precedente/successivo (come RetroArch).
- Le mappature configurabili andranno in `deckard-config` (fase successiva); i trackpad
  (mouse/lightgun) sono una fase successiva.

## 5. Sistema operativo (archiso) `[CONDIVISO]`

- Profilo in `os/archiso/`, derivato dal profilo `releng` di archiso ma ridotto.
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
- **Cambio sessione**: il frontend scrive la sessione successiva in un file di
  stato (es. `~/.local/state/deckard/next-session`) ed esce; il session manager
  legge il file e avvia l'altra sessione. Lo stesso meccanismo va reso
  disponibile dalla modalità Steam (voce/scorciatoia non-Steam).
- Ventola, TDP, luminosità, batteria: tramite sysfs/strumenti standard. Nessuna
  modifica a firmware/BIOS del Deck.
- Prima: immagine live ISO avviabile da USB. Installer su disco interno: fase
  successiva (mai scrivere su disco senza conferma esplicita dell'utente finale).

## 6. Ambiente di sviluppo e comandi `[CONDIVISO]`

Lo sviluppatore lavora su **Windows 11**. Quindi:
- Il **frontend** deve restare compilabile anche su Windows/Linux desktop per lo
  sviluppo (winit/wgpu/cpal sono cross-platform; su Windows i core sono `.dll`).
  Il codice specifico Linux (evdev, sysfs, sessioni) sta dietro
  `#[cfg(target_os = "linux")]` in `deckard-input`/`deckard-system`.
- La **build dell'ISO** richiede Arch Linux: usare un container `archlinux`
  (Docker/Podman, `--privileged` per i loop device) o WSL2 con Arch.

Comandi (da `frontend/`):
```bash
cargo build                      # build debug
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
`--save-dir`, `--max-frames N` (nascosta: esce dopo N frame, per verifiche
automatiche). Log: `RUST_LOG=debug`; i messaggi dei core hanno target `core`.
Per provare il core di test via configurazione, copialo in `cores/` come
`deckard_test_core_libretro.dll` (convenzione `<nome>_libretro`).

Su Linux la build richiede `libasound2-dev` (ALSA) e `libudev-dev` (gilrs).
Sul portatile di sviluppo il driver Vulkan della Intel UHD va in crash in
`vkCreateDevice`: per questo si usa la GPU NVIDIA (o `WGPU_BACKEND=dx12`).
Build ISO (in ambiente Arch, dalla root del repo):
```bash
sudo mkarchiso -v -w /tmp/deckard-work -o out os/archiso
```
Se aggiungi o cambi comandi, aggiorna questa sezione.

## 7. Regole di codice `[CONDIVISO]`

- Rust stable, **edition 2024**. Versione minima supportata (MSRV) **1.88** (richiesta da wgpu e libloading),
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
- Script shell: `bash` con `set -euo pipefail`, verificati con `shellcheck`.

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
   - **2b** menu XMB disegnato con wgpu + glyphon (font OFL, es. Inter), navigazione
     come macchina a stati testata, avvio di un gioco e ritorno al menu;
   - **2c** menu rapido in gioco (Guide/Steam o `F1`), opzioni del core dal menu,
     impostazioni video (scala intera, filtro). Copertine solo locali.
4. **Fase 3** — Profilo archiso avviabile su Steam Deck LCD con Deckard in autostart.
5. **Fase 4** — Dual mode con Steam/gamescope e cambio sessione.
6. **Fase 5** — Core hardware (Vulkan, poi GL), shader, menu rapido completo, installer.

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
