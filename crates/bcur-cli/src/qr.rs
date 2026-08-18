//! Terminal QR rendering and fountain animation.

use std::io::{self, Write as _};
use std::time::Duration;

use bcur::{Encoder, qr_string};
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size,
};
use qrcode::render::unicode::Dense1x2;
use qrcode::{EcLevel, QrCode};

use crate::error::{Error, Result};

/// Version → modules → alphanumeric capacity at ECC Q (ISO/IEC 18004).
const ALNUM_Q_CAPACITY: [(usize, usize); 10] = [
    (21, 16),
    (25, 29),
    (29, 47),
    (33, 67),
    (37, 87),
    (41, 108),
    (45, 125),
    (49, 157),
    (53, 177),
    (57, 207),
];

/// Approx. alphanumeric (ECC Q) payload that fits a terminal using half-block QR.
///
/// Module count `21 + 4*(v-1)` plus a 4-module quiet zone; two modules per row.
pub(crate) fn tty_max_chars() -> Option<usize> {
    let (cols, rows) = size().ok()?;
    if !io::IsTerminal::is_terminal(&io::stdout()) {
        return None;
    }
    let max_cols = usize::from(cols).saturating_sub(2);
    let max_rows = usize::from(rows).saturating_sub(2);
    let max_modules = max_cols
        .saturating_sub(8)
        .min(max_rows.saturating_mul(2).saturating_sub(8));
    ALNUM_Q_CAPACITY
        .iter()
        .rev()
        .find(|(modules, _)| *modules <= max_modules)
        .map(|(_, cap)| *cap)
}

/// Render one UR as a static terminal QR (exits after printing).
pub(crate) fn show_static(ur: &str) -> Result<()> {
    let art = render_qr(ur)?;
    check_fits(&art)?;
    println!("{art}");
    Ok(())
}

/// Cycle fountain parts in an alternate screen until `q` or Ctrl-C.
pub(crate) fn animate_encoder(
    encoder: &mut Encoder,
    first_part: Option<String>,
    interval_ms: u64,
) -> Result<()> {
    let mut pending = first_part;
    run_animation(interval_ms, || {
        let part = match pending.take() {
            Some(p) => p,
            None => encoder.next_part()?,
        };
        let status = format!(
            "seq={} K={}  q quit",
            encoder.current_index(),
            encoder.fragment_count()
        );
        Ok((part, status))
    })
}

/// Cycle an already-encoded list of UR lines.
pub(crate) fn animate_parts(parts: &[String], interval_ms: u64) -> Result<()> {
    if parts.is_empty() {
        return Err(Error::msg("no UR lines to display"));
    }
    if let [only] = parts {
        return show_static(only);
    }
    let n = parts.len();
    let mut i = 0_usize;
    run_animation(interval_ms, || {
        let shown = i.saturating_add(1);
        let part = parts
            .get(i)
            .cloned()
            .ok_or_else(|| Error::msg("empty part list"))?;
        i = (i + 1) % n;
        Ok((part, format!("frame {shown}/{n}  q quit")))
    })
}

fn run_animation<F>(interval_ms: u64, mut next: F) -> Result<()>
where
    F: FnMut() -> Result<(String, String)>,
{
    if !io::IsTerminal::is_terminal(&io::stdout()) {
        return Err(Error::msg(
            "--qr/--animate display requires a terminal (stdout is not a tty)",
        ));
    }

    let _guard = AltScreen::enter()?;
    let interval = Duration::from_millis(interval_ms.max(1));
    let mut out = io::stdout();

    loop {
        let (part, status) = next()?;
        let art = render_qr(&part)?;
        check_fits(&art)?;
        execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
        for line in art.lines() {
            write!(out, "{line}\r\n")?;
        }
        write!(out, "\r\n{status}\r\n")?;
        out.flush()?;

        let deadline = std::time::Instant::now() + interval;
        loop {
            let remain = deadline.saturating_duration_since(std::time::Instant::now());
            if remain.is_zero() {
                break;
            }
            if quit_requested(remain)? {
                return Ok(());
            }
        }
    }
}

fn quit_requested(timeout: Duration) -> Result<bool> {
    if !event::poll(timeout)? {
        return Ok(false);
    }
    Ok(match event::read()? {
        Event::Key(key) => key_means_quit(&key),
        _ => false,
    })
}

fn key_means_quit(key: &event::KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q' | 'Q') | KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn render_qr(ur: &str) -> Result<String> {
    let payload = qr_string(ur);
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::Q)
        .map_err(|e| Error::qr(format!("QR encode failed: {e}")))?;
    Ok(code
        .render::<Dense1x2>()
        .quiet_zone(true)
        .module_dimensions(1, 1)
        .build())
}

fn check_fits(art: &str) -> Result<()> {
    if !io::IsTerminal::is_terminal(&io::stdout()) {
        return Ok(());
    }
    let rows = art.lines().count();
    let cols = art
        .lines()
        .map(str::chars)
        .map(Iterator::count)
        .max()
        .unwrap_or(0);
    let (term_cols, term_rows) = size()?;
    let need_rows = rows.saturating_add(2);
    if cols > usize::from(term_cols) || need_rows > usize::from(term_rows) {
        return Err(Error::qr(format!(
            "QR is {cols}×{need_rows} cells; terminal is {term_cols}×{term_rows}. \
             shrink the font, enlarge the window, or lower --max-chars"
        )));
    }
    Ok(())
}

struct AltScreen;

impl AltScreen {
    fn enter() -> Result<Self> {
        execute!(io::stdout(), EnterAlternateScreen)?;
        if let Err(e) = enable_raw_mode() {
            execute!(io::stdout(), LeaveAlternateScreen).ok();
            return Err(e.into());
        }
        Ok(Self)
    }
}

impl Drop for AltScreen {
    fn drop(&mut self) {
        execute!(io::stdout(), LeaveAlternateScreen).ok();
        disable_raw_mode().ok();
    }
}
