# GEMINI.md — Deckard

> Istruzioni per agenti IA (Gemini CLI) che lavorano su questo repository.
> **CLAUDE.md contiene le stesse regole condivise**: se modifichi una sezione
> marcata `[CONDIVISO]`, applica la stessa modifica anche in CLAUDE.md.
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

Esistono già (Fase 0): `libretro-sys`, `deckard`, la CI in
`.github/workflows/ci.yml` e `docs/decisions/`. Gli altri crate e cartelle sono
previsti: creali nella fase in cui servono, non prima. Aggiorna questa sezione
quando la struttura cambia.

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
│       ├── deckard-input/     # gamepad (gilrs/evdev), mappature, hotkey
│       ├── deckard-ui/        # menu XMB, temi, font, animazioni
│       ├── deckard-library/   # scansione ROM, playlist, database, copertine
│       ├── deckard-config/    # configurazione utente (TOML) e opzioni dei core
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
1. `deckard` avvia winit + wgpu e mostra il menu XMB (`deckard-ui`).
2. L'utente sceglie un gioco → `deckard-library` risolve core + ROM.
3. `deckard-core` carica il core con `libloading`, verifica `retro_api_version() == 1`,
   registra le callback, chiama `retro_init` → `retro_load_game`.
4. Run loop: `retro_run()` una volta per frame; video e audio vengono consegnati
   tramite callback ai crate `deckard-video` / `deckard-audio`.
5. Hotkey (es. `…` + `A`) apre il menu rapido in sovrimpressione: salva/carica stato,
   opzioni del core, esci.

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
- `unsafe` è ammesso **solo** in `libretro-sys` e `deckard-core` (più l'interop
  GPU in `deckard-video`). Ogni blocco `unsafe` ha un commento `// SAFETY:`
  (imposto dal lint clippy `undocumented_unsafe_blocks`).
- `libretro-sys` è scritto a mano (ADR 0001): nomi identici all'header C, enum C
  come `c_int` + costanti. **Ogni costante aggiunta va inserita anche in
  `tests/header_consistency.rs`**, che la confronta con `include/libretro.h`.
- La callback di log libretro è variadica: va implementata con un piccolo shim
  in C (crate `cc`), perché Rust stable non può definire funzioni variadiche.

### 4.3 Environment callback — priorità di implementazione
Fase 1 (core software): `GET_SYSTEM_DIRECTORY`, `GET_SAVE_DIRECTORY`,
`SET_PIXEL_FORMAT` (XRGB8888, RGB565, 0RGB1555), `GET_VARIABLE`,
`SET_VARIABLES`, `GET_VARIABLE_UPDATE`, `GET_CORE_OPTIONS_VERSION`,
`SET_CORE_OPTIONS_V2`, `GET_LOG_INTERFACE`, `SET_INPUT_DESCRIPTORS`,
`GET_CAN_DUPE`, `SET_GEOMETRY`, `SET_SYSTEM_AV_INFO`.
Fase 2: `SET_HW_RENDER` con **Vulkan** (`RETRO_HW_CONTEXT_VULKAN`, condividendo il
device di wgpu tramite `wgpu-hal`), poi OpenGL/GLES tramite EGL + import dmabuf.
Comandi non gestiti: restituire `false` e loggare una volta sola a livello `debug`.

### 4.4 Video, audio, sincronizzazione
- Il framebuffer del core viene caricato in una texture e scalato (integer scale /
  aspect corretto / stretch) con shader WGSL. Shader CRT ecc. sono fase successiva.
- Pannello LCD: nativamente **800×1280 verticale**, usato ruotato a 1280×800,
  refresh 60 Hz (range utile ~40–60 Hz). La rotazione va gestita (gamescope o
  trasformazione in wgpu) — non dare per scontato l'orientamento.
- Sincronizzazione: vsync + **controllo dinamico del rate audio** (resampling con
  `rubato`) per evitare crepitii e frame duplicati quando l'FPS del core ≠ 60 Hz.
- Budget di latenza: input letto subito prima di `retro_run()`; niente buffering
  video oltre il necessario.

### 4.5 Input
- Senza Steam in esecuzione, i controlli del Deck sono esposti dal driver kernel
  `hid-steam` come gamepad evdev. Con Steam attivo, Steam li gestisce.
- Le mappature RetroPad sono in `deckard-config`; i trackpad sono opzionali
  (mouse/lightgun) e vanno trattati in fase successiva.

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
cargo run -p deckard -- --core <percorso_core> --rom <percorso_rom>   # avvio diretto di un gioco
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```
Build ISO (in ambiente Arch, dalla root del repo):
```bash
sudo mkarchiso -v -w /tmp/deckard-work -o out os/archiso
```
Se aggiungi o cambi comandi, aggiorna questa sezione.

## 7. Regole di codice `[CONDIVISO]`

- Rust stable, **edition 2024**. Versione minima supportata (MSRV) **1.85**,
  dichiarata in `rust-version` di `frontend/Cargo.toml` e verificata in CI.
  Lint condivisi in `[workspace.lints]`: ogni crate deve avere `[lints] workspace = true`.
- `cargo fmt` e `cargo clippy -D warnings` devono passare prima di considerare
  finito un lavoro.
- Errori: `thiserror` nei crate di libreria, `anyhow` solo nel binario `deckard`.
  Nessun `unwrap()`/`expect()` nel codice di produzione, salvo invarianti
  documentate.
- Logging con `tracing`. Niente `println!` nel codice finale.
- Dipendenze: preferire crate maturi e mantenuti; ogni nuova dipendenza va
  motivata nel messaggio di commit. Verificare la compatibilità di licenza con GPL-3.0.
- Test: unità per parsing/config/library; test d'integrazione del run loop con un
  core libretro di test (es. il core di esempio di `libretro-samples`) compilato
  nei test, mai binari scaricati.
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
2. **Fase 1** — Avvio di un core software da riga di comando (video+audio+input), su desktop.
3. **Fase 2** — Menu XMB, libreria ROM, configurazione, save state.
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

## 12. Note specifiche per Gemini CLI

- Questo file è letto automaticamente da Gemini CLI. CLAUDE.md è il suo gemello
  per Claude Code: le sezioni `[CONDIVISO]` devono restare identiche.
- Ambiente host: Windows 11, shell PowerShell e Git Bash. Per la build ISO serve
  un ambiente Arch (container o WSL2): non tentare `mkarchiso` direttamente su Windows.
- Prima di eseguire comandi che modificano file o sistema, mostra il comando e
  attendi conferma se non sei in modalità di approvazione automatica.
