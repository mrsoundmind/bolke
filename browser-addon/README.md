# Bolke browser add-on

Gives meeting notes real names. During a Google Meet call it tells the Bolke
app who is in the call and whose tile lights up while they talk. After the
meeting, Bolke puts each name on the voice that talked at the same moments.

It reads names and the "talking" ring only, never video, chat or the screen,
and sends them only to the Bolke app on the same computer
(`127.0.0.1:47213`, see `src-tauri/src/call_names.rs`).

## Add it to Chrome, Arc, Edge or Brave

Until it's in the Chrome Web Store:

1. Open `chrome://extensions` (in Arc: `arc://extensions`).
2. Turn on **Developer mode** (top right).
3. Click **Load unpacked** and choose this `browser-addon` folder.

## Check it works

Join a Meet call and click the Bolke icon in the browser's toolbar. It lists
the people it sees and who is talking. While Bolke records a meeting it says
"names are on".
