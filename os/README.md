# Immagine di sistema di Deckard

Immagine live (ISO avviabile da chiavetta USB) per **Steam Deck LCD**, basata su
Arch Linux e costruita con [archiso](https://wiki.archlinux.org/title/Archiso).
All'avvio entra da sola nella sessione Deckard: niente installazione sul disco
interno.

## Come è fatta

`scripts/build-iso.sh` parte dal profilo ufficiale **releng** di archiso (avvio
BIOS e UEFI già collaudato) e ci applica il contenuto di `os/archiso/`:

| File | Cosa aggiunge |
|---|---|
| `archiso/packages.x86_64` | Pacchetti in più: Deckard, greetd, gamescope, cage, Mesa/RADV, PipeWire, firmware audio del Deck, Bluetooth, core libretro |
| `archiso/profiledef.overrides.sh` | Nome dell'immagine e permessi dei file dell'utente |
| `archiso/airootfs/` | File copiati nell'immagine: greetd, utente `deck` (sysusers), sudo, hostname, configurazione di Deckard |

Il pacchetto `deckard` viene compilato da questo repository
(`packages/deckard/PKGBUILD`) e messo in un repository pacman locale usato solo
durante la build.

**Avvio**: greetd entra come utente `deck` (senza password, è un'immagine live)
e lancia `deckard-session` (`session/`), che avvia Deckard dentro **gamescope**
sul Deck oppure dentro **cage** in una macchina virtuale. Se la sessione si
chiude, su tty1 compare un login testuale per il debug. Il log della sessione è
in `~/.local/state/deckard/session.log`.

**Core**: i core libretro dei pacchetti Arch stanno in `/usr/lib/libretro`;
Deckard li cerca lì dopo la cartella `cores/` dell'utente.

## Costruire l'ISO

**Con GitHub Actions**: scheda *Actions* → *ISO* → *Run workflow* (o push di un
tag `v*`); l'ISO si scarica dagli artifact della run.

**In locale (Arch, anche WSL2)**:

```bash
sudo pacman -S --needed archiso base-devel rust pkgconf git
sudo scripts/build-iso.sh          # risultato in out/
```

## Provarla

**In QEMU** (prima di usarla sul Deck):

```bash
sudo pacman -S --needed qemu-desktop edk2-ovmf
scripts/run-qemu.sh                # usa l'ISO più recente in out/
```

In macchina virtuale Deckard usa la grafica software: è lento ma permette di
verificare avvio, accesso automatico e menu.

**Sul Deck**: scrivi l'ISO su una chiavetta (per esempio con Ventoy, balenaEtcher
o `dd`), spegni il Deck, tieni premuto **Volume giù** e premi **Accensione**
per aprire il Boot Manager, poi scegli la chiavetta. Il disco interno non viene
toccato.

## Da verificare sul Deck

- Orientamento del pannello in gamescope (il pannello LCD è verticale):
  eventuali argomenti extra vanno in `DECKARD_GAMESCOPE_ARGS`
  (`~/.config/deckard/session.env`).
- Controlli integrati (driver `hid-steam`), audio (firmware SOF), Wi-Fi,
  Bluetooth, sospensione.
