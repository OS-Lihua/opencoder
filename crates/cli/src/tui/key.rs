//! Key binding definitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{Action, InputMode, Screen};

pub fn handle_key(key: KeyEvent, screen: &Screen, input_mode: &InputMode) -> Action {
    // Global: Ctrl+C always quits or cancels
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        if *screen == Screen::Session {
            return Action::CancelAgent;
        }
        return Action::Quit;
    }

    match screen {
        Screen::Home => handle_home_key(key),
        Screen::Session => match input_mode {
            InputMode::Normal => handle_session_normal_key(key),
            InputMode::Editing => handle_session_editing_key(key),
        },
    }
}

fn handle_home_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('n') => Action::NewSession,
        KeyCode::Char('d') => Action::DeleteSession,
        KeyCode::Char('/') => Action::StartSearch,
        KeyCode::Up | KeyCode::Char('k') => Action::MoveUp,
        KeyCode::Down | KeyCode::Char('j') => Action::MoveDown,
        KeyCode::Enter => Action::EnterSession,
        _ => Action::Noop,
    }
}

fn handle_session_normal_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::BackToHome,
        KeyCode::Char('i') => Action::Noop, // Would switch to editing mode
        KeyCode::Up | KeyCode::Char('k') => Action::ScrollUp,
        KeyCode::Down | KeyCode::Char('j') => Action::ScrollDown,
        KeyCode::PageUp => Action::ScrollUp,
        KeyCode::PageDown => Action::ScrollDown,
        _ => Action::Noop,
    }
}

fn handle_session_editing_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::BackToHome,
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::ALT)
                || key.modifiers.contains(KeyModifiers::CONTROL)
            {
                Action::InsertNewline
            } else {
                Action::SendMessage
            }
        }
        KeyCode::Backspace => Action::DeleteChar,
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'j' => Action::InsertNewline,
                    'l' => Action::Noop, // clear screen
                    _ => Action::Noop,
                }
            } else {
                Action::InsertChar(c)
            }
        }
        KeyCode::PageUp => Action::ScrollUp,
        KeyCode::PageDown => Action::ScrollDown,
        _ => Action::Noop,
    }
}
