# Product

## Register

product

## Users

A single developer (the project owner) on Fedora/GNOME, working at a desk
through multi-hour focus sessions. The app sits beside their editor all day.
Job to be done: pick a task, run focused pomodoros against it, and trust that
counts and minutes land back in Notion without babysitting the sync.

## Product Purpose

Offline-first GTK4/libadwaita desktop app that mirrors a Notion task
database locally, times work with a Pomodoro workflow, and syncs progress
back on demand. Success = the user stops thinking about tracking: start
timer, work, glance at sync status, done.

## Brand Personality

Calm, GNOME-native, invisible. The app should feel like a stock GNOME app
that shipped with the desktop. The running timer is the single loud element;
everything else stays quiet. Voice in copy: plain, short, no exclamation
marks, no productivity-guru tone.

## Anti-references

- Gamified pomodoro apps: streaks, confetti, mascots, badges, celebratory
  modals. A completed pomodoro earns one notification, nothing more.
- Anything that fights the platform: custom-drawn chrome, web-style
  components, non-Adwaita colors used decoratively.

## Design Principles

1. **HIG first, invention second.** Reach for the stock Adwaita widget and
   pattern before any custom styling; custom CSS only where the HIG has no
   answer (the big countdown).
2. **The timer owns the loudness budget.** Phase color, size, and motion are
   spent on timer state; the rest of the UI stays neutral so state changes
   are unmissable.
3. **State is always visible, never nagging.** Sync status, dirty flags, and
   conflicts are glanceable indicators, not popups; the user pulls detail
   when they want it.
4. **Offline is the normal case.** No UI element should look degraded or
   apologetic when offline; sync affordances appear enabled-and-idle, not
   broken.

## Accessibility & Inclusion

GNOME defaults: follow system light/dark, respect font scaling and
reduced-motion settings, keep all actions keyboard-reachable, rely on stock
widgets for Orca compatibility. Custom elements (countdown label) must keep
≥4.5:1 contrast in both themes.
