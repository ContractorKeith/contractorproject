use chrono::{Datelike, NaiveDate, NaiveDateTime, Weekday};
use contractorproject_lib::scheduling::{
    calculate_schedule, CalendarWeekday, FinishStartDependency, ScheduleError, ScheduleInput,
    ScheduleTask, WorkingCalendar,
};

fn at(date: &str, time: &str) -> NaiveDateTime {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .expect("valid fixture date")
        .and_hms_opt(
            time[0..2].parse().expect("valid hour"),
            time[3..5].parse().expect("valid minute"),
            0,
        )
        .expect("valid fixture time")
}

fn standard_calendar() -> WorkingCalendar {
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
    }
}

fn calendar_weekday(weekday: Weekday) -> CalendarWeekday {
    match weekday {
        Weekday::Mon => CalendarWeekday::Monday,
        Weekday::Tue => CalendarWeekday::Tuesday,
        Weekday::Wed => CalendarWeekday::Wednesday,
        Weekday::Thu => CalendarWeekday::Thursday,
        Weekday::Fri => CalendarWeekday::Friday,
        Weekday::Sat => CalendarWeekday::Saturday,
        Weekday::Sun => CalendarWeekday::Sunday,
    }
}

fn task(id: &str, duration_minutes: i64) -> ScheduleTask {
    ScheduleTask {
        id: id.into(),
        parent_task_id: None,
        duration_minutes: Some(duration_minutes),
    }
}

fn child_task(id: &str, parent_task_id: &str, duration_minutes: i64) -> ScheduleTask {
    ScheduleTask {
        id: id.into(),
        parent_task_id: Some(parent_task_id.into()),
        duration_minutes: Some(duration_minutes),
    }
}

fn summary(id: &str) -> ScheduleTask {
    ScheduleTask {
        id: id.into(),
        parent_task_id: None,
        duration_minutes: None,
    }
}

fn fs(predecessor: &str, successor: &str, lag_minutes: i64) -> FinishStartDependency {
    FinishStartDependency {
        predecessor_task_id: predecessor.into(),
        successor_task_id: successor.into(),
        lag_minutes,
    }
}

#[test]
fn schedules_a_finish_to_start_chain_on_working_time() {
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("A", 960), task("B", 480), task("C", 0)],
        dependencies: vec![fs("A", "B", 0), fs("B", "C", 0)],
    })
    .expect("valid schedule");

    assert_eq!(result.schedule_start, at("2026-01-05", "08:00"));
    assert_eq!(result.schedule_finish, at("2026-01-07", "16:00"));
    assert_eq!(result.critical_path, vec!["A", "B", "C"]);

    let a = &result.tasks[0];
    assert_eq!(a.early_start, at("2026-01-05", "08:00"));
    assert_eq!(a.early_finish, at("2026-01-06", "16:00"));
    assert_eq!(a.late_start, a.early_start);
    assert_eq!(a.late_finish, a.early_finish);
    assert_eq!(a.total_float_minutes, 0);
    assert!(a.critical);

    let b = &result.tasks[1];
    assert_eq!(b.early_start, at("2026-01-07", "08:00"));
    assert_eq!(b.early_finish, at("2026-01-07", "16:00"));
    assert_eq!(b.total_float_minutes, 0);

    let milestone = &result.tasks[2];
    assert_eq!(milestone.early_start, at("2026-01-07", "16:00"));
    assert_eq!(milestone.early_finish, milestone.early_start);
    assert!(milestone.milestone);
}

#[test]
fn calculates_float_and_a_stable_driving_path_through_a_branch_and_merge() {
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![
            task("A", 480),
            task("B", 240),
            task("C", 480),
            task("D", 120),
            task("M", 0),
        ],
        dependencies: vec![
            fs("A", "B", 0),
            fs("A", "C", 0),
            fs("B", "D", 0),
            fs("C", "D", 0),
            fs("D", "M", 0),
        ],
    })
    .expect("valid schedule");

    assert_eq!(result.schedule_finish, at("2026-01-07", "10:00"));
    assert_eq!(result.critical_task_ids, vec!["A", "C", "D", "M"]);
    assert_eq!(result.critical_path, vec!["A", "C", "D", "M"]);

    let short_branch = &result.tasks[1];
    assert_eq!(short_branch.early_start, at("2026-01-06", "08:00"));
    assert_eq!(short_branch.early_finish, at("2026-01-06", "12:00"));
    assert_eq!(short_branch.late_start, at("2026-01-06", "12:00"));
    assert_eq!(short_branch.late_finish, at("2026-01-06", "16:00"));
    assert_eq!(short_branch.total_float_minutes, 240);
    assert!(!short_branch.critical);
}

#[test]
fn skips_non_working_weekend_days() {
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 9).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("A", 480), task("B", 480)],
        dependencies: vec![fs("A", "B", 0)],
    })
    .expect("valid schedule");

    assert_eq!(result.tasks[0].early_start, at("2026-01-09", "08:00"));
    assert_eq!(result.tasks[0].early_finish, at("2026-01-09", "16:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-12", "08:00"));
    assert_eq!(result.tasks[1].early_finish, at("2026-01-12", "16:00"));
}

#[test]
fn derives_summary_dates_duration_float_and_critical_state_from_children() {
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![
            summary("S1"),
            child_task("A", "S1", 480),
            child_task("B", "S1", 480),
            summary("S2"),
            child_task("C", "S2", 480),
        ],
        dependencies: vec![fs("A", "B", 0)],
    })
    .expect("valid schedule");

    let critical_summary = &result.tasks[0];
    assert!(critical_summary.summary);
    assert_eq!(critical_summary.early_start, at("2026-01-05", "08:00"));
    assert_eq!(critical_summary.early_finish, at("2026-01-06", "16:00"));
    assert_eq!(critical_summary.duration_minutes, 960);
    assert_eq!(critical_summary.total_float_minutes, 0);
    assert!(critical_summary.critical);

    let floating_summary = &result.tasks[3];
    assert_eq!(floating_summary.early_start, at("2026-01-05", "08:00"));
    assert_eq!(floating_summary.early_finish, at("2026-01-05", "16:00"));
    assert_eq!(floating_summary.late_start, at("2026-01-06", "08:00"));
    assert_eq!(floating_summary.late_finish, at("2026-01-06", "16:00"));
    assert_eq!(floating_summary.duration_minutes, 480);
    assert_eq!(floating_summary.total_float_minutes, 480);
    assert!(!floating_summary.critical);
}

#[test]
fn consumes_positive_finish_to_start_lag_in_working_minutes() {
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("A", 480), task("M", 0), task("B", 480)],
        dependencies: vec![fs("A", "M", 960), fs("M", "B", 0)],
    })
    .expect("valid schedule");

    assert_eq!(result.tasks[0].early_finish, at("2026-01-05", "16:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-07", "16:00"));
    assert_eq!(result.tasks[1].early_finish, at("2026-01-07", "16:00"));
    assert_eq!(result.tasks[2].early_start, at("2026-01-08", "08:00"));
    assert_eq!(result.tasks[2].early_finish, at("2026-01-08", "16:00"));
    assert_eq!(result.critical_path, vec!["A", "M", "B"]);
}

#[test]
fn chooses_the_same_primary_path_when_equal_branches_are_shuffled() {
    let input = |tasks| ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks,
        dependencies: vec![
            fs("A", "C", 0),
            fs("C", "D", 0),
            fs("A", "B", 0),
            fs("B", "D", 0),
        ],
    };

    let first = calculate_schedule(&input(vec![
        task("A", 480),
        task("C", 960),
        task("B", 960),
        task("D", 480),
    ]))
    .expect("valid schedule");
    let second = calculate_schedule(&input(vec![
        task("D", 480),
        task("B", 960),
        task("C", 960),
        task("A", 480),
    ]))
    .expect("valid schedule");

    assert_eq!(first.critical_path, vec!["A", "B", "D"]);
    assert_eq!(second.critical_path, first.critical_path);
}

#[test]
fn rejects_dependency_cycles_with_a_stable_error() {
    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("C", 480), task("A", 480), task("B", 480)],
        dependencies: vec![fs("B", "C", 0), fs("C", "A", 0), fs("A", "B", 0)],
    })
    .expect_err("cyclic schedule must be rejected");

    assert_eq!(error.code(), "dependency_cycle");
    assert_eq!(
        error,
        ScheduleError::DependencyCycle {
            task_ids: vec!["A".into(), "B".into(), "C".into()]
        }
    );
}

#[test]
fn rejects_invalid_task_dependency_and_calendar_inputs() {
    let cases = [
        (
            vec![task("A", 480)],
            vec![fs("A", "A", 0)],
            standard_calendar(),
            "dependency_self_link",
        ),
        (
            vec![task("A", 480)],
            vec![fs("A", "missing", 0)],
            standard_calendar(),
            "dependency_task_missing",
        ),
        (
            vec![task("A", -1)],
            vec![],
            standard_calendar(),
            "task_duration_negative",
        ),
        (
            vec![task("A", 480)],
            vec![],
            WorkingCalendar {
                working_weekdays: vec![],
                workday_start_minute: 480,
                workday_duration_minutes: 480,
            },
            "calendar_no_working_days",
        ),
    ];

    for (tasks, dependencies, calendar, expected_code) in cases {
        let error = calculate_schedule(&ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar,
            tasks,
            dependencies,
        })
        .expect_err("invalid fixture must be rejected");
        assert_eq!(error.code(), expected_code);
    }

    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![summary("S"), child_task("A", "S", 480), task("B", 480)],
        dependencies: vec![fs("S", "B", 0)],
    })
    .expect_err("summary dependencies must be rejected");
    assert_eq!(error.code(), "dependency_summary_endpoint");
}

#[test]
fn reports_an_out_of_range_schedule_instead_of_panicking() {
    let different_workday = match NaiveDate::MAX.weekday() {
        Weekday::Mon => CalendarWeekday::Tuesday,
        _ => CalendarWeekday::Monday,
    };
    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::MAX,
        calendar: WorkingCalendar {
            working_weekdays: vec![different_workday],
            workday_start_minute: 480,
            workday_duration_minutes: 480,
        },
        tasks: vec![],
        dependencies: vec![],
    })
    .expect_err("unrepresentable schedule must return an error");

    assert_eq!(error.code(), "schedule_out_of_range");

    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::MAX,
        calendar: WorkingCalendar {
            working_weekdays: vec![calendar_weekday(NaiveDate::MAX.weekday())],
            workday_start_minute: 480,
            workday_duration_minutes: 480,
        },
        tasks: vec![task("A", 960)],
        dependencies: vec![],
    })
    .expect_err("unrepresentable calculated dates must return an error");
    assert_eq!(error.code(), "schedule_out_of_range");

    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("A", i64::MAX), task("B", 1)],
        dependencies: vec![fs("A", "B", 0)],
    })
    .expect_err("overflowing working-minute arithmetic must return an error");
    assert_eq!(error.code(), "schedule_out_of_range");

    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("A", 1_000_000_000_000_000)],
        dependencies: vec![],
    })
    .expect_err("an unrepresentable terminal finish must return an error");
    assert_eq!(error.code(), "schedule_out_of_range");
}

#[test]
fn rejects_a_hierarchy_deeper_than_the_supported_wbs_limit() {
    let mut tasks = Vec::new();
    for index in 0..257 {
        tasks.push(ScheduleTask {
            id: format!("S{index:03}"),
            parent_task_id: (index > 0).then(|| format!("S{:03}", index - 1)),
            duration_minutes: None,
        });
    }
    tasks.push(ScheduleTask {
        id: "leaf".into(),
        parent_task_id: Some("S256".into()),
        duration_minutes: Some(480),
    });

    let error = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks,
        dependencies: vec![],
    })
    .expect_err("over-deep task hierarchy must be rejected");

    assert_eq!(error.code(), "task_hierarchy_too_deep");
}
