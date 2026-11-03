# ADR 0010 — Installer su disco intero

- **Data**: 2026-11-03
- **Stato**: accettata

## Contesto

L'immagine live perde tutto al riavvio. Per usare Deckard ogni giorno (giochi,
salvataggi, configurazione) serve installarlo su un disco del Deck.

## Decisioni

1. **Disco intero a scelta** (scelta dell'utente): SSD interno o microSD, che
   viene cancellato completamente. Niente dual boot con SteamOS: ridimensionare
   le sue partizioni è rischioso e gli aggiornamenti di SteamOS possono
   romperlo. Scegliendo la microSD, SteamOS sull'SSD resta intatto.
2. **Backend in bash, `installer/deckard-install`** (installato in
   `/usr/bin`), eseguito come root con `sudo -n`:
   - `list` elenca solo dischi ≥ 16 GB, non montati, diversi dal supporto di avvio;
   - `install <disco> --yes` rifiuta qualsiasi disco non elencato e scrive
     righe `PROGRESS <percento> <passo>`, poi `DONE` o `ERROR <messaggio>`.
3. **Copia del sistema live** invece di reinstallare i pacchetti: GPT con ESP
   da 1 GiB (FAT32) e root ext4 (scelta dell'utente), `rsync` del sistema
   dell'immagine (`/run/archiso/airootfs`), rimozione dei file che servono solo
   in live (elenco preso dal profilo releng: journal volatile, accesso root SSH,
   servizi del CD live, hook archiso…), initramfs rigenerato, `fstab`,
   systemd-boot (con l'ESP montata privata), chiavi di pacman. Funziona offline
   e il sistema installato è identico a quello provato in live.
4. **Interfaccia nel menu** (scelta dell'utente): "Installa su disco" compare in
   Impostazioni solo sull'immagine live. Modello puro in
   `deckard-ui/src/installer.rs`: scelta del disco → **due conferme** →
   avanzamento → riavvio. Durante l'installazione il dialogo non si chiude.
   Il processo è seguito da `deckard-system/src/installer.rs` su un thread.
5. **Test end-to-end in QEMU** (`scripts/install-test.sh`, job `install-test`
   del workflow ISO): installa su un disco virtuale vuoto, spegne, riavvia dal
   solo disco e verifica che Deckard sia in esecuzione.

## Aggiornamento: copytoram

Sul Deck la prima installazione si fermava a "Configuring the system". Con
abbastanza RAM archiso copia il sistema in memoria (`copytoram=auto`) e
smonta la chiavetta, quindi `/run/archiso/bootmnt` non esiste più. Le
conseguenze erano due:

- il kernel non si trovava più sulla chiavetta;
- la chiavetta stessa compariva tra i dischi installabili.

In QEMU non succedeva perché l'ISO partiva come CD con 4 GB. Correzioni:

- il kernel si prende da `/usr/lib/modules/<versione>/vmlinuz` del sistema copiato;
- il disco di avvio si riconosce anche da `archisosearchuuid` sulla riga di comando del kernel;
- gli errori riportano l'ultima riga di `/var/log/deckard-install.log`;
- il test di installazione avvia l'ISO da una chiavetta USB virtuale con 6 GB,
  così `copytoram` scatta come sul Deck, e fallisce se la chiavetta viene
  offerta come disco.

## Conseguenze

- Il sistema installato ha, come la live, l'utente `deck` senza password con
  `sudo` libero e root senza password sulla console: le password arriveranno con
  la tastiera a schermo (necessaria anche per il Wi-Fi).
- Nessun aggiornamento automatico: il sistema installato si aggiorna con pacman
  (il pacchetto `deckard` viene dal repository locale usato in build, quindi
  per ora gli aggiornamenti di Deckard si fanno reinstallando).
