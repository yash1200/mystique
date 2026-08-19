# Mystique DB logo

**Mark:** a sitting cat (front view, tail curled beside its paws) on a jade tile.  
**Metaphor:** Mystique — watchful, a little mysterious. Chrome stays quiet; the cat is the signal.  
**Signal color:** jade `#2FBF96` (dark) / `#0E7A60` (light) on warm ink `#131210`.

## Files

| File | Use |
| --- | --- |
| `mystique-mark.svg` | Primary badge (jade tile + sitting cat). README, reference. |
| `mystique-app-icon.svg` | 1024 master for macOS / Windows / Linux icons. |
| `mystique-app-icon.png` | Raster of the master; input to `yarn tauri icon`. |

In-app chrome uses the inline SVG in `BrandMark.svelte` (same cat path). Do not re-draw the mark.

## Clear space

Keep an empty margin of at least **¼ the mark height** on every side. Do not crowd it with other chrome.

## Minimum sizes

- Tiled mark: **24px**
- App icon source: **1024px**

## Color

- Full color on warm ink or warm paper.
- Monochrome: one ink. On dark surfaces use jade or paper; on light surfaces use ink or dark jade.
- Do not recolor with connection hues. Those are identity for connections, not the product.

## Do not

- Stretch or rotate the cat
- Redraw the cat as a cartoon mascot or add extra highlights
- Put the mark on a busy photograph without a solid field
- Use purple, Inter, or a generic “AI SaaS” treatment

## Regenerating raster icons

```bash
# 1024 PNG from the master SVG (resvg, rsvg-convert, or Quick Look)
# then:
yarn tauri icon static/brand/mystique-app-icon.png
```

That writes `src-tauri/icons/*`. Copy a 32×32 PNG to `static/favicon.png` if the favicon should match.
