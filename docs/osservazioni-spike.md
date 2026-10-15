# Osservazioni dello spike sui primi due agenti

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

## Punti aperti

- Le estensioni bloccanti `cursor/ask_question` e `cursor/create_plan`
  hanno l'handler collegato ma non sono mai scattate: compiti che le
  provocano.
- Nessuna richiesta di permesso osservata da Cursor in queste corse.
- `codex-acp` e `claude-agent-acp` non installati su questa macchina:
  le loro corse mancano.
- macOS e Windows: corse locali mancanti (la CI copre solo
  fmt, clippy e test).
- Lo spike non stampa la risposta finale dell'agente nel turno di
  ripresa, solo nel turno di verifica del cambio configurazione.
