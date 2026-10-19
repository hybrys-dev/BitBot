# BitBot

Companion desktop che pilota agenti di coding (Claude Code, Codex, OpenCode, Cursor, OpenClaw, Hermes) e una chat con qualsiasi LLM.
Il piano completo è in `docs/PLAN.md`: leggi la sezione pertinente prima di pianificare un compito.

## Stato attuale
- Fase 0, Fondamenta, ancora aperta. Lo spike ACP (`crates/bitbot-drivers`) dichiara quattro profili (OpenCode, Cursor, Codex, Claude Code); il crate dello stato unificato è `crates/bitbot-protocol`. Il codice distingue Claude solo per il comando di avvio (`claude-agent-acp`, senza ripiego). La corsa reale ACP è esclusa da questa fase per la scelta «nessuna API a consumo». L'abbonamento resta il terminale gestito e gli hook, già previsti dopo.
- Provato contro agenti reali su Linux il 16 ottobre 2026. OpenCode e Cursor: sessione nuova, permessi, cambio di configurazione e ripresa. Codex, dopo `codex login` e la correzione del catalogo modelli: sessione nuova, cambio di `reasoning_effort` e ripresa. Claude Code non è stato avviato. Le differenze osservate sono in `docs/osservazioni-spike.md`.
- Il 18 ottobre 2026 Codex ha chiesto il suo primo permesso: in `workspace-write` con `approval_policy = on-request` (config temporanea, ripristinata) la scrittura fuori dal progetto chiede `session/request_permission`, opzione `allow_once` con id `approved`; letture e scritture interne no. L'opzione ACP `mode` («Approval Preset») non rispecchia la sandbox della config. Nella stessa data Cursor non eseguiva compiti («Add a payment method to continue»): è un bug noto di fatturazione ACP sui modelli espliciti; lo spike lo aggira con `--model 'default[]'` (valore che l'agente dichiara) e ora stampa i valori dichiarati di ogni selettore. `cursor/ask_question` resta non scattata anche con istruzione esplicita.
- Il 19 ottobre 2026, su Linux (Ubuntu 24.04 di questo ambiente), `claude-agent-acp` non è nel PATH. Lo spike non è stato avviato: non c'è un errore reale da registrare e nessuna corsa con un provider.
- Il primo manifest per agente è in `crates/bitbot-drivers` (`manifest.rs`): solo stranezze osservate. La voce di Claude Code resta «non osservato» (via ACP esclusa da questa fase: nessuna API a consumo), senza effort né permessi inventati.
- Mancano ancora, e tengono aperta la fase 0: `cursor/ask_question` non scattata, ora bloccata anche dal pagamento richiesto da Cursor; corse su macOS e Windows. La corsa reale ACP di Claude Code non è un residuo di questa fase.
- La CI esiste dal 2026-10-15 (PR #7) e gira su Linux, macOS e Windows (fmt, clippy, test).
- Il demone `bitbotd` (`crates/bitbot-core`) serve JSON-RPC 2.0 una riga per messaggio su socket Unix 0600 (named pipe su Windows, via `interprocess`). Oltre a `version`, `session/start` avvia una sessione ACP in memoria (profilo, cartella, prompt, modello opzionale) e pubblica `session/event` con lo stato unificato e `session/request` per permesso, domanda e piano; `session/respond` risponde, anche con `cancelled`. Se non resta nessun client collegato, la richiesta viene annullata così l'agente non resta fermo. Niente indice, ripresa al riavvio né supervisione: se il demone cade, la sessione cade con lui. Il 18 ottobre 2026, su Linux (Ubuntu di questo ambiente), `bitbotd` ha portato OpenCode 2.0.32 da `avvio` a `finito` su un repo di prova: il test unittest generato passava. In quella corsa non è arrivata nessuna `session/request`. La scrittura è stata tradotta come `modifica … +0 −0`, senza un diff nel tool call. Permessi, domande e piani del demone restano provati solo con l'agente finto. Sul portatile CachyOS non è stata fatta questa corsa.
- Non esistono ancora interfaccia, gateway LLM e integrazioni.
- Aggiorna questa sezione quando un cancello della roadmap viene superato. Non scrivere che qualcosa funziona se non è stato provato.

## Principi non negoziabili
- Il demone `bitbotd` possiede processi, stato e chiavi; UI e CLI sono solo client.
- I segreti non arrivano mai alla UI e non finiscono nei log o nei commit.
- Selettori di modello ed effort costruiti da ciò che l'agente dichiara, mai liste fisse.
- Ogni evento degli agenti passa dal modello di stato unificato in `bitbot-protocol`.
- Nessun nome di agente scritto a mano nel codice generico: le differenze tra agenti stanno in profili o moduli dedicati, come `cursor.rs`.
- Ogni richiesta di un agente (permesso, domanda, piano) riceve una risposta, anche `cancelled`: altrimenti l'agente resta bloccato.
- Codice specifico per sistema operativo dietro `cfg`, sempre con un ripiego.
- Zero telemetria.

## Verifica, obbligatoria prima di dire «fatto»
```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Modo di lavorare
- Un compito per volta, in un branch e in una PR dedicati. Parti da `main` e apri la PR verso `main`: niente PR impilate.
- Fai solo il compito richiesto: niente refactor o dipendenze non discusse. Se una decisione non è nel piano, fermati e chiedi.
- Nel corpo della PR spunta solo le verifiche eseguite davvero e scrivi in chiaro cosa non hai potuto verificare (agente reale, altri sistemi operativi).
- Commenti di documentazione, messaggi per l'utente, commit e PR in italiano; identificatori e nomi dei test in inglese.
