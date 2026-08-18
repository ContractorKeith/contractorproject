use chrono::{Datelike, NaiveDate, NaiveDateTime, Weekday};
use contractorproject_lib::scheduling::{
    calculate_schedule, calculate_schedule_with_constraints, calculate_schedule_with_progress,
    CalendarWeekday, FinishStartDependency, ScheduleError, ScheduleInput, ScheduleProgress,
    ScheduleTask, TaskConstraint, TaskProgress, WorkingCalendar,
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

fn constraint(task_id: &str, snet: Option<&str>, fnlt: Option<&str>) -> TaskConstraint {
    let date = |value: &str| NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date");
    TaskConstraint {
        task_id: task_id.into(),
        start_no_earlier_than: snet.map(date),
        finish_no_later_than: fnlt.map(date),
    }
}

fn progress(
    task_id: &str,
    percent_complete: u8,
    actual_start: Option<&str>,
    actual_finish: Option<&str>,
) -> TaskProgress {
    let date = |value: &str| NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date");
    TaskProgress {
        task_id: task_id.into(),
        percent_complete,
        actual_start: actual_start.map(date),
        actual_finish: actual_finish.map(date),
    }
}

fn status(data_date: Option<&str>, entries: Vec<TaskProgress>) -> ScheduleProgress {
    let date = |value: &str| NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date");
    ScheduleProgress {
        data_date: data_date.map(date),
        entries,
    }
}

#[test]
fn applies_weekday_and_weekend_snet_without_changing_unconstrained_results() {
    let input = ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![task("A", 480), task("B", 480)],
        dependencies: vec![],
    };
    let unconstrained = calculate_schedule(&input).expect("valid schedule");
    assert_eq!(
        unconstrained.tasks[0].early_start,
        at("2026-01-05", "08:00")
    );
    let pre_start =
        calculate_schedule_with_constraints(&input, &[constraint("A", Some("2026-01-02"), None)])
            .expect("valid pre-start SNET");
    assert_eq!(pre_start.tasks[0].early_start, at("2026-01-05", "08:00"));

    let result = calculate_schedule_with_constraints(
        &input,
        &[
            constraint("A", Some("2026-01-07"), None),
            constraint("B", Some("2026-01-10"), None),
        ],
    )
    .expect("valid constrained schedule");
    assert_eq!(result.tasks[0].early_start, at("2026-01-07", "08:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-12", "08:00"));
    assert!(result.directly_violated_leaf_task_ids.is_empty());
}

#[test]
fn pre_start_snet_milestone_stays_at_the_normalized_project_start() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("M", 0)],
            dependencies: vec![],
        },
        &[constraint("M", Some("2026-01-02"), None)],
    )
    .expect("valid constrained milestone");
    assert_eq!(result.tasks[0].early_start, at("2026-01-05", "08:00"));
    assert_eq!(result.tasks[0].early_finish, result.tasks[0].early_start);
}

#[test]
fn applies_fnlt_and_reports_negative_float_and_direct_violation() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("A", 960)],
            dependencies: vec![],
        },
        &[constraint("A", None, Some("2026-01-05"))],
    )
    .expect("valid constrained schedule");

    let a = &result.tasks[0];
    assert_eq!(a.early_start, at("2026-01-05", "08:00"));
    assert_eq!(a.early_finish, at("2026-01-06", "16:00"));
    assert_eq!(a.late_start, at("2026-01-02", "08:00"));
    assert_eq!(a.total_float_minutes, -480);
    assert!(a.critical);
    assert!(a.constraint_violated);
    assert_eq!(result.directly_violated_leaf_task_ids, vec!["A"]);
    assert_eq!(result.critical_path, vec!["A"]);
}

#[test]
fn normalizes_weekend_fnlt_and_supports_conflicting_leaf_constraints() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("A", 480)],
            dependencies: vec![],
        },
        &[constraint("A", Some("2026-01-12"), Some("2026-01-10"))],
    )
    .expect("valid constrained schedule");
    let a = &result.tasks[0];
    assert_eq!(a.early_start, at("2026-01-12", "08:00"));
    assert_eq!(a.late_finish, at("2026-01-09", "16:00"));
    assert_eq!(a.total_float_minutes, -480);
    assert!(a.constraint_violated);
}

#[test]
fn fnlt_before_schedule_start_preserves_the_prior_working_finish() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("A", 480)],
            dependencies: vec![],
        },
        &[constraint("A", None, Some("2026-01-04"))],
    )
    .expect("pre-start FNLT is valid");
    assert_eq!(result.tasks[0].late_start, at("2026-01-02", "08:00"));
    assert_eq!(result.tasks[0].late_finish, at("2026-01-02", "16:00"));
    assert_eq!(result.tasks[0].total_float_minutes, -480);
    assert!(result.tasks[0].constraint_violated);
}

#[test]
fn pre_start_fnlt_milestone_preserves_its_deadline_boundary() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("M", 0)],
            dependencies: vec![],
        },
        &[constraint("M", None, Some("2026-01-04"))],
    )
    .expect("pre-start milestone FNLT is valid");
    let milestone = &result.tasks[0];
    assert_eq!(milestone.early_start, at("2026-01-05", "08:00"));
    assert_eq!(milestone.late_start, at("2026-01-02", "16:00"));
    assert_eq!(milestone.late_finish, milestone.late_start);
    assert_eq!(milestone.total_float_minutes, 0);
    assert!(milestone.constraint_violated);
}

#[test]
fn nonterminal_fnlt_is_preserved_while_propagating_successor_dates() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("A", 480), task("B", 480)],
            dependencies: vec![fs("A", "B", 0)],
        },
        &[constraint("A", None, Some("2026-01-05"))],
    )
    .expect("valid constrained schedule");
    assert_eq!(result.tasks[0].late_finish, at("2026-01-05", "16:00"));
    assert_eq!(result.tasks[0].late_start, at("2026-01-05", "08:00"));
}

#[test]
fn impossible_milestone_constraint_keeps_late_boundary_dates_equal() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("M", 0)],
            dependencies: vec![],
        },
        &[constraint("M", Some("2026-01-06"), Some("2026-01-05"))],
    )
    .expect("valid constrained schedule");
    let milestone = &result.tasks[0];
    assert_eq!(milestone.early_start, at("2026-01-06", "08:00"));
    assert_eq!(milestone.early_finish, milestone.early_start);
    assert_eq!(milestone.late_start, at("2026-01-05", "16:00"));
    assert_eq!(milestone.late_start, milestone.late_finish);
    assert_eq!(milestone.total_float_minutes, 0);
    assert!(milestone.constraint_violated);
}

#[test]
fn zero_float_civil_violation_does_not_replace_the_unconstrained_path_rule() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![task("A", 480), task("M", 0)],
            dependencies: vec![],
        },
        &[constraint("M", Some("2026-01-06"), Some("2026-01-05"))],
    )
    .expect("valid constrained schedule");
    assert!(result.tasks[1].constraint_violated);
    assert_eq!(result.tasks[1].total_float_minutes, 0);
    assert_eq!(result.critical_path, vec!["A"]);
}

#[test]
fn summary_preserves_a_constrained_milestone_civil_instant() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![summary("S"), child_task("M", "S", 0)],
            dependencies: vec![],
        },
        &[constraint("M", Some("2026-01-06"), None)],
    )
    .expect("valid constrained schedule");
    assert_eq!(result.tasks[0].early_start, at("2026-01-06", "08:00"));
    assert_eq!(result.tasks[0].early_finish, result.tasks[0].early_start);
    assert_eq!(result.tasks[1].early_start, result.tasks[0].early_start);
    assert_eq!(result.tasks[1].early_finish, result.tasks[0].early_finish);
}

#[test]
fn constraints_propagate_through_dependencies_milestones_and_summaries() {
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
            calendar: standard_calendar(),
            tasks: vec![
                summary("S"),
                child_task("A", "S", 480),
                child_task("M", "S", 0),
                child_task("B", "S", 480),
            ],
            dependencies: vec![fs("A", "M", 480), fs("M", "B", 0)],
        },
        &[constraint("B", None, Some("2026-01-06"))],
    )
    .expect("valid constrained schedule");
    assert_eq!(result.tasks[1].early_finish, at("2026-01-05", "16:00"));
    assert_eq!(result.tasks[2].early_finish, at("2026-01-06", "16:00"));
    assert_eq!(result.tasks[3].early_finish, at("2026-01-07", "16:00"));
    assert!(result.tasks[0].constraint_violated);
    assert_eq!(result.directly_violated_leaf_task_ids, vec!["B"]);
}

#[test]
fn negative_float_path_uses_lexical_branch_when_input_is_shuffled() {
    let input = |tasks| ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks,
        dependencies: vec![fs("A", "B", 0), fs("A", "C", 0)],
    };
    let constraints = [
        constraint("B", None, Some("2026-01-05")),
        constraint("C", None, Some("2026-01-05")),
    ];
    let first = calculate_schedule_with_constraints(
        &input(vec![task("C", 960), task("A", 480), task("B", 960)]),
        &constraints,
    )
    .expect("schedule");
    let second = calculate_schedule_with_constraints(
        &input(vec![task("B", 960), task("C", 960), task("A", 480)]),
        &constraints,
    )
    .expect("schedule");
    assert_eq!(first.critical_path, vec!["A", "B"]);
    assert_eq!(second.critical_path, first.critical_path);
}

#[test]
fn rejects_invalid_constraint_targets_stably() {
    let input = ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks: vec![summary("S"), child_task("A", "S", 480)],
        dependencies: vec![],
    };
    for (constraints, code) in [
        (
            vec![constraint("", None, None)],
            "task_constraint_id_required",
        ),
        (
            vec![constraint("missing", None, None)],
            "task_constraint_task_missing",
        ),
        (vec![constraint("S", None, None)], "task_constraint_summary"),
        (
            vec![
                constraint("A", Some("2026-01-05"), None),
                constraint("A", Some("2026-01-05"), None),
            ],
            "task_constraint_duplicate",
        ),
        (vec![constraint("A", None, None)], "task_constraint_empty"),
    ] {
        assert_eq!(
            calculate_schedule_with_constraints(&input, &constraints)
                .expect_err("invalid constraint")
                .code(),
            code
        );
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

fn progress_input(
    tasks: Vec<ScheduleTask>,
    dependencies: Vec<FinishStartDependency>,
) -> ScheduleInput {
    ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("fixture date"),
        calendar: standard_calendar(),
        tasks,
        dependencies,
    }
}

#[test]
fn no_progress_reports_default_status_fields() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![fs("A", "B", 0)],
    ))
    .expect("valid schedule");
    assert_eq!(result.data_date, None);
    for scheduled in &result.tasks {
        assert_eq!(scheduled.percent_complete, 0);
        assert_eq!(scheduled.actual_start, None);
        assert_eq!(scheduled.actual_finish, None);
    }
    // The widest seam with empty progress equals the constraint seam.
    let via_progress = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &ScheduleProgress::default(),
    )
    .expect("valid schedule");
    assert_eq!(via_progress, result);
}

#[test]
fn data_date_without_entries_pushes_incomplete_work_to_the_data_date() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(Some("2026-01-07"), vec![]),
    )
    .expect("valid empty status update");
    assert_eq!(result.data_date, Some(at("2026-01-07", "08:00")));
    assert_eq!(result.tasks[0].early_start, at("2026-01-07", "08:00"));
    assert_eq!(result.tasks[0].early_finish, at("2026-01-07", "16:00"));
    assert_eq!(result.tasks[1].early_start, at("2026-01-08", "08:00"));
    assert_eq!(result.tasks[1].early_finish, at("2026-01-08", "16:00"));
    assert_eq!(result.tasks[0].percent_complete, 0);
}

#[test]
fn complete_predecessor_anchors_and_in_progress_leaf_reschedules_remaining_work() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 960)], vec![fs("A", "B", 0)]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![
                progress("A", 100, Some("2026-01-05"), Some("2026-01-05")),
                progress("B", 50, Some("2026-01-06"), None),
            ],
        ),
    )
    .expect("valid progress");

    assert_eq!(result.data_date, Some(at("2026-01-07", "08:00")));

    let a = &result.tasks[0];
    assert_eq!(a.percent_complete, 100);
    assert_eq!(a.early_start, at("2026-01-05", "08:00"));
    assert_eq!(a.early_finish, at("2026-01-05", "16:00"));
    assert_eq!(a.actual_start, Some(at("2026-01-05", "08:00")));
    assert_eq!(a.actual_finish, Some(at("2026-01-05", "16:00")));
    assert_eq!(a.total_float_minutes, 0);
    assert!(!a.critical);

    let b = &result.tasks[1];
    assert_eq!(b.percent_complete, 50);
    // Reported early start is the actual start; remaining work is scheduled from
    // the data date and finishes half a shift of 960 minutes later.
    assert_eq!(b.early_start, at("2026-01-06", "08:00"));
    assert_eq!(b.early_finish, at("2026-01-07", "16:00"));
    assert_eq!(b.actual_start, Some(at("2026-01-06", "08:00")));
    assert_eq!(b.actual_finish, None);
    assert_eq!(b.total_float_minutes, 0);
    assert!(b.critical);

    assert_eq!(result.schedule_finish, at("2026-01-07", "16:00"));
    assert_eq!(result.critical_task_ids, vec!["B"]);
    // The complete predecessor is excluded from the primary driving path.
    assert_eq!(result.critical_path, vec!["B"]);
}

#[test]
fn predecessor_finish_pushes_a_successor_past_the_data_date() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 1440), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(Some("2026-01-06"), vec![]),
    )
    .expect("valid empty status update");
    // A's remaining work starts at the data date and consumes three shifts.
    assert_eq!(result.tasks[0].early_start, at("2026-01-06", "08:00"));
    assert_eq!(result.tasks[0].early_finish, at("2026-01-08", "16:00"));
    // B honors the predecessor finish, not just the data date.
    assert_eq!(result.tasks[1].early_start, at("2026-01-09", "08:00"));
    assert_eq!(result.tasks[1].early_finish, at("2026-01-09", "16:00"));
}

#[test]
fn complete_leaf_reports_fnlt_violation_against_its_actual_finish() {
    let violated = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480)], vec![]),
        &[constraint("A", None, Some("2026-01-07"))],
        &status(
            Some("2026-01-09"),
            vec![progress("A", 100, Some("2026-01-07"), Some("2026-01-08"))],
        ),
    )
    .expect("valid progress");
    let a = &violated.tasks[0];
    assert_eq!(a.percent_complete, 100);
    assert_eq!(a.early_finish, at("2026-01-08", "16:00"));
    assert!(a.constraint_violated);
    assert_eq!(a.total_float_minutes, 0);
    assert!(!a.critical);
    assert_eq!(violated.directly_violated_leaf_task_ids, vec!["A"]);

    let clean = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480)], vec![]),
        &[constraint("A", None, Some("2026-01-07"))],
        &status(
            Some("2026-01-09"),
            vec![progress("A", 100, Some("2026-01-05"), Some("2026-01-06"))],
        ),
    )
    .expect("valid progress");
    assert!(!clean.tasks[0].constraint_violated);
    assert!(clean.directly_violated_leaf_task_ids.is_empty());
}

#[test]
fn complete_milestone_anchors_at_its_actual_event() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("M", 0)], vec![]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![progress("M", 100, Some("2026-01-06"), Some("2026-01-06"))],
        ),
    )
    .expect("valid milestone progress");
    let m = &result.tasks[0];
    assert!(m.milestone);
    assert_eq!(m.percent_complete, 100);
    assert_eq!(m.early_start, at("2026-01-06", "16:00"));
    assert_eq!(m.early_finish, m.early_start);
    assert_eq!(m.total_float_minutes, 0);
    assert!(!m.critical);
    assert_eq!(m.actual_start, Some(at("2026-01-06", "08:00")));
    assert_eq!(m.actual_finish, Some(at("2026-01-06", "16:00")));
}

#[test]
fn summary_rolls_up_duration_weighted_percent_complete() {
    let result = calculate_schedule_with_progress(
        &progress_input(
            vec![
                summary("S"),
                child_task("A", "S", 480),
                child_task("B", "S", 1440),
            ],
            vec![],
        ),
        &[],
        &status(
            Some("2026-01-12"),
            vec![
                progress("A", 100, Some("2026-01-05"), Some("2026-01-05")),
                progress("B", 50, Some("2026-01-06"), None),
            ],
        ),
    )
    .expect("valid progress");
    // floor((480*100 + 1440*50) / 1920) == 62.
    assert!(result.tasks[0].summary);
    assert_eq!(result.tasks[0].percent_complete, 62);
    assert_eq!(result.tasks[1].percent_complete, 100);
    assert_eq!(result.tasks[2].percent_complete, 50);
}

#[test]
fn all_milestone_summary_rolls_up_completeness() {
    let tasks = || {
        vec![
            summary("S"),
            child_task("M1", "S", 0),
            child_task("M2", "S", 0),
        ]
    };
    let all_complete = calculate_schedule_with_progress(
        &progress_input(tasks(), vec![]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![
                progress("M1", 100, Some("2026-01-05"), Some("2026-01-05")),
                progress("M2", 100, Some("2026-01-06"), Some("2026-01-06")),
            ],
        ),
    )
    .expect("valid progress");
    assert_eq!(all_complete.tasks[0].percent_complete, 100);

    let partial = calculate_schedule_with_progress(
        &progress_input(tasks(), vec![]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![progress("M1", 100, Some("2026-01-05"), Some("2026-01-05"))],
        ),
    )
    .expect("valid progress");
    assert_eq!(partial.tasks[0].percent_complete, 0);
}

#[test]
fn an_all_complete_schedule_has_an_empty_driving_path() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![
                progress("A", 100, Some("2026-01-05"), Some("2026-01-05")),
                progress("B", 100, Some("2026-01-06"), Some("2026-01-06")),
            ],
        ),
    )
    .expect("valid progress");
    assert!(result.critical_path.is_empty());
    assert!(result.critical_task_ids.is_empty());
    assert!(!result.tasks[0].critical);
    assert!(!result.tasks[1].critical);
}

#[test]
fn normalizes_weekend_actuals_and_a_non_working_data_date() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480)], vec![]),
        &[],
        &status(
            Some("2026-01-10"),
            vec![progress("A", 100, Some("2026-01-09"), Some("2026-01-10"))],
        ),
    )
    .expect("valid progress");
    // The Saturday data date rolls forward to Monday's working start.
    assert_eq!(result.data_date, Some(at("2026-01-12", "08:00")));
    let a = &result.tasks[0];
    // Actual start rolls forward to Friday's start; actual finish rolls back to
    // Friday's finish.
    assert_eq!(a.actual_start, Some(at("2026-01-09", "08:00")));
    assert_eq!(a.actual_finish, Some(at("2026-01-09", "16:00")));
    assert_eq!(a.early_start, at("2026-01-09", "08:00"));
    assert_eq!(a.early_finish, at("2026-01-09", "16:00"));
}

#[test]
fn rejects_invalid_progress_inputs_stably() {
    let input = progress_input(
        vec![
            summary("S"),
            child_task("C", "S", 480),
            task("A", 480),
            task("M", 0),
        ],
        vec![],
    );
    let cases = [
        (
            status(None, vec![progress("A", 50, Some("2026-01-05"), None)]),
            "progress_data_date_required",
        ),
        (
            status(Some("2026-01-09"), vec![progress("", 0, None, None)]),
            "progress_task_id_required",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("A", 0, None, None), progress("A", 0, None, None)],
            ),
            "progress_duplicate",
        ),
        (
            status(Some("2026-01-09"), vec![progress("missing", 0, None, None)]),
            "progress_task_missing",
        ),
        (
            status(Some("2026-01-09"), vec![progress("S", 0, None, None)]),
            "progress_summary",
        ),
        (
            status(Some("2026-01-09"), vec![progress("A", 150, None, None)]),
            "progress_percent_out_of_range",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("A", 0, Some("2026-01-05"), None)],
            ),
            "progress_actuals_forbidden",
        ),
        (
            status(Some("2026-01-09"), vec![progress("A", 50, None, None)]),
            "progress_actual_start_required",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("A", 50, Some("2026-01-05"), Some("2026-01-06"))],
            ),
            "progress_actual_finish_forbidden",
        ),
        (
            status(Some("2026-01-09"), vec![progress("A", 100, None, None)]),
            "progress_actual_start_required",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("A", 100, Some("2026-01-05"), None)],
            ),
            "progress_actual_finish_required",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("M", 50, Some("2026-01-05"), None)],
            ),
            "progress_milestone_percent",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("M", 100, Some("2026-01-05"), Some("2026-01-06"))],
            ),
            "progress_milestone_actuals_unequal",
        ),
        (
            status(
                Some("2026-01-09"),
                vec![progress("A", 100, Some("2026-01-06"), Some("2026-01-05"))],
            ),
            "progress_actual_order",
        ),
        (
            status(
                Some("2026-01-06"),
                vec![progress("A", 100, Some("2026-01-05"), Some("2026-01-08"))],
            ),
            "progress_actual_after_data_date",
        ),
    ];
    for (update, code) in cases {
        assert_eq!(
            calculate_schedule_with_progress(&input, &[], &update)
                .expect_err("invalid progress")
                .code(),
            code
        );
    }
}
