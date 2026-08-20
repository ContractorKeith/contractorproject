// Executable contract for dated non-working calendar exceptions (issue #53).
// One rule: an exception date is a non-working civil day, treated identically to
// a weekly non-working day by every traversal.

use std::collections::HashSet;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};
use contractorproject_lib::scheduling::{
    calculate_schedule, calculate_schedule_with_constraints, calculate_schedule_with_progress,
    CalendarWeekday, DependencyType, FinishStartDependency, ScheduleInput, ScheduleProgress,
    ScheduleTask, TaskConstraint, TaskProgress, WorkingCalendar,
};

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date")
}

fn at(day: &str, time: &str) -> NaiveDateTime {
    date(day)
        .and_hms_opt(
            time[0..2].parse().expect("hour"),
            time[3..5].parse().expect("minute"),
            0,
        )
        .expect("fixture instant")
}

/// Monday-Friday, 08:00-16:00, with the supplied dated exceptions.
fn calendar_with(exceptions: &[&str]) -> WorkingCalendar {
    WorkingCalendar {
        working_weekdays: vec![
            CalendarWeekday::Monday,
            CalendarWeekday::Tuesday,
            CalendarWeekday::Wednesday,
            CalendarWeekday::Thursday,
            CalendarWeekday::Friday,
        ],
        workday_start_minute: 8 * 60,
        workday_duration_minutes: 8 * 60,
        exceptions: exceptions.iter().map(|value| date(value)).collect(),
    }
}

fn task(id: &str, duration_minutes: i64) -> ScheduleTask {
    ScheduleTask {
        id: id.into(),
        parent_task_id: None,
        duration_minutes: Some(duration_minutes),
    }
}

fn fs(predecessor: &str, successor: &str, lag_minutes: i64) -> FinishStartDependency {
    FinishStartDependency {
        predecessor_task_id: predecessor.into(),
        successor_task_id: successor.into(),
        dependency_type: DependencyType::FinishStart,
        lag_minutes,
    }
}

fn dep(pred: &str, succ: &str, kind: DependencyType, lag: i64) -> FinishStartDependency {
    FinishStartDependency {
        predecessor_task_id: pred.into(),
        successor_task_id: succ.into(),
        dependency_type: kind,
        lag_minutes: lag,
    }
}

fn constraint(task_id: &str, snet: Option<&str>, fnlt: Option<&str>) -> TaskConstraint {
    TaskConstraint {
        task_id: task_id.into(),
        start_no_earlier_than: snet.map(date),
        finish_no_later_than: fnlt.map(date),
    }
}

fn progress_entry(
    task_id: &str,
    percent: u8,
    actual_start: Option<&str>,
    actual_finish: Option<&str>,
) -> TaskProgress {
    TaskProgress {
        task_id: task_id.into(),
        percent_complete: percent,
        actual_start: actual_start.map(date),
        actual_finish: actual_finish.map(date),
    }
}

const MONDAY: &str = "2026-01-05";

// --- Executable examples ----------------------------------------------------

#[test]
fn a_mid_task_exception_splits_the_task_across_the_closure() {
    // A(1440) = three working days from Monday, but Wednesday is closed, so the
    // run is Mon, Tue, [Wed skipped], Thu.
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: date(MONDAY),
        calendar: calendar_with(&["2026-01-07"]),
        tasks: vec![task("A", 1440)],
        dependencies: vec![],
    })
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_start, at(MONDAY, "08:00"));
    assert_eq!(result.tasks[0].early_finish, at("2026-01-08", "16:00"));
}

#[test]
fn positive_lag_consumes_working_minutes_past_an_exception() {
    // A finishes Monday 16:00. FS+480 consumes one working day; Tuesday is closed
    // so the lag lands on Wednesday, and B resumes Thursday.
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: date(MONDAY),
        calendar: calendar_with(&["2026-01-06"]),
        tasks: vec![task("A", 480), task("B", 480)],
        dependencies: vec![fs("A", "B", 480)],
    })
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_finish, at(MONDAY, "16:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-08", "08:00"));
    assert_eq!(result.tasks[1].early_finish, at("2026-01-08", "16:00"));
}

#[test]
fn negative_lag_pulls_a_successor_back_across_an_exception() {
    // A is pinned to Wednesday by an SNET while Tuesday is closed. SS-480 lets B
    // start one working day before A: Tuesday is skipped, so B lands Monday.
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-06"]),
            tasks: vec![task("A", 480), task("B", 480)],
            dependencies: vec![dep("A", "B", DependencyType::StartStart, -480)],
        },
        &[constraint("A", Some("2026-01-07"), None)],
    )
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_start, at("2026-01-07", "08:00"));
    assert_eq!(result.tasks[1].early_start, at(MONDAY, "08:00"));
}

#[test]
fn snet_on_an_exception_normalizes_forward_to_the_next_working_day() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-07"]),
            tasks: vec![task("A", 480)],
            dependencies: vec![],
        },
        &[constraint("A", Some("2026-01-07"), None)],
    )
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_start, at("2026-01-08", "08:00"));
}

#[test]
fn fnlt_on_an_exception_normalizes_back_to_the_prior_working_day() {
    // A(1440) runs Mon, Tue, [Wed closed], Thu. FNLT Wednesday (closed) normalizes
    // to the finish of the last working day on or before it: Tuesday 16:00. The
    // Thursday early finish exceeds that, so A is directly violated.
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-07"]),
            tasks: vec![task("A", 1440)],
            dependencies: vec![],
        },
        &[constraint("A", None, Some("2026-01-07"))],
    )
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_finish, at("2026-01-08", "16:00"));
    assert_eq!(result.tasks[0].late_finish, at("2026-01-06", "16:00"));
    assert!(result.tasks[0].constraint_violated);
    assert_eq!(result.directly_violated_leaf_task_ids, vec!["A"]);
}

#[test]
fn a_data_date_on_an_exception_advances_to_the_next_working_instant() {
    // Data date Wednesday (closed) -> Thursday 08:00. Both leaves start no earlier
    // than Thursday.
    let result = calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-07"]),
            tasks: vec![task("A", 480), task("B", 480)],
            dependencies: vec![fs("A", "B", 0)],
        },
        &[],
        &ScheduleProgress {
            data_date: Some(date("2026-01-07")),
            entries: vec![],
        },
    )
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_start, at("2026-01-08", "08:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-09", "08:00"));
}

#[test]
fn in_progress_remaining_work_resumes_across_an_exception_next_to_the_data_date() {
    // A(1440) is 50% done (720 remaining). Data date Thursday, Friday closed. The
    // remaining work runs Thu (480), skips Fri and the weekend, finishes Monday
    // at 240 minutes into the day.
    let result = calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-09"]),
            tasks: vec![task("A", 1440)],
            dependencies: vec![],
        },
        &[],
        &ScheduleProgress {
            data_date: Some(date("2026-01-08")),
            entries: vec![progress_entry("A", 50, Some(MONDAY), None)],
        },
    )
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_start, at(MONDAY, "08:00"));
    assert_eq!(result.tasks[0].early_finish, at("2026-01-12", "12:00"));
}

#[test]
fn a_complete_actual_finish_on_an_exception_normalizes_backward() {
    let result = calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-07"]),
            tasks: vec![task("A", 480)],
            dependencies: vec![],
        },
        &[],
        &ScheduleProgress {
            data_date: Some(date("2026-01-09")),
            entries: vec![progress_entry("A", 100, Some(MONDAY), Some("2026-01-07"))],
        },
    )
    .expect("valid schedule");
    assert_eq!(
        result.tasks[0].actual_finish,
        Some(at("2026-01-06", "16:00"))
    );
}

#[test]
fn actuals_that_invert_around_an_exception_reject_atomically() {
    // Equal actuals on a closed day: the start normalizes forward to Thursday and
    // the finish backward to Tuesday, inverting the civil window.
    let error = calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-07"]),
            tasks: vec![task("A", 480)],
            dependencies: vec![],
        },
        &[],
        &ScheduleProgress {
            data_date: Some(date("2026-01-09")),
            entries: vec![progress_entry(
                "A",
                100,
                Some("2026-01-07"),
                Some("2026-01-07"),
            )],
        },
    )
    .expect_err("inverted actuals reject");
    assert_eq!(error.code(), "progress_normalized_order");
}

#[test]
fn a_multi_day_exception_run_bridges_a_weekend() {
    // Friday and the following Monday are both closed. A finishes Thursday 16:00;
    // its FS successor skips Fri, Sat, Sun, Mon and starts Tuesday.
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: date(MONDAY),
            calendar: calendar_with(&["2026-01-09", "2026-01-12"]),
            tasks: vec![task("A", 480), task("B", 480)],
            dependencies: vec![fs("A", "B", 0)],
        },
        &[constraint("A", Some("2026-01-08"), None)],
    )
    .expect("valid schedule");
    assert_eq!(result.tasks[0].early_finish, at("2026-01-08", "16:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-13", "08:00"));
}

// --- No-exception regressions ----------------------------------------------

#[test]
fn an_exception_on_a_non_working_weekday_is_a_no_op() {
    // Saturday is already non-working, so listing it changes nothing.
    let base = ScheduleInput {
        schedule_start: date(MONDAY),
        calendar: calendar_with(&[]),
        tasks: vec![task("A", 2400), task("B", 480)],
        dependencies: vec![fs("A", "B", 240)],
    };
    let baseline = calculate_schedule(&base).expect("baseline");
    let with_noop = calculate_schedule(&ScheduleInput {
        calendar: calendar_with(&["2026-01-10"]), // a Saturday
        ..base.clone()
    })
    .expect("no-op exception");
    assert_eq!(baseline.tasks, with_noop.tasks);
    assert_eq!(baseline.critical_path, with_noop.critical_path);
}

#[test]
fn an_exception_before_the_schedule_start_is_a_no_op() {
    let base = ScheduleInput {
        schedule_start: date(MONDAY),
        calendar: calendar_with(&[]),
        tasks: vec![task("A", 1440), task("B", 480)],
        dependencies: vec![fs("A", "B", 0)],
    };
    let baseline = calculate_schedule(&base).expect("baseline");
    let with_prior = calculate_schedule(&ScheduleInput {
        calendar: calendar_with(&["2026-01-01"]), // before the first working day
        ..base.clone()
    })
    .expect("prior exception");
    assert_eq!(baseline.tasks, with_prior.tasks);
}

// --- Closed-form / iterative agreement (property-style) ---------------------

/// Reference Nth working date computed by a plain day-by-day scan, independent
/// of the scheduler's closed-form offset math.
fn reference_working_date(
    start: NaiveDate,
    working_weekdays: &HashSet<Weekday>,
    exceptions: &HashSet<NaiveDate>,
    index: usize,
) -> NaiveDate {
    let mut date = start;
    let mut seen = 0usize;
    loop {
        let working = working_weekdays.contains(&date.weekday()) && !exceptions.contains(&date);
        if working {
            if seen == index {
                return date;
            }
            seen += 1;
        }
        date += Duration::days(1);
    }
}

#[test]
fn closed_form_and_iterative_offsets_agree_over_a_chain_of_working_days() {
    // A chain of 30 one-day tasks exercises working_date_after across many
    // exceptions at assorted offsets; each start must match the reference scan.
    let exceptions = [
        "2026-01-07", // Wed, offset 2
        "2026-01-08", // Thu, adjacent run
        "2026-01-14", // Wed
        "2026-01-20", // Tue
        "2026-02-02", // Mon
        "2026-02-03", // Tue, adjacent run
        "2026-02-13", // Fri before a weekend
    ];
    let working_weekdays: HashSet<Weekday> = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
    ]
    .into_iter()
    .collect();
    let exception_set: HashSet<NaiveDate> = exceptions.iter().map(|value| date(value)).collect();

    let count = 30usize;
    let tasks: Vec<ScheduleTask> = (0..count).map(|i| task(&format!("t{i:02}"), 480)).collect();
    let dependencies: Vec<FinishStartDependency> = (1..count)
        .map(|i| fs(&format!("t{:02}", i - 1), &format!("t{i:02}"), 0))
        .collect();

    let result = calculate_schedule(&ScheduleInput {
        schedule_start: date(MONDAY),
        calendar: calendar_with(&exceptions),
        tasks,
        dependencies,
    })
    .expect("valid chain");

    for (i, scheduled) in result.tasks.iter().enumerate() {
        let expected = reference_working_date(date(MONDAY), &working_weekdays, &exception_set, i);
        assert_eq!(
            scheduled.early_start.date(),
            expected,
            "offset {i} disagreed"
        );
    }
}

// --- Validation rejections --------------------------------------------------

fn reject_calendar(exceptions: &[&str]) -> String {
    calculate_schedule(&ScheduleInput {
        schedule_start: date(MONDAY),
        calendar: calendar_with(exceptions),
        tasks: vec![task("A", 480)],
        dependencies: vec![],
    })
    .expect_err("invalid calendar")
    .code()
    .to_string()
}

#[test]
fn duplicate_exceptions_are_rejected() {
    assert_eq!(
        reject_calendar(&["2026-01-07", "2026-01-07"]),
        "calendar_duplicate_exception"
    );
}

#[test]
fn out_of_range_exception_years_are_rejected() {
    assert_eq!(
        reject_calendar(&["1999-12-31"]),
        "calendar_exception_out_of_range"
    );
    assert_eq!(
        reject_calendar(&["2101-01-01"]),
        "calendar_exception_out_of_range"
    );
}

#[test]
fn more_than_four_thousand_exceptions_are_rejected() {
    let mut day = date("2026-01-01");
    let mut exceptions = Vec::new();
    for _ in 0..4001 {
        exceptions.push(day.format("%Y-%m-%d").to_string());
        day += Duration::days(1);
    }
    let refs: Vec<&str> = exceptions.iter().map(String::as_str).collect();
    assert_eq!(reject_calendar(&refs), "calendar_too_many_exceptions");
}
