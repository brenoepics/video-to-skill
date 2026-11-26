//! The persistent terminal a recipe edits still by still: a title bar,
//! a prompt, and the lines on screen. Shared by the renderer and by the
//! gold-set checks that confirm every gold command was shown on screen.

use crate::eval::recipe::TerminalOp;

/// Rows the terminal shows; older lines scroll off the top.
pub const ROWS: usize = 18;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalState {
    pub title: String,
    pub prompt: String,
    pub lines: Vec<String>,
}

impl Default for TerminalState {
    fn default() -> TerminalState {
        TerminalState {
            title: "demo - bash".to_string(),
            prompt: "$ ".to_string(),
            lines: vec!["$ ".to_string()],
        }
    }
}

impl TerminalState {
    /// Apply one still's edits in field order: `lines`, `title`,
    /// `prompt`, `type`, `output`.
    pub fn apply(&mut self, op: &TerminalOp) {
        if let Some(lines) = &op.lines {
            self.lines.clone_from(lines);
        }
        if let Some(title) = &op.title {
            self.title.clone_from(title);
        }
        if let Some(prompt) = &op.prompt {
            if self.lines.last() == Some(&self.prompt) {
                self.lines.pop();
                self.lines.push(prompt.clone());
            }
            self.prompt.clone_from(prompt);
        }
        if let Some(typed) = &op.typed {
            self.pop_bare_prompt();
            self.lines.push(format!("{}{typed}", self.prompt));
        }
        if let Some(output) = &op.output {
            self.pop_bare_prompt();
            self.lines.extend(output.iter().cloned());
            self.lines.push(self.prompt.clone());
        }
    }

    /// The lines currently on screen, at most [`ROWS`], newest last.
    #[must_use]
    pub fn visible(&self) -> &[String] {
        let skip = self.lines.len().saturating_sub(ROWS);
        &self.lines[skip..]
    }

    fn pop_bare_prompt(&mut self) {
        if self.lines.last().is_some_and(|l| *l == self.prompt) {
            self.lines.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TerminalState, ROWS};
    use crate::eval::recipe::TerminalOp;

    fn typed(command: &str) -> TerminalOp {
        TerminalOp {
            typed: Some(command.into()),
            ..TerminalOp::default()
        }
    }

    fn output(lines: &[&str]) -> TerminalOp {
        TerminalOp {
            output: Some(lines.iter().map(|l| (*l).to_string()).collect()),
            ..TerminalOp::default()
        }
    }

    #[test]
    fn typing_replaces_the_bare_prompt_and_output_restores_it() {
        let mut term = TerminalState::default();
        term.apply(&typed("git init"));
        assert_eq!(term.lines, vec!["$ git init"]);
        term.apply(&output(&["Initialized empty repository"]));
        assert_eq!(
            term.lines,
            vec!["$ git init", "Initialized empty repository", "$ "]
        );
    }

    #[test]
    fn prompt_changes_apply_before_typing_in_the_same_still() {
        let mut term = TerminalState::default();
        term.apply(&TerminalOp {
            prompt: Some("demo@host:~$ ".into()),
            typed: Some("uptime".into()),
            ..TerminalOp::default()
        });
        assert_eq!(term.lines, vec!["demo@host:~$ uptime"]);
    }

    #[test]
    fn the_screen_scrolls_once_lines_exceed_the_rows() {
        let mut term = TerminalState::default();
        let many: Vec<String> = (0..ROWS + 5).map(|i| format!("line {i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        term.apply(&output(&refs));
        let visible = term.visible();
        assert_eq!(visible.len(), ROWS);
        assert_eq!(visible.last().unwrap(), "$ ");
        assert_eq!(visible[0], format!("line {}", ROWS + 5 - (ROWS - 1)));
    }

    #[test]
    fn lines_reset_the_whole_screen() {
        let mut term = TerminalState::default();
        term.apply(&typed("ls"));
        term.apply(&TerminalOp {
            lines: Some(vec!["fresh".into()]),
            ..TerminalOp::default()
        });
        assert_eq!(term.lines, vec!["fresh"]);
    }
}
