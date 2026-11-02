# ADR 0008 — Shader post-processing CRT e scanline (Tappa 5a)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

Deckard esegue giochi retrò (NES, SNES, Game Boy, Mega Drive, PS1) su schermi
moderni ad alta risoluzione (lo schermo LCD dello Steam Deck a 1280×800 e monitor
desktop). La resa puramente a pixel nitidi (nearest) o sfocati (bilineare) non
riproduce l'aspetto originale su monitor a tubo catodico (CRT), per cui la
grafica dell'epoca era stata disegnata.

## Decisioni

1. **Pipeline di shader integrata in WGSL**:
   Invece di dipendere da motori shader esterni complessi (come slang/librashader),
   Deckard integra shader WGSL leggeri, nativi ed efficienti direttamente in
   `deckard-video`, eseguiti tramite wgpu.

2. **Tre modalità di base**:
   - `None`: visualizzazione standard a pixel puliti (nearest se `smooth=false`,
     bilineare se `smooth=true`).
   - `Scanlines`: linee di scansione orizzontali allineate alla risoluzione nativa
     del frame emulato, con profilo sinusoidale calibrato per mantenere la luminosità.
   - `Crt`: simulazione CRT completa con:
     - scanline adattive alla risoluzione sorgente;
     - maschera d'apertura RGB (shadow mask / phosphor triad);
     - leggera curvatura geometrica opzionale del tubo (barrel distortion);
     - vignettatura ai bordi.

3. **Parametri passati via Uniform Buffer**:
   La GPU riceve le dimensioni del frame della console (`source_size`), le dimensioni
   del viewport di visualizzazione (`target_size`) e l'indice dello shader selezionato.
   In questo modo le scanline si adattano automaticamente se un gioco cambia risoluzione
   (es. da 240p a 480i su PS1).

4. **Configurazione e controllo runtime**:
   - Salvato in `config.toml` come `shader = "none" | "scanlines" | "crt"` sotto `[video]`.
   - Modificabile sia dal menu **Impostazioni** che in tempo reale dal **menu rapido in gioco**,
     consentendo all'utente di verificare immediatamente la resa visiva senza uscire dalla partita.

## Conseguenze

- Funziona identicamente e senza librerie esterne su tutte le GPU supportate da wgpu
  (Vulkan su Linux/Deck, DX12/Vulkan su Windows, Metal su macOS, lavapipe/swrast).
- Nessun costo prestazionale significativo a 800p/60fps sullo Steam Deck.
