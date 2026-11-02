# ADR 0004 — Menu XMB disegnato con wgpu e glyphon (Fase 2b)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

Serve un menu in stile XMB (categorie orizzontali, voci verticali, animazioni
fluide), utilizzabile con il gamepad del Deck e con la tastiera su desktop,
tradotto in italiano e inglese.

## Decisioni

1. **Disegno proprio su wgpu**, scelta dell'utente al posto di egui: controllo
   completo su animazioni e aspetto. Tre pipeline semplici (sfondo animato,
   rettangoli arrotondati con SDF, immagini) e il testo con **glyphon 0.12**
   (cosmic-text), che usa la stessa versione di wgpu (30).
2. **Logica separata dal disegno**: `nav` (stato e azioni), `repeat` (tasti
   tenuti → passi con ritardo e ripetizione), `layout` (stato → `Scene`). Sono
   funzioni pure con test di unità; il renderer disegna solo la `Scene`.
3. **Il menu è un overlay del renderer video** (`deckard_video::Overlay`): un
   solo render pass, lo stesso codice potrà disegnare il menu rapido sopra il
   gioco in Fase 2c.
4. **Font Inter** (OFL 1.1, file e licenza in `deckard-ui/assets/fonts/`),
   l'unico caricato: aspetto identico su Windows, Linux e Deck, nessuna
   scansione dei font di sistema. Il font variabile fornisce anche il grassetto.
5. **Icone senza asset**: riquadri colorati con la sigla del sistema e tre
   barre per le Impostazioni (nessun materiale Sony o di terzi).
6. **Testi da file TOML** (`assets/i18n/en.toml`, `it.toml`) inclusi nel
   binario; la lingua si sceglie con `language` in `config.toml`.
7. **Input del menu robusto**: le pressioni vengono memorizzate fino al frame
   successivo (un tocco più breve di un frame non si perde) e i tasti di
   navigazione si leggono dal loro significato, così funzionano anche le frecce
   del tastierino numerico.
8. **Una sola applicazione per menu e gioco**: chiudere un gioco scarica il
   core e torna al menu nello stesso processo (`launcher.rs`).

## Conseguenze

- Nuove dipendenze: `glyphon` (testo), `png` (copertine).
- Le copertine sono solo PNG locali, decodificate in un thread separato.
- Il menu viene ridisegnato a ogni refresh anche quando è fermo: va bene su
  desktop, sul Deck si potrà ridurre il consumo ridisegnando solo durante le
  animazioni (`MenuView::animating`).
