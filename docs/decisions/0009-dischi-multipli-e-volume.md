# ADR 0009 — Gestione dischi multipli e controllo volume (Tappa 5b)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

Diversi titoli emulati (specialmente per PlayStation 1, Sega Saturn e Sega CD)
sono distribuiti su più CD-ROM e richiedono il cambio del disco durante la partita
(es. *"Inserire il Disco 2 per continuare"*). Inoltre, durante il gioco, l'utente
desidera poter regolare il volume audio senza dover uscire dall'emulazione.

## Decisioni

1. **Supporto Libretro Disk Control Interface**:
   `deckard-core` gestisce sia `RETRO_ENVIRONMENT_SET_DISK_CONTROL_INTERFACE` che
   `RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE`. Quando un core registra una
   delle interfacce, Deckard memorizza i puntatori e offre un'API sicura per:
   - leggere il numero di dischi (`disk_count`) e il disco corrente (`current_disk`);
   - espellere o inserire il carrello virtuale (`set_eject_state`);
   - cambiare disco (`change_disk`), gestendo la sequenza richiesta da libretro
     (apertura carrello -> selezione indice -> chiusura carrello).

2. **Verifica tramite il core di test (`deckard-test-core`)**:
   Il core di test registra un'interfaccia a 2 dischi (Disco 1 e Disco 2). Un test
   di integrazione verifica l'espulsione, il cambio indice e il recupero delle etichette.

3. **Menu rapido in gioco dinamico**:
   - La voce *"Disco"* appare nel menu rapido **solo** se il gioco corrente espone
     un'interfaccia a più dischi (`disk_count > 1`). Se il gioco è a disco singolo,
     la voce non compare per mantenere il menu pulito.
   - La voce *"Volume"* consente la regolazione a passi del 10% (0%–100%), applicata
     immediatamente al buffer audio in `deckard-audio` e salvata in `config.toml`.

## Conseguenze

- I giochi multidisco possono essere giocati fino in fondo senza ricorrere a complessi
  workaround manuali.
- Il controllo del volume è integrato sia nelle impostazioni che nel menu rapido.

## Note di implementazione (revisione)

- Deckard risponde `1` a `RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION`:
  i core reali (es. mednafen_psx) chiedono la versione prima di scegliere
  l'interfaccia, e senza questa risposta registrerebbero quella base, senza
  etichette dei dischi. Il core di test ora fa la stessa negoziazione, così il
  test di integrazione la copre.
- Le funzioni dell'interfaccia dischi vengono chiamate dentro `Core::with_core`,
  perché il core può richiamare l'environment durante un cambio disco.
- Il volume salvato in `config.toml` deve stare tra 0 e 100 (altrimenti la
  configurazione viene rifiutata); dal menu Impostazioni il volume passa al
  multiplo di 10 successivo e da 100 torna a 0 (`next_volume_step`).
