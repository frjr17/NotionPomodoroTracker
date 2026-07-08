---
target: main window
total_score: 22
p0_count: 0
p1_count: 3
timestamp: 2026-07-08T17-55-28Z
slug: src-ui-windows-main-window-rs
---
# Critique: Main Window (src/ui/windows/main_window.rs)

Evidence: light+dark screenshots with seeded data (running timer, conflict, dirty tasks) + source review. Detector: 0 findings (no scannable web markup in Rust sources; screenshots were the visual evidence).

## Scores (Nielsen, 22/40 — Acceptable)

1. Visibility of System Status: 2 — timer never names the task it is timing
2. Match System/Real World: 2 — UTC timestamps, raw ISO dates
3. User Control and Freedom: 3 — no undo for Stop/Complete
4. Consistency and Standards: 3 — emoji breaks symbolic icon vocabulary
5. Error Prevention: 3 — Stop discards pomodoro without confirm
6. Recognition Rather Than Recall: 2 — two unlabeled "All" dropdowns
7. Flexibility and Efficiency: 1 — zero keyboard shortcuts
8. Aesthetic and Minimalist Design: 3 — calm; metadata prose row flat
9. Error Recovery: 2 — raw error strings in toasts; sync log has no UI
10. Help and Documentation: 1 — no in-app help or first-run guidance

## Anti-patterns verdict
Not AI-looking; reads as stock GNOME (intended). Unfinished tells: broken window icon (no app icon installed) and broken empty-state icon (checkbox-checked-symbolic doesn't exist in theme).

## Priority issues
- [P1] Timer doesn't display which task it is timing; selection change makes it ambiguous. Fix: task-name label in TimerView bound to timer.task_id. (polish)
- [P1] Broken icons: empty-state icon name invalid; no installed app icon. Fix: object-select-symbolic + install app icon. (polish)
- [P1] Twin unlabeled "All" filter dropdowns (status vs priority indistinguishable; bad for Orca). Fix: labels/accessible names. (clarify/polish)
- [P2] UTC timestamps + ISO dates throughout detail/session history. Fix: local time, humanized dates. (clarify)
- [P2] No keyboard shortcuts at all (Space, Ctrl+F, Ctrl+N, F5) and no shortcuts window. (harden)

## Persona red flags
- Alex: fully mouse-bound; no accelerators anywhere.
- Jordan: first run shows empty list + "Pick a task from the list" with no tasks and no pointer to Settings/Notion setup.
- Sam: dropdowns announce as "All"/"All"; phase transitions not announced; emoji read as "tomato".

## Minor
- Truncated new-task placeholder; red Stop is the loudest element; conflict icon uncolored; session list clips mid-row; 🍅 emoji only color glyph.

## Questions
- Should selecting another task switch the timed target (banking elapsed time)?
- Dedicated first-run welcome state?
- Is "Complete Pomodoro" needed?
