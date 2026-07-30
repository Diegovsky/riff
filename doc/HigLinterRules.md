# GNOME HIG - Linter-Verifiable Rules

A comprehensive list of rules from the [GNOME Human Interface Guidelines](https://developer.gnome.org/hig/guidelines.html) that can be statically verified by analyzing source code via a linter.

The linter would parse:
- UI files (`.ui` XML or Blueprint `.blp`) for widget properties, style classes, accessible names, tooltips, mnemonic settings, dialog structure
- Source code strings (gettext `_("...")`, `C_("...")`, etc.) for writing style, capitalization, and Unicode compliance
- CSS files for hardcoded values, animations, color-only differentiation
- `.desktop` files and AppStream XML for app naming rules
- GAction/shortcut definitions in source for keyboard shortcut compliance

---

## Linter Inventory

| Script | Purpose |
|--------|---------|
|        |         |

---

## 1. Writing Style & Text Content

These rules apply to translatable strings in source code and UI definitions.

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 1.1 | No trailing periods on labels | Translatable strings used as labels, descriptions, headings, or tooltips should not end with `.` unless they are multi-sentence paragraphs. |  |
| 1.2 | Ellipsis character usage | Labels indicating further action (e.g. "Save As") must use the Unicode ellipsis `...` (U+2026) rather than three dots `...`. |  |
| 1.3 | No Latin abbreviations | Strings should not contain "i.e.", "e.g.", "etc." - use full English equivalents instead. |  |
| 1.4 | Header capitalization on short labels | Button labels, menu items, switch labels, tab titles, tooltips, and window titles should use header capitalization (capitalize words >= 4 letters, all verbs, all nouns, first/last word). |  |
| 1.5 | Sentence capitalization on long-form labels | Checkbox labels, radio button labels, descriptive text, and body text should use sentence capitalization. |  |
| 1.6 | No "my" pronoun usage | Strings should prefer "your" over "my" when referring to user possessions. Flag any occurrence of "My" in user-facing labels. |  |
| 1.7 | Ellipsis only on action labels | Labels that do NOT denote an action (like "Properties", "Preferences") should NOT have an ellipsis. |  |

---

## 2. Typography & Unicode

These rules apply to user-visible strings in code and UI definitions.

| #    | Rule | What to Check | Verified By |
|------|------|---------------|-------------|
| 2.1  | Use typographic quotes | Strings should use U+201C and U+201D instead of straight `"` quotes for user-visible quotations. |  |
| 2.2  | Use multiplication sign for dimensions | Dimension strings (e.g. resolution) should use U+00D7 not `x`. |  |
| 2.3  | Use Unicode ellipsis | Trailing dots in labels should be U+2026 not `...`. |  |
| 2.4  | Use typographic apostrophe | Apostrophes should use U+2019 rather than ASCII `'`. |  |
| 2.5  | Use bullet character for lists | Bullet lists in UI text should use U+2022 not `*` or `-`. |  |
| 2.6  | Use en dash for ranges | Ranges (date ranges, number ranges) should use U+2013 not `-`. |  |
| 2.7  | Use narrow no-break space before units | A value followed by a unit abbreviation should use U+202F (narrow no-break space) between them, not a regular space. |  |
| 2.8  | No all-caps text | Labels should never capitalize every letter. Check for all-uppercase translatable strings (excluding acronyms). |  |
| 2.9  | No hardcoded font sizes | CSS or style properties should not use absolute `px` font sizes. Font sizes should be expressed relatively or use standard style classes. |  |
| 2.10 | Use standard font style classes | Prefer named CSS classes (`heading`, `title-1`, `caption`, `body`, etc.) over custom font-size/weight declarations. |  |

---

## 3. Accessibility

These rules can be checked in UI definition files and source code.

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 3.1 | All widgets must have accessible names/labels | Every interactive widget (buttons, entries, switches, sliders) must have an `accessible-name`, `accessible-label`, or associated `<label>` element. Icon-only buttons especially. |  |
| 3.2 | No color-only differentiation | Custom CSS or styling should not rely solely on color to convey meaning. Check for widgets where the only distinguishing property is a color change without an accompanying text, icon, or shape change. |  |
| 3.3 | Access keys (mnemonics) on labelled controls | All labelled controls should have a mnemonic (underline character). Check for `use_underline` being `true` and that labels contain an `_` prefix on one character. |  |
| 3.4 | No duplicate access keys in same context | Within a single dialog or view, no two controls should share the same mnemonic letter. |  |
| 3.5 | Keyboard-focusable widgets | Interactive widgets should have `focusable` set to `true` (or not explicitly disabled). Custom widgets must not set `can-focus` to `false`. |  |

---

## 4. Keyboard Interaction

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 4.1 | Standard shortcuts must be bound | If the app supports standard operations (copy, paste, undo, save, quit, close, etc.), verify that the standard GNOME shortcut keys are bound (Ctrl+C, Ctrl+V, Ctrl+Z, Ctrl+S, Ctrl+Q, Ctrl+W). |  |
| 4.2 | No use of Super key in app shortcuts | App-defined keyboard shortcuts must not use the Super key. |  |
| 4.3 | No bare Alt shortcuts | App-defined shortcuts should not use Alt alone (conflicts with access keys). |  |
| 4.4 | Escape closes transient containers | Dialogs, popovers, and menus should bind Escape to close/cancel. |  |
| 4.5 | Return activates default button in dialogs | Dialogs should have a `default-widget` set (when the action is not destructive). |  |

---

## 5. Dialogs

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 5.1 | Dialogs must have a parent/modal | Dialog widgets should have `modal` set to `true` and be attached to a parent window. |  |
| 5.2 | Cancel button appears before affirmative | In dialog button layouts, the cancel/dismiss response should be listed before the affirmative response. |  |
| 5.3 | Destructive actions labeled with destructive style | Buttons for destructive operations should have the `destructive-action` style class. |  |
| 5.4 | Action dialog affirmative button uses specific verb | Dialog confirm buttons should use specific verbs (Save, Print, Delete) not generic labels (OK, Done, Yes). |  |
| 5.5 | Maximum one suggested-action or destructive-action button per view | No more than one button in a given view should carry `suggested-action` or `destructive-action` class. |  |

---

## 6. Buttons & Controls

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 6.1 | Buttons should contain icon OR label, not both (outside header bars) | Non-header-bar buttons that have both a child label and an icon simultaneously. |  |
| 6.2 | Button labels use imperative verbs | Button labels should be actionable verbs (Save, Open, Delete) not nouns or descriptions. Can partially lint by flagging labels that are clearly noun-phrases. |  |
| 6.3 | Insensitive state for invalid actions | Buttons wired to actions that have preconditions should be conditionally insensitive (check that `sensitive` is bound or toggled) rather than showing error after click. |  |
| 6.4 | Header bar buttons must have tooltips | Buttons that are children of `AdwHeaderBar` / `GtkHeaderBar` must have a `tooltip-text` property set. |  |
| 6.5 | Tooltip text uses header capitalization | `tooltip-text` property values should follow header capitalization rules. |  |
| 6.6 | If one control in a container has a tooltip, all must | If any sibling button in a box/header-bar has `tooltip-text`, all siblings should too. |  |

---

## 7. App Naming & Metadata

These apply to `.desktop` files and AppStream metadata.

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 7.1 | App name length < 15 characters | The `Name` field in `.desktop` and AppStream metadata should be under 15 characters. |  |
| 7.2 | No "G" prefix in app name | App name should not start with "G" as a GNOME prefix pattern. |  |
| 7.3 | App name uses header capitalization | The app name should follow header capitalization rules. |  |
| 7.4 | No non-standard punctuation in name | App name should not contain unusual whitespace or punctuation (e.g. camelCase-smashed words like "SuperWriter"). |  |
| 7.5 | App ID consistency | The app ID must be consistent across metainfo, desktop entry, flatpak manifest, and gschema. |  |
| 7.6 | License file and declaration present | A LICENSE/COPYING file must exist and project_license must be in metainfo. |  |
| 7.7 | Content rating present | OARS content_rating must be in metainfo. |  |
| 7.8 | Bug tracker and homepage URLs present | metainfo must declare bugtracker and homepage URLs. |  |
| 7.9 | Developer info present | metainfo must include developer information. |  |
| 7.10 | Hardware/display support declared | metainfo must include `<recommends>` or `<requires>` hardware info. |  |
| 7.11 | GTK 4 + libadwaita dependency | Cargo.toml must depend on gtk4 and libadwaita. |  |
| 7.12 | GNOME Platform runtime | Flatpak manifest must use org.gnome.Platform. |  |
| 7.13 | Code of Conduct referenced | README must reference the GNOME Code of Conduct. |  |
| 7.14 | .doap file present | A .doap project description file must exist. |  |

---

## 8. UI Styling & Theming

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 8.1 | Dark style preference has three options | If the app exposes a style preference, it should provide light, dark, and follow-system options (check for `AdwStyleManager` usage with all three modes). |  |
| 8.2 | No custom colors without CSS variables | Custom CSS should use Adwaita CSS variables (`--accent-color`, `--window-bg-color`, etc.) rather than hardcoded hex/rgb values. |  |
| 8.3 | No flashing/blinking UI elements | CSS animations should not include rapid opacity toggling or blink-style keyframes. |  |
| 8.4 | No custom font-family | Use the default system font, not a custom font-family declaration. |  |
| 8.5 | No italic/oblique font-style | The HIG advises against italic faces. |  |

---

## 9. Adaptive Layout

| #   | Rule | What to Check | Verified By |
|-----|------|---------------|-------------|
| 9.1 | Minimum window size set | Window `default-width` and `default-height` or size constraints should accommodate 360px width for phone-targeted apps, or at minimum 1024x600 for desktop-only. |  |
| 9.2 | Use of adaptive containers | Apps should use `AdwNavigationView`, `AdwOverlaySplitView`, `AdwBreakpoint`, or similar adaptive widgets rather than fixed-width layouts. |  |
| 9.3 | Content containers have max-width | Long text content areas should have a `max-content-width` or equivalent constraint to prevent overly long lines at large window sizes. |  |

---

## 10. Navigation Structure

| #    | Rule | What to Check | Verified By |
|------|------|---------------|-------------|
| 10.1 | View switchers have <= 5 pages | `AdwViewSwitcher` instances should not contain more than a small number of child pages. |  |
| 10.2 | Navigation hierarchy depth <= 2 | Nested `AdwNavigationView` push depths should generally not exceed one level. |  |

---

## 11. Internationalization

| #    | Rule | What to Check | Verified By |
|------|------|---------------|-------------|
| 11.1 | POTFILES lists all translatable files | Every source file with translatable strings must be listed in po/POTFILES. |  |
| 11.2 | POTFILES entries exist on disk | Every file listed in po/POTFILES must exist. |  |
| 11.3 | User-facing strings are translatable | Blueprint strings for label, tooltip-text, placeholder-text should be wrapped in `_()`. |  |

---

## 12. Widget Correctness (GTK-specific)

| #    | Rule | What to Check | Verified By |
|------|------|---------------|-------------|
| 12.1 | No deprecated widgets | GtkDialog, GtkMessageDialog, GtkAboutDialog, GtkInfoBar, GtkFileChooserDialog, GtkComboBox should be replaced with modern equivalents. |  |
| 12.2 | GtkStack visible-child-name not set as property | Setting visible-child-name in Blueprint causes runtime warnings. |  |
| 12.3 | No grid layout properties in Box children | Children of GtkBox must not use Grid layout properties (column, row, column-span, row-span). |  |
| 12.4 | Control icons must be symbolic | Icons on buttons/controls must end with `-symbolic`. |  |
| 12.5 | Preference groups use Adw rows | Raw GtkSwitch/GtkCheckButton/GtkDropDown inside AdwPreferencesGroup should use AdwSwitchRow/AdwComboRow etc. |  |

---
