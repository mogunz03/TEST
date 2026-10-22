I'm giving you the Shopify theme for https://www.dauze.shop. Customise the look and fix the bugs below. Work in the theme files I've attached. Do not change where content comes from.

## Attached files
1. The full theme export (layout/, sections/, snippets/, templates/, assets/, config/, locales/).
2. `font-update.zip`: Metropolis `.woff2` files (Regular 400, Medium 500, SemiBold 600, Bold 700) with their SIL OFL licence in `assets/fonts/`, plus a `fonts.css` that defines the `@font-face` rules and the `--font-heading` and `--font-body` variables. Reuse these; do not reconvert.

If the theme is not in the repo, stop and tell me before doing anything else.

## Rules
- Content (text, products, collections, pages, menus, images) must keep coming from Shopify objects and settings (`page.content`, `section.settings`, `linklists`, metafields, and so on). Do not hardcode or generate content in the theme. Only change the look and behaviour.
- Read the relevant files before editing. Find the real cause of each bug and tell me which file and line it is in.
- Do not create a pull request. Develop on branch `claude/gallant-turing-sz1jos`, commit with clear messages, and push to that branch only.
- Report honestly: tell me what you tested and what you could not test.

## Fonts
- Headings: **Libre Caslon Text** from Google Fonts (weights 400 and 700, `font-display: swap`). This is a stand-in for Kelvinch, which I'll supply later. Keep it in a single variable so swapping is one line.
- Body text, nav, buttons and forms: **Metropolis**, self-hosted from `assets/fonts/`, with a `Helvetica Neue, Arial, sans-serif` fallback.
- Remove or replace every other font reference in the theme, including any that never loaded.
- Use heading weight 400 for large sizes and 700 only below about 24px, because Libre Caslon is thin.

## Bugs and changes
1. **Infinite vertical scroll.** The homepage should loop forever. Right now, after the last section it jumps back to the top (Section One). Clone the first section after the last and reposition silently when the clone is reached, so the loop looks continuous in both directions (wheel, touch and keyboard). Tell me if the current structure makes this impractical.
2. **Cookie consent popup.**
   - Currently it sits at the bottom and covers the main menu.
   - Centre it exactly (horizontal and vertical) with rounded corners.
   - Use a solid card, NOT glassmorphic, with no blur. It should still match the overall design (colours and fonts).
   - Show it once. Store the choice in `localStorage` (wrapped in try/catch) so it never reappears on the same browser after the visitor answers.
   - Include Accept and Decline buttons, make it keyboard accessible, and trap focus while it's open.
3. **Search icon.**
   - When clicked, the header turns white and a cross button appears. Keep the cross, but remove the white background. The header must stay transparent and keep only a positioning indicator.
   - It must close when the visitor clicks anywhere outside it, and also on Escape.
   - On mobile, iPad and other small screens, the search bar currently drops to the bottom of the screen and causes visibility problems. It must stay pinned at the top at every breakpoint, like a standard mobile site. Account for the on-screen keyboard.
4. **Duplicated text on `/pages/about-us`.** Some text renders twice. Keep one copy. Find the cause (for example `page.content` rendered by both the template and a section). Then check every other page and section for the same mistake and fix them.
5. **Grid system.** Audit the grid and breakpoints across the theme (desktop, tablet, mobile). Fix misalignment, overflow and horizontal scroll. List what you found and changed.
6. **Animation bugs.** Review all animations and fix them. Known problems from the demo I started with: the animations use separate hard-coded delays instead of one timeline (the intro can finish before the loader counter reaches 100), the counter shows a hardcoded "29" before the script starts, and there is no `prefers-reduced-motion` handling. Look for the same patterns in the theme.
7. **Accessibility and responsiveness basics.** Add `alt` text where missing and make the navigation usable on touch devices.

## Output
- Make the changes in the theme files.
- Give me a final list: what you changed, in which files, what you tested, and anything you could not verify (for example, the live store).
- If something in my instructions conflicts with the theme's structure, tell me and propose an alternative. Do not silently pick one.
