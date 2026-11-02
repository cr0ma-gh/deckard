# ADR 0001 — Binding libretro scritti a mano

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

Il frontend Rust deve chiamare i core libretro tramite FFI. Le opzioni erano:
generare i binding con `bindgen` a ogni build, usare un crate esterno già
pubblicato, oppure scriverli a mano.

## Decisione

I binding stanno nel crate interno `frontend/crates/libretro-sys`, **scritti a
mano**, coprendo solo il sottoinsieme dell'API che serve a ogni fase.

- L'header ufficiale è copiato in `libretro-sys/include/libretro.h` come riferimento.
- Il test `tests/header_consistency.rs` legge l'header e verifica che ogni
  costante Rust abbia lo stesso valore. Ogni nuova costante va aggiunta anche al test.
- Un test di unità verifica dimensioni e allineamento delle struct su 64 bit.
- Le enum C sono esposte come `c_int` + costanti, mai come enum Rust (un core
  può passare valori fuori intervallo, e per una enum Rust sarebbe undefined behavior).

## Conseguenze

- Nessuna dipendenza da libclang: la build funziona uguale su Windows e Linux.
- Più lavoro manuale quando servono nuove parti dell'API, compensato dal test
  di coerenza con l'header.
- La callback di log libretro è variadica (`printf`): Rust stable non può
  *definire* funzioni variadiche, quindi in Fase 1 serve un piccolo shim in C
  (crate `cc`) che formatta il messaggio e lo inoltra a Rust.
- Per aggiornare l'header: sostituire `include/libretro.h` con la versione
  ufficiale più recente ed eseguire `cargo test -p libretro-sys`.
