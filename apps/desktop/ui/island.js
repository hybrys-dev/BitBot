// Aggiorna la pillola con l'evento `pill-label` (vedi `pill::PILL_EVENT`).
// All'avvio chiede anche la riga già nota, così un evento arrivato prima
// del listener non resta fuori. Senza API Tauri il testo resta «BitBot».
(function () {
  var tauri = window.__TAURI__;
  if (!tauri || !tauri.event || typeof tauri.event.listen !== "function") {
    return;
  }

  var seen = false;

  function show(payload) {
    var pill = document.querySelector(".pill");
    if (!pill || typeof payload !== "string") {
      return;
    }
    pill.textContent = payload;
  }

  var listening = tauri.event.listen("pill-label", function (event) {
    seen = true;
    show(event.payload);
  });

  if (typeof tauri.invoke !== "function" || !listening || typeof listening.then !== "function") {
    return;
  }

  listening
    .then(function () {
      return tauri.invoke("pill_label");
    })
    .then(function (payload) {
      if (!seen) {
        show(payload);
      }
    })
    .catch(function () {});
})();
