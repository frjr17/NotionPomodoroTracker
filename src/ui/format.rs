//! Small shared formatters for task metadata, so the list sidebar and the
//! detail pane render dates and priority the same way.

use chrono::NaiveDate;

/// How urgent a due date is, mapped to an Adwaita state helper class so both
/// light/dark themes color it correctly (no hardcoded colors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueUrgency {
    Overdue,
    Soon,
    Normal,
}

impl DueUrgency {
    pub fn css_class(self) -> &'static str {
        match self {
            DueUrgency::Overdue => "error",
            DueUrgency::Soon => "warning",
            DueUrgency::Normal => "dim-label",
        }
    }
}

/// Human-readable due date relative to `today`, plus its urgency.
/// e.g. `Overdue 3d`, `Today`, `Tomorrow`, `Wed`, `Jul 15`.
pub fn humanize_due(due: NaiveDate, today: NaiveDate) -> (String, DueUrgency) {
    let days = (due - today).num_days();
    match days {
        d if d < 0 => (format!("{}d late", -d), DueUrgency::Overdue),
        0 => ("Today".into(), DueUrgency::Soon),
        1 => ("Tomorrow".into(), DueUrgency::Soon),
        2..=6 => (due.format("%a").to_string(), DueUrgency::Normal),
        _ => (due.format("%b %-d").to_string(), DueUrgency::Normal),
    }
}

/// Adwaita state helper class for a priority's colored dot. Substring match so
/// Notion values like "High"/"Medium"/"Low" (and the "Hight" typo) all map.
pub fn priority_dot_class(priority: &str) -> &'static str {
    let p = priority.to_lowercase();
    if p.contains("high") || p.contains("urgent") {
        "error"
    } else if p.contains("med") {
        "warning"
    } else {
        "dim-label"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn humanize_due_buckets() {
        let today = d("2026-07-10");
        assert_eq!(
            humanize_due(d("2026-07-07"), today),
            ("3d late".into(), DueUrgency::Overdue)
        );
        assert_eq!(humanize_due(today, today).1, DueUrgency::Soon);
        assert_eq!(humanize_due(today, today).0, "Today");
        assert_eq!(humanize_due(d("2026-07-11"), today).0, "Tomorrow");
        assert_eq!(humanize_due(d("2026-07-13"), today).1, DueUrgency::Normal); // weekday
        assert_eq!(humanize_due(d("2026-08-15"), today).0, "Aug 15");
    }

    #[test]
    fn priority_classes() {
        assert_eq!(priority_dot_class("High"), "error");
        assert_eq!(priority_dot_class("Hight"), "error"); // typo tolerated
        assert_eq!(priority_dot_class("Medium"), "warning");
        assert_eq!(priority_dot_class("Low"), "dim-label");
    }
}
