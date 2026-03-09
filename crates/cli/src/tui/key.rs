//! Key binding definitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{Action, ActiveOverlay, InputMode, QuestionDialogState, Screen};

pub fn handle_key(
    key: KeyEvent,
    screen: &Screen,
    input_mode: &InputMode,
    overlay: &ActiveOverlay,
) -> Action {
    // When an overlay is active, route keys to it first
    match overlay {
        ActiveOverlay::None => {}
        ActiveOverlay::Permission(state) => return handle_permission_overlay_key(key, state.selected),
        ActiveOverlay::Question(state) => return handle_question_overlay_key(key, state),
    }

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

fn handle_permission_overlay_key(key: KeyEvent, selected: usize) -> Action {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            Action::OverlaySelect(selected.saturating_sub(1))
        }
        KeyCode::Down | KeyCode::Char('j') => {
            Action::OverlaySelect((selected + 1).min(2))
        }
        KeyCode::Enter => Action::OverlayConfirm,
        KeyCode::Esc => Action::OverlayDismiss,
        // Shortcuts: y=allow, n=deny, a=always
        KeyCode::Char('y') => {
            // Select Allow then confirm
            Action::OverlayConfirm // selected is set to 0 by default (Allow)
        }
        KeyCode::Char('n') => Action::OverlayDismiss,
        _ => Action::Noop,
    }
}

fn handle_question_overlay_key(key: KeyEvent, state: &QuestionDialogState) -> Action {
    if state.options.is_empty() {
        // Free-text input mode
        match key.code {
            KeyCode::Enter => Action::OverlayConfirm,
            KeyCode::Esc => Action::OverlayDismiss,
            KeyCode::Backspace => Action::OverlayBackspace,
            KeyCode::Char(c) => Action::OverlayInput(c),
            _ => Action::Noop,
        }
    } else {
        // Option selection mode
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                Action::OverlaySelect(state.selected_option.saturating_sub(1))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                Action::OverlaySelect((state.selected_option + 1).min(state.options.len().saturating_sub(1)))
            }
            KeyCode::Enter => Action::OverlayConfirm,
            KeyCode::Esc => Action::OverlayDismiss,
            _ => Action::Noop,
        }
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
