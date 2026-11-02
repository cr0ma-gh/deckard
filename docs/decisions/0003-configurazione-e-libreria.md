# ADR 0003 — Configurazione, libreria e save state (Fase 2a)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

Prima del menu (Fase 2b) servono: dove stanno ROM, core, salvataggi e
copertine; come si associa un gioco al suo core; come si salvano stati e
opzioni dei core.

## Decisioni

1. **Una cartella per sistema** (`roms/snes/`, `roms/gba/`, …), scelta
   dell'utente: semplice e prevedibile, nessun database da distribuire. Il
   riconoscimento per CRC con i database libretro resta possibile in futuro.
2. **Configurazione TOML** (`deckard-config`) in `~/.config/deckard/config.toml`,
   dati in `~/.local/share/deckard/` (cartelle XDG tramite il crate `dirs`).
   Il file viene creato con i default se manca; i campi mancanti prendono il
   default, così i file vecchi restano validi. Se il file definisce dei sistemi,
   sostituiscono quelli predefiniti (nessuna fusione implicita).
3. **Core per nome**: `core = "snes9x"` diventa `<cores>/snes9x_libretro.so`
   (`.dll` su Windows), la convenzione dei core libretro; un valore con
   separatori di percorso o con l'estensione della libreria è preso come percorso.
4. **Sistema di un file**: prima la cartella sotto `roms/` (risolve estensioni
   ambigue come `.bin`), poi la prima estensione compatibile.
5. **Copertine solo locali**, scelta dell'utente: `thumbnails/<id>/<file>.png` o
   la struttura di libretro-thumbnails (`Named_Boxarts`, caratteri speciali
   sostituiti da `_`). Il download arriverà con la gestione della rete.
6. **Save state** in `states/<id sistema>/<file>.state[N]` (nomi compatibili con
   l'uso comune libretro), 10 slot configurabili, scrittura atomica.
7. **Opzioni dei core** salvate in `core-options/<core>.toml`, solo i valori
   diversi dal default; i valori passati con `-o` valgono per una sola
   sessione e non sovrascrivono quelli salvati.

## Conseguenze

- Nuove dipendenze: `serde`, `toml` (configurazione), già presente `dirs`.
- La libreria è una scansione al volo (veloce su poche migliaia di file); una
  cache potrà servire più avanti se le collezioni diventano grandi.
- Le mappature dei controlli non sono ancora configurabili.
