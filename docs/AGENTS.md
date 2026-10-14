# BitBot

Companion desktop che pilota agenti di coding e una chat LLM.
Il piano completo è in docs/PLAN.md: leggi la sezione pertinente prima di pianificare.

## Principi non negoziabili
- Il demone `bitbotd` possiede processi, stato e chiavi; UI e CLI sono solo client.
- I segreti non arrivano mai alla UI.
- Selettori di modello ed effort costruiti da ciò che l'agente dichiara, mai liste fisse.
- Ogni evento degli agenti passa dal modello di stato unificato in `bitbot-protocol`.
- Codice specifico per sistema operativo dietro `cfg`, sempre con un ripiego.
- Zero telemetria.

## Verifica, obbligatoria prima di dire "fatto"
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

## Modo di lavorare
- Fai solo il compito richiesto: niente refactor o dipendenze non discusse.
- Se una decisione non è nel piano, fermati e chiedi.
