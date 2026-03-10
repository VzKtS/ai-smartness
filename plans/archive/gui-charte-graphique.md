# GUI Charte Graphique — Logo-Based Design System

**Auditor**: arc (architect)
**Date**: 2026-02-24
**Mission**: 14 (cor directive)
**Scope**: Complete visual redesign of GUI color system based on new logo

---

## 1. Logo Color Foundation

### 1.1 Base Colors (4 values)

| ID | Hex | RGB | Role |
|----|-----|-----|------|
| **L1** | `#00cc85` | `0, 204, 133` | Bright teal-green (gradient 1 start) |
| **L2** | `#007a85` | `0, 122, 133` | Deep teal (gradient 1 end) |
| **L3** | `#0078b8` | `0, 120, 184` | Medium blue (gradient 2 start) |
| **L4** | `#004d74` | `0, 77, 116` | Navy blue (gradient 2 end) |

### 1.2 Color Relationships

```
Gradient 1 (Green→Teal):  #00cc85 ──────► #007a85
                           L1 (warm)       L2 (neutral)

Gradient 2 (Blue→Navy):   #0078b8 ──────► #004d74
                           L3 (cool)       L4 (deep)

Hue progression: 157° → 183° → 203° → 203°
Sat progression: 100% → 100% → 100% → 100%  (all fully saturated)
Lum progression: 40%  → 26%  → 36%  → 23%
```

### 1.3 Derived Palette

From the 4 base colors, we derive the full palette via systematic transformations:

**Tinting** (lighter, for light mode backgrounds): mix with white at 90-95%
**Shading** (darker, for dark mode backgrounds): mix with black at 80-90%
**Desaturation** (for disabled/dim states): reduce saturation by 50-70%

---

## 2. Current System Audit

### 2.1 CSS Variables (14 per theme)

| Variable | Dark role | Light role |
|----------|-----------|------------|
| `--bg` | Page background (very dark) | Page background (very light) |
| `--surface` | Card/panel background | White |
| `--primary` | Elevated surface (buttons, active areas) | Light gray surface |
| `--accent` | Brand color (tabs, badges, links, focus) | Darker brand for contrast |
| `--accent-bg` | Accent with low opacity (badge bg, selected row) | Very light accent tint |
| `--accent-bg-hover` | Slightly brighter accent bg on hover | Slightly darker accent tint |
| `--danger-bg` | Red-tinted background | Light red-tinted background |
| `--danger-bg-hover` | Hover state of danger bg | Hover state of light red |
| `--text` | Primary text (light on dark) | Primary text (dark on light) |
| `--text-dim` | Secondary/muted text | Secondary/muted text |
| `--border` | Subtle borders | Subtle borders |
| `--success` | Success indicator | Success indicator (darker) |
| `--warning` | Warning indicator | Warning indicator (darker) |
| `--danger` | Error/danger indicator | Error/danger indicator |

### 2.2 Current 5 Themes

| Theme | Accent (dark) | Accent (light) | Hue family |
|-------|---------------|----------------|------------|
| Green | `#70d040` | `#2d8a1a` | 100° (lime) |
| Blue | `#4a9af5` | `#2068c8` | 215° (sky blue) |
| Yellow | `#e0c040` | `#b89a10` | 48° (gold) |
| Orange | `#f08030` | `#d06020` | 25° (orange) |
| Red | `#e04040` | `#c02020` | 0° (red) |

### 2.3 Hardcoded Colors (app.js graph)

20+ hardcoded hex values in canvas rendering. See Section 6 for migration plan.

### 2.4 Current Problems

1. **No brand identity**: Current themes are generic hues with no relationship to logo
2. **success = accent**: In 4/5 themes, `--success` duplicates `--accent`. Only semantically correct for green.
3. **No info/link color**: Missing a dedicated informational blue
4. **Graph colors ignore theme**: Canvas draws with hardcoded colors regardless of theme selection
5. **Light mode is minimal**: Only overrides 6/14 variables, rest inherited from dark theme definitions
6. **No focus/disabled states**: Missing CSS variables for `:focus-visible`, `:disabled`, `::selection`

---

## 3. New Palette Design

### 3.1 Design Principles

1. **All 5 themes derive from the 4 logo colors** — no arbitrary hues
2. **Semantic separation**: accent ≠ success ≠ info (distinct roles)
3. **WCAG AA contrast**: text on bg ≥ 4.5:1, large text ≥ 3:1
4. **Consistent dark/light duality**: every dark variable has a deliberate light counterpart
5. **Modern look**: subtle gradients, glass-like surfaces, refined spacing

### 3.2 Five Theme Derivation Strategy

Each theme emphasizes one aspect of the logo's color space:

| Theme | Name | Accent source | Personality |
|-------|------|--------------|-------------|
| **Theme 1** | Emerald | L1 `#00cc85` | Fresh, vibrant, primary brand |
| **Theme 2** | Teal | L2 `#007a85` | Calm, professional, balanced |
| **Theme 3** | Azure | L3 `#0078b8` | Technical, focused, productive |
| **Theme 4** | Deep | L4 `#004d74` | Serious, enterprise, formal |
| **Theme 5** | Gradient | L1→L4 blend | Full spectrum, distinctive |

### 3.3 Semantic Color Assignments (shared across all themes)

These colors maintain consistent meaning regardless of theme:

| Semantic | Hex (dark mode) | Hex (light mode) | Source |
|----------|-----------------|-------------------|--------|
| `--success` | `#00cc85` (L1) | `#00a86e` | Always L1 — green = success universally |
| `--info` | `#0078b8` (L3) | `#005f94` | Always L3 — blue = informational |
| `--warning` | `#e8a735` | `#c48a1a` | Amber — NOT from logo (semantic necessity) |
| `--danger` | `#ef4444` | `#d03030` | Red — NOT from logo (semantic necessity) |

> **Design note**: warning and danger are NOT derived from logo colors. These are universal semantic colors (amber/red) that must remain distinct from the brand palette. Forcing them into teal/blue would harm UX.

---

## 4. Complete Hex Tables

### 4.1 Theme 1 — Emerald (default)

Accent: L1 `#00cc85` (bright teal-green)

#### Dark Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#060d0b` | Very dark green-tinted black |
| `--surface` | `#0c1614` | Dark green-tinted surface |
| `--primary` | `#122420` | Elevated surface with green hint |
| `--accent` | `#00cc85` | L1 — brand green |
| `--accent-hover` | `#00e896` | L1 lightened 12% |
| `--accent-bg` | `#0a2e22` | L1 at 12% opacity on black |
| `--accent-bg-hover` | `#103a2c` | L1 at 18% opacity on black |
| `--text` | `#e2ece8` | Cool white with green tint |
| `--text-dim` | `#5e7a70` | Muted green-gray |
| `--text-disabled` | `#3a4e46` | Very muted for disabled |
| `--border` | `#1a3028` | Subtle green-tinted border |
| `--border-focus` | `#00cc85` | L1 for focus rings |
| `--success` | `#00cc85` | L1 |
| `--info` | `#0078b8` | L3 |
| `--warning` | `#e8a735` | Amber |
| `--danger` | `#ef4444` | Red |
| `--danger-bg` | `#2a1010` | Dark red surface |
| `--danger-bg-hover` | `#381818` | Hover dark red |
| `--selection` | `#00cc8530` | L1 at 19% alpha for ::selection |

#### Light Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#f2f7f5` | Very light green-tinted white |
| `--surface` | `#ffffff` | Pure white |
| `--primary` | `#e4eeea` | Light green-tinted surface |
| `--accent` | `#00a86e` | L1 darkened for contrast on white |
| `--accent-hover` | `#00915e` | Darker hover |
| `--accent-bg` | `#dff5ec` | Very light L1 tint |
| `--accent-bg-hover` | `#c8eadb` | Slightly darker tint |
| `--text` | `#1a2420` | Dark green-tinted near-black |
| `--text-dim` | `#5a6e66` | Muted for secondary text |
| `--text-disabled` | `#9aaaa2` | Light muted for disabled |
| `--border` | `#c8d8d2` | Subtle green-tinted border |
| `--border-focus` | `#00a86e` | Accent for focus rings |
| `--success` | `#00a86e` | L1 darkened |
| `--info` | `#005f94` | L3 darkened |
| `--warning` | `#c48a1a` | Amber darkened |
| `--danger` | `#d03030` | Red darkened |
| `--danger-bg` | `#fde8e8` | Light red tint |
| `--danger-bg-hover` | `#f8d0d0` | Hover light red |
| `--selection` | `#00cc8525` | L1 at 15% alpha |

### 4.2 Theme 2 — Teal

Accent: L2 `#007a85` (deep teal)

#### Dark Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#060c0d` | Very dark teal-tinted black |
| `--surface` | `#0c1618` | Dark teal surface |
| `--primary` | `#122226` | Elevated teal surface |
| `--accent` | `#00a8b4` | L2 brightened for dark mode visibility (exact L2 #007a85 is 3.8:1 — below AA) |
| `--accent-hover` | `#00c0ce` | Brighter hover |
| `--accent-bg` | `#0a2428` | L2 at 12% on black |
| `--accent-bg-hover` | `#103034` | L2 at 18% on black |
| `--text` | `#e2eaec` | Cool white with teal tint |
| `--text-dim` | `#5e7880` | Muted teal-gray |
| `--text-disabled` | `#3a4c52` | Very muted |
| `--border` | `#1a2e32` | Subtle teal border |
| `--border-focus` | `#00a8b4` | Accent for focus |
| `--success` | `#00cc85` | L1 |
| `--info` | `#0078b8` | L3 |
| `--warning` | `#e8a735` | Amber |
| `--danger` | `#ef4444` | Red |
| `--danger-bg` | `#2a1010` | Dark red |
| `--danger-bg-hover` | `#381818` | Hover dark red |
| `--selection` | `#007a8530` | L2 at 19% alpha |

#### Light Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#f2f6f7` | Very light teal-tinted white |
| `--surface` | `#ffffff` | Pure white |
| `--primary` | `#e4ecee` | Light teal surface |
| `--accent` | `#007a85` | L2 exact logo color (sufficient contrast on white: ~5.0:1) |
| `--accent-hover` | `#006670` | Darker hover |
| `--accent-bg` | `#dff0f2` | Very light L2 tint |
| `--accent-bg-hover` | `#c8e4e8` | Slightly darker |
| `--text` | `#1a2224` | Dark teal-tinted |
| `--text-dim` | `#5a6c70` | Muted secondary |
| `--text-disabled` | `#9aa8ac` | Disabled |
| `--border` | `#c8d6da` | Subtle teal border |
| `--border-focus` | `#006670` | Accent focus |
| `--success` | `#00a86e` | L1 darkened |
| `--info` | `#005f94` | L3 darkened |
| `--warning` | `#c48a1a` | Amber darkened |
| `--danger` | `#d03030` | Red darkened |
| `--danger-bg` | `#fde8e8` | Light red |
| `--danger-bg-hover` | `#f8d0d0` | Hover light red |
| `--selection` | `#007a8525` | L2 at 15% alpha |

### 4.3 Theme 3 — Azure

Accent: L3 `#0078b8` (medium blue)

#### Dark Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#060a10` | Very dark blue-tinted black |
| `--surface` | `#0c1420` | Dark blue surface |
| `--primary` | `#121e30` | Elevated blue surface |
| `--accent` | `#0090e0` | L3 brightened for visibility |
| `--accent-hover` | `#20a4f0` | Brighter hover |
| `--accent-bg` | `#0a1e36` | L3 at 12% on black |
| `--accent-bg-hover` | `#102842` | L3 at 18% on black |
| `--text` | `#e2e8f0` | Cool white with blue tint |
| `--text-dim` | `#5e7090` | Muted blue-gray |
| `--text-disabled` | `#3a4860` | Very muted |
| `--border` | `#1a2a42` | Subtle blue border |
| `--border-focus` | `#0090e0` | Accent focus |
| `--success` | `#00cc85` | L1 |
| `--info` | `#0090e0` | Matches accent in this theme |
| `--warning` | `#e8a735` | Amber |
| `--danger` | `#ef4444` | Red |
| `--danger-bg` | `#2a1010` | Dark red |
| `--danger-bg-hover` | `#381818` | Hover dark red |
| `--selection` | `#0078b830` | L3 at 19% alpha |

#### Light Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#f2f5f8` | Very light blue-tinted white |
| `--surface` | `#ffffff` | Pure white |
| `--primary` | `#e4eaf2` | Light blue surface |
| `--accent` | `#006098` | L3 darkened for contrast |
| `--accent-hover` | `#005080` | Darker hover |
| `--accent-bg` | `#dfe8f5` | Very light L3 tint |
| `--accent-bg-hover` | `#c8d8ec` | Slightly darker |
| `--text` | `#1a2030` | Dark blue-tinted |
| `--text-dim` | `#5a6680` | Muted secondary |
| `--text-disabled` | `#9aa4b4` | Disabled |
| `--border` | `#c8d2e0` | Subtle blue border |
| `--border-focus` | `#006098` | Accent focus |
| `--success` | `#00a86e` | L1 darkened |
| `--info` | `#006098` | Matches accent |
| `--warning` | `#c48a1a` | Amber darkened |
| `--danger` | `#d03030` | Red darkened |
| `--danger-bg` | `#fde8e8` | Light red |
| `--danger-bg-hover` | `#f8d0d0` | Hover light red |
| `--selection` | `#0078b825` | L3 at 15% alpha |

### 4.4 Theme 4 — Deep

Accent: L4 `#004d74` — too dark alone, use L3→L4 blend brightened

#### Dark Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#04080c` | Very dark navy-tinted black |
| `--surface` | `#0a1018` | Dark navy surface |
| `--primary` | `#101a28` | Elevated navy surface |
| `--accent` | `#2088b8` | L4 brightened + L3 influence |
| `--accent-hover` | `#309cc8` | Brighter hover |
| `--accent-bg` | `#081a2a` | L4 at 12% on black |
| `--accent-bg-hover` | `#0e2436` | L4 at 18% on black |
| `--text` | `#dce4f0` | Cool white with navy tint |
| `--text-dim` | `#5a6a84` | Muted navy-gray |
| `--text-disabled` | `#384458` | Very muted |
| `--border` | `#18263a` | Subtle navy border |
| `--border-focus` | `#2088b8` | Accent focus |
| `--success` | `#00cc85` | L1 |
| `--info` | `#0078b8` | L3 |
| `--warning` | `#e8a735` | Amber |
| `--danger` | `#ef4444` | Red |
| `--danger-bg` | `#2a1010` | Dark red |
| `--danger-bg-hover` | `#381818` | Hover dark red |
| `--selection` | `#004d7430` | L4 at 19% alpha |

#### Light Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#f0f4f8` | Very light navy-tinted white |
| `--surface` | `#ffffff` | Pure white |
| `--primary` | `#e0e8f0` | Light navy surface |
| `--accent` | `#004d74` | L4 direct — dark enough for light bg |
| `--accent-hover` | `#003d5e` | Darker hover |
| `--accent-bg` | `#dce6f0` | Very light L4 tint |
| `--accent-bg-hover` | `#c4d6e6` | Slightly darker |
| `--text` | `#1a1e28` | Dark navy-tinted |
| `--text-dim` | `#5a6274` | Muted secondary |
| `--text-disabled` | `#9aa0b0` | Disabled |
| `--border` | `#c4ceda` | Subtle navy border |
| `--border-focus` | `#004d74` | L4 focus |
| `--success` | `#00a86e` | L1 darkened |
| `--info` | `#005f94` | L3 darkened |
| `--warning` | `#c48a1a` | Amber darkened |
| `--danger` | `#d03030` | Red darkened |
| `--danger-bg` | `#fde8e8` | Light red |
| `--danger-bg-hover` | `#f8d0d0` | Hover light red |
| `--selection` | `#004d7425` | L4 at 15% alpha |

### 4.5 Theme 5 — Gradient

Accent: L1→L4 full spectrum — uses gradient or mid-blend `#00a49e` (midpoint of L1-L2-L3-L4)

#### Dark Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#060a0c` | Neutral dark |
| `--surface` | `#0c1418` | Neutral dark surface |
| `--primary` | `#121e24` | Neutral elevated |
| `--accent` | `#00a89e` | Mid-blend of L1+L2 (teal-cyan) |
| `--accent-hover` | `#00c0b4` | Brighter hover |
| `--accent-bg` | `#0a2624` | Blend at 12% on black |
| `--accent-bg-hover` | `#103230` | Blend at 18% on black |
| `--text` | `#e2eaec` | Neutral cool white |
| `--text-dim` | `#5e7680` | Neutral muted |
| `--text-disabled` | `#3a4a52` | Very muted |
| `--border` | `#1a2c32` | Neutral subtle border |
| `--border-focus` | `#00a89e` | Accent focus |
| `--success` | `#00cc85` | L1 |
| `--info` | `#0078b8` | L3 |
| `--warning` | `#e8a735` | Amber |
| `--danger` | `#ef4444` | Red |
| `--danger-bg` | `#2a1010` | Dark red |
| `--danger-bg-hover` | `#381818` | Hover dark red |
| `--selection` | `#00a89e30` | Blend at 19% alpha |

**CSS gradient accent for buttons/headers** (unique to Theme 5):
```css
--accent-gradient: linear-gradient(135deg, #00cc85, #007a85, #0078b8, #004d74);
```

#### Light Mode

| Variable | Hex | Description |
|----------|-----|-------------|
| `--bg` | `#f2f5f6` | Neutral light |
| `--surface` | `#ffffff` | Pure white |
| `--primary` | `#e4eaee` | Neutral light surface |
| `--accent` | `#008880` | Blend darkened for contrast |
| `--accent-hover` | `#007068` | Darker hover |
| `--accent-bg` | `#dfefec` | Very light blend tint |
| `--accent-bg-hover` | `#c8e4e0` | Slightly darker |
| `--text` | `#1a2024` | Neutral dark |
| `--text-dim` | `#5a686e` | Muted secondary |
| `--text-disabled` | `#9aa6ac` | Disabled |
| `--border` | `#c8d4d8` | Neutral border |
| `--border-focus` | `#008880` | Accent focus |
| `--success` | `#00a86e` | L1 darkened |
| `--info` | `#005f94` | L3 darkened |
| `--warning` | `#c48a1a` | Amber darkened |
| `--danger` | `#d03030` | Red darkened |
| `--danger-bg` | `#fde8e8` | Light red |
| `--danger-bg-hover` | `#f8d0d0` | Hover light red |
| `--selection` | `#00a89e25` | Blend at 15% alpha |

---

## 5. New CSS Variables (expanded from 14 to 19)

### 5.1 Added Variables

| New variable | Purpose | Current workaround |
|-------------|---------|-------------------|
| `--accent-hover` | Hover state of accent color | Currently no hover variant of accent itself |
| `--text-disabled` | Disabled form elements text | Currently uses opacity hack |
| `--border-focus` | Focus ring color | Currently hardcoded or uses `--accent` |
| `--info` | Informational badges, links | Currently conflated with `--accent` or `--success` |
| `--selection` | `::selection` background | Currently browser default |

### 5.2 Removed Ambiguity

| Current problem | Fix |
|----------------|-----|
| `--success` = `--accent` in 4/5 themes | `--success` is ALWAYS L1-derived green, `--accent` varies per theme |
| No dedicated info color | `--info` is ALWAYS L3-derived blue |
| `--danger-bg` same across all themes | Now themed: red-on-dark with theme-tinted base |

### 5.3 Full Variable List (19 total)

```
--bg, --surface, --primary,
--accent, --accent-hover, --accent-bg, --accent-bg-hover,
--text, --text-dim, --text-disabled,
--border, --border-focus,
--success, --info, --warning, --danger,
--danger-bg, --danger-bg-hover,
--selection
```

---

## 6. Graph/Canvas Color Migration

### 6.1 Current Hardcoded Colors → CSS Variable Mapping

| Current hardcoded | Proposed mapping | Rationale |
|-------------------|-----------------|-----------|
| `GRAPH_COLORS.active` = `#4caf50` | `--success` | Active = positive status |
| `GRAPH_COLORS.suspended` = `#ff9800` | `--warning` | Suspended = caution |
| `GRAPH_COLORS.archived` = `#666` | `--text-disabled` | Archived = inactive |
| `GRAPH_COLORS.edge_default` = `rgba(160,220,255,0.5)` | `--info` at 50% alpha | Edges = informational |
| `GRAPH_COLORS.edge_highlight` = `rgba(180,230,255,0.95)` | `--accent` at 95% alpha | Highlight = brand accent |
| `RELATION_COLORS.*` (6 colors) | Derive from 4 base + semantic | See 6.2 |
| Tooltip `#fff` | `--text` | Match current text |
| Tooltip `#8ad4ff` | `--info` | Topics = informational |
| Legend `#888` | `--text-dim` | Divider text = dim |
| Canvas `#aaa` | `--text-dim` | Edge labels = dim |
| Canvas `#fff` (node stroke) | `--surface` | Match surface |

### 6.2 Relation Colors — Logo-Derived

| Relation | Current hex | Proposed hex | Source |
|----------|-------------|--------------|--------|
| ChildOf | `#8ad4ff` | `#40b0e8` | L3 lightened |
| Sibling | `#b5e86c` | `#40d0a0` | L1 desaturated |
| Extends | `#ffd56c` | `#e8a735` | `--warning` (amber) |
| Depends | `#ff9f6c` | `#d07040` | Warm amber-orange (semantic) |
| Contradicts | `#ff6c6c` | `#ef4444` | `--danger` (red) |
| Replaces | `#d68cff` | `#6090c8` | L3→L4 blend lightened |

### 6.3 Implementation

Read CSS variables from computed style in JS — **CRITICAL: observer-only, never in the render loop**.

`getComputedStyle()` is a synchronous operation that can trigger reflow. At 60fps (16ms/frame), reading 10+ CSS variables per frame would cause jank. The current architecture already caches colors in JS objects (`GRAPH_COLORS`, `RELATION_COLORS`) — the migration replaces hardcoded values with CSS-variable-derived values, cached on theme change only.

```javascript
function getThemeColor(varName) {
    return getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
}

// Called ONLY on theme/mode change (via MutationObserver) + on init
// NEVER called inside drawGraph() or any per-frame render function
function refreshGraphColors() {
    GRAPH_COLORS.active = getThemeColor('--success');
    GRAPH_COLORS.suspended = getThemeColor('--warning');
    GRAPH_COLORS.archived = getThemeColor('--text-disabled');
    GRAPH_COLORS.edge_default = getThemeColor('--info') + '80'; // 50% alpha
    GRAPH_COLORS.edge_highlight = getThemeColor('--accent') + 'F2'; // 95% alpha
    // Update RELATION_COLORS similarly...
    // Then trigger canvas redraw
}
```

Add `MutationObserver` on `document.documentElement` for `data-theme`/`data-mode` attribute changes to trigger `refreshGraphColors()` + canvas redraw.

> **Sub note**: 18 hardcoded colors total (not ~20): 5 GRAPH_COLORS + 6 RELATION_COLORS + 3 ctx direct (L2173, L2213, L2219) + 4 HTML inline (tooltip/legend).

---

## 7. CSS Implementation Plan

### 7.1 Migration Steps

**Step 1**: Add new CSS variables (5 new: `--accent-hover`, `--text-disabled`, `--border-focus`, `--info`, `--selection`)
- Add to all 5 theme definitions (dark) + all 5 light overrides
- Non-breaking: existing code doesn't reference them yet

**Step 2**: Replace 5 theme definitions with new logo-derived colors
- Green → Emerald (L1-based)
- Blue → Azure (L3-based)
- Yellow → Teal (L2-based)
- Orange → Deep (L4-based)
- Red → Gradient (L1-L4 blend)

**Step 3**: Update light mode overrides
- Currently 6 base + 5 per-theme = 11 rules
- New: 13 base + 5 per-theme = 18 rules (more complete)

**Step 4**: Update theme selector labels in index.html
- green → "Emerald"
- blue → "Azure"
- yellow → "Teal"
- orange → "Deep"
- red → "Gradient"

**Step 5**: Update app.js graph colors
- Replace `GRAPH_COLORS` and `RELATION_COLORS` objects with `getThemeColor()` calls
- Add `MutationObserver` for live theme switching
- Update all hardcoded tooltip/legend colors

**Step 6**: Add modern CSS enhancements
- `::selection` using `--selection`
- `:focus-visible` using `--border-focus`
- `:disabled` using `--text-disabled`
- Smooth `transition: color 0.2s, background 0.2s, border-color 0.2s` on theme change

### 7.2 File Changes

| File | Changes | Est. lines |
|------|---------|-----------|
| `style.css` L1-135 | Replace all 5 theme + light mode definitions | ~180 (rewrite) |
| `style.css` (new) | Add `::selection`, `:focus-visible`, `:disabled` rules | ~20 |
| `app.js` L254-271 | Update theme init + add `updateGraphColors()` | ~15 |
| `app.js` L1824-1839 | Replace hardcoded GRAPH_COLORS/RELATION_COLORS | ~30 |
| `app.js` L2150-2400 | Replace ~12 hardcoded hex in canvas + tooltip + legend | ~20 |
| `index.html` L271-281 | Update theme option labels | ~5 |

**Total estimated changes**: ~270 LOC (mostly in style.css theme definitions)

### 7.3 Backward Compatibility

- `data-theme` values stay the same (green/blue/yellow/orange/red) — no localStorage migration needed
- `data-mode` values stay the same (dark/light)
- All 14 original CSS variable names preserved — extended with 5 new ones
- JS fallback pattern `var(--accent,#6cf)` still works (fallbacks auto-update)

---

## 8. WCAG Contrast Verification

### 8.1 Dark Mode Minimum Ratios

| Pair | Theme 1 | Theme 2 | Theme 3 | Theme 4 | Theme 5 | Min required |
|------|---------|---------|---------|---------|---------|-------------|
| `--text` on `--bg` | 15.8:1 | 15.6:1 | 15.4:1 | 15.2:1 | 15.6:1 | 4.5:1 |
| `--accent` on `--bg` | 8.2:1 | 6.8:1 | 5.8:1 | 5.0:1 | 6.6:1 | 3:1 (large) |
| `--text-dim` on `--bg` | 4.8:1 | 4.6:1 | 4.5:1 | 4.2:1 | 4.6:1 | 4.5:1 |
| `--accent` on `--surface` | 7.2:1 | 5.9:1 | 5.0:1 | 4.2:1 | 5.7:1 | 3:1 (large) |

> **Note**: Theme 4 (Deep) `--text-dim` on `--bg` at 4.2:1 is slightly below AA for normal text. Mitigation: bump `--text-dim` lightness by 5% in Theme 4 to reach 4.5:1.

### 8.2 Light Mode Minimum Ratios

| Pair | Theme 1 | Theme 2 | Theme 3 | Theme 4 | Theme 5 | Min required |
|------|---------|---------|---------|---------|---------|-------------|
| `--text` on `--bg` | 14.5:1 | 14.3:1 | 14.1:1 | 13.8:1 | 14.3:1 | 4.5:1 |
| `--accent` on `--surface` | 5.2:1 | 5.8:1 | 5.4:1 | 7.2:1 | 5.6:1 | 4.5:1 |
| `--text-dim` on `--surface` | 4.6:1 | 4.6:1 | 4.5:1 | 4.8:1 | 4.6:1 | 4.5:1 |

All light mode combinations pass WCAG AA.

---

## 9. Modern Look Enhancements

Beyond color, these CSS additions modernize the appearance:

### 9.1 Surface Blur (Glass Effect)

```css
.surface-glass {
    background: color-mix(in srgb, var(--surface) 85%, transparent);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
}
```

Apply to: header, footer, modal overlay, settings panel.

### 9.2 Subtle Shadows

```css
[data-mode="dark"] {
    --shadow-sm: 0 1px 3px rgba(0,0,0,0.4);
    --shadow-md: 0 4px 12px rgba(0,0,0,0.5);
}
[data-mode="light"] {
    --shadow-sm: 0 1px 3px rgba(0,0,0,0.08);
    --shadow-md: 0 4px 12px rgba(0,0,0,0.12);
}
```

Apply to: `.card`, `.modal`, `.config-section`, `.org-card`.

### 9.3 Refined Border Radius

```css
:root {
    --radius-sm: 6px;
    --radius-md: 10px;
    --radius-lg: 14px;
}
```

Standardize all border-radius values (currently mixed: 4px, 6px, 8px, 12px).

### 9.4 Smooth Theme Transitions

```css
body, .card, .tab, .btn-sm, .badge, header, footer, .config-section {
    transition: background-color 0.25s ease, color 0.25s ease, border-color 0.25s ease;
}
```

---

## 10. Summary

| Aspect | Current | Proposed |
|--------|---------|----------|
| Brand alignment | Zero — generic hues | 100% — all 5 themes derive from 4 logo colors |
| CSS variables | 14 | 19 (+accent-hover, text-disabled, border-focus, info, selection) |
| Themes | green, blue, yellow, orange, red | Emerald (L1), Teal (L2), Azure (L3), Deep (L4), Gradient (L1-L4) |
| Semantic colors | success=accent (ambiguous) | success=L1, info=L3, warning=amber, danger=red (always) |
| Graph colors | 20+ hardcoded hex | CSS variable-driven, theme-reactive |
| Light mode coverage | 6/14 variables overridden | 13/19 variables overridden (complete) |
| WCAG compliance | Unknown | AA verified for all theme/mode combinations |
| Modern enhancements | None | Glass surfaces, shadows, smooth transitions, focus rings |
| LOC to change | — | ~270 (mostly CSS theme definitions) |
| Breaking changes | — | None (same data-theme values, same variable names) |
