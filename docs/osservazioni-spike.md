# Osservazioni dello spike

Prime corse dello spike contro agenti reali, 16 ottobre 2026, su Linux
(CachyOS, cargo 1.99.0, crate `agent-client-protocol` 3.3.0). È la tabella
delle differenze osservate prevista dal criterio di completamento dello
spike in `PLAN.md`: il primo manifest per agente nasce da qui.

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
| Permessi osservati | modifica file e comando bash; opzioni `allow_once`, `allow_always`, `reject_once` (id `once`, `always`, `reject`) | nessuna richiesta per la creazione di un file nel progetto |
| Estensioni `cursor/*` | — | non scattate in queste corse |
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
riporta «Logged in using ChatGPT». Lo spike non arriva a `initialize`,
quindi l'agente non dichiara un metodo di autenticazione.

Una prima coppia di corse si è chiusa subito: l'adattatore rifiutava
`model_reasoning_effort = "max"` in `~/.codex/config.toml`. Quel valore
è stato poi portato a `high` (backup `.bak-20261016-162533`).

Le corse rilanciate, sessione nuova e `--resume`, sul repo
`/tmp/bitbot-spike-codex`, escono ancora prima della sessione. I log
grezzi sono `/tmp/bitbot-codex-new.txt` e `/tmp/bitbot-codex-resume.txt`.
Entrambe stampano `[codex] avvio` e poi:

```text
error loading config: failed to parse model_catalog_json path `/home/hybrys/.codex/models.json` as JSON: unknown variant `max`, expected one of `none`, `minimal`, `low`, `medium`, `high`, `xhigh` at line 7 column 38
```

Alla riga 7 il catalogo ha `default_reasoning_level` uguale a `max` per
il modello `glm-5.3`. Lo stesso valore compare tra i livelli di
ragionamento supportati. L'adattatore 0.16.0 non lo accetta.

| Aspetto | Codex (`codex-acp` 0.16.0) |
| --- | --- |
| Comando d'avvio | `codex-acp` |
| Versione di protocollo | non osservata |
| `session/list` | non osservata |
| `session/resume` | non osservata |
| `session/load` | non osservata |
| Login dichiarato | non osservato: il processo esce durante il caricamento della config |
| Opzioni di sessione | non osservate |
| Effort | non osservato |
| Valori del modello | non osservati |
| Cambio di configurazione | non osservato |
| Permessi osservati | non osservati |
| Estensioni `cursor/*` | — |
| Evento di consumo | non osservato |
| Titolo della sessione | non osservato |

### Claude Code

Nessun processo `claude-agent-acp` è stato avviato. La via ACP di Claude
Code richiede un'API key o un provider cloud: Anthropic vieta l'OAuth
degli abbonamenti nei prodotti di terze parti, compresi quelli basati
sull'Agent SDK (enforcement gennaio-febbraio 2026; sezione Login di
`PLAN.md`). In questa verifica non si usa un'API a consumo. L'uso
dell'abbonamento resta la modalità «terminale gestito + hook», prevista
in una fase successiva.

## Punti aperti

- Le estensioni bloccanti `cursor/ask_question` e `cursor/create_plan`
  hanno l'handler collegato ma non sono mai scattate: compiti che le
  provocano.
- Nessuna richiesta di permesso osservata da Cursor in queste corse.
- Codex: le due corse partono, ma `codex-acp` 0.16.0 non apre una
  sessione finché `~/.codex/models.json` contiene la variante `max`.
- Claude Code: il binario è installato e non è stato avviato. La via
  ACP resta bloccata dalla scelta di non usare un'API a consumo.
- macOS e Windows: corse locali mancanti (la CI copre solo
  fmt, clippy e test).
- Lo spike non stampa la risposta finale dell'agente nel turno di
  ripresa, solo nel turno di verifica del cambio configurazione.
