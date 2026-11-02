# Deckard

Distribuzione Linux per **Steam Deck LCD**, basata su Arch Linux, con un frontend
libretro nativo scritto in Rust e un'interfaccia a menu in stile XMB.
Funziona in dual mode: modalità Deckard (emulazione) e modalità Steam.

> Stato: **Fase 2a** — il frontend avvia giochi da riga di comando, con video,
> audio, input, salvataggi e save state, leggendo configurazione e libreria.
> Il menu XMB arriva nella tappa 2b.

## Struttura

| Percorso | Contenuto |
|---|---|
| `frontend/crates/libretro-sys/` | Binding FFI dell'API libretro (`include/libretro.h` è l'header ufficiale) |
| `frontend/crates/deckard-core/` | Caricamento ed esecuzione dei core, opzioni, SRAM, save state |
| `frontend/crates/deckard-video/` | Output video con wgpu |
| `frontend/crates/deckard-audio/` | Output audio, ricampionamento, controllo dinamico della frequenza |
| `frontend/crates/deckard-input/` | Gamepad e tastiera → RetroPad |
| `frontend/crates/deckard-config/` | Configurazione (`config.toml`) e opzioni salvate dei core |
| `frontend/crates/deckard-library/` | Libreria giochi: cartelle per sistema, titoli, copertine |
| `frontend/crates/deckard-test-core/` | Core libretro minimale usato nei test |
| `frontend/crates/deckard/` | Binario principale |
| `docs/decisions/` | Decisioni architetturali (ADR) |

## Provarlo

Serve Rust stable (versione minima 1.88). Su Linux servono anche
`libasound2-dev` e `libudev-dev`. Da `frontend/`:

```bash
cargo build -p deckard -p deckard-test-core
cargo run -p deckard -- --core target/debug/deckard_test_core.dll
```

Su Linux il core di test si chiama `target/debug/libdeckard_test_core.so`.

### Con i tuoi giochi

Al primo avvio Deckard crea `config.toml` (Windows: `%APPDATA%\deckard\`,
Linux: `~/.config/deckard/`) e le cartelle dei dati (Windows:
`%APPDATA%\deckard\`, Linux: `~/.local/share/deckard/`):

- metti i core libretro in `cores/` (es. `snes9x_libretro.dll` / `.so`);
- metti i giochi in `roms/<sistema>/` (es. `roms/snes/`);
- le copertine opzionali in `thumbnails/<sistema>/<nome del file>.png`.

```bash
cargo run -p deckard -- --list                 # cosa vede Deckard
cargo run -p deckard -- --rom <percorso_rom>   # il core si sceglie da solo
```

Comandi: frecce = croce direzionale, `X` = A (con il core di test suona un tono),
`F2`/`F4` = salva/carica stato, `F6`/`F7` = cambia slot, `Esc` = esci.
Opzioni dei core per una sola sessione: `-o chiave=valore`.

## Sviluppo

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Il frontend si sviluppa anche su Windows o Linux desktop; l'immagine del sistema
si costruirà in un ambiente Arch Linux (container o WSL2).

Per gli agenti IA: vedi [CLAUDE.md](CLAUDE.md) e [GEMINI.md](GEMINI.md).

## Contenuti

Deckard **non** include ROM, BIOS o firmware di console: vanno forniti
dall'utente, nel rispetto delle leggi del proprio paese.

## Licenza

GPL-3.0-or-later (vedi [LICENSE](LICENSE)).
`frontend/crates/libretro-sys/include/libretro.h` è distribuito con la propria
licenza MIT, riportata nell'intestazione del file.
