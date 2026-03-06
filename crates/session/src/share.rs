//! Session sharing functionality.
//!
//! Generates share URLs for sessions.

use anyhow::Result;

use crate::session::SessionService;

/// Share a session, generating a URL.
pub fn share(session_id: &str, session_svc: &SessionService) -> Result<String> {
    // Generate a random share token
    let token: String = (0..32)
        .map(|_| {
            let idx = rand::random::<u8>() % 36;
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();

    let share_url = format!("https://share.opencode.ai/s/{token}");

    // Update session with share URL
    session_svc.update_share_url(session_id, Some(&share_url))?;

    Ok(share_url)
}

/// Remove sharing for a session.
pub fn unshare(session_id: &str, session_svc: &SessionService) -> Result<()> {
    session_svc.update_share_url(session_id, None)?;
    Ok(())
}
