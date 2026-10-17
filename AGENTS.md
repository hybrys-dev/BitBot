# BitBot

Companion desktop che pilota agenti di coding (Claude Code, Codex, OpenCode, Cursor, OpenClaw, Hermes) e una chat con qualsiasi LLM.
Il piano completo è in `docs/PLAN.md`: leggi la sezione pertinente prima di pianificare un compito.

## Stato attuale
- Fase 0, Fondamenta. Lo spike ACP (`crates/bitbot-drivers`) dichiara quattro profili (OpenCode, Cursor, Codex, Claude Code); il crate dello stato unificato è `crates/bitbot-protocol`.
- Provato contro agenti reali su Linux il 16 ottobre 2026. OpenCode e Cursor: sessione nuova, permessi, cambio di configurazione e ripresa. Codex, dopo `codex login` e la correzione del catalogo modelli: sessione nuova, cambio di `reasoning_effort` e ripresa. Nessuna richiesta di permesso, con modalità `full-access`. Claude Code non è stato avviato. Le differenze osservate sono in `docs/osservazioni-spike.md`.
- Il primo manifest per agente è in `crates/bitbot-drivers` (`manifest.rs`): solo stranezze osservate il 16 ottobre 2026. La voce di Claude Code è «non osservato» (via ACP bloccata dal vincolo API key).
- Mancano ancora: le corse reali ACP di Claude Code, bloccate dal vincolo API key (scelta: nessuna API a consumo); `cursor/ask_question` non scattata; corse su macOS e Windows.
- La CI esiste dal 2026-10-15 (PR #7) e gira su Linux, macOS e Windows (fmt, clippy, test).
- Non esistono ancora demone, interfaccia, gateway LLM e integrazioni.
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
