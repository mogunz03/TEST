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

| Path                  | What                                                                                         |
| --------------------- | -------------------------------------------------------------------------------------------- |
| `crates/loba-core/`   | The pipeline. No Tauri; `cargo test` runs in seconds.                                        |
| `src-tauri/src/loba/` | Wiring: state, Tauri commands, the seam in `actions.rs`.                                     |
| `src/loba/`           | The UI shell: Home, History, Dictionary, Styles, Cleanup; Handy's settings under **Engine**. |

## Speech models

- **Whisper large-v3-turbo** (GGUF, Metal) is the recommended default: auto
  language detect, handles Mandarin/English/French in one session.
- **SenseVoice Small (ONNX)** is selectable in Engine → Models, for Mandarin-heavy
  dictation. It covers zh / yue / en / ja / ko only — **no French**.
- Code-switching inside one utterance: Whisper detects one language per 30 s
  window. Embedded words from another language usually come through in their
  own script; long clause-level switches can come out transliterated or
  translated, and no cleanup pass can recover words the ASR lost. Leave the
  language on _Auto_ — forcing one hurts mixed speech more than it helps.

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
- Hard budget (default 4 s, configurable). On timeout, server down, or an
  answer that looks wrong (grew, gained a preamble, changed script = likely
  translated), the raw transcript is pasted instead.

Files in `~/Library/Application Support/app.loba.desktop/loba/`:

- `config.json` — `cleanup_enabled`, `model`, `timeout_ms`, `ollama_url`, `keep_alive`.
- `dictionary.json` — your terms and how they get misheard:

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
reads as formal) → `default`. `rewrite: false` skips the LLM entirely —
dictionary rules only — so a terminal command is never "improved". Mappings to
a style that doesn't exist are dropped on load. **Styles → Check an app**
waits 3 s, then shows what L.O.B.A sees in whatever app you switched to.

## Command mode

Select text, hold **⌥⌘/** (rebind in Engine → General), say what to do,
release. Release — not press — triggers the copy: L.O.B.A waits for the
physical modifiers to come up (a synthetic ⌘C with ⌥⇧ still held is ⌥⇧⌘C to
the target app), grabs the selection through a marker-checked ⌘C, restores
your clipboard, then asks the model. The answer is pasted over the selection.
No selection → the answer is inserted at the cursor.

Failure (timeout — 15 s by default, `command_timeout_ms` — server down, empty
answer, Esc) pastes nothing and shows why; your selection is untouched.
Clipboard restore is text-only: an image on the clipboard before a command is
not preserved.

## History

The last 50 dictations (Handy's `history_limit`, now defaulting to 50),
newest first, grouped by day. Each row shows the destination app and style,
a dot for what the pipeline did (green cleaned · orange fell back · grey not
rewritten), the pasted text — click it to copy — and, when cleanup changed
anything, the raw transcript one click away with its own copy button.
Metadata lives in a side table (`loba_meta`) so Handy's schema and queries are
unchanged.

## Build

CI builds an ad-hoc-signed `.dmg` on every push touching `loba/`
(`.github/workflows/loba-macos.yml`, artifact **L.O.B.A-macos-arm64-…**).
It is not notarized: first launch needs right-click → Open, or
`xattr -dr com.apple.quarantine /Applications/L.O.B.A.app`.

Locally:

```sh
cd loba
bun install
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build --target aarch64-apple-darwin --bundles app,dmg
```

## Offline

No telemetry, no accounts. The updater has no endpoints and update checks are
off by default. The only network use is the model download you start yourself
in onboarding / Engine → Models.
