# ADR 0011 — Wi-Fi dal menu e tastiera a schermo

- **Data**: 2026-11-03
- **Stato**: accettata

## Contesto

Sul Deck non c'è un cavo di rete: per Samba (6c) e il caricamento dal browser
(6d) serve collegarsi al Wi-Fi dal menu, con il solo controller. Le reti
protette chiedono una password, quindi serve anche una tastiera a schermo.

## Decisioni

1. **NetworkManager** al posto di systemd-networkd + iwd del profilo releng
   (`build-iso.sh` rimuove i loro link e abilita NetworkManager): salva da solo
   le reti e si riconnette all'avvio, e `nmcli` ha un output stabile per gli
   script (`-t`). systemd-resolved resta attivo. Due gestori di rete sulle
   stesse interfacce si darebbero fastidio, quindi ne resta uno solo.
2. **Permessi con polkit, non sudo**: la regola
   `50-deckard-networkmanager.rules` dà al gruppo `wheel` (di cui fa parte
   `deck`) le azioni `org.freedesktop.NetworkManager.*`. Il frontend chiama
   `nmcli` come utente normale.
3. **Backend in `deckard-system/src/wifi.rs`**: comandi `nmcli` (scansione,
   connessione con password, disconnessione) eseguiti su un thread
   (`WifiWorker`), perché una connessione può durare fino a 30 s e il menu non
   deve bloccarsi. Con `DECKARD_FAKE_WIFI=1` si usa un backend simulato (reti
   di prova, password `password`) per provare l'interfaccia su Windows.
4. **Pagina Wi-Fi** (`deckard-ui/src/wifi.rs`, voce in Impostazioni dopo la
   lingua): "Cerca di nuovo", poi le reti ordinate (connessa per prima, poi per
   segnale). Rete protetta → tastiera; rete connessa → disconnetti; rete
   aperta → connetti subito.
5. **Tastiera a schermo** (`deckard-ui/src/keyboard.rs`), riusabile, sul
   modello di quella di SteamOS (provata sul Deck, la prima versione a griglia
   era scomoda). Non esiste una tastiera pronta da riusare: i crate Rust offrono
   solo la navigazione col gamepad, le tastiere Wayland (squeekboard, wvkbd)
   sono solo touch e gamescope non ha i protocolli input-method, e quella di
   Steam esiste solo con Steam in esecuzione. Come è fatta:
   - layout da PC a tutta larghezza agganciato in basso (con Maiusc temporaneo,
     Bloc Maiusc, pagina di accenti e simboli, frecce per il cursore, Chiudi);
   - **touch**: si toccano i tasti (anche col mouse o con il trackpad);
   - **tasti del Deck**: A preme, B chiude, X cancella (si ripete tenendolo
     premuto), Y spazio, L2 Maiusc, R2 o Start Invio, L1/R1 spostano il cursore;
   - con una tastiera fisica si scrive direttamente (Invio conferma, Esc chiude).
   Aspetto ispirato a SteamOS, disegnato da zero senza asset di Valve.

## Conseguenze

- La password digitata viene passata a `nmcli` come argomento: è visibile per
  un istante nell'elenco dei processi. Accettabile su un dispositivo personale
  monoutente; si può passare a `nmcli --ask` o a D-Bus se serve.
- Il test di avvio (`boot-test.sh` con diagnostica) controlla che
  NetworkManager sia attivo e che `deck` abbia i permessi.
- La tastiera servirà anche per le password dell'utente (vedi ADR 0010).
