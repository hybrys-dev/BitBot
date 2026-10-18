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
- macOS e Windows: corse locali mancanti (la CI copre solo
  fmt, clippy e test).
- Lo spike non stampa la risposta finale dell'agente nel turno di
  ripresa, solo nel turno di verifica del cambio configurazione.
