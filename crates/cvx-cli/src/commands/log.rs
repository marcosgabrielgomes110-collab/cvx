use cvx_core::{Error, Result};
use cvx_repository::{LogOptions, Repository, Signature};
use time::{Month, OffsetDateTime, UtcOffset, Weekday};

use crate::cli::LogArgs;

/// Executes `cvx log`.
pub fn run(args: LogArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let entries = repository.log(&LogOptions {
        revision: args.revision,
        limit: args.max_count,
    })?;

    for (index, entry) in entries.iter().enumerate() {
        if index > 0 && !args.oneline {
            println!();
        }

        if args.oneline {
            println!("{} {}", entry.oid.short(7), entry.commit.subject());
            continue;
        }

        println!("commit {}", entry.oid);
        if entry.commit.parents.len() > 1 {
            let parents: Vec<String> = entry
                .commit
                .parents
                .iter()
                .map(|parent| parent.short(7))
                .collect();
            println!("Merge: {}", parents.join(" "));
        }
        println!(
            "Author: {} <{}>",
            entry.commit.author.name, entry.commit.author.email
        );
        println!("Date:   {}", format_date(&entry.commit.author)?);
        println!();

        for line in entry.commit.message.trim_end().lines() {
            if line.is_empty() {
                println!();
            } else {
                println!("    {line}");
            }
        }
    }

    Ok(())
}

fn format_date(signature: &Signature) -> Result<String> {
    let datetime = OffsetDateTime::from_unix_timestamp(signature.timestamp)
        .map_err(|error| Error::Message(format!("invalid commit timestamp: {error}")))?;
    let offset = UtcOffset::from_whole_seconds(signature.offset_minutes * 60)
        .map_err(|error| Error::Message(format!("invalid commit timezone: {error}")))?;
    let local = datetime.to_offset(offset);

    let sign = if signature.offset_minutes < 0 {
        '-'
    } else {
        '+'
    };
    let absolute = signature.offset_minutes.unsigned_abs();

    Ok(format!(
        "{} {} {} {:02}:{:02}:{:02} {} {}{:02}{:02}",
        weekday_short(local.weekday()),
        month_short(local.month()),
        local.day(),
        local.hour(),
        local.minute(),
        local.second(),
        local.year(),
        sign,
        absolute / 60,
        absolute % 60
    ))
}

fn weekday_short(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn month_short(month: Month) -> &'static str {
    match month {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}
