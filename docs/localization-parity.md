> Modified for EsanCraft; original notices retained. See MODIFICATIONS.md.

# Localization parity

> **Last reviewed:** 2026-10-19 · **Last updated:** 2026-10-19 · **Change:** minor (Lao text support) · **Target:** Avid Pro Tools Ultimate 2026.4.1

SoundCraft's interface is English only. There is no string catalog: every menu label, dialog and
tooltip is a literal in Rust (`crates/engine/src/cmd/*`, `crates/ui-egui/src/*`). Pro Tools ships its UI in
**8 languages** (bundle `.lproj` folders: English, German, Spanish, French, Japanese, Korean,
Simplified Chinese, Traditional Chinese).

**Localization: ~12 %** (measured: 1 of the target's 8 UI languages, and English is the only one
complete). Remaining ≈ 70–110 h: a catalog and extraction pass (≈ 15–25 h), bidi layout,
visual cursor mapping and platform input verification (≈ 25–40 h), then ≈ 3–5 h of agent
translation per language, plus native-speaker review (human).

Open work: PR #143 (French and Spanish, a Language setting), issue #67 (Ukrainian).

Bundled fonts cover Lao, Thai, CJK, Arabic, Hebrew and Indic scripts on native platforms and
Web/WASM. egui 0.36 shapes combining marks and contextual glyphs; full bidi layout remains
limited. Lao coverage includes letters, vowels, tone marks and digits in regular, bold and
monospace text, typing and paste in real rename dialogs, and session serialization.
This is text support; Lao application menus remain untranslated. See [text support](text-support.md).

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Est. to full (h) | In Pro Tools |
|---|---|---|---|---|---|---|---:|---|
| English | en | all (source) | ✅ | Latin ✅ | — | **full** | 0 | ✅ |
| Simplified Chinese | zh-Hans | 0 (0 %) | ❌ | 🟡 bundled font; platform IME unverified | no | none | 6–10 | ✅ |
| Spanish | es | 0 (0 %) — PR #143 pending | ❌ | Latin ✅ | no | none | 3–5 | ✅ |
| Hindi | hi | 0 (0 %) | ❌ | 🟡 bundled font and shaping; input unverified | no | none | 8–15 | ❌ |
| Arabic | ar | 0 (0 %) | ❌ | 🟡 bundled font and shaping; bidi limited | no | none | 15–25 | ❌ |
| French | fr | 0 (0 %) — PR #143 pending | ❌ | Latin ✅ | no | none | 3–5 | ✅ |
| Portuguese | pt | 0 (0 %) | ❌ | Latin ✅ | no | none | 3–5 | ❌ |
| Indonesian | id | 0 (0 %) | ❌ | Latin ✅ | no | none | 3–5 | ❌ |
| Japanese | ja | 0 (0 %) | ❌ | 🟡 bundled font; platform IME unverified | no | none | 6–10 | ✅ |
| German | de | 0 (0 %) | ❌ | Latin ✅ | no | none | 3–5 | ✅ |
| Korean | ko | 0 (0 %) | ❌ | 🟡 bundled font; platform IME unverified | no | none | 6–10 | ✅ |
| Vietnamese | vi | 0 (0 %) | ❌ | 🟡 Latin with stacked diacritics, untested | no | none | 3–5 | ❌ |
| Lao | lo | 0 (0 %) | ❌ | ✅ bundled font; combining marks, typing, paste and serialization tested | no | text support; UI untranslated | not estimated | ❌ |

Other languages shipped: **0**. (Pro Tools also ships Traditional Chinese, zh-Hant.)

Per-language hours exclude the shared catalog and script work above.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-19 | minor | Lao text support added with bundled fonts and typing/paste/save coverage; UI translations remain at 0 % |
| 2026-10-10 | major | Created: English only, no catalog; the twelve key languages at 0 %; Pro Tools' 8 languages listed from its bundle |
