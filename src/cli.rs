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
    #[arg(long, value_name = "YYYY-MM", value_parser = parse_month_year)]
    pub from: MonthYear,

    /// End of simulation timespan
    #[arg(long, value_name = "YYYY-MM", value_parser = parse_month_year)]
    pub to: MonthYear,

    /// Number of clients
    #[arg(long)]
    pub clients: u64,
}

impl Cli {
    pub fn parse() -> Cli {
        <Cli as Parser>::parse()
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

    // Get the Last second in this month as a DateTime object
    pub(crate) fn last_datetime(&self) -> DateTime<Utc> {
        // last day
        let d = NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, 1).unwrap()
            + chrono::Months::new(1)
            - chrono::Days::new(1);
        let t = NaiveTime::from_hms_opt(23, 59, 59).unwrap();
        Utc.from_utc_datetime(&NaiveDateTime::new(d, t))
    }
}

fn parse_month_year(s: &str) -> Result<MonthYear, String> {
    // common error
    let err = || "Invalid month. Required format is YYYY-MM".to_string();

    if s.len() != 7 || s.chars().nth(4) != Some('-') {
        return Err(err());
    }

    let year = s[..4].parse::<u16>().map_err(|_| err())?;
    let month = s[5..].parse::<u8>().map_err(|_| err())?;

    Ok(MonthYear { year, month })
}
