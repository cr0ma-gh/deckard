# ADR 0002 — Ciclo principale, audio e core di test (Fase 1)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

La Fase 1 deve far girare un core libretro software con video, audio e input.
Servivano scelte su thread, sincronizzazione, ricampionamento audio e su come
testare il frontend senza scaricare core di terze parti.

## Decisioni

1. **Un solo thread** per finestra ed emulazione: il core gira nel thread
   principale, dentro il ciclo eventi di winit (obbligatorio su Windows/macOS
   per le finestre). L'audio ha il suo thread (cpal), alimentato da un ring
   buffer lock-free (`rtrb`).
2. **Sincronizzazione a due modalità** (`deckard/src/pacing.rs`):
   - *Vsync*: se la frequenza del display differisce da quella del core al
     massimo del 5%, si esegue un frame del core per ogni refresh (presentazione
     Fifo) e la frequenza audio d'ingresso viene scalata dello stesso rapporto.
     È il caso dello Steam Deck LCD a 60 Hz con quasi tutte le console.
   - *Timed*: altrimenti (PAL a 50 Hz, monitor a 144 Hz) i frame del core sono
     schedulati sull'orologio, con recupero massimo di 3 frame dopo uno stallo.
3. **Controllo dinamico della frequenza audio** (±0,5%) che mantiene il buffer
   pieno a metà; il buffer viene pre-riempito di silenzio quando è vuoto
   (avvio o dopo uno stallo), altrimenti il controllo impiegherebbe secondi.
4. **Ricampionatore proprio** (Hermite cubico, `deckard-audio/src/resampler.rs`)
   invece di `rubato`: poche righe, rapporto variabile a ogni chiamata (quello
   che serve al controllo dinamico), testato su una sinusoide.
5. **Core di test scritto in Rust** (`deckard-test-core`, cdylib) invece di
   `libretro-samples` in C: nessun download, nessun compilatore C aggiuntivo per
   i test, e ogni funzione del frontend ha qualcosa di osservabile (video, tono
   con A, opzione `deckard_test_speed`, SRAM, save state, log). Il test di
   integrazione di `deckard-core` lo compila e lo carica davvero.
6. **Conversione pixel su CPU** verso BGRA8 (risoluzioni retro: costo
   trascurabile), filtro nearest, letterbox/pillarbox all'aspect ratio del core.
7. **GPU preferita: quella ad alte prestazioni** (portatili ibridi). Sul Deck
   c'è una sola GPU. `WGPU_POWER_PREF` e `WGPU_BACKEND` permettono di cambiarla.

## Conseguenze

- Su Windows, sul portatile di sviluppo, il driver Vulkan della Intel UHD va in
  crash in `vkCreateDevice` (fuori dal nostro codice). Con la preferenza
  "alte prestazioni" si usa la NVIDIA; in alternativa `WGPU_BACKEND=dx12`.
- Core hardware (OpenGL/Vulkan), rotazione e rumble restano non supportati
  (rispondono `false` all'environment) fino alla Fase 5.
- Il frontend non legge ancora file `.zip`: va passata la ROM estratta.
