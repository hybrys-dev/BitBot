# BitBot

Companion desktop che pilota agenti di coding (Claude Code, Codex, OpenCode, Cursor, OpenClaw, Hermes) e una chat con qualsiasi LLM.
Il piano completo è in `docs/PLAN.md`: leggi la sezione pertinente prima di pianificare un compito.

## Stato attuale
- Fase 0, Fondamenta. Esiste solo uno spike ACP che pilota OpenCode (`crates/bitbot-drivers`) e il crate dello stato unificato (`crates/bitbot-protocol`).
- Lo spike non è mai stato eseguito contro un agente reale e non c'è ancora una CI: non scrivere che qualcosa funziona se non è stato provato.
- Non esistono ancora demone, interfaccia, gateway LLM e integrazioni.
- Aggiorna questa sezione quando un cancello della roadmap viene superato.

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
