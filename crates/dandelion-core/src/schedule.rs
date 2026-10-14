//! Civil-time selection is separate from monotonic runtime timers.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayNight {
    pub day_start_hour: u8,
    pub night_start_hour: u8,
    pub utc_offset_minutes: i32,
    pub day_bytes_per_second: u64,
    pub night_bytes_per_second: u64,
}
impl DayNight {
    pub fn validate(&self) -> Result<()> {
        if self.day_start_hour > 23
            || self.night_start_hour > 23
            || self.day_start_hour == self.night_start_hour
            || !(-840..=840).contains(&self.utc_offset_minutes)
            || [self.day_bytes_per_second, self.night_bytes_per_second]
                .iter()
                .any(|n| !(1..=1_073_741_824).contains(n))
        {
            return Err(Error::Config(
                "invalid day/night hours, UTC offset, or positive bounded rates".into(),
            ));
        }
        Ok(())
    }
    pub fn rate_at(&self, unix_seconds: i64) -> u64 {
        let second = (unix_seconds.rem_euclid(86400) + i64::from(self.utc_offset_minutes) * 60)
            .rem_euclid(86400);
        let day = i64::from(self.day_start_hour) * 3600;
        let night = i64::from(self.night_start_hour) * 3600;
        let daytime = if day < night {
            (day..night).contains(&second)
        } else {
            second >= day || second < night
        };
        if daytime {
            self.day_bytes_per_second
        } else {
            self.night_bytes_per_second
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn day_night_offset_and_midnight() {
        let mut s = DayNight {
            day_start_hour: 8,
            night_start_hour: 20,
            utc_offset_minutes: 0,
            day_bytes_per_second: 10,
            night_bytes_per_second: 20,
        };
        s.validate().unwrap();
        assert_eq!(s.rate_at(8 * 3600 - 1), 20);
        assert_eq!(s.rate_at(8 * 3600), 10);
        assert_eq!(s.rate_at(20 * 3600), 20);
        s.utc_offset_minutes = 480;
        assert_eq!(s.rate_at(0), 10);
        s.day_start_hour = 20;
        s.night_start_hour = 8;
        s.utc_offset_minutes = 0;
        assert_eq!(s.rate_at(-1), 10);
        assert_eq!(s.rate_at(12 * 3600), 20);
    }
    #[test]
    fn contradictory_schedules_fail() {
        let s = DayNight {
            day_start_hour: 8,
            night_start_hour: 8,
            utc_offset_minutes: 0,
            day_bytes_per_second: 1,
            night_bytes_per_second: 2,
        };
        assert!(s.validate().is_err());
    }
}
