// Names from the call window (design/wireframes/MeetingNames.dc.html).
//
// For the first minutes of a meeting, Bolke looks at the call window (Google
// Meet in a browser, Zoom, Teams) about twice a second: it finds the tile
// with the coloured "talking" ring and reads the name written on it. Nothing
// is saved but the names; the picture is thrown away at once.
//
// Called from Rust (src/call_screen.rs) on a background thread.

import CoreGraphics
import Foundation
import ScreenCaptureKit
import Vision

private let browserIDs: Set<String> = [
  "company.thebrowser.Browser", "com.google.Chrome", "com.google.Chrome.beta",
  "com.microsoft.edgemac", "com.brave.Browser", "com.apple.Safari",
  "com.operasoftware.Opera", "com.vivaldi.Vivaldi", "org.chromium.Chromium",
  "company.thebrowser.dia", "ai.perplexity.comet", "com.openai.atlas",
  "com.kagi.kagimacOS", "app.zen-browser.zen", "org.mozilla.firefox",
]
private let zoomIDs: Set<String> = ["us.zoom.xos"]
private let teamsIDs: Set<String> = ["com.microsoft.teams2", "com.microsoft.teams"]

/// Words on tiles that aren't anyone's name.
private let notNames: Set<String> = [
  "you", "presentation", "presenting", "pin", "unpin", "mute", "muted",
  "more options", "remove", "meeting details", "people", "chat", "activities",
]

private func json(_ object: [String: Any]) -> UnsafeMutablePointer<CChar>? {
  let data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data("{}".utf8)
  return strdup(String(decoding: data, as: UTF8.self))
}

@_cdecl("bolke_free_string")
public func bolkeFreeString(_ s: UnsafeMutablePointer<CChar>?) {
  free(s)
}

/// 1 if Bolke may look at other windows. With `ask`, macOS asks the user
/// (once; afterwards it only says no until it's allowed in Settings).
@_cdecl("bolke_screen_access")
public func bolkeScreenAccess(_ ask: Int32) -> Int32 {
  if CGPreflightScreenCaptureAccess() { return 1 }
  if ask != 0 { return CGRequestScreenCaptureAccess() ? 1 : 0 }
  return 0
}

/// One look at the call window: who has the talking ring. Blocks for up to
/// two seconds.
@_cdecl("bolke_call_frame")
public func bolkeCallFrame() -> UnsafeMutablePointer<CChar>? {
  guard #available(macOS 14.0, *) else { return json(["ok": false, "error": "old_macos"]) }
  guard CGPreflightScreenCaptureAccess() else {
    return json(["ok": false, "error": "no_permission"])
  }
  var result: [String: Any] = ["ok": false, "error": "timeout"]
  let done = DispatchSemaphore(value: 0)
  Task {
    result = await look()
    done.signal()
  }
  _ = done.wait(timeout: .now() + 2)
  return json(result)
}

@available(macOS 14.0, *)
private func look() async -> [String: Any] {
  guard
    // Not only windows in view: the call may sit behind other apps or on
    // another desktop while the user works.
    let content = try? await SCShareableContent.excludingDesktopWindows(
      true, onScreenWindowsOnly: false)
  else { return ["ok": false, "error": "no_windows"] }
  guard let (window, app) = callWindow(in: content.windows) else {
    return ["ok": false, "error": "no_call_window"]
  }
  let filter = SCContentFilter(desktopIndependentWindow: window)
  let config = SCStreamConfiguration()
  // Points, not pixels: plenty to read a name, and four times lighter.
  config.width = max(1, Int(window.frame.width))
  config.height = max(1, Int(window.frame.height))
  config.showsCursor = false
  guard
    let image = try? await SCScreenshotManager.captureImage(
      contentFilter: filter, configuration: config)
  else { return ["ok": false, "error": "capture_failed"] }

  let rings = talkingRings(in: image)
  var speaking: [String] = []
  for ring in rings {
    if let name = nameInside(ring, of: image), !speaking.contains(name) {
      speaking.append(name)
    }
  }
  return ["ok": true, "app": app, "rings": rings.count, "speaking": speaking]
}

/// The window most likely to be a call: Meet in a browser, or Zoom / Teams.
@available(macOS 14.0, *)
private func callWindow(in windows: [SCWindow]) -> (SCWindow, String)? {
  var best: (SCWindow, String, CGFloat)?
  for w in windows where w.windowLayer == 0 && w.frame.width >= 400 && w.frame.height >= 300 {
    guard let id = w.owningApplication?.bundleIdentifier else { continue }
    let title = (w.title ?? "").lowercased()
    var app: String?
    // A browser window's title is its showing tab's: "Meet – …" for a call
    // (not "Meeting notes"). A Meet tab hidden behind another tab can't be
    // seen at all.
    if browserIDs.contains(id), title.hasPrefix("meet ") { app = "meet" }
    if zoomIDs.contains(id), title.contains("zoom") { app = "zoom" }
    if teamsIDs.contains(id), title.contains("meeting") || title.contains("call") {
      app = "teams"
    }
    guard let app else { continue }
    let area = w.frame.width * w.frame.height
    if best == nil || area > best!.2 { best = (w, app, area) }
  }
  return best.map { ($0.0, $0.1) }
}

/// A rectangle in image pixels, top-left origin.
private struct Ring {
  var x0: Int, y0: Int, x1: Int, y1: Int
}

/// Finds hollow rectangles drawn in one bright colour: the ring a call puts
/// around whoever is talking (pale blue in Meet, green or yellow in Zoom,
/// purple in Teams). Filled buttons and plain borders don't count.
private func talkingRings(in image: CGImage) -> [Ring] {
  // Work on a copy at most 960 wide.
  let scale = min(1.0, 960.0 / Double(image.width))
  let w = max(1, Int(Double(image.width) * scale))
  let h = max(1, Int(Double(image.height) * scale))
  var px = [UInt8](repeating: 0, count: w * h * 4)
  guard
    let ctx = CGContext(
      data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
  else { return [] }
  ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))

  // Hue (0-360) of each "ring-coloured" pixel, or -1.
  var hue = [Float](repeating: -1, count: w * h)
  for i in 0..<(w * h) {
    let r = Float(px[i * 4]) / 255, g = Float(px[i * 4 + 1]) / 255, b = Float(px[i * 4 + 2]) / 255
    let mx = max(r, g, b), mn = min(r, g, b), d = mx - mn
    guard mx >= 0.55, d / mx >= 0.2 else { continue }
    var hh: Float
    if mx == r { hh = 60 * ((g - b) / d).truncatingRemainder(dividingBy: 6) }
    else if mx == g { hh = 60 * ((b - r) / d + 2) }
    else { hh = 60 * ((r - g) / d + 4) }
    if hh < 0 { hh += 360 }
    hue[i] = hh
  }
  func near(_ a: Float, _ b: Float) -> Bool {
    let diff = abs(a - b)
    return min(diff, 360 - diff) <= 14
  }

  // Long horizontal runs of one colour: candidate top and bottom edges.
  struct Run { var y: Int, x0: Int, x1: Int, hue: Float }
  let minRun = max(60, w / 12)
  var runs: [Run] = []
  for y in 0..<h {
    var x = 0
    while x < w {
      let hh = hue[y * w + x]
      guard hh >= 0 else { x += 1; continue }
      var end = x
      while end + 1 < w, hue[y * w + end + 1] >= 0, near(hue[y * w + end + 1], hh) { end += 1 }
      if end - x + 1 >= minRun { runs.append(Run(y: y, x0: x, x1: end, hue: hh)) }
      x = end + 1
    }
  }

  var rings: [Ring] = []  // in the small copy's pixels until the end
  let minHeight = max(50, h / 12)
  for (i, top) in runs.enumerated() {
    // Skip a run just below another of the same edge (rings are 2-4 px).
    if rings.contains(where: { abs($0.y0 - top.y) < 10 && abs($0.x0 - top.x0) < 10 }) { continue }
    for bottom in runs[(i + 1)...] where bottom.y - top.y >= minHeight {
      guard abs(bottom.x0 - top.x0) <= 6, abs(bottom.x1 - top.x1) <= 6,
        near(bottom.hue, top.hue)
      else { continue }
      // Both sides drawn in the same colour most of the way down.
      // Rounded corners: the side sits up to ~14 px outside the top edge's
      // straight part.
      func side(_ x: Int, _ out: Int) -> Bool {
        var hit = 0
        for y in top.y...bottom.y {
          for step in -3...14 {
            let dx = step * out
            let xx = x + dx
            if xx >= 0, xx < w, hue[y * w + xx] >= 0, near(hue[y * w + xx], top.hue) {
              hit += 1
              break
            }
          }
        }
        return Double(hit) >= 0.8 * Double(bottom.y - top.y + 1)
      }
      guard side(top.x0, -1), side(top.x1, 1) else { continue }
      // Hollow: just inside the edge isn't the ring's colour (a filled
      // button is; what's in the middle, like an avatar, doesn't matter).
      var inside = 0, looked = 0
      for y in (top.y + 8)...(top.y + 14) where y < bottom.y {
        for x in stride(from: top.x0, through: top.x1, by: 4) {
          looked += 1
          if hue[y * w + x] >= 0, near(hue[y * w + x], top.hue) { inside += 1 }
        }
      }
      if looked == 0 || Double(inside) > 0.3 * Double(looked) { continue }
      rings.append(Ring(x0: top.x0, y0: top.y, x1: top.x1, y1: bottom.y))
      break
    }
  }
  return rings.map {
    Ring(
      x0: Int(Double($0.x0) / scale), y0: Int(Double($0.y0) / scale),
      x1: Int(Double($0.x1) / scale), y1: Int(Double($0.y1) / scale))
  }
}

/// The name written on a tile: the text nearest its lower-left corner, where
/// Meet, Zoom and Teams all put it. Only that corner is read, to be quick.
private func nameInside(_ ring: Ring, of image: CGImage) -> String? {
  let tw = max(1, ring.x1 - ring.x0), th = max(1, ring.y1 - ring.y0)
  let corner = max(40, th / 4)
  let rect = CGRect(x: ring.x0, y: ring.y1 - corner, width: tw * 2 / 3, height: corner)
  guard let tile = image.cropping(to: rect) else { return nil }
  let request = VNRecognizeTextRequest()
  request.recognitionLevel = .accurate
  request.usesLanguageCorrection = false
  let handler = VNImageRequestHandler(cgImage: tile, options: [:])
  guard (try? handler.perform([request])) != nil, let found = request.results else { return nil }

  var best: (String, Double)?
  for observation in found {
    guard let text = observation.topCandidates(1).first?.string else { continue }
    let name = text.trimmingCharacters(in: .whitespacesAndNewlines)
    let lower = name.lowercased()
    guard name.count >= 2, name.count <= 40, !notNames.contains(lower),
      name.rangeOfCharacter(from: .letters) != nil
    else { continue }
    // Vision's boxes have a bottom-left origin: low y is near the bottom.
    let box = observation.boundingBox
    let score = Double(box.minY + box.minX * 0.5)
    if best == nil || score < best!.1 { best = (name, score) }
  }
  return best?.0
}
