# Corse manuali su macOS e Windows

Il criterio «tutto funziona su macOS, Windows e Linux» dello spike in
`docs/PLAN.md` non è spuntato. La CI (`.github/workflows/ci.yml`) esegue
`cargo fmt`, clippy e `cargo test` su `ubuntu-latest`, `macos-latest` e
`windows-latest`. Non avvia agenti. Le corse reali ACP e il giro di
`bitbotd` contro un agente vero sono stati fatti solo su Linux.

Questo file è la lista da eseguire a mano. Ogni riga dice dove è già
stata fatta. macOS e Windows restano da fare: nessuno di questi comandi
è stato lanciato su quelle macchine da chi ha scritto il file.

I test automatici, senza agenti reali, coprono il resto:

- due cartelle diverse tengono due socket aperti insieme
  (`two_directories_accept_connections_at_the_same_time`);
- su Unix il socket è `0600`;
- su Windows il nome della pipe non contiene `\` dopo `\\.\pipe\` e
  cambia con la cartella;
- lo shim `.cmd` / `.bat` entra nel comando di avvio, `.exe` resta il
  nome nudo (`windows_spawn_name_prefers_exe_then_cmd_then_bat`, e su
  Windows `windows_cmd_shim_is_the_spawn_name_and_exe_wins`);
- `prepare_cwd` toglie il prefisso `\\?\` che `canonicalize` aggiunge
  su Windows.

L'agente finto di `crates/bitbot-core/tests/api.rs` resta `#[cfg(unix)]`:
è uno script `#!/usr/bin/python3`. Su Windows quella corsa non parte.

Claude Code non è in questa lista: la via ACP non è stata avviata, per
il vincolo sull'API key (`docs/osservazioni-spike.md`).

## Prima di ogni corsa

Dalla radice del repository, con l'agente già installato e autenticato
(`opencode`, `cursor-agent` o `agent`, `codex-acp`). Su Windows uno shim
npm è `nome.cmd`: lo spike lo risolve da solo.

```bash
cargo build -p bitbot-drivers -p bitbot-core
```

Binari:

| | macOS | Windows |
| --- | --- | --- |
| spike | `./target/debug/bitbot-spike` | `.\target\debug\bitbot-spike.exe` |
| demone | `./target/debug/bitbotd` | `.\target\debug\bitbotd.exe` |

Uso dello spike, uguale sui due sistemi:

```text
bitbot-spike [--agent <nome>] [--model <valore>] [--resume] <cartella> <compito>
```

Senza `--agent` il profilo è `opencode`. Permesso, domanda e piano non
sono flag: durante la corsa il terminale chiede `permesso? [sì/no/sempre]`
oppure `sì/no` per il piano. La risposta `sì` sblocca il turno. Il cambio
di effort o di modello avviene nello stesso processo, dopo il primo
turno: non c'è un secondo comando.

Repo di prova, una funzione Python come il 16 ottobre 2026.

macOS:

```bash
REPO="$(mktemp -d /tmp/bitbot-spike.XXXXXX)"
cat > "$REPO/calcola.py" <<'EOF'
def somma(a, b):
    return a + b
EOF
```

Windows (PowerShell):

```powershell
$repo = Join-Path $env:TEMP "bitbot-spike"
New-Item -ItemType Directory -Force -Path $repo | Out-Null
@'
def somma(a, b):
    return a + b
'@ | Set-Content -Encoding utf8 (Join-Path $repo "calcola.py")
```

Il compito della sessione nuova è quello che ha dato i titoli registrati
nelle osservazioni: `Aggiungi un test unittest per la funzione somma in calcola.py`.
Il testo preciso dei prompt di ripresa non è archiviato: sotto c'è la
forma del comando, con un compito di continuazione.

## OpenCode

Per vedere un permesso, come su Linux: variabile `OPENCODE_CONFIG` con
edit e bash a `ask` (`docs/osservazioni-spike.md`, metodo del 16 ottobre).
Il file di quella corsa non è nel repository. Lo schema che corrisponde
a quella frase è:

```json
{
  "permission": {
    "edit": "ask",
    "bash": "ask"
  }
}
```

```bash
export OPENCODE_CONFIG="$REPO/opencode.json"
```

```powershell
$env:OPENCODE_CONFIG = Join-Path $repo "opencode.json"
```

Se la versione installata rifiuta la chiave `permission`, la
documentazione di OpenCode 2 usa `permissions` con l'azione `shell` al
posto di `bash`. Va adattato prima di leggere il risultato: non è stato
riverificato qui.

| Passo | macOS | Windows | Linux | macOS | Windows |
| --- | --- | --- | --- | --- | --- |
| Sessione nuova e prompt | `./target/debug/bitbot-spike --agent opencode "$REPO" "Aggiungi un test unittest per la funzione somma in calcola.py"` | `.\target\debug\bitbot-spike.exe --agent opencode $repo "Aggiungi un test unittest per la funzione somma in calcola.py"` | fatto (`docs/osservazioni-spike.md`, 16 ottobre 2026, OpenCode v2.0.24) | non fatto | non fatto |
| Permesso | stesso comando; al prompt `permesso? [sì/no/sempre]` rispondere `sì` | stesso comando; rispondere `sì` | fatto (modifica file e bash, id `once` / `always` / `reject`; stesso file) | non fatto | non fatto |
| Cambio di effort | stesso processo: dopo il primo turno stampa `cambio thought_level` e un secondo prompt | stesso processo | fatto (`effort=none` nel turno dopo; stesso file) | non fatto | non fatto |
| Ripresa | `./target/debug/bitbot-spike --resume --agent opencode "$REPO" "Continua il compito precedente."` | `.\target\debug\bitbot-spike.exe --resume --agent opencode $repo "Continua il compito precedente."` | fatto (`session/resume` senza replay; stesso file). Il testo del prompt di ripresa non è archiviato | non fatto | non fatto |

## Cursor

Il 18 ottobre 2026 un modello esplicito chiudeva il turno con «Add a
payment method to continue». Il valore che l'agente dichiara, e che lo
spike deve passare prima del primo prompt, è `default[]`.

| Passo | macOS | Windows | Linux | macOS | Windows |
| --- | --- | --- | --- | --- | --- |
| Sessione nuova e prompt | `./target/debug/bitbot-spike --agent cursor --model 'default[]' "$REPO" "Aggiungi un test unittest per la funzione somma in calcola.py"` | `.\target\debug\bitbot-spike.exe --agent cursor --model 'default[]' $repo "Aggiungi un test unittest per la funzione somma in calcola.py"` | fatto il 16 ottobre senza `--model` (Cursor 2026.10.01, `docs/osservazioni-spike.md`). Il 18 ottobre il turno parte solo con `--model 'default[]'` | non fatto | non fatto |
| Permesso | stesso comando; rispondere `sì` a `permesso? [sì/no/sempre]` | stesso comando; rispondere `sì` | fatto il 16 ottobre per bash (`allow-once`). Una scrittura fuori dal progetto non ha chiesto permesso (punti aperti, stesso file) | non fatto | non fatto |
| Cambio di modello | stesso processo: Cursor non ha `thought_level`, lo spike cambia l'opzione `model` | stesso processo | fatto il 16 ottobre (cambio `model` nel turno dopo). Il 18 ottobre un modello esplicito dopo `default[]` muore con il messaggio di pagamento | non fatto | non fatto |
| Ripresa | `./target/debug/bitbot-spike --resume --agent cursor --model 'default[]' "$REPO" "Continua il compito precedente."` | `.\target\debug\bitbot-spike.exe --resume --agent cursor --model 'default[]' $repo "Continua il compito precedente."` | fatto il 16 ottobre via `session/load` con replay (lo spike non usa `session/resume`, che Cursor non dichiara). Testo del prompt non archiviato | non fatto | non fatto |

`cursor/ask_question` non è in tabella: su Linux non è scattata
(`docs/osservazioni-spike.md`, punti aperti).

## Codex

Serve `codex login` già fatto. Il 16 ottobre il catalogo in
`~/.codex/config.toml` e `~/.codex/models.json` non accettava `max`; i
valori usati dopo la correzione sono `high` e `xhigh`
(`docs/osservazioni-spike.md`). Non rifare quel modifica se il catalogo
locale è già accettato da `codex-acp`.

| Passo | macOS | Windows | Linux | macOS | Windows |
| --- | --- | --- | --- | --- | --- |
| Sessione nuova e prompt | `./target/debug/bitbot-spike --agent codex "$REPO" "Aggiungi un test unittest per la funzione somma in calcola.py"` | `.\target\debug\bitbot-spike.exe --agent codex $repo "Aggiungi un test unittest per la funzione somma in calcola.py"` | fatto il 16 ottobre (`codex-acp` 0.16.0, stesso file) | non fatto | non fatto |
| Permesso | prima della corsa, in `~/.codex/config.toml`: `approval_policy = "on-request"` e `sandbox_mode = "workspace-write"` (fare una copia e ripristinare dopo). Poi lo stesso comando con un compito che scrive anche fuori dal repo, per esempio `"Scrivi report.txt nel progetto e un file fuori dal progetto."`. Rispondere `sì` | stessi passi, con il profilo utente di Windows al posto di `~/.codex` se Codex non usa la home Unix | fatto il 18 ottobre: la scrittura fuori dal progetto chiede `allow_once` con id `approved`. Letture e scritture interne no. L'opzione ACP `mode` non rispecchia la sandbox (stesso file, sezione del 18 ottobre) | non fatto | non fatto |
| Cambio di effort | stesso processo della sessione nuova: `cambio thought_level` su `reasoning_effort` | stesso processo | fatto il 16 e il 18 ottobre (`high` → `xhigh` nel turno dopo). Alla ripresa l'opzione riletta torna `high` | non fatto | non fatto |
| Ripresa | `./target/debug/bitbot-spike --resume --agent codex "$REPO" "Continua il compito precedente."` | `.\target\debug\bitbot-spike.exe --resume --agent codex $repo "Continua il compito precedente."` | fatto il 16 ottobre via `session/resume` senza replay. Lo spike non stampa la risposta finale di quel turno. Testo del prompt non archiviato | non fatto | non fatto |

## `bitbotd` contro un agente reale

Il demone stampa la riga `bitbotd in ascolto su …`. Su macOS è il file
del socket (`TMPDIR` o `XDG_RUNTIME_DIR`, altrimenti `/tmp/bitbotd.sock`).
Su Windows è la pipe `\\.\pipe\bitbot-<hash della cartella temporanea>-bitbotd.sock`.
Il client usa quella stringa, non un percorso fisso.

Linux, 18 ottobre 2026, è registrato in `AGENTS.md` (non in
`docs/osservazioni-spike.md`): OpenCode 2.0.32 da `avvio` a `finito`,
test unittest generato passato, nessuna `session/request`. Permessi,
domande e piani del demone restano provati solo con l'agente finto.

| Passo | macOS | Windows | Linux | macOS | Windows |
| --- | --- | --- | --- | --- | --- |
| Avvio | `./target/debug/bitbotd` | `.\target\debug\bitbotd.exe` | fatto il 18 ottobre (`AGENTS.md`) | non fatto | non fatto |
| `version` e `session/start` | vedi sotto | vedi sotto | fatto nella stessa corsa, fino a `finito` | non fatto | non fatto |
| `session/respond` se arriva `session/request` | `outcome` `allow_once`, oppure `cancelled` | stesso JSON | non arrivata in quella corsa. L'agente finto copre permesso, domanda, piano e `cancelled` | non fatto | non fatto |

macOS, in un altro terminale, con `BITBOT_SOCK` copiato dalla riga di
avvio e `BITBOT_REPO` uguale a `$REPO`:

```bash
python3 - <<'PY'
import json, os, socket
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(os.environ["BITBOT_SOCK"])
buf = b""

def send(message):
    sock.sendall((json.dumps(message, separators=(",", ":")) + "\n").encode())

def read_line():
    global buf
    while b"\n" not in buf:
        chunk = sock.recv(4096)
        if not chunk:
            raise SystemExit("socket chiuso")
        buf += chunk
    line, buf = buf.split(b"\n", 1)
    return json.loads(line.decode())

next_id = 3
send({"jsonrpc": "2.0", "id": 1, "method": "version"})
print(read_line())
send({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "session/start",
    "params": {
        "agent": "opencode",
        "cwd": os.environ["BITBOT_REPO"],
        "prompt": "Aggiungi un test unittest per la funzione somma in calcola.py",
    },
})
while True:
    message = read_line()
    print(message)
    if message.get("method") == "session/request":
        send({
            "jsonrpc": "2.0",
            "id": next_id,
            "method": "session/respond",
            "params": {
                "id": message["params"]["id"],
                "requestId": message["params"]["requestId"],
                "outcome": "allow_once",
            },
        })
        next_id += 1
    status = message.get("params", {}).get("status", {})
    if message.get("method") == "session/event" and status.get("state") in ("finished", "failed"):
        break
    if message.get("error") is not None and message.get("method") is None:
        break
PY
```

Windows (PowerShell). `$full` è la stringa stampata da `bitbotd`, `$repo`
è la cartella di prova. `ConvertTo-Json` scrive il JSON: il percorso
non va concatenato a mano.

```powershell
$full = '\\.\pipe\bitbot-INCOLLARE-bitbotd.sock'
if ($full -notmatch '^\\\\\.\\pipe\\(.+)$') { throw "percorso pipe inatteso: $full" }
$name = $Matches[1]
$pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', $name)
$pipe.Connect(5000)
$utf8 = New-Object System.Text.UTF8Encoding $false
$writer = [System.IO.StreamWriter]::new($pipe, $utf8)
$reader = [System.IO.StreamReader]::new($pipe, $utf8)
$writer.AutoFlush = $true
$script:nextId = 3

function Send-Line($obj) {
    $writer.WriteLine(($obj | ConvertTo-Json -Compress -Depth 6))
}

Send-Line @{ jsonrpc = '2.0'; id = 1; method = 'version' }
$reader.ReadLine()
Send-Line @{
    jsonrpc = '2.0'
    id = 2
    method = 'session/start'
    params = @{
        agent = 'opencode'
        cwd = $repo
        prompt = 'Aggiungi un test unittest per la funzione somma in calcola.py'
    }
}
while ($true) {
    $line = $reader.ReadLine()
    if ($null -eq $line) { break }
    $line
    $msg = $line | ConvertFrom-Json
    if ($msg.method -eq 'session/request') {
        Send-Line @{
            jsonrpc = '2.0'
            id = $script:nextId
            method = 'session/respond'
            params = @{
                id = $msg.params.id
                requestId = $msg.params.requestId
                outcome = 'allow_once'
            }
        }
        $script:nextId++
    }
    $state = $msg.params.status.state
    if ($msg.method -eq 'session/event' -and ($state -eq 'finished' -or $state -eq 'failed')) {
        break
    }
}
```

Dopo la corsa, fermare `bitbotd` con Ctrl+C. Non c'è ripresa al
riavvio: se il demone esce, la sessione esce con lui.
