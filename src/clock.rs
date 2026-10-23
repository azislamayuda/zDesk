use crate::AppResult;
use std::mem::MaybeUninit;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LocalClock {
    pub hour: i32,
    pub minute: i32,
    // libc uses Sunday = 0, matching the Conky `%w` weekday value.
    pub weekday: usize,
}

pub fn local_clock() -> AppResult<LocalClock> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let timestamp: libc::time_t = seconds.try_into()?;
    let mut local = MaybeUninit::<libc::tm>::uninit();
    // localtime_r writes the whole tm structure and is thread safe.
    let result = unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) };
    if result.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let local = unsafe { local.assume_init() };
    Ok(LocalClock {
        hour: local.tm_hour,
        minute: local.tm_min,
        weekday: usize::try_from(local.tm_wday)?,
    })
}

pub fn milliseconds_to_next_minute() -> AppResult<i32> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    Ok((60_000 - now % 60_000) as i32)
}
