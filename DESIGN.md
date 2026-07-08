# Design

Visual system for Notion Pomodoro Tracker. The design system IS libadwaita;
this file records how we use it, plus the few custom pieces. Consult the
GNOME HIG (developer.gnome.org/hig) before inventing anything.

## Theme

Follows the system light/dark preference automatically via
`adw::StyleManager`. Never hardcode theme colors; use Adwaita named colors
and CSS classes so both themes work for free.

## Color

- **Strategy: restrained.** Neutral Adwaita surfaces everywhere; color is
  reserved for state.
- Accent/suggested actions: stock Adwaita accent (Start/Resume buttons via
  `suggested-action`).
- Destructive: stock `destructive-action` (Stop button).
- Pomodoro red (`#e01b24`, GNOME red 3) appears only in brand assets
  (banner, future app icon) — not as an in-app decorative color.
- State indicators: warning icon for conflicts, edit icon for dirty tasks,
  `dim-label` for secondary text.

## Typography

- App-wide: system font (Cantarell on GNOME) via stock widgets; standard
  Adwaita classes for hierarchy: `title-2`, `title-4`, `heading`, `caption`,
  `dim-label`.
- **Custom: `.timer-display`** — the countdown. 64px light (300), tabular
  numerals (`font-feature-settings: 'tnum'`) so digits don't jitter each
  second. Defined in `main_window.rs::load_css()`. This is the app's single
  display-scale element; nothing else may exceed `title-1`.

## Layout

- `adw::OverlaySplitView`: 260px min sidebar (task list) + content pane
  (timer above, task detail below, separated by a `gtk::Separator`).
- Spacing: GNOME 6/12/18/24 px rhythm (margins 6 within controls, 12–18
  around content blocks, 24 above the timer).
- Rows: `adw::ActionRow` inside `boxed-list` / `navigation-sidebar`
  ListBoxes. No cards.

## Components

- **Timer controls**: pill buttons (`pill` class); visibility swaps by state
  (Start ↔ Pause/Resume/Stop/Complete) rather than disabling everything.
- **Sync status**: subtitle of `adw::WindowTitle` + spinner in the header;
  toasts (`adw::ToastOverlay`) for sync outcomes.
- **Conflict resolution**: `adw::Banner` on the task + `adw::AlertDialog`
  with Keep Local / Keep Notion.
- **Settings**: `adw::PreferencesDialog` with `SpinRow`, `SwitchRow`,
  `PasswordEntryRow`, `ComboRow` for property mappings.
- **Empty state**: `adw::StatusPage` when no task is selected.

## Motion

Stock Adwaita transitions only (dialogs, banner reveal, toast slide). No
custom animation; respect system reduced-motion via the platform defaults.

## Iconography

Symbolic icons from the system theme only: `alarm-symbolic`,
`emblem-synchronizing-symbolic`, `emblem-system-symbolic`,
`list-add-symbolic`, `dialog-warning-symbolic`, `document-edit-symbolic`,
`checkbox-checked-symbolic`. The 🍅 emoji appears only as a pomodoro-count
badge in list rows and task detail.

## Brand assets

- `data/banner.svg` / `.png` — 1280×640 social banner: charcoal `#1c1b25`,
  GNOME red `#e01b24`, timer-ring + check logo mark, Cantarell wordmark.
- App icon: `data/icons/com.frjr17.NotionPomodoroTracker.svg` — the banner's
  ring+check mark on a charcoal disc; installed to hicolor by `just install`.
