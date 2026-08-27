# UI Color Conventions — Canonical Reference

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

This document is the **single authoritative source** for every color used in the platform UI. Any component, indicator spec, or state-machine doc that references a color must resolve to the semantic categories defined here. No other document may introduce a new color meaning without updating this reference.

---

## 1. Semantic Color Categories

Every color in the UI carries exactly **one** of the following five semantic meanings. Colors are never overloaded across categories.

| Color | Semantic meaning | Used for | Explicitly NOT used for |
|-------|-----------------|----------|------------------------|
| **Red** | Bearish | Downside pressure, selling activity, long liquidations, bearish signals, negative bias, cascade danger (`Sustained`), bearish acceleration (bright red), bearish deceleration (dark red) | Error states, failed connections, broken pipelines |
| **Green** | Bullish | Upside momentum, buying activity, short liquidations, bullish signals, positive market bias, bullish acceleration (bright green), bullish deceleration (dark green) | Connected status, healthy indicators, all-clear |
| **Amber / Gold** | Range / Neutral / Sideways / Risky | Range-bound markets, neutral bias (±score threshold), sideways consolidation, transitional states, stale data, loading indicators, cascade detection (`Detected`), moderate risk thresholds, warning banners | — |
| **Grey** | Error / Disabled | FAILED pipeline state, calculator errors, stopped instances, disabled-by-config indicators, silent-state indicators, secondary / dim text | Market direction, connected state |
| **Blue** | Connected / Safe / All-OK | LIVE pipeline state, Live indicator status, connected exchanges, healthy connection quality, cascade recovery (`Exhausted`), safe / nominal state | Market direction |

### 1.1 The four canonical histogram colors (MACD, Squeeze)

Two indicator specs ([04-02-17-macd.md](../engines/market-monitoring-engine/indicators/04-02-17-macd.md), [04-02-28-squeeze.md](../engines/market-monitoring-engine/indicators/04-02-28-squeeze.md)) share a four-color histogram convention:

| Hex | Tone | Meaning |
|-----|------|---------|
| `#26a69a` | Light Green | Bullish acceleration (active) |
| `#00695c` | Dark Green | Bullish deceleration (exhausted) |
| `#ff1744` | Bright Red | Bearish acceleration (active) |
| `#b71c1c` | Dark Red | Bearish deceleration (exhausted) |

The unified semantic is **bright = active threat, dark = exhausted** — directional expansion is the threat; directional contraction is the release.

### 1.2 System state badge colors

Pipeline and indicator lifecycle state badges follow the semantic categories above:

| State | Badge color | Category |
|-------|------------|----------|
| LIVE / Connected / Running | **Blue** | Connected / Safe / All-OK |
| STALE / Loading / WAITING FEED | **Amber** | Range / Neutral / Sideways / Risky |
| FAILED / Error | **Grey** | Error / Disabled |
| STOPPED / Silent / Disabled | **Grey** | Error / Disabled |

### 1.3 Connection quality threshold colors

The `ConnectionQualityPanel` color-bands scores and uptime by degree of health using the blue category for healthy and grey for failure ([08-05](../operations-and-compliance/08-05-connection-quality.md)):

| Score | Color | Category |
|-------|-------|----------|
| ≥ 90 | **Blue** | Connected / Safe |
| ≥ 75 | **Blue** (lighter) | Connected / Safe |
| ≥ 50 | **Amber** | Risky |
| < 50 | **Grey** | Error |

| Uptime | Color | Category |
|--------|-------|----------|
| ≥ 99 % | **Blue** | Connected / Safe |
| ≥ 95 % | **Blue** (lighter) | Connected / Safe |
| < 95 % | **Grey** | Error |

---

## 2. Design Rules

### 2.1 Color is never the sole carrier of meaning

Any element whose meaning is expressed through a color must also carry a text label, numeric badge, icon, or border style that communicates the same information. This rule is enforced across all panels (see [07-04 §2](07-04-ui-liquidity-panel-spec.md) line 308). A user who cannot perceive the color must still receive the full semantic signal.

### 2.2 No emojis as color substitutes

The platform uses no emojis to carry color semantics. The green/red dot emojis appearing in doc code-blocks are illustrative shorthand for human readers; the actual UI renders CSS-colored dots, badges, borders, and text.

### 2.3 Bright = active threat, dark = exhausted

For directional market colors (red and green), **bright tones signal active / accelerating conditions** (the threat is present and building), and **dark tones signal decelerating / exhausting conditions** (the move is fading). This convention is identical across MACD and TTM Squeeze.

---

## 3. Cross-Reference Index

Every file in the docs corpus that references colors should explicitly note its conformance to this document. The following files are the canonical anchors:

| File | What it defines | Status |
|------|----------------|--------|
| `07-06-ui-color-conventions.md` | **This document** — canonical semantic mapping | Authoritative |
| `07-02-ui-dashboard-layout.md` §10 | Shell design tokens (backgrounds, text, lines, buttons, brand accent) | ✅ Conforms |
| `07-04-ui-liquidity-panel-spec.md` §2 | Liquidity-specific semantic tokens (`.bullish`, `.bearish`, cascade badge mappings) | ✅ Conforms |
| `04-02-17-macd.md` §Visual Chart Annotation | Four-color histogram convention | ✅ Conforms |
| `04-02-28-squeeze.md` §Four-Color Histogram Key | Four-color histogram convention (cross-referenced to MACD) | ✅ Conforms |
| `03-02-15-mme-indicator-lifecycle-states.md` ILS-15 | Lifecycle badge colors (green → blue, red → grey) | ⚠️ Needs update to match §1.2 |
| `03-01-06-die-candle-pipeline-states.md` | Pipeline state badge colors (green → blue, red → grey) | ⚠️ Needs update to match §1.2 |
| `08-05-connection-quality.md` | Score/uptime threshold colors (green → blue, red → grey) | ⚠️ Needs update to match §1.3 |
| `03-02-09-mme-indicators-guide.md` | Lifecycle badge colors (green → blue) | ⚠️ Needs update to match §1.2 |
| `08-01-user-manual.md` | Environment header color-coded by `directional_guidance` | ✅ Conforms |
| `03-02-07-mme-layer6-decision-support.md` | Environment header color-coded by `directional_guidance` | ✅ Conforms |

**Legend.** ✅ Conforms = already maps to the semantic categories in §1. ⚠️ Needs update = currently uses green for connected and red for error; must be updated to blue for connected and grey for error per §1.

### 2.4 Direction-first discipline (v10.1)

> **Amendment v10.1 (quant-metrics hardening).** Green = LONG only, red = SHORT only, amber = every caution/state (dashed = broken bracket), grey = informational. Reference brackets are amber/grey (never red); geometry-inverted is dashed amber; STOP-LOSS rows are red only on actionable cards; scores are 3-band green/amber/grey; confluence tags never wear direction colors; evaluated-setup cards tint by resolved side. `riskDangerColor` keeps danger-red. This refines §1 without changing the five semantic categories.

### 3.1 Indicator-specific color references

The following indicator specs reference colors for chart rendering. All directional colors (red/green) conform to the §1 bearish/bullish mapping. Any implementation that renders these chart overlays must use the hex values declared in each spec.

| File | Colors defined |
|------|---------------|
| `04-02-05-adx.md` | +DI green, −DI red; ADX line grey/amber/red (congestion/accelerating/decelerating/exhaustion) |
| `04-02-08-ichimoku.md` | Tenkan-sen magenta, Kijun-sen blue, Senkou A green, Senkou B red, Chikou purple; cloud fill green/red for bullish/bearish |
| `04-02-15-awesome-oscillator.md` | Bar green when rising, red when falling |
| `04-02-25-atr.md` | Bright green (expanding), grey (stable), dark red (contracting) |
| `04-02-33-pivot-points.md` | R1–R3 red, central pivot brown, S1–S3 green |
| `04-02-35-candlestick.md` | Confirmed patterns green ▲ below or red ▼ above; merely-formed circles |
| `03-02-13-mme-volume-profile-layer.md` | BUY green `rgba(38,166,154,0.85)`, SELL red `rgba(239,83,80,0.85)`, POC yellow border |
| `03-02-12-mme-configurable-activation.md` | Disabled indicator pane greyed |

---

## 4. Palette Cheat Sheet

For quick reference, here is every hex value used anywhere in the UI mapped to its semantic category:

| Hex | Name | Semantic category | Where used |
|-----|------|-------------------|-----------|
| `#26a69a` | Light Green / Teal | **Green — Bullish** | Bullish acceleration (MACD, Squeeze), `.bullish` class, VOL PROFILE buy bars |
| `#00695c` | Dark Green | **Green — Bullish** | Bullish deceleration (MACD, Squeeze) |
| `#ef5350` | Red | **Red — Bearish** | `.bearish` class, cascade danger, VOL PROFILE sell bars |
| `#ff1744` | Bright Red | **Red — Bearish** | Bearish acceleration (MACD, Squeeze) |
| `#b71c1c` | Dark Red | **Red — Bearish** | Bearish deceleration (MACD, Squeeze) |
| `#10b981` | Emerald | **Green — Bullish** | ATR expanding |
| `#ef4444` | Rose Red | **Red — Bearish** | ATR contracting |
| `#f59e0b` | Amber | **Amber — Neutral / Risky** | WAITING FEED pill |
| `#ffb74d` | Light Amber | **Amber — Neutral / Risky** | Cascade detected (`cascadeWarning` text) |
| `#42a5f5` | Blue | **Blue — Connected / Safe** | Drag handle hover, LIVE badge |
| `#4dd0e1` | Cyan | **Blue — Connected / Safe** | Cascade exhausted (`cascadeCooling` text) |
| `#3b82f6` | Blue | **Blue — Connected / Safe** | Alignment dimension: MOMENTUM |
| `#22c55e` | Green | **Green — Bullish** | Alignment dimension: TREND |
| `#a78bfa` | Purple | — (structure) | Alignment dimension: VOL.TREND |
| `#facc15` | Yellow | **Amber — Neutral / Risky** | Magnet gradient start |
| `#f97316` | Orange | **Amber — Neutral / Risky** | Magnet gradient end |
| `#8f929d` | Grey | **Grey — Error / Disabled** | ATR stable, ADX congestion |
| `#f5f5f7` | White | — (neutral text) | Primary text |
| `#000000` | Black | — (background) | `--bg` |
| `#0a0a0a` | Near-black | — (background) | `--bg-elev` |
| `#0f0f0f` | Deeper near-black | — (background) | `--bg-elev-2` |
| `#64ffda` | Mint | — (brand) | Brand trigger accent bar |
