# Verifica del nome BitBot

Verifiche fatte il 19 ottobre 2026, prima dell'uscita pubblica richiesta dalla fase 0. Nessuna di queste è una pratica di registrazione: è un rilievo di ciò che esiste oggi.

## Esiti

| Dove | Esito | Dettagli |
| --- | --- | --- |
| crates.io | libero | `bitbot`, `bit-bot` e `bitbot-core` non esistono. Solo match fuzzy non attinenti: `bitbot_bsp` (BSP per il robot educativo 4tronix Bit:Bot XL su micro:bit, v0.0.1 del 2024) e `bitbottle` (formato archivio) |
| GitHub | occupato da progetti minori, nessuno nel settore | Il più noto è `bitbot-irc/bitbot` (ex `jesopo/bitbot`), bot IRC in Python: 148★, ultimo push gennaio 2025, semi-dormiente, possiede bitbot.dev. Altri: bot Telegram/Discord/Slack sotto le 15★, un bot crypto fermo al 2017, i Bit-Bots di Amburgo (RoboCup, 86★, nome diverso) |
| npm | occupato, non ripubblicabile | `bitbot` esiste ma è un security holding package: npm ha ritirato il nome (gennaio 2026) e non consente di ripubblicarlo. Rilevante solo se un domani servirà un pacchetto npm con questo nome |
| Domini | tutti i principali presi o in vendita | bitbot.com preso nel 2001 (Bitbot, Inc., agenzia creativa); bitbot.dev è del bot IRC; bitbot.app e bitbot.ai parcheggiati; bitbot.gg in vendita su Atom; bitbot.tg è un bot crypto attivo; bitbot.io probabilmente libero (RDAP vuoto, non confermato) |
| Marchi | nessuna registrazione attiva trovata nel software | USPTO: solo «Bit Bot Media LLC» (fumetti/media), abbandonata ottobre 2025. EUIPO: nessun riscontro tramite ricerca, registro non consultato direttamente. Il player commerciale più visibile col nome è il bot crypto Bitbot (bitbot.tg, token $BITBOT), settore lontano dai developer tool |

## Decisione

Il nome non è libero, ma nessuno degli occupanti opera nei developer tool desktop. Per la fase personale BitBot resta il nome: il crate Rust su crates.io è pubblicabile senza conflitti.

Prima dell'uscita open source serve però una scelta dedicata: il nome pubblico non avrà né un dominio naturale (tutti i principali presi o in vendita), né il pacchetto npm. Quella scelta, con l'eventuale riversamento del crate, è rimandata a quando l'uscita si profilerà, come già prevede la tabella dei rischi.
