# Osservazioni dello spike

Prime corse dello spike contro agenti reali, 16 ottobre 2026, su Linux
(CachyOS, cargo 1.99.0, crate `agent-client-protocol` 3.3.0). È la tabella
delle differenze osservate prevista dal criterio di completamento dello
spike in `PLAN.md`. Il primo manifest per agente è in
`crates/bitbot-drivers/src/manifest.rs`.

## Metodo

- Repo di prova minimale in `/tmp` con una funzione Python da testare.
- OpenCode è stato avviato con una config minimale (`OPENCODE_CONFIG`)
  con `edit` e `bash` a `ask`, per forzare le richieste di permesso.
- Lo spike chiede permessi e domande su stdin: le corse senza terminale
  usano uno pseudo-TTY (`script -qec … /dev/null`) con le risposte pipate.
- Quattro corse: sessione nuova e ripresa, per ciascun agente.

## Differenze osservate

| Aspetto | OpenCode v2.0.24 | Cursor 2026.10.01 |
| --- | --- | --- |
| Comando d'avvio | `opencode acp` | `cursor-agent acp` (ripiego `agent`) |
| Versione di protocollo | 1 | 1 |
| `session/list` | sì | sì |
| `session/resume` | sì, senza replay: il turno continua la sessione | no |
| `session/load` | sì | sì, con replay: 24 aggiornamenti già in coda |
| Login dichiarato | `opencode-login` («Run `opencode auth login`») | `cursor_login` («Run 'agent login' first») |
| Opzioni di sessione | `model`, `effort` (categoria `thought_level`), `mode` | `mode`, `model` |
| Effort | opzione dedicata: `default → none` e valori successivi | incorporato nel valore del modello, nessuna opzione `thought_level` |
| Valori del modello | `provider/modello` (es. `opencode/nemotron-3.5-super-vl-free`) | stringa composta: `grok-4.7[context=256k,reasoning_effort=high,fast=true]` |
| Cambio di configurazione | `effort=none` applicato e usato nel turno dopo | cambio `model` applicato e usato nel turno dopo |
| Permessi osservati | modifica file e comando bash; opzioni `allow_once`, `allow_always`, `reject_once` (id `once`, `always`, `reject`) | comandi bash (`ls`/`find` e `python -m unittest`) con `allow_once` (id `allow-once`); `sì` pipato sblocca il turno. `ls /usr > /tmp/bitbot-cursor-fuori.txt` non ha chiesto permesso |
| Estensioni `cursor/*` | — | `cursor/create_plan` scattata (piano «CLI somma verbose»): `sì` accettato e il turno è proseguito. `cursor/ask_question` non scattata: la domanda sulla libreria è restata nel messaggio |
| Evento di consumo | presente a fine turno | non visto in queste corse |
| Titolo della sessione | presente nell'elenco (es. «Test unittest funzione somma in calcola.py») | presente (es. «Python Read Function») |

## Prime note per il manifest per agente

- **OpenCode**: sforzo come `thought_level` con pochi valori; permessi
  con id `once`/`always`/`reject` già coperti dal confronto per `kind`.
  La ripresa naturale è `session/resume`; `session/load` c'è comunque.
- **Cursor**: la ripresa passa solo da `session/load` (con replay). Il
  selettore di effort non può usare la categoria ACP dedicata: l'effort
  vive nella stringa composta del modello, quindi il manifest deve
  saperla leggere ed etichettare.

## Codex e Claude Code

Stesso metodo, sempre il 16 ottobre 2026, dopo le corse di OpenCode e
Cursor. Pacchetti installati con `npm -g`: `@zed-industries/codex-acp`
0.16.0 e `@zed-industries/claude-agent-acp` 0.23.1. I binari sono
`codex-acp` e `claude-agent-acp`, come nel profilo. Entrambi i pacchetti
npm sono deprecati a favore di `@agentclientprotocol` con gli stessi
binari; il profilo non è stato cambiato.

### Codex

`codex login` ha completato il login nel browser. `codex login status`
riporta «Logged in using ChatGPT».

Prima di una sessione, due corse si chiudevano al caricamento della
config. I log di quel blocco restano `/tmp/bitbot-codex-new.txt` e
`/tmp/bitbot-codex-resume.txt`.

- `~/.codex/config.toml`: `model_reasoning_effort = "max"` rifiutato.
  Portato a `high` (backup `.bak-20261016-162533`).
- `~/.codex/models.json`: `default_reasoning_level` e un livello
  supportato di `glm-5.3` ancora a `max`. Portati a `xhigh` (backup
  `.bak-20261016-173654`). L'adattatore 0.16.0 accetta solo `none`,
  `minimal`, `low`, `medium`, `high`, `xhigh`.

Dopo il catalogo, sessione nuova e `--resume` sul repo
`/tmp/bitbot-spike-codex` aprono la sessione ed escono con codice 0.
Log: `/tmp/bitbot-codex-new-2.txt` e `/tmp/bitbot-codex-resume-2.txt`.

| Aspetto | Codex (`codex-acp` 0.16.0) |
| --- | --- |
| Comando d'avvio | `codex-acp` |
| Versione di protocollo | 1 |
| `session/list` | sì |
| `session/resume` | sì, senza replay: «nessun aggiornamento già pronto», il turno continua la sessione |
| `session/load` | sì, non usata: la ripresa è passata da `session/resume` |
| Login dichiarato | `chatgpt` («Login with ChatGPT»), `codex-api-key`, `openai-api-key` |
| Opzioni di sessione | `mode`, `model`, `reasoning_effort` (categoria `thought_level`) |
| Effort | opzione dedicata: nella sessione nuova `high → xhigh` |
| Valori del modello | `glm-5.3` |
| Cambio di configurazione | `reasoning_effort=xhigh` applicato e usato nel turno dopo |
| Permessi osservati | nessuna richiesta; la modalità dichiarata è `full-access`. L'agente ha eseguito comandi, tra cui `python -m unittest -v`, senza chiedere |
| Estensioni `cursor/*` | — |
| Evento di consumo | presente durante il turno e subito prima di `finito` |
| Titolo della sessione | presente (es. «Aggiungi un test unittest per la funzione somma in calcola.py») |

Il processo di ripresa è nuovo e rilegge `reasoning_effort=high`: il
valore `xhigh` vale nel turno successivo della stessa sessione, non
risulta nell'opzione riletta al riavvio. Lo spike non stampa la
risposta finale del turno di ripresa.

### Claude Code

Nessun processo `claude-agent-acp` è stato avviato. La via ACP di Claude
Code richiede un'API key o un provider cloud: Anthropic vieta l'OAuth
degli abbonamenti nei prodotti di terze parti, compresi quelli basati
sull'Agent SDK (enforcement gennaio-febbraio 2026; sezione Login di
`PLAN.md`). In questa verifica non si usa un'API a consumo. L'uso
dell'abbonamento resta la modalità «terminale gestito + hook», prevista
in una fase successiva.

## Corse del 18 ottobre 2026: permessi di Codex, Cursor bloccato

Stesso metodo del 16, sempre su Linux. Obiettivo: permessi di Codex
fuori da `full-access` e far scattare `cursor/ask_question`.

### Codex in modalità restrittiva

Ottenuta senza toccare lo spike: `~/.codex/config.toml` con
`approval_policy = "on-request"` e `sandbox_mode = "workspace-write"`
(backup `config.toml.bak-20261018-permessi`, ripristinata a fine corsa).
Repo di prova `/tmp/bitbot-spike-permessi`, sessione nuova, compito con
scrittura di un report dentro il progetto.

- Letture fuori dal progetto (`ls`, `cat` sul vault): nessun permesso.
- Scritture dentro il progetto (`test_calcola.py`, `report.txt`): nessun
  permesso.
- Scrittura fuori dal progetto: `session/request_permission` scattata.
  L'opzione scelta aveva kind `allow_once` e **id `approved`** (OpenCode
  usa `once`, Cursor `allow-once`): gli id non formano una mappa completa.
- L'opzione ACP `mode` («Approval Preset», valore `auto`) non rispecchia
  `sandbox_mode`/`approval_policy` della config: cambiare la modalità via
  ACP non equivale a cambiare la sandbox.
- Cambio `reasoning_effort` `high → xhigh` e verifica nel turno dopo:
  confermati anche in questa corsa.

### Cursor

Il turno termina subito senza chiamare strumenti, con la risposta
«Add a payment method to continue»: l'account non può eseguire compiti
in questo momento (funzionava il 16). `cursor/ask_question` resta non
verificata finché l'account non torna attivo.

### Il blocco di Cursor è un bug noto di fatturazione ACP

Non è l'account: sul forum di Cursor è confermato che l'ACP fattura
male i modelli espliciti e chiede un metodo di pagamento anche con
l'abbonamento a posto («use `auto` as a workaround»). Con il pool
dello X Subscription al 48% l'errore è identico.

- `auto` non è un valore dell'opzione ACP `model`: «Invalid model
  value: auto».
- Lo spike ora stampa i valori dichiarati di ogni selettore
  (`valori=[…]`): il primo valore di Cursor è `default[]`, la stringa
  composta vuota, equivalente di `auto`.
- `--model 'default[]'` prima del primo prompt aggira il bug: il
  turno corre. Il turno di verifica che passa a un modello esplicito
  muore ancora con «Add a payment method to continue».
- `cursor/ask_question` non è scattata nemmeno con l'istruzione
  esplicita di usare «lo strumento di domanda all'utente» (due task
  diversi): la domanda resta nel messaggio dell'agente.

## Punti aperti

- `cursor/ask_question` ha l'handler collegato ma non è scattata:
  né col compito sulla libreria di test, né con l'istruzione esplicita
  di usare lo strumento di domanda con `model=default[]`. La domanda
  resta nel messaggio dell'agente.
- Il comando che scrive fuori dal progetto (`ls /usr` verso
  `/tmp/bitbot-cursor-fuori.txt`) non ha chiesto permesso.
- Claude Code: il binario è installato e non è stato avviato. La via
  ACP resta bloccata dalla scelta di non usare un'API a consumo.
- macOS e Windows: le corse reali del 20 ottobre 2026 hanno coperto
  OpenCode su entrambi i sistemi (macchine virtuali, `docs/corse-sistemi.md`).
  Cursor e Codex restano pilotati solo su Linux: nelle VM Cursor chiede
  un `agent login` interattivo e Codex muore sul provider, per l'account
  ChatGPT Go senza i modelli richiesti e Z.AI con la quota esaurita.
  La CI copre solo fmt, clippy e test.
- Lo spike stampa `risposta agente:` anche nel turno di ripresa
  (19 ottobre 2026, test sul codice puro). Una corsa reale che mostri
  quella riga non è stata rifatta.

Stato al 20 ottobre 2026, sera: la fase 0 resta chiusa con questi punti
come limiti noti accettati, non come fatti. Chiusi stasera con corse
vere: la riga `risposta agente:` del turno di ripresa (portatile e
Windows) e le righe Cursor e Codex su Windows. Restano: le righe di
macOS (Cursor: `agent login` interattivo; Codex: provider), il turno di
verifica del cambio modello di Cursor, fermato dal bug di fatturazione
ACP, `cursor/ask_question` — in attesa del fix di Cursor per la sessione
ACP — e il permesso di Cursor per la scrittura fuori dal progetto. Il
cancello della fase 1 li riprende.

## Verifica del 19 ottobre 2026: ripresa, domanda, scrittura fuori progetto

Linux, senza un agente reale nel `PATH`: non ci sono `cursor-agent` né
`agent`. Nessuna corsa ACP, nessun login toccato, nessuna API a consumo.
Lo schema letto è quello del crate `agent-client-protocol` 3.3.0
(schema ACP 1.11).

### Risposta del turno di ripresa

`prompt_restored` leggeva il turno con `drive_turn` e scartava il
`TurnRecord`. Il turno di verifica del cambio di configurazione stampa
invece `risposta agente:` da `agent_text`, dopo `one_line`. La ripresa
ora usa la stessa riga. Se il testo è vuoto, o solo spazi, non stampa
nulla e non inventa una risposta. Il test
`resume_turn_reply_uses_the_verification_line_and_skips_empty_text`
copre la riga sul codice puro: non avvia un agente e non prova la
ripresa contro un processo vero.

### `cursor/ask_question`

Il client la registra in due punti, insieme a `cursor/create_plan`:

- un handler sulla connessione, prima di `initialize`, per i metodi
  senza `sessionId` (il router di sessione consegna solo i messaggi che
  portano l'id della sessione);
- `print_dispatch`, se la richiesta arriva nel turno.

La risposta serializzata è
`{"outcome":{"outcome":"answered","answers":[{"questionId":"…","selectedOptionIds":["…"]}]}}`,
la forma documentata da Cursor. `cancelled` e `skipped` usano lo stesso
inviluppo. Il 16 ottobre `cursor/create_plan`, sullo stesso handler e
con lo stesso inviluppo `outcome`, è stato accettato e il turno è
proseguito: Cursor non scarta questa forma.

Lo schema non ha una capacità che accende lo strumento. `initialize`
resta senza `fs` e senza `terminal`. `_meta` non è un interruttore di
`AskQuestion`: il 25 settembre 2026 il forum di Cursor ha scritto che
la sessione ACP non offre lo strumento, e che non dipende dalle
capacità mandate dal client. Il client è già corretto. Non si forza
l'evento e non si simula una richiesta.

`cursor-agent` e `agent` non sono nel `PATH` di questa macchina. Non
c'è una corsa con `--model 'default[]'` e non si è visto un prompt di
pagamento.

### Permesso per `ls /usr > /tmp/bitbot-cursor-fuori.txt`

Ogni `session/request_permission` che arriva nel turno passa da
`settle_permission` e viene chiesta all'utente. Il client non legge il
comando, non distingue un percorso fuori dal progetto e non risponde
da solo `cancelled` né `allow_once`. Se il metodo è quello giusto ma il
corpo non si legge, il turno fallisce: non è un permesso concesso in
silenzio.

Il 16 ottobre gli altri comandi bash (`ls`/`find` e
`python -m unittest`) sono arrivati e sono stati chiesti con
`allow-once`. La scrittura fuori dal progetto non ha prodotto
`session/request_permission`: è il comportamento dell'agente, non un
filtro del client. Non si aggiusta. Oggi il binario non c'è, quindi la
corsa non è stata ripetuta.

## Corse del 20 ottobre 2026 su macchine virtuali

Tre VM sul cluster Proxmox, una per sistema: Fedora 44 (GNOME),
Windows 11, macOS Ventura 13.7.8 (OpenCore). Nessun portatile: gli
agenti girano dentro le VM. Gli agenti sono installati da zero su
Windows e macOS e autenticati con le credenziali copiate dalla
macchina di lavoro.

### OpenCode 1.19.3 su Windows e macOS

- Sessione nuova, primo turno e `test_calcola.py` creato su entrambi.
- `thought_level` compare fra le opzioni solo **dopo** il primo turno:
  lo spike sceglie quindi il modello (`big-pickle` →
  `deepseek-v4.1-flash-free`) invece dell'effort. Con OpenCode v2.0.24
  su Linux l'opzione c'era subito. Non è un difetto dello spike: è
  l'ordine con cui l'agente pubblica le opzioni.
- Permesso: con il compito che scrive fuori dal progetto l'agente
  chiede (`in attesa di permesso`), la risposta `si` sblocca e
  `report.txt` viene scritto su entrambi.
- Ripresa: `session/resume` sulla stessa sessione, nessun replay, su
  entrambi.
- Senza un terminale (stdin non TTY) lo spike si ferma al primo
  permesso con `stdin non è un terminale` e non risponde da solo: è il
  comportamento voluto.

### Cursor CLI 2026.10.19 su Windows e macOS

La CLI si installa (Windows: `%LOCALAPPDATA%\cursor-agent`, comandi
`agent.cmd` e `cursor-agent.cmd`; macOS: `~/.local/bin/agent`), ma la
sessione ACP viene rifiutata:

```text
Authentication required: { "message": "Authentication required. Please run 'agent login' first, then call authenticate() with methodId 'cursor_login'." }
```

Su macOS prima ancora: `Your macOS login keychain is locked`. Dopo
`security unlock-keychain` resta l'autenticazione mancante: i file
`~/.cursor/cli-config.json` e `agent-cli-state.json` non bastano, il
login passa da `agent login`, che è interattivo. Nessuna riga Cursor
della matrice è stata percorsa su Windows o macOS.

### `codex-acp` su Windows e macOS

Due blocchi, entrambi sul provider, non sul sistema:

- account ChatGPT Go: `The 'gpt-5.5' model is not supported when using
  Codex with a ChatGPT account`; lo stesso per `gpt-5.1-codex` e
  `gpt-5.1`.
- provider Z.AI: `Weekly/Monthly Limit Exhausted`, e prima ancora il
  catalogo modelli non si decodifica (`unknown variant 'max', expected
  one of none, minimal, low, medium, high, xhigh`).

La sessione ACP parte e il turno muore sulla risposta del provider:
niente sessione completata, niente permessi, niente ripresa.

### Isola su Windows

L'app Tauri compila con MSVC (`bitbot-desktop.exe`) e disegna la
striscia arrotondata traslucida in alto al centro, sopra le altre
finestre: verificato con uno screenshot della console, con PowerShell
aperto sotto. Da una sessione SSH il processo nasce nella sessione 0 e
non si vede: per lanciarlo nella sessione interattiva serve un task
pianificato con principal `Interactive` (`Register-ScheduledTask`),
perché `schtasks /run` su un task creato con `/ru ... /rp ... /it`
risponde `Element not found`.

### Isola su macOS

L'app Tauri compila con i Command Line Tools 14.2 e mostra il pannello
non attivante sopra la barra dei menu (`bitbot-desktop: isola su macOS,
pannello non attivante sopra la barra dei menu`). Da SSH il processo
non ha la sessione grafica: serve `launchctl asuser <uid> sudo -u
<utente>`. La VM non ha notch: il ramo «nessuno notch, striscia sul
bordo alto» è l'unico percorso visibile, il posizionamento attorno al
notch resta non verificato.

### Installazione headless su macOS

Il Setup Assistant si è completato da QMP (`input-send-event` con
coordinate assolute, mai `sendkey` per i clic). I Command Line Tools
sono arrivati da `softwareupdate -i "Command Line Tools for
Xcode-14.2"`: l'estrazione dei pacchetti resta a CPU alta per venti
minuti e sembra ferma, ma finisce. Remote Login si accende da Impostazioni
di Sistema → Generali → Condivisione; da lì tutto il resto passa da
SSH.

## Corse del 20 ottobre 2026, sera: Codex e Cursor su Windows

Sempre nella VM Windows 11, dopo il login di Cursor completato con il
link `loginDeepControl` aperto nel browser dell'utente (la CLI stampa
il link e attende: nessun browser nella VM). Z.AI, alla mattina con la
quota esaurita, la sera rispondeva.

### Codex

- Il catalogo decide le opzioni dichiarate all'avvio. Senza
  `models.json` copiato e senza `model_catalog_json` in cima a
  `config.toml`, `codex-acp` dichiara solo `model` con i valori `[glm-5.3
  gpt-5.5 gpt-5.4 …]`: lo spike cambia quindi il modello, `gpt-5.5`
  non è servito da Z.AI e il turno muore con «Unknown Model».
- Con `models.json` copiato e `model_catalog_json` in cima (una riga
  messa dopo una sezione `[model_providers.ZAI]` appartiene a quella
  sezione: Codex non la legge e il provider risulta senza nome), le
  opzioni al via sono `model` con `valori=[glm-5.3]` e
  `reasoning_effort` (`low high xhigh`): lo spike cambia `high →
  xhigh` e il turno di verifica finisce.
- Con `approval_policy = on-request` e sandbox `workspace-write`, ogni
  comando PowerShell chiede permesso, anche le letture fuori dal
  progetto: su Linux, il 18, chiedeva solo la scrittura fuori. Le
  risposte `si` (allow_once) sbloccano il turno.
- Ripresa: `session/resume` senza replay, risposta finale del turno
  stampata (`risposta agente:`).

### Cursor

- Login completato via link nel browser: «Logged in», token salvati.
  Con `--model 'default[]'` la sessione parte e il turno lavora: il
  permesso per `py -m unittest test_calcola.py -v` arriva e la
  risposta `si` sblocca.
- Il cambio di modello è applicato (`muse-wonder-1.0[…]` →
  `gpt-5.6-sol[…]`, il valore successivo dichiarato) ma il turno di
  verifica chiude subito con «Add a payment method to continue»: il
  bug noto di fatturazione ACP, già visto su Linux il 18.
- Ripresa: il turno continua la sessione, sei permessi risposti (tra
  cui `winget install Python`, proposto dall'agente per eseguire i
  test), risposta finale stampata.
- `cursor/ask_question` non è scattata nemmeno con il login attivo:
  coerente con quanto scritto dal forum di Cursor, lo strumento non è
  offerto dalla sessione ACP.

### Metodo per i permessi

Su Windows la risposta ai permessi passa da una finestra console
interattiva avviata da task pianificato, con l'auto-risponditore
PowerShell descritto in `docs/corse-sistemi.md`: da SSH lo spike
rifiuta stdin non terminale e `ssh -tt` non basta.
