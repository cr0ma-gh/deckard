# ADR 0005 — Menu rapido in gioco e impostazioni (Fase 2c)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

Durante il gioco servono salvataggi, opzioni del core e uscita senza tastiera
(sul Deck c'è solo il gamepad). Dal menu principale servono le impostazioni
video e la lingua.

## Decisioni

1. **Menu rapido come pannello sopra il gioco in pausa**, aperto e chiuso con
   il tasto Guide/Steam o `F1`. Il core non avanza e l'audio viene messo in
   pausa (`AudioOutput::set_paused`): così non si contano interruzioni finte e
   il gioco riprende esattamente dov'era.
2. **Stessa architettura del menu principale**: modello puro in
   `deckard-ui/src/quick.rs` con test, layout in `layout::game_overlay`, stesso
   renderer (overlay senza sfondo animato) e stesso `Repeater` per l'input.
3. **Voci**: Riprendi, Salva stato, Carica stato, Slot (sinistra/destra),
   Opzioni del core, Riavvia gioco, Chiudi gioco. "Carica stato" e "Riavvia"
   chiudono il menu; "Salva stato" resta aperto e mostra un messaggio.
4. **Opzioni del core**: una pagina con tutte le opzioni dichiarate dal core;
   sinistra/destra/conferma scorrono i valori, che si applicano subito (il
   core le legge al frame successivo) e si salvano all'uscita. Un valore
   cambiato dal menu viene salvato anche se all'avvio era stato passato con `-o`.
5. **Impostazioni nel menu principale**: scala intera, filtro morbido, lingua.
   Si applicano subito e vengono salvate in `config.toml`.
6. **Scala intera sull'altezza**: l'altezza è un multiplo intero di quella del
   frame, la larghezza segue l'aspect ratio del core (pixel verticali nitidi,
   proporzioni corrette); se non entra nemmeno 1× si usa il ridimensionamento
   normale.

## Conseguenze

- Salvare `config.toml` dal menu lo riscrive: i commenti scritti a mano si perdono.
- Le etichette dei valori delle opzioni (definite dal core in v1/v2) non sono
  ancora conservate: si mostrano i valori grezzi (es. `enabled`).
- Il tempo passato nel menu rapido è escluso dagli FPS riportati nel log.
