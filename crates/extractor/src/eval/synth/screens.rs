//! Layouts for the screen kinds a recipe can show: a terminal, a desktop
//! with an optional dialog, and a slide. Colours and sizes are fixed so
//! the same recipe renders the same pixels everywhere.

use super::canvas::{Canvas, Rgb};
use super::terminal::TerminalState;
use crate::eval::recipe::{Desktop, Dialog, Slide};

const TEXT_SCALE: u32 = 4;
const TITLE_SCALE: u32 = 6;
const MARGIN: i32 = 28;

const TERMINAL_BG: Rgb = [24, 26, 32];
const TERMINAL_BAR: Rgb = [58, 61, 70];
const TERMINAL_TEXT: Rgb = [222, 222, 222];
const TERMINAL_PROMPT: Rgb = [140, 220, 140];
const DESKTOP_BG: Rgb = [86, 118, 160];
const MENU_BAR: Rgb = [232, 232, 236];
const DIALOG_BG: Rgb = [238, 238, 238];
const DIALOG_BAR: Rgb = [196, 198, 208];
const DIALOG_TEXT: Rgb = [30, 30, 34];
const BUTTON_BG: Rgb = [210, 212, 220];
const SLIDE_BG: Rgb = [250, 250, 245];
const SLIDE_TEXT: Rgb = [28, 30, 40];
const SLIDE_ACCENT: Rgb = [196, 76, 60];

/// The terminal's visible rows under a title bar; prompt lines in green.
#[must_use]
pub fn terminal(width: u32, height: u32, state: &TerminalState) -> Canvas {
    let mut canvas = Canvas::new(width, height, TERMINAL_BG);
    let bar_h = Canvas::line_height(TEXT_SCALE);
    canvas.rect(0, 0, width, bar_h, TERMINAL_BAR);
    canvas.text(MARGIN, 4, TEXT_SCALE, TERMINAL_TEXT, &state.title);
    let line_h = i32::try_from(Canvas::line_height(TEXT_SCALE)).unwrap_or(36);
    let mut y = i32::try_from(bar_h).unwrap_or(36) + MARGIN / 2;
    for line in state.visible() {
        let color = if line.starts_with(&state.prompt) {
            TERMINAL_PROMPT
        } else {
            TERMINAL_TEXT
        };
        canvas.text(MARGIN, y, TEXT_SCALE, color, line);
        y += line_h;
    }
    canvas
}

/// A desktop: menu bar, optional status text bottom-left.
#[must_use]
pub fn desktop(width: u32, height: u32, desktop: &Desktop) -> Canvas {
    let mut canvas = Canvas::new(width, height, DESKTOP_BG);
    let bar_h = Canvas::line_height(TEXT_SCALE);
    canvas.rect(0, 0, width, bar_h, MENU_BAR);
    canvas.text(MARGIN, 4, TEXT_SCALE, DIALOG_TEXT, "File  Edit  View  Help");
    if let Some(status) = &desktop.status {
        let y = i32::try_from(height).unwrap_or(0) - i32::try_from(bar_h).unwrap_or(0) - MARGIN;
        canvas.text(MARGIN, y, TEXT_SCALE, MENU_BAR, status);
    }
    canvas
}

/// The desktop with a centred dialog: title bar, rows, buttons.
#[must_use]
pub fn dialog(width: u32, height: u32, dialog: &Dialog) -> Canvas {
    let mut canvas = desktop(
        width,
        height,
        &Desktop {
            status: dialog.status.clone(),
        },
    );
    let line_h = Canvas::line_height(TEXT_SCALE);
    let rows = u32::try_from(dialog.rows.len()).unwrap_or(0);
    let box_w = (width * 3 / 5).max(480);
    let box_h = line_h * (rows + 4) + 24;
    let x0 = i32::try_from((width - box_w) / 2).unwrap_or(0);
    let y0 = i32::try_from(height.saturating_sub(box_h) / 2).unwrap_or(0);
    canvas.rect(x0 + 6, y0 + 6, box_w, box_h, [40, 44, 60]);
    canvas.rect(x0, y0, box_w, box_h, DIALOG_BG);
    canvas.rect(x0, y0, box_w, line_h, DIALOG_BAR);
    canvas.text(x0 + 16, y0 + 4, TEXT_SCALE, DIALOG_TEXT, &dialog.title);
    let line_h_i = i32::try_from(line_h).unwrap_or(36);
    let mut y = y0 + line_h_i + 16;
    for row in &dialog.rows {
        canvas.text(x0 + 24, y, TEXT_SCALE, DIALOG_TEXT, row);
        y += line_h_i;
    }
    let mut x = x0 + i32::try_from(box_w).unwrap_or(0) - 24;
    let button_y = y0 + i32::try_from(box_h).unwrap_or(0) - line_h_i - 16;
    for label in dialog.buttons.iter().rev() {
        let w = Canvas::text_width(label, TEXT_SCALE) + 32;
        x -= i32::try_from(w).unwrap_or(0);
        canvas.rect(x, button_y, w, line_h, BUTTON_BG);
        canvas.text(x + 16, button_y + 4, TEXT_SCALE, DIALOG_TEXT, label);
        x -= 16;
    }
    canvas
}

/// A slide: large title, bullets, accent rule and footer.
#[must_use]
pub fn slide(width: u32, height: u32, slide: &Slide) -> Canvas {
    let mut canvas = Canvas::new(width, height, SLIDE_BG);
    let title_h = Canvas::line_height(TITLE_SCALE);
    canvas.text(
        MARGIN * 2,
        MARGIN * 2,
        TITLE_SCALE,
        SLIDE_TEXT,
        &slide.title,
    );
    let rule_y = MARGIN * 2 + i32::try_from(title_h).unwrap_or(0);
    canvas.rect(MARGIN * 2, rule_y, width / 3, 8, SLIDE_ACCENT);
    let line_h = i32::try_from(Canvas::line_height(TEXT_SCALE)).unwrap_or(36);
    let mut y = rule_y + MARGIN * 2;
    for bullet in &slide.bullets {
        canvas.rect(MARGIN * 2, y + line_h / 3, 12, 12, SLIDE_ACCENT);
        canvas.text(MARGIN * 2 + 32, y, TEXT_SCALE, SLIDE_TEXT, bullet);
        y += line_h + 8;
    }
    if let Some(footer) = &slide.footer {
        let y = i32::try_from(height).unwrap_or(0) - line_h - MARGIN;
        canvas.text(MARGIN * 2, y, TEXT_SCALE, SLIDE_ACCENT, footer);
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::{desktop, dialog, slide, terminal, DESKTOP_BG, SLIDE_BG, TERMINAL_BG};
    use crate::eval::recipe::{Desktop, Dialog, Slide};
    use crate::eval::synth::terminal::TerminalState;

    #[test]
    fn each_screen_fills_its_background_and_draws_something() {
        let shell = terminal(320, 180, &TerminalState::default());
        assert_eq!(shell.pixel(300, 170), TERMINAL_BG);
        assert_ne!(shell.pixel(2, 2), TERMINAL_BG, "title bar drawn");

        let background = desktop(320, 180, &Desktop { status: None });
        assert_eq!(background.pixel(160, 120), DESKTOP_BG);

        let window = dialog(
            640,
            360,
            &Dialog {
                title: "Preferences".into(),
                rows: vec!["Theme: Light".into()],
                buttons: vec!["Cancel".into(), "OK".into()],
                status: None,
            },
        );
        assert_ne!(
            window.pixel(320, 180),
            DESKTOP_BG,
            "dialog covers the centre"
        );

        let page = slide(
            640,
            360,
            &Slide {
                title: "Recap".into(),
                bullets: vec!["one".into()],
                footer: Some("1/3".into()),
            },
        );
        assert_eq!(page.pixel(600, 10), SLIDE_BG);
        assert_ne!(page.pixel(60, 60), SLIDE_BG, "title drawn");
    }
}
