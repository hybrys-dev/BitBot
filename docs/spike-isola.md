# Spike dell'isola

Finestra minima del guscio Tauri 2, in `apps/desktop`. Non è l'isola del prodotto: niente Bit, pillole, schede, cockpit o chat. Mostra una pillola scura con la scritta «BitBot», larga 240 punti e alta 36, centrata sul bordo alto dello schermo principale. Lo sfondo della finestra è trasparente. La forma che riceve i clic è la pillola: gli angoli del rettangolo che la contiene no.

Il binario è `bitbot-desktop`. Si compila con `cargo build -p bitbot-desktop`. Non c'è uno script Tauri separato e non c'è un installer.

La fase 0 è chiusa il 20 ottobre 2026 con dei limiti noti: questo spike
non dimostra che l'isola appaia bene su ogni sistema. Hyprland, Sway,
Plasma e il notch vero restano senza prova visiva, come elencato in
«Cosa non ho visto».

## Cosa fa il codice

| Sistema | Finestra | Clic | Schermo intero |
| --- | --- | --- | --- |
| macOS | Pannello `NSPanel` non attivante, livello status (sopra la barra dei menu), su tutti gli spazi normali, senza `FullScreenAuxiliary`. Se il sistema dichiara il notch, il rettangolo sta fra le due aree del menu bar; altrimenti è la striscia sul bordo alto. L'app resta accessoria, così non prende il posto di un'applicazione normale. | All'inizio la finestra ignora il mouse. Un controllo sul thread principale lo riaccende solo quando il cursore è dentro la pillola. | Non c'è un polling. Senza `FullScreenAuxiliary` AppKit non porta il pannello negli spazi a schermo intero. Lo spike non ha nulla che richieda l'utente, quindi non lo riporta sopra un'app a schermo intero. |
| Windows | Finestra trasparente, `WS_EX_NOACTIVATE`, `WS_EX_TOPMOST`, fuori dalla barra delle applicazioni. Viene mostrata con `SW_SHOWNOACTIVATE`. | `SetWindowRgn` con un rettangolo arrotondato: fuori da quella regione i clic passano. | Ogni 400 ms confronta la finestra in primo piano con il monitor. Se la copre, l'isola si nasconde; quando non è più così, torna. Una finestra massimizzata, che si ferma sulla barra, non conta. |
| Linux con KDE, Hyprland, Sway o un altro Wayland che non è GNOME | Overlay `gtk-layer-shell` sul layer overlay, ancorato solo al bordo alto (il compositor lo centra), zona esclusiva 0, tastiera disattivata. Il contenuto di Tauri viene spostato su una finestra GTK nuova, perché layer-shell va inizializzato prima che la finestra esista. Se `is_supported()` è falso, si usa il ripiego sempre in primo piano. | Maschera di input GDK uguale alla pillola. | Su Sway e Hyprland si legge il socket del compositor (`fullscreen_mode` e il campo `fullscreen`: per Hyprland conta il valore 2, non la finestra massimizzata). Altrimenti, se c'è `DISPLAY`, si legge `_NET_WM_STATE_FULLSCREEN` della finestra attiva. Se non si riesce a saperlo, la finestra resta dov'è: il layer overlay non si ritira da solo. |
| Linux con GNOME su Wayland | Prima dell'avvio di GTK si imposta `GDK_BACKEND=x11`, se non è già impostato. La finestra è di tipo dock, appiccicata a tutti i desktop, senza focus. | Come sopra. | Come su X11, tramite EWMH. |
| Linux con X11, e GNOME già su X11 | Finestra normale, `_NET_WM_STATE_ABOVE`, senza barra delle applicazioni e senza focus. | Come sopra. | EWMH, ogni 400 ms. |
| Altri sistemi | Stessa finestra trasparente sempre in primo piano, senza maschera di input e senza controllo dello schermo intero. Il modulo esiste perché il codice di piattaforma abbia un ripiego che compila. | Il rettangolo intero riceve i clic. | Nessuno. |

Le scorciatoie globali non ci sono. Il portale XDG non è collegato.

## Cosa ho visto

La VM è Linux, sessione X11 con XFCE (`xfwm4`, pannello `xfce4-panel` alto 29 px, schermo 1920×1200). `XDG_SESSION_TYPE` e `XDG_CURRENT_DESKTOP` erano vuoti e non c'era `WAYLAND_DISPLAY`, quindi il programma ha scelto il percorso X11 e ha scritto `isola su X11, finestra sempre in primo piano`.

Ho compilato `bitbot-desktop` su questa macchina e l'ho avviato su quel display. La finestra «BitBot» era visibile, 240×36, centrata in orizzontale (x = 840). Il window manager l'ha tenuta a y = 29, subito sotto il pannello, non sopra il bordo fisico dello schermo: un `XMoveWindow` non ha spostato la cornice. Le proprietà lette con `xprop` erano `_NET_WM_STATE_ABOVE`, `_NET_WM_STATE_SKIP_TASKBAR`, `_NET_WM_STATE_SKIP_PAGER` e il tipo `_NET_WM_WINDOW_TYPE_NORMAL`.

Ho visto la pillola scura con la scritta BitBot. Intorno, lo sfondo della finestra lasciava vedere il pannello e lo sfondo del desktop: non un rettangolo opaco. La maschera di input, letta con l'estensione SHAPE, è la pillola (agli angoli le righe sono più corte, al centro la riga è larga 240). Non ho fatto un clic per verificare che il clic arrivi all'applicazione sotto.

Ho aperto una finestra a schermo intero. Mentre risultava la finestra attiva, la mappa della finestra BitBot era `IsUnmapped`. Chiusa quella finestra, BitBot è tornata `IsViewable`, di nuovo 240×36 a 840,29. Non ho fotografato il momento in cui era nascosta: l'ho letto dallo stato della finestra.

Questa corsa è quella dello spike, prima della correzione in main che fa scendere l'isola sotto il pannello in alto (PR #24).

Corse successive, tutte in macchina virtuale:

- 19 ottobre 2026, Fedora 44 con GNOME 44: la pillola è stata vista per intero sotto la barra in alto, alla finestra 240×36. È la corsa per cui sono entrate in main la correzione che la fa scendere sotto il pannello e quella che aspetta lo schermo all'avvio della sessione (PR #24).
- 20 ottobre 2026, Windows 11: la striscia arrotondata traslucida in alto al centro, sopra le altre finestre, verificata con uno screenshot della console con PowerShell aperto sotto.
- 20 ottobre 2026, macOS Ventura 13.7.8: il pannello non attivante sopra la barra dei menu. La VM non ha notch, quindi si vede solo il ramo «nessun notch, striscia sul bordo alto».
- Plasma 6 in Wayland: l'isola parte e sceglie il percorso layer-shell («overlay layer-shell ancorato in alto»). Non c'è ancora uno screenshot.

## Cosa non ho visto

- Il notch del Mac: la VM Ventura non ne ha, e il posizionamento attorno al notch resta non verificato.
- Il ritiro negli spazi a schermo intero su macOS e il ritiro su Windows a schermo intero: la finestra è stata vista, il comportamento no.
- Plasma 6: l'isola parte e sceglie il percorso layer-shell, ma non c'è uno screenshot.
- Hyprland e Sway, e quindi layer-shell sul bordo alto.
- Un compositor in cui la finestra X11 resti davvero sul bordo fisico, sopra un pannello.
- Un clic che attraversa gli angoli trasparenti.
- L'aspetto a schermo intero, oltre allo stato unmapped descritto sopra.

La CI su `ubuntu-latest`, `macos-latest` e `windows-latest` (fmt, clippy e test) è verde sul commit che chiude gli avvisi di compilazione. Compilare non mostra la finestra: l'aspetto resta non visto sui sistemi elencati sopra. Le librerie Linux aggiunte al workflow sono `libwebkit2gtk-4.1-dev`, `libgtk-layer-shell-dev` e il minimo per compilare Tauri e collegare X11.
