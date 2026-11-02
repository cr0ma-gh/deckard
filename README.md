# Deckard

Distribuzione Linux per **Steam Deck LCD**, basata su Arch Linux, con un frontend
libretro nativo scritto in Rust e un'interfaccia a menu in stile XMB.
Funziona in dual mode: modalità Deckard (emulazione) e modalità Steam.

> Stato: **Fase 0** — scheletro del progetto. Non è ancora avviabile.

## Struttura

| Percorso | Contenuto |
|---|---|
| `frontend/` | Workspace Rust del frontend libretro |
| `frontend/crates/libretro-sys/` | Binding FFI dell'API libretro (`include/libretro.h` è l'header ufficiale) |
| `frontend/crates/deckard/` | Binario principale |
| `os/` | Profilo archiso dell'immagine (dalla Fase 3) |
| `docs/` | Documentazione e decisioni architetturali (ADR) |

## Sviluppo

Serve Rust stable (versione minima 1.85). Da `frontend/`:

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Il frontend si sviluppa anche su Windows o Linux desktop; l'immagine del sistema
si costruisce in un ambiente Arch Linux (container o WSL2).

Per gli agenti IA: vedi [CLAUDE.md](CLAUDE.md) e [GEMINI.md](GEMINI.md).

## Contenuti

Deckard **non** include ROM, BIOS o firmware di console: vanno forniti
dall'utente, nel rispetto delle leggi del proprio paese.

## Licenza

GPL-3.0-or-later (vedi [LICENSE](LICENSE)).
`frontend/crates/libretro-sys/include/libretro.h` è distribuito con la propria
licenza MIT, riportata nell'intestazione del file.
