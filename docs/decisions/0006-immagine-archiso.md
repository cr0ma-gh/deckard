# ADR 0006 — Immagine live con archiso, greetd e gamescope (Fase 3)

- **Data**: 2026-11-02
- **Stato**: accettata (build e test sul Deck ancora da verificare)

## Contesto

Deckard deve avviarsi da chiavetta sullo Steam Deck LCD ed entrare da solo nel
menu. Lo sviluppatore lavora su Windows, senza WSL né Docker installati.

## Decisioni

1. **Profilo `releng` di archiso come base**, modificato dallo script di build
   invece di copiarlo nel repository: avvio BIOS/UEFI e bootloader restano
   quelli collaudati da Arch, e nel repository c'è solo ciò che è di Deckard.
   Il costo è un'immagine più grande (releng include strumenti di sistema); si
   potrà alleggerire più avanti.
2. **Pacchetto `deckard` compilato dal checkout** (PKGBUILD in
   `packages/deckard/`) e servito da un repository pacman locale durante la
   build: l'ISO contiene sempre il codice che si sta costruendo.
3. **Build sia in CI sia in locale** (scelta dell'utente): workflow `ISO` su
   GitHub Actions in un container Arch privilegiato, e lo stesso script in WSL2.
   Il repository è pubblico su GitHub (`cr0ma-gh/deckard`).
4. **greetd** con `initial_session` per l'accesso automatico dell'utente `deck`,
   creato con systemd-sysusers (eseguito da archiso durante l'installazione dei
   pacchetti). Se la sessione termina resta un login testuale per il debug.
5. **gamescope** come compositor sul Deck (scelta dell'utente): lo stesso di
   SteamOS e della futura modalità Steam (Fase 4), gestisce pannello, vsync e
   scaling. **cage** solo in macchina virtuale, dove gamescope non trova una
   GPU Vulkan; Deckard usa lì la Vulkan software di Mesa (`vulkan-swrast`).
6. **Solo core pacchettizzati da Arch** come predefiniti (nestopia,
   snes9x, gambatte, mgba, genesis_plus_gx, mednafen_psx): l'immagine funziona
   senza scaricare nulla. fceumm e pcsx_rearmed non sono nei repository Arch.
7. **Kernel Arch standard** (come previsto in CLAUDE.md); il kernel Valve si
   valuterà solo davanti a problemi hardware concreti.

## Conseguenze

- L'utente `deck` non ha password e può usare `sudo` senza password: va bene
  per un'immagine live, non per un sistema installato.
- Orientamento del pannello, controlli, audio e Wi-Fi sul Deck vanno verificati
  sull'hardware: in CI e in QEMU si verificano solo build, avvio e menu.
- Nessuna rete configurata da interfaccia: per ora resta quella di releng
  (systemd-networkd e iwd).
