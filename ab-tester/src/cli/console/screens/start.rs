use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use ratatui::prelude::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::cli::console::action::Action;
use crate::cli::console::components::Component;
use crate::cli::console::theme::Theme;
use crate::cli::console::tui::Frame;

pub struct StartScreen {}

impl StartScreen {
    pub fn new() -> Self {
        Self {}
    }
}

impl Component for StartScreen {
    fn handle_key_events(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        eprintln!("key: {key:?}");
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect, theme: &Theme) -> Result<()> {
        let rects = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Min(10)].as_ref())
            .split(area);

        let _left_pane = rects[0];
        let _right_pane = rects[1];

        self.draw_environments_pane(f, rects[0], theme)?;

        Ok(())
    }
}

impl StartScreen {
    fn draw_environments_pane(
        &mut self,
        f: &mut Frame<'_>,
        area: Rect,
        _theme: &Theme,
    ) -> Result<()> {
        let widget = Block::new()
            .title("Environments")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        f.render_widget(widget, area);

        Ok(())
    }

    fn draw_config_pane(&mut self, _f: &mut Frame<'_>, _area: Rect, _theme: &Theme) -> Result<()> {
        let _widget = Block::new();
        Ok(())
    }
}
