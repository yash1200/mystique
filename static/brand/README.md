# Mystique DB logo

**Mark:** a smiling dog face (closed happy eyes, floppy ears) on a warm-ink tile.  
**Metaphor:** Mystique — friendly, a little mysterious. Chrome stays quiet; the dog is the signal.  
**Tile:** warm ink `#131210`. **Face:** mint `#DAE7DE`. **Features:** sage `#345453`.  
**Signal color** (chrome, not the mark tile): jade `#2FBF96` (dark) / `#0E7A60` (light).

## Files

| File                      | Use                                                           |
| ------------------------- | ------------------------------------------------------------- |
| `mystique-dog.svg`        | Dog only (no tile). Converted from the source raster.         |
| `mystique-dog-source.png` | Original raster of the face.                                  |
| `mystique-mark.svg`       | Primary badge (ink tile + dog). README, `BrandMark`, favicon. |
| `mystique-app-icon.svg`   | 1024 master for macOS / Windows / Linux icons.                |
| `mystique-app-icon.png`   | Raster of the master; input to `yarn tauri icon`.             |

In-app chrome uses `BrandMark.svelte` → `/brand/mystique-mark.svg`. Do not re-draw the mark.

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

- Stretch or rotate the dog
- Redraw the dog or add extra highlights
- Put the mark on a busy photograph without a solid field
- Use purple, Inter, or a generic “AI SaaS” treatment

## Regenerating raster icons

```bash
# 1024 PNG from the master SVG (resvg, rsvg-convert, or Quick Look)
# then:
yarn tauri icon static/brand/mystique-app-icon.png
```

That writes `src-tauri/icons/*`. Copy a 32×32 PNG to `static/favicon.png` if the favicon should match.
