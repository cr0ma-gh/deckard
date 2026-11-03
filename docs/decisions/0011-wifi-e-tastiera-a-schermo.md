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
5. **Tastiera a schermo** (`deckard-ui/src/keyboard.rs`), riusabile: griglia
   navigabile con croce/stick, pagine minuscole/maiuscole/simboli, tasti
   Maiusc, simboli, spazio, cancella, fatto; B annulla. Con una tastiera fisica
   si può anche scrivere direttamente (Invio conferma, Esc annulla).

## Conseguenze

- La password digitata viene passata a `nmcli` come argomento: è visibile per
  un istante nell'elenco dei processi. Accettabile su un dispositivo personale
  monoutente; si può passare a `nmcli --ask` o a D-Bus se serve.
- Il test di avvio (`boot-test.sh` con diagnostica) controlla che
  NetworkManager sia attivo e che `deck` abbia i permessi.
- La tastiera servirà anche per le password dell'utente (vedi ADR 0010).
