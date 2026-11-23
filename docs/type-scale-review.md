# Text sizes for the whole app (#766)

Body text stays at 15px on a 22px line, chosen to match Slack. Everything else is set from it.

## The verdict

The scale from #737 is sound at the top (15, 17, 22, 32) and too small at the bottom. With a 15px base, 11px text is
73% of the body size, and most of it is also grey, monospaced or in capitals. That is the text you squint at on the
Desk. The fix is a floor: nothing under 12px, and anything you read as words (not a label or a number) at 13px or more.

## One scale

| Role | Size / line | Weight | Today |
|---|---|---|---|
| Page heading | 22 / 28 | 700 | 22, fine |
| Pane or card title | 17 / 24 | 600 | 17, fine |
| Chat body, what you type | 15 / 22 | 400 | 15, fine |
| Item titles, session names | 15 / 22 | 600 | 15, fine |
| Buttons, tabs | 13 / 18 | 600 | 13, fine |
| Side-panel text | 13 / 18 | 400 | 13 and 12 mixed |
| Meta lines (#id · session) | 13 / 18 | 400 | 11 and 12 |
| Status words ("Ready to check") | 13 / 18 | 500 | 12 |
| Section labels (WAITING ON YOU) | 12 / 16 | 700, capitals, 0.06em spacing | 11 |
| Progress-bar times, counts, badges | 12 / 16 | 500, even-width digits | 11 |
| Big numbers | 32 / 36 | 700 | 32, fine |

Six sizes: 12, 13, 15, 17, 22, 32. The 11px step goes away.

## Under its size today

On the Desk (uploads/1795449265436-0.png):

- Progress-bar times ("12s 1m", "24m"): 11px mono grey. Should be 12.
- "Ready to check" in the Now column: 12. Should be 13.
- WAITING ON YOU, UP NEXT, DONE TODAY and the ITEM / PROGRESS / NOW heads: 11. Should be 12.
- Meta lines under item titles: the #id is 11px mono and the session name 12. Both should be 13.
- Sessions column: the line under each name ("#694 · ready to check") and crew rows are 12, should be 13. The
  THIS MAC label and the Lead pill are 11, should be 12. The count line at the top ("191 sessions · 1 working") is
  12px mono, should be 13 plain.
- "33 more", "1 more", "drag to reorder", and the lead panel's small grey lines ("in cue-765's chat · 38m"): 12,
  should be 13. One arrow glyph is a stray 10px.
- In the chat, the mono "Ready to check" tag on Cue's lines: 12, should be 13.

Across the rest of the app the same two steps are used a lot: 54 rules at 11px and 156 at 12px, mostly in the
sessions list, the item page and the Board cards. They follow the same rule: labels and numbers up to 12, words up to 13.

## The font and weights

The font is right. Cue uses the Mac's own system face, which is drawn for exactly this size and adjusts its spacing
by size without any work from us. A bundled face (Inter, Lato as Slack uses) would look less at home and gain nothing.

Two things are off:

- **Too many weights.** The styles use eight (400, 500, 550, 600, 650, 700, 750, 800). Nobody can tell 600 from 650.
  Use four: 400 for reading, 500 for status words and numbers, 600 for names, titles and buttons, 700 for capital
  labels and page headings.
- **Too much monospace at small sizes.** Small grey mono is the hardest text in the app to read. Keep mono for
  commands, code and #ids. Times and counts should be the normal face with even-width digits
  (`font-variant-numeric: tabular-nums`), which line up just as well.

## Changing the whole scale from one place

Every size already comes from eight lines in `ui/styles/base.css` (`--type-tag` to `--type-figure`); only one rule
in the app sets a size by hand. So:

1. **Now:** change `--type-tag` from 11px to 12px. One line lifts every label and time in the app.
2. **Then:** `--type-pill` (12px) is shared by capital heads and by meta lines. Move the meta lines and status words
   to `--type-small` (13px) and leave the heads on 12. This is the only part that touches many rules.
3. **Later, to raise or lower everything:** add one number, `--type-scale: 1`, and write each size as
   `round(15px * var(--type-scale), 1px)`. Setting it to 1.07 gives a 16px body with every other size following, in
   whole pixels. Row and button heights are fixed in pixels today, so a step of more than one size needs those
   checked too.

When this ships, the design system's type section needs the same numbers; it still describes the older 10 to 13px scale.
