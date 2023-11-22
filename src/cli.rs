use std::path::PathBuf;

use chrono::prelude::*;
use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// Seed for the random number generators. If 0 or omitted, generate and print
    /// a random seed.
    #[clap(long, default_value_t = 0)]
    pub seed: u64,

    /// Location of consensus and descriptor files
    #[arg(long, value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    pub tor_data: PathBuf,

    /// Begin of simulation timespan
    #[arg(long, value_name = "YYYY-MM[-DD[:HH[:mm]]]", value_parser = parse_simulation_range_edge)]
    pub from: SimulationRangeEdge,

    /// End of simulation timespan
    #[arg(long, value_name = "YYYY-MM[-DD[:HH[:mm]]]", value_parser = parse_simulation_range_edge)]
    pub to: SimulationRangeEdge,

    /// Number of clients. If omitted, use values from PrivCount measurements.
    #[arg(long)]
    pub clients: Option<u64>,

    /// Number of adversarial guards
    #[arg(long, requires = "adv_guards_bw")]
    pub adv_guards_num: Option<u64>,

    /// Consensus weight per adversarial guard
    #[arg(long, requires = "adv_guards_num")]
    pub adv_guards_bw: Option<u64>,

    /// Number of adversarial exits
    #[arg(long, requires = "adv_exits_bw")]
    pub adv_exits_num: Option<u64>,

    /// Consensus weight per adversarial exit
    #[arg(long, requires = "adv_exits_num")]
    pub adv_exits_bw: Option<u64>,

    /// Path to the (prepared) stream model JSON file
    #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub stream_model: PathBuf,

    /// Path to the (prepared) packet model JSON file
    #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub packet_model: PathBuf,

    /// Path to the output message trace file
    #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub output_trace: PathBuf,
}

impl Cli {
    pub fn parse() -> Cli {
        <Cli as Parser>::parse()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SimulationRangeEdge {
    MonthYear(MonthYear),
    DayMonthYear(DayMonthYear),
    HourDayMonthYear(HourDayMonthYear),
    MinuteHourDayMonthYear(MinuteHourDayMonthYear),
}

impl SimulationRangeEdge {
    /// Get the first second described by this simulation range edge
    pub(crate) fn first_datetime(&self) -> DateTime<Utc> {
        match self {
            SimulationRangeEdge::MonthYear(x) => x.first_datetime(),
            SimulationRangeEdge::DayMonthYear(x) => x.first_datetime(),
            SimulationRangeEdge::HourDayMonthYear(x) => x.first_datetime(),
            SimulationRangeEdge::MinuteHourDayMonthYear(x) => x.first_datetime(),
        }
    }

    /// Get the first second described by this simulation range edge
    pub(crate) fn last_datetime(&self) -> DateTime<Utc> {
        match self {
            SimulationRangeEdge::MonthYear(x) => x.last_datetime(),
            SimulationRangeEdge::DayMonthYear(x) => x.last_datetime(),
            SimulationRangeEdge::HourDayMonthYear(x) => x.last_datetime(),
            SimulationRangeEdge::MinuteHourDayMonthYear(x) => x.last_datetime(),
        }
    }

    /// Get the year described by this simulation range edge
    pub(crate) fn year(&self) -> u16 {
        match self {
            SimulationRangeEdge::MonthYear(x) => x.year,
            SimulationRangeEdge::DayMonthYear(x) => x.year,
            SimulationRangeEdge::HourDayMonthYear(x) => x.year,
            SimulationRangeEdge::MinuteHourDayMonthYear(x) => x.year,
        }
    }

    /// Get the month described by this simulation range edge
    pub(crate) fn month(&self) -> u8 {
        match self {
            SimulationRangeEdge::MonthYear(x) => x.month,
            SimulationRangeEdge::DayMonthYear(x) => x.month,
            SimulationRangeEdge::HourDayMonthYear(x) => x.month,
            SimulationRangeEdge::MinuteHourDayMonthYear(x) => x.month,
        }
    }

    /// Get the day described by this simulation range edge, if present
    pub(crate) fn day(&self) -> Option<u8> {
        match self {
            SimulationRangeEdge::MonthYear(_) => None,
            SimulationRangeEdge::DayMonthYear(x) => Some(x.day),
            SimulationRangeEdge::HourDayMonthYear(x) => Some(x.day),
            SimulationRangeEdge::MinuteHourDayMonthYear(x) => Some(x.day),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MonthYear {
    pub year: u16,
    pub month: u8,
}

impl MonthYear {
    // Get the first second in this month as a DateTime object
    pub(crate) fn first_datetime(&self) -> DateTime<Utc> {
        let d = NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, 1).unwrap();
        let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }

    // Get the last second in this month as a DateTime object
    pub(crate) fn last_datetime(&self) -> DateTime<Utc> {
        // last day
        let d = NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, 1).unwrap()
            + chrono::Months::new(1)
            - chrono::Days::new(1);
        let t = NaiveTime::from_hms_opt(23, 59, 59).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DayMonthYear {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl DayMonthYear {
    // Get the first second of this day as a DateTime object
    pub(crate) fn first_datetime(&self) -> DateTime<Utc> {
        let d =
            NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32).unwrap();
        let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }

    // Get the last second of this day as a DateTime object
    pub(crate) fn last_datetime(&self) -> DateTime<Utc> {
        let d =
            NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32).unwrap();
        let t = NaiveTime::from_hms_opt(23, 59, 59).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HourDayMonthYear {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
}

impl HourDayMonthYear {
    // Get the first second of this day as a DateTime object
    pub(crate) fn first_datetime(&self) -> DateTime<Utc> {
        let d =
            NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32).unwrap();
        let t = NaiveTime::from_hms_opt(self.hour as u32, 0, 0).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }

    // Get the last second of this day as a DateTime object
    pub(crate) fn last_datetime(&self) -> DateTime<Utc> {
        let d =
            NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32).unwrap();
        let t = NaiveTime::from_hms_opt(self.hour as u32, 59, 59).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MinuteHourDayMonthYear {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

impl MinuteHourDayMonthYear {
    // Get the first second of this day as a DateTime object
    pub(crate) fn first_datetime(&self) -> DateTime<Utc> {
        let d =
            NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32).unwrap();
        let t = NaiveTime::from_hms_opt(self.hour as u32, self.minute as u32, 0).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }

    // Get the last second of this day as a DateTime object
    pub(crate) fn last_datetime(&self) -> DateTime<Utc> {
        let d =
            NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32).unwrap();
        let t = NaiveTime::from_hms_opt(self.hour as u32, self.minute as u32, 59).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }
}

fn parse_simulation_range_edge(s: &str) -> Result<SimulationRangeEdge, String> {
    // common error
    let err = || {
        "Invalid time for range. Required format is YYYY-MM or YYYY-MM-DD or YYYY-MM-DD:HH or YYYY-MM-DD:HH:mm".to_string()
    };

    if s.len() == 7 {
        // parse YYYY-MM

        if s.chars().nth(4) != Some('-') {
            return Err(err());
        }

        let year = s[..4].parse::<u16>().map_err(|_| err())?;
        let month = s[5..].parse::<u8>().map_err(|_| err())?;

        return Ok(SimulationRangeEdge::MonthYear(MonthYear { year, month }));
    } else if s.len() == 10 {
        // parse YYYY-MM-DD

        if s.chars().nth(4) != Some('-') || s.chars().nth(7) != Some('-') {
            return Err(err());
        }

        let year = s[..4].parse::<u16>().map_err(|_| err())?;
        let month = s[5..7].parse::<u8>().map_err(|_| err())?;
        let day = s[8..].parse::<u8>().map_err(|_| err())?;

        return Ok(SimulationRangeEdge::DayMonthYear(DayMonthYear {
            year,
            month,
            day,
        }));
    } else if s.len() == 13 {
        // parse YYYY-MM-DD:HH

        if s.chars().nth(4) != Some('-')
            || s.chars().nth(7) != Some('-')
            || s.chars().nth(10) != Some(':')
        {
            return Err(err());
        }

        let year = s[..4].parse::<u16>().map_err(|_| err())?;
        let month = s[5..7].parse::<u8>().map_err(|_| err())?;
        let day = s[8..10].parse::<u8>().map_err(|_| err())?;
        let hour = s[11..].parse::<u8>().map_err(|_| err())?;

        return Ok(SimulationRangeEdge::HourDayMonthYear(HourDayMonthYear {
            year,
            month,
            day,
            hour,
        }));
    } else if s.len() == 16 {
        // parse YYYY-MM-DD:HH:mm

        if s.chars().nth(4) != Some('-')
            || s.chars().nth(7) != Some('-')
            || s.chars().nth(10) != Some(':')
            || s.chars().nth(13) != Some(':')
        {
            return Err(err());
        }

        let year = s[..4].parse::<u16>().map_err(|_| err())?;
        let month = s[5..7].parse::<u8>().map_err(|_| err())?;
        let day = s[8..10].parse::<u8>().map_err(|_| err())?;
        let hour = s[11..13].parse::<u8>().map_err(|_| err())?;
        let minute = s[14..].parse::<u8>().map_err(|_| err())?;

        return Ok(SimulationRangeEdge::MinuteHourDayMonthYear(
            MinuteHourDayMonthYear {
                year,
                month,
                day,
                hour,
                minute,
            },
        ));
    }

    Err(err())
}
