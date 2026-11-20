chrome.runtime.sendMessage({ type: "status" }, (s) => {
  const state = document.getElementById("state");
  const text = document.getElementById("text");
  if (!s || !s.reachable) {
    state.className = "state off";
    text.textContent = "Open the Bolke app to use names from calls.";
  } else if (s.recording) {
    state.className = "state on";
    text.textContent = "Bolke is recording: names are on.";
  } else {
    state.className = "state on";
    text.textContent = "Ready. Record a meeting in Bolke to use names.";
  }
  if (s && s.names && s.names.length) {
    const call = document.getElementById("call");
    call.hidden = false;
    call.textContent =
      "In this call: " +
      s.names.join(", ") +
      (s.talking.length ? " · Talking: " + s.talking.join(", ") : "");
  }
});
