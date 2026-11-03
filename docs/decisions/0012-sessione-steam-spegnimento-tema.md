# ADR 0012 — Sessione Steam robusta, spegnimento dal menu, tema Steam Deck

- **Data**: 2026-11-03
- **Stato**: accettata

## Contesto

Provando l'immagine sul Deck sono emersi tre punti:

1. passando a Steam si vedeva l'aggiornamento del client, poi si tornava al
   login testuale;
2. mancava un modo per spegnere o riavviare il dispositivo dal menu;
3. i colori dell'interfaccia (blu-viola) stonavano con il resto del Deck.

## Decisioni

1. **Steam in un ciclo dentro gamescope** (`session/deckard-steam-session`).
   Dopo un aggiornamento il client di Steam si chiude e si aspetta che la
   sessione lo riavvii, come fa SteamOS. Prima la sua uscita chiudeva anche
   gamescope e, senza un file `next-session`, `deckard-session` lasciava il
   posto al login testuale. Ora:
   - Steam viene riavviato finché qualcuno non chiede un cambio di sessione;
   - dopo tre uscite di fila entro 20 s si torna a Deckard;
   - se la sessione Steam finisce senza una richiesta si torna comunque a
     Deckard, non al login.
   gamescope parte con `-e` (integrazione con Steam: focus, overlay, tastiera
   di Steam).
2. **`steamos-session-select`**: con `-steamos3` la voce "Passa al desktop" del
   menu di Steam esegue questo comando; quello di Deckard scrive
   `next-session = deckard` e chiude Steam, quindi si torna a Deckard.
3. **Riavvia / Spegni** in fondo alle Impostazioni, con `systemctl reboot` e
   `systemctl poweroff` (logind li permette all'utente della sessione locale
   attiva, senza password). Le voci compaiono solo su Linux dentro la sessione
   Deckard (`deckard-session` esporta `DECKARD_SESSION=1`), così una prova sul
   PC di sviluppo non lo spegne.
4. **Tema Steam Deck**: tutti i colori dell'interfaccia stanno in
   `deckard-ui/src/theme.rs`:
   - sfondo blu-grigio scuro con l'onda in blu Steam (`render/ui.wgsl`);
   - categoria e voce selezionate in blu Steam (`#1A9FFF`);
   - pannelli grigio scuro, testi in grigi chiari.
   I colori sono ispirati a quelli di SteamOS, senza asset o loghi di Valve. Il
   colore per sistema (`Category::color`) resta nel modello ma non viene più
   disegnato.

## Conseguenze

- Il ciclo di Steam e "Passa al desktop" vanno verificati sul Deck: in QEMU la
  sessione usa cage, dove Steam non parte.
- Le nuove voci e i nuovi colori si cambiano in un solo punto (`theme.rs`).
