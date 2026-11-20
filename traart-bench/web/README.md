# The bench in a browser tab

The same panel, compiled to WebAssembly. From `traart-bench/`:

```
rustup target add wasm32-unknown-unknown
trunk serve            # http://127.0.0.1:8080, rebuilds on save
trunk build --release --public-url ./     # dist/, ready for any static host
```

`index.html` one level up is trunk's entry point. GitHub Pages publishes the
result at `/bench/` (`.github/workflows/pages.yml`).

What differs from the desktop build, and why:

| desktop | browser |
| --- | --- |
| cpal | Web Audio; silent until the page is first clicked, as browsers insist |
| `tunings.json` in the settings directory | this browser's local storage, seeded from `presets.json` |
| Save WAV writes to `~/Downloads` | Save WAV is a download |
| `--serve`, `--render`, `--stats`, `--selftest` | not there: a tab has no arguments and cannot listen |
| Times New Roman and Courier New from the system | `fonts/serif.ttf` and `fonts/mono.ttf`, fetched at start |

## Fonts

A tab cannot read the system's fonts, so the page fetches two files from
`fonts/` and falls back to egui's own face when they are missing. They are
not in git. The Pages workflow copies Liberation Serif and Liberation Mono
(SIL Open Font License, metric-compatible with the two faces the desktop build
uses) into `web/fonts/` before building. For a local build that looks the
same, copy any serif and monospace TrueType file there under those names:

```
cp "/System/Library/Fonts/Supplemental/Times New Roman.ttf" web/fonts/serif.ttf
cp "/System/Library/Fonts/Supplemental/Courier New.ttf"     web/fonts/mono.ttf
```
