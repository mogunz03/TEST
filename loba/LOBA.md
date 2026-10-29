# L.O.B.A

A fork of [Handy](https://github.com/cjpais/Handy) (MIT) for macOS on Apple Silicon.
Handy records, transcribes and pastes. L.O.B.A adds one layer between
transcription and paste, and never lets that layer hold up the paste.

```
hotkey ─▶ capture ─▶ ASR ─▶ ┌──────── L.O.B.A ────────┐ ─▶ paste
 (Handy)   (Handy)          │ cleanup via local Ollama │    (Handy)
                            │ 4 s budget → raw on miss │
                            └──────────────────────────┘
```

## Untouched upstream code

Hotkeys, audio capture and text injection are byte-identical to the vendored
Handy commit (`git log -- loba` shows the import):
`src-tauri/src/{shortcut/, audio_toolkit/, managers/audio.rs, clipboard.rs, input.rs, paste_tx/, transcription_coordinator.rs}`.
Check with:

```sh
git diff <vendor-commit> -- loba/src-tauri/src/shortcut loba/src-tauri/src/audio_toolkit \
  loba/src-tauri/src/managers/audio.rs loba/src-tauri/src/clipboard.rs \
  loba/src-tauri/src/input.rs loba/src-tauri/src/paste_tx loba/src-tauri/src/transcription_coordinator.rs
```

## Layout

| Path                  | What                                                                                                               |
| --------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `crates/loba-core/`   | The pipeline. No Tauri; `cargo test` runs in seconds.                                                              |
| `src-tauri/src/loba/` | Wiring: state, Tauri commands, the seam in `actions.rs`.                                                           |
| `src/loba/`           | The UI: Home, History, Dictionary, Styles, and Settings (General, Cleanup, and Handy's own settings under Engine). |

## Speech models

- **Whisper Turbo** (large-v3-turbo, GGUF, Metal) is the recommended model,
  and the only bundled one that hears English, French and Chinese alike.
  Onboarding offers it first.
- **SenseVoice Small** is faster but covers zh / yue / en / ja / ko only. **No
  French.** **Parakeet V3** covers European languages only. **No Chinese.**
- Whisper guesses one language per recording. Mixed sentences usually keep
  each part in its own script; long switches mid-sentence can still come out
  in one language, and cleanup can't bring back words the speech model lost.

## Languages you speak

Settings → General → Languages. Stored as `languages` in `config.json`.

- **None picked:** Whisper's guess stands.
- **One:** every dictation is transcribed in that language. No guessing.
- **Several:** if Whisper guesses a language that isn't on the list, the
  recording is transcribed again in the closest one that is: Chinese
  characters stay Chinese, Portuguese or Italian becomes French, Dutch
  becomes English (`loba_core::languages::steer`). That costs one extra
  pass, only on the dictations it fixes.
- If the current speech model can't hear a picked language, the section
  says so and offers Whisper Turbo (download, then switch, one click).

**Fixing one that came out wrong.** History → hover a dictation → _Wrong
language?_ → pick the language. L.O.B.A transcribes the saved recording again
in that language, cleans it up for the app it was made in, saves it over the
old text and copies it, ready to paste over what landed. Recordings are kept
for the last 50 dictations (Handy's default retention).

## Cleanup (Ollama)

```sh
brew install ollama
ollama serve            # or the Ollama.app
ollama pull qwen3:8b
```

- Talks only to loopback (`127.0.0.1:11434`); any other host in the config is
  refused and replaced with the default.
- Thinking is disabled (`think: false`) and the model is preloaded the moment
  the hotkey goes down, so its weights are resident by the time you stop
  speaking.
- **Edit level** (Settings → Cleanup). _Light_ keeps your wording: fillers,
  false starts and self-corrections go, punctuation is fixed, spoken lists
  become lists. _Polished_ rewrites for clarity the way Typeless does:
  tighter sentences, related points together, one paragraph per idea, lists
  for steps. Both keep your meaning and language, never add facts and never
  translate, and the same guard rejects answers that grew, gained a preamble
  or switched script. Polished has its own time limit (default 8 s,
  `polished_timeout_ms`); `qwen3:8b` handles it, `qwen3:14b` writes better
  on a Mac with 32 GB or more. Command mode and literal apps are unaffected.
- Hard budget (default 4 s, configurable). On timeout, server down, or an
  answer that looks wrong (grew, gained a preamble, changed script = likely
  translated), the raw transcript is pasted instead.

Files in `~/Library/Application Support/app.loba.desktop/loba/`:

- `config.json`: `cleanup_enabled`, `model`, `edit_level` (`light` / `polished`), `timeout_ms`, `polished_timeout_ms`, `ollama_url`, `keep_alive`.
- `dictionary.json`: your terms and how they get misheard:

  ```json
  { "entries": [{ "term": "DAUZE", "sounds_like": ["those"] }] }
  ```

  Terms (newest first, ~600 chars) become Whisper's `initial_prompt`.
  `sounds_like` rewrites are whole-word, case-insensitive (Han patterns match
  anywhere), and run both before the LLM and after it, so the model can't
  "correct" a term back. A rule is literal: `those → DAUZE` rewrites _every_
  "those" you say. Hand edits are picked up on the next dictation.

## App-aware tone

On hotkey-down L.O.B.A notes the frontmost app (NSWorkspace) and its focused
window title (Accessibility API, 250 ms timeout, off the hotkey thread). At
cleanup time that resolves to a style in `styles.json`:

```json
{
  "default": "neutral",
  "styles": {
    "casual": { "tone": "Chat message. Relaxed and short…", "rewrite": true },
    "formal": { "tone": "Email. Complete sentences…", "rewrite": true },
    "literal": { "tone": "", "rewrite": false }
  },
  "apps": {
    "WeChat": "casual",
    "Slack": "casual",
    "Mail": "formal",
    "Code": "literal",
    "Terminal": "literal"
  },
  "window_titles": { "Gmail": "formal" }
}
```

Order: app name / bundle id → window-title substring (how Gmail in a browser
reads as formal) → `default`. `rewrite: false` skips the LLM entirely
(dictionary rules only), so a terminal command is never "improved". Mappings to
a style that doesn't exist are dropped on load. **Styles → Check an app**
waits 3 s, then shows what L.O.B.A sees in whatever app you switched to.

## Shortcuts

Both shortcuts show on Home and under Settings → General, each with a
**Change** button. Change opens a dialog that listens: press the keys, let
go, and they come back as keycaps. Nothing is saved until **Save**, so you
can try as many times as you like. Modifier-only chords (⌘⇧, ⌘⌥) work.
A single plain key is refused because it would fire while typing, and so is
a chord the other shortcut already uses. Esc or Cancel leaves the old one in
place; **Reset to** puts the default back.

| Action       | Default (macOS) |
| ------------ | --------------- |
| Dictate      | hold **⌘⌥**     |
| Command mode | hold **⌃⌥⌘**    |

Why a modifier-only chord is safe here: dictation defaults to **Holding**
(push-to-talk), and any recording under 0.3 s is dropped. So ⌥⌘I, ⌥⌘Esc and
the like pass through to other apps without a stray paste. At worst the
overlay blinks. _Tapping_ and _Either_ are available but a bad match for a
modifier-only chord, since a quick ⌥⌘ shortcut would lock recording on.
Holding ⌘⌥ longer while doing something else (⌥⌘-drag in Finder) will
record; pick a keyed shortcut if that's your habit.

**While you dictate** (Settings → General): _Leave_, _Mute_ or _Pause_.
Mute is Handy's own mute, unchanged: the Mac's output goes silent while the
mic is open, then comes back (a Mac that was already muted stays muted).
Pause presses the keyboard's play/pause key when recording starts and again
when it stops, so music and videos stop and resume where they were. Because
that key starts Music when nothing is playing, it is only pressed when
Core Audio reports another app producing sound right now and that app is a
media player or browser (macOS 14.2+ lists audio clients per process). If a
call app is producing sound (Zoom, Teams, FaceTime, WeChat, Discord, Slack,
Feishu…) nothing is pressed. On stop it only presses again if nothing has
started playing in the meantime, so media you resumed by hand stays on.

**Sounds** are soft G♯ tones (start G♯4, stop G♯3 or G♯5 depending on the
theme, landing G♯), about 10 dB quieter than Handy's, with a volume slider
in Settings → General.

## Home

A greeting, four running totals (words, time saved, dictations, day
streak), the two shortcuts with Change buttons, whether cleanup is working
(with the exact terminal lines to fix it when it isn't), and the last four
dictations, one click to copy. Totals live in `stats.json` next to the other
files, so they survive History's 50-entry limit. Time saved assumes 40 wpm
typing against 130 wpm speaking. Command-mode answers don't count.

## Command mode

Select text, hold **⌃⌥⌘** (change it under Settings → General), say what to do,
release. Release, not press, triggers the copy: L.O.B.A waits for the
physical modifiers to come up (a synthetic ⌘C with ⌥⇧ still held is ⌥⇧⌘C to
the target app), grabs the selection through a marker-checked ⌘C, restores
your clipboard, then asks the model. The answer is pasted over the selection.
No selection → the answer is inserted at the cursor.

Failure (timeout, 15 s by default via `command_timeout_ms`; server down; empty
answer, Esc) pastes nothing and shows why; your selection is untouched.
Clipboard restore is text-only: an image on the clipboard before a command is
not preserved.

## History

The last 50 dictations (Handy's `history_limit`, now defaulting to 50),
newest first, grouped by day. Each row shows the destination app and style,
a dot for what the pipeline did (green cleaned · orange fell back · grey not
rewritten), the pasted text (click it to copy) and, when cleanup changed
anything, the raw transcript one click away with its own copy button.
Metadata lives in a side table (`loba_meta`) so Handy's schema and queries are
unchanged.

## Build

CI builds an ad-hoc-signed `.dmg` on every push touching `loba/`
(`.github/workflows/loba-macos.yml`, artifact **L.O.B.A-macos-arm64-…**).
It is not notarized: first launch needs right-click → Open, or
`xattr -dr com.apple.quarantine /Applications/L.O.B.A.app`.

Locally, two commands:

```sh
cd loba
./scripts/build-mac.sh          # add --keep to keep the build tree for faster rebuilds
./scripts/install-mac.sh        # replaces /Applications/L.O.B.A.app and opens it
```

`build-mac.sh` checks free space first (needs ~6 GB; a lean build measured
2.7 GB. If short it lists exactly which caches are safe to clear and stops),
builds natively without LTO, signs the app, puts the DMG in `~/Downloads`,
then deletes `src-tauri/target`. It also ignores conda and Homebrew's Rust,
which break native builds.

Signing: macOS remembers Accessibility and Input Monitoring per signature,
and an ad-hoc signature changes on every build, so each rebuild used to lose
those permissions. The first local build creates a self-signed certificate,
**L.O.B.A Local Signing**, in your login keychain and signs every build with
it from then on. macOS asks once whether `codesign` may use the key: choose
**Always Allow**. Grant Accessibility once after the first signed install and
it stays granted. The certificate is for running your own builds only; it
can't notarize, and the CI build stays ad-hoc.

`install-mac.sh` takes the newest DMG in `~/Downloads`, detaches any older
L.O.B.A volume still mounted (same name, easy to drag from by mistake),
replaces the app, clears quarantine and opens it. `--reset` also clears
L.O.B.A's permission entries, for when the permission screen sits on
"Waiting" even though the switch is on.

One-time setup: `xcode-select --install`, rustup (`~/.cargo/bin`, not
Homebrew's `rust`), `bun`, `brew install cmake`. Full Xcode is optional:
without it the Apple Intelligence bridge builds as a stub, which L.O.B.A
doesn't use.

## Meetings

**Meetings** in the sidebar → **Start meeting**. A small pill floats over every
app with a timer and a Stop button (drag it anywhere). While it runs:

- **Two tracks, two speakers.** The microphone is "You". The Mac's own sound
  output, which during a call is the other people, is "Others", captured
  with ScreenCaptureKit (audio only, 16 kHz mono; L.O.B.A's own sounds are
  excluded). The first time, macOS asks for **Screen & System Audio
  Recording**. Without it the meeting still records, mic only, and says so.
- **Transcribed as it goes.** Each track is cut at a pause once it holds 8 s
  of audio (at 25 s if nobody pauses), checked for actual speech (Whisper
  invents text on silence, so near-silent chunks are dropped), transcribed
  with the same model as dictation, run through the dictionary, and added
  with its timestamp. Text appears on the Meetings page as you go.
- **Recap at the end.** Stop saves the transcript, then the cleanup model
  writes a recap: Summary, Decisions, Action items (who, what, by when),
  Open questions, in the language the meeting was held in, from the
  transcript only. Long meetings are noted part by part first, then
  recapped, with a 16k-token window so nothing falls off the end. If Ollama
  is down, the transcript is kept and **Write recap** tries again.
- **Stored locally** as one JSON file per meeting in `loba/meetings/`.
  Copy the recap, or recap plus transcript, as Markdown.

Phase 1 tells "You" from "Others". Telling individual people apart within
"Others" (speaker diarization) is the planned next step. Recording other
people may need their consent where you are.

## Release (for other people's Macs)

A build from your own Mac only opens on someone else's after they override
Gatekeeper. For a download page, the app has to carry Apple's notarization:

```sh
./scripts/build-mac.sh --release
```

That signs every binary with your **Developer ID Application** certificate
(hardened runtime, secure timestamp), sends the app to Apple, staples the
ticket to it, builds the DMG, then signs, notarizes and staples the DMG as
well. It ends by printing what Gatekeeper will say on another Mac; you want
`source=Notarized Developer ID` for both.

One-time setup:

1. Join the Apple Developer Program (developer.apple.com, 99 USD a year).
2. Xcode → Settings → Accounts → your team → Manage Certificates → **+** →
   **Developer ID Application**. Check it landed:
   `security find-identity -v -p codesigning`.
3. At account.apple.com → Sign-In and Security → App-Specific Passwords,
   make one called "L.O.B.A notary", then store it in the keychain:

   ```sh
   xcrun notarytool store-credentials LOBA_NOTARY --apple-id YOU@EXAMPLE.COM --team-id TEAMID
   ```

   The Team ID is the 10 characters in brackets after your name in step 2.

The script reads both from the keychain and never sees the password. To
pick a specific certificate or profile, set `LOBA_DEVELOPER_ID` or
`LOBA_NOTARY_PROFILE`. If Apple rejects a submission, the script prints
Apple's log, which names the file and the reason.

This is distribution outside the Mac App Store. The App Store requires the
app sandbox, which doesn't allow a global hotkey that types into other apps,
so L.O.B.A ships the way Typeless and Handy do: a notarized DMG from your own
site.

## Offline

No telemetry, no accounts. The updater has no endpoints and update checks are
off by default. The only network use is the model download you start yourself
in onboarding or Settings → Engine → Models.

## Download page (asmbly.store)

`site/loba-download.html` is a self-contained block for the product page: a
$1 to $25 slider, "Pay $N and download" through Stripe Checkout, and "Skip
this time", which downloads straight away. Set `dmgUrl` and
`checkoutEndpoint` in its `LOBA_DOWNLOAD` config.

`site/loba-checkout.ts` is the server side: one function that creates the
Checkout session for the chosen amount (whole dollars, $1 to $500). It needs
`STRIPE_SECRET_KEY`, `LOBA_SUCCESS_URL` and `LOBA_CANCEL_URL`, and runs as a
Next.js route, a Netlify function, or behind any `fetch` handler.
