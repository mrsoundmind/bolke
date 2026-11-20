// Passes what the call page saw to the Bolke app, which listens on this
// computer only (src-tauri/src/call_names.rs). Bolke keeps it only while a
// meeting is recording.

const BOLKE = "http://127.0.0.1:47213";

/** Last answer from Bolke, for the popup. */
let last = {
  reachable: false,
  recording: false,
  names: [],
  talking: [],
  at: 0,
};

async function send(report, talkingNow) {
  try {
    const res = await fetch(`${BOLKE}/call`, {
      method: "POST",
      // text/plain keeps it a simple request; Bolke reads it as JSON.
      headers: { "Content-Type": "text/plain" },
      body: JSON.stringify(report),
    });
    const answer = await res.json();
    last = {
      reachable: res.ok,
      recording: !!answer.recording,
      names: report.people,
      talking: talkingNow || [],
      at: Date.now(),
    };
  } catch {
    last = { ...last, reachable: false, recording: false, at: Date.now() };
  }
  return last;
}

async function check() {
  try {
    const res = await fetch(`${BOLKE}/bolke`);
    const answer = await res.json();
    last = { ...last, reachable: true, recording: !!answer.recording };
    // Names seen more than 10 s ago are from a call that's over.
    if (Date.now() - last.at > 10000)
      last = { ...last, names: [], talking: [] };
  } catch {
    last = { ...last, reachable: false, recording: false };
  }
  return last;
}

chrome.runtime.onMessage.addListener((message, _sender, reply) => {
  if (message?.type === "report") {
    send(message.report, message.talkingNow).then(reply);
    return true;
  }
  if (message?.type === "status") {
    check().then(reply);
    return true;
  }
  return false;
});
