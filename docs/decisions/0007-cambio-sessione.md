# ADR 0007 — Cambio sessione tra Deckard e Steam (Fase 4)

- **Data**: 2026-11-02
- **Stato**: accettata

## Contesto

L'utente vuole poter passare dal frontend Deckard alla modalità Steam Big
Picture e viceversa, senza riavviare il sistema. Il meccanismo deve funzionare
sullo Steam Deck LCD con gamescope come compositor per entrambe le sessioni.

## Decisioni

1. **Protocollo basato su file**: il frontend (Deckard o Steam) scrive il nome
   della prossima sessione (`deckard` o `steam`) in
   `~/.local/state/deckard/next-session` ed esce. Un file assente o vuoto
   significa `deckard` (la sessione predefinita).

2. **Session manager a loop**: `deckard-session` passa da un singolo `exec` a
   un ciclo `while true` che a ogni iterazione:
   - legge `next-session` (default: `deckard`);
   - rimuove il file prima di lanciare la sessione;
   - avvia il processo e attende la sua uscita;
   - se `next-session` non esiste dopo l'uscita (nessuno l'ha riscritto),
     termina — greetd mostra il login testuale di debug.
   Questo evita riavvii infiniti in caso di crash.

3. **Crate `deckard-system`**: nuovo crate con funzioni di integrazione OS,
   inizialmente solo il cambio sessione. `request_session_switch(SessionTarget)`
   scrive il file con scrittura atomica. Su piattaforme diverse da Linux viene
   registrato un log di avviso che segnala l'assenza del session manager.
   I test unitari verificano la scrittura in una cartella temporanea.

4. **Voce di menu "Passa a Steam"**: nel menu Impostazioni, sotto "Esci da
   Deckard". L'azione `SwitchSession` chiama `deckard-system` e poi chiude
   l'applicazione. Su desktop (Windows/macOS) la voce non appare nel menu
   perché il cambio sessione non è disponibile.

5. **Ritorno da Steam**: script `switch-to-deckard.sh` installato in
   `/usr/bin/` che scrive `next-session=deckard` e chiude Steam con
   `steam -shutdown`. L'utente lo aggiunge alla libreria Steam come gioco
   non-Steam ("Torna a Deckard").

6. **Pacchetto Steam**: `steam` dal repository `multilib` in `packages.x86_64`,
   con `multilib` abilitato nello script di build. Steam scarica il client a
   runtime: nessun componente proprietario ridistribuito.

## Conseguenze

- Il cambio sessione non è istantaneo: la sessione corrente esce e la nuova
  parte (qualche secondo, dominato dall'avvio di Steam o Deckard).
- In caso di crash il ciclo non riparte (file assente): l'utente trova il login
  testuale e può diagnosticare.
- La voce "Passa a Steam" appare solo se Steam è installato sul sistema;
  in pratica è presente sull'immagine live (che include il pacchetto) e assente
  sui desktop di sviluppo.
