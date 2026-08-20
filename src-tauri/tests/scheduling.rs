use chrono::{Datelike, NaiveDate, NaiveDateTime, Weekday};
use contractorproject_lib::scheduling::{
    calculate_schedule, calculate_schedule_with_constraints, calculate_schedule_with_progress,
    CalendarGap, CalendarWeekday, DependencyType, FinishStartDependency, LateFinishLimit,
    ScheduleDriver, ScheduleError, ScheduleInput, ScheduleProgress, ScheduleResult, ScheduleTask,
    TaskConstraint, TaskExplanation, TaskProgress, WorkingCalendar,
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
        exceptions: Vec::new(),
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
    dep(
        predecessor,
        successor,
        DependencyType::FinishStart,
        lag_minutes,
    )
}

fn dep(
    predecessor: &str,
    successor: &str,
    dependency_type: DependencyType,
    lag_minutes: i64,
) -> FinishStartDependency {
    FinishStartDependency {
        predecessor_task_id: predecessor.into(),
        successor_task_id: successor.into(),
        dependency_type,
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
                exceptions: Vec::new(),
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
            exceptions: Vec::new(),
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
            exceptions: Vec::new(),
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

#[test]
fn complete_milestone_before_schedule_start_anchors_at_its_actual_event() {
    // Actuals fall on the working day before schedule start; anchoring must use
    // the stored actual instant, not an offset that would land on the wrong day.
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("M", 0)], vec![]),
        &[],
        &status(
            Some("2026-01-05"),
            vec![progress("M", 100, Some("2026-01-02"), Some("2026-01-02"))],
        ),
    )
    .expect("valid milestone progress");
    let m = &result.tasks[0];
    assert!(m.milestone);
    assert_eq!(m.early_start, at("2026-01-02", "16:00"));
    assert_eq!(m.early_finish, at("2026-01-02", "16:00"));
    assert_eq!(m.late_start, at("2026-01-02", "16:00"));
    assert_eq!(m.late_finish, at("2026-01-02", "16:00"));
    assert_eq!(m.actual_start, Some(at("2026-01-02", "08:00")));
    assert_eq!(m.actual_finish, Some(at("2026-01-02", "16:00")));
    assert_eq!(m.total_float_minutes, 0);
    assert!(!m.critical);
    assert_eq!(result.data_date, Some(at("2026-01-05", "08:00")));
}

#[test]
fn project_finish_ignores_complete_leaves_when_computing_float() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![
                progress("A", 100, Some("2026-01-07"), Some("2026-01-07")),
                progress("B", 90, Some("2026-01-06"), None),
            ],
        ),
    )
    .expect("valid progress");
    // A's late actual finish must not inflate B's float: with 48 remaining
    // minutes B is the sole driver.
    let b = &result.tasks[1];
    assert_eq!(b.percent_complete, 90);
    assert_eq!(b.total_float_minutes, 0);
    assert!(b.critical);
    assert!(!result.tasks[0].critical);
    assert_eq!(result.critical_task_ids, vec!["B"]);
    assert_eq!(result.critical_path, vec!["B"]);
}

#[test]
fn summary_float_and_critical_derive_from_incomplete_descendants() {
    // A long independent chain gives the not-started leaf real float; the
    // complete sibling must be excluded from the rollup.
    let floating = calculate_schedule_with_progress(
        &progress_input(
            vec![
                summary("S"),
                child_task("A", "S", 480),
                child_task("B", "S", 480),
                task("C", 2880),
            ],
            vec![],
        ),
        &[],
        &status(
            Some("2026-01-05"),
            vec![progress("A", 100, Some("2026-01-05"), Some("2026-01-05"))],
        ),
    )
    .expect("valid progress");
    let summary_row = &floating.tasks[0];
    assert!(summary_row.summary);
    assert_eq!(summary_row.total_float_minutes, 2400);
    assert!(!summary_row.critical);

    // Every descendant complete: the summary mirrors the complete-leaf rule.
    let all_complete = calculate_schedule_with_progress(
        &progress_input(
            vec![
                summary("S"),
                child_task("A", "S", 480),
                child_task("B", "S", 480),
            ],
            vec![],
        ),
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
    assert_eq!(all_complete.tasks[0].total_float_minutes, 0);
    assert!(!all_complete.tasks[0].critical);
}

#[test]
fn rejects_normalized_actual_date_inversion() {
    // Saturday actuals roll the start forward to Monday and the finish back to
    // the prior Friday, inverting the normalized civil window.
    let error = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480)], vec![]),
        &[],
        &status(
            Some("2026-01-12"),
            vec![progress("A", 100, Some("2026-01-10"), Some("2026-01-10"))],
        ),
    )
    .expect_err("normalized inversion must be rejected");
    assert_eq!(error.code(), "progress_normalized_order");
}

// ---------------------------------------------------------------------------
// Dependency types (SS/FF/SF) and signed lag (issue #47).
// ---------------------------------------------------------------------------

fn ss(predecessor: &str, successor: &str, lag_minutes: i64) -> FinishStartDependency {
    dep(
        predecessor,
        successor,
        DependencyType::StartStart,
        lag_minutes,
    )
}

fn ff(predecessor: &str, successor: &str, lag_minutes: i64) -> FinishStartDependency {
    dep(
        predecessor,
        successor,
        DependencyType::FinishFinish,
        lag_minutes,
    )
}

fn sf(predecessor: &str, successor: &str, lag_minutes: i64) -> FinishStartDependency {
    dep(
        predecessor,
        successor,
        DependencyType::StartFinish,
        lag_minutes,
    )
}

fn leaf<'a>(
    result: &'a contractorproject_lib::scheduling::ScheduleResult,
    id: &str,
) -> &'a contractorproject_lib::scheduling::ScheduledTask {
    result
        .tasks
        .iter()
        .find(|task| task.id == id)
        .expect("scheduled task present")
}

#[test]
fn start_start_lag_binds_the_successor_start() {
    for (lag, start, finish) in [
        (0, ("2026-01-05", "08:00"), ("2026-01-05", "16:00")),
        (480, ("2026-01-06", "08:00"), ("2026-01-06", "16:00")),
    ] {
        let result = calculate_schedule(&progress_input(
            vec![task("A", 480), task("B", 480)],
            vec![ss("A", "B", lag)],
        ))
        .expect("valid SS schedule");
        assert_eq!(leaf(&result, "B").early_start, at(start.0, start.1));
        assert_eq!(leaf(&result, "B").early_finish, at(finish.0, finish.1));
    }
}

#[test]
fn negative_start_start_lag_pulls_the_successor_earlier_and_clamps_at_start() {
    // A starts Monday; SS-480 would pull C before the schedule start, so it
    // clamps to Monday rather than erroring.
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("C", 480)],
        vec![ss("A", "C", -480)],
    ))
    .expect("valid negative SS schedule");
    assert_eq!(leaf(&result, "C").early_start, at("2026-01-05", "08:00"));

    // With B pushed to Tuesday, SS-480 pulls C back exactly one day to Monday.
    let chained = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480), task("C", 480)],
        vec![fs("A", "B", 0), ss("B", "C", -480)],
    ))
    .expect("valid chained negative SS schedule");
    assert_eq!(leaf(&chained, "B").early_start, at("2026-01-06", "08:00"));
    assert_eq!(leaf(&chained, "C").early_start, at("2026-01-05", "08:00"));
}

#[test]
fn finish_finish_lag_binds_the_successor_finish() {
    for (lag, start, finish) in [
        (0, ("2026-01-06", "08:00"), ("2026-01-06", "16:00")),
        (480, ("2026-01-07", "08:00"), ("2026-01-07", "16:00")),
    ] {
        let result = calculate_schedule(&progress_input(
            vec![task("A", 960), task("B", 480)],
            vec![ff("A", "B", lag)],
        ))
        .expect("valid FF schedule");
        assert_eq!(leaf(&result, "B").early_start, at(start.0, start.1));
        assert_eq!(leaf(&result, "B").early_finish, at(finish.0, finish.1));
    }
}

#[test]
fn start_finish_lag_binds_the_successor_finish_to_the_predecessor_start() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![sf("A", "B", 960)],
    ))
    .expect("valid SF schedule");
    // EF(B) >= ES(A) + 960 = Tuesday 16:00, so B starts Tuesday.
    assert_eq!(leaf(&result, "B").early_start, at("2026-01-06", "08:00"));
    assert_eq!(leaf(&result, "B").early_finish, at("2026-01-06", "16:00"));
}

#[test]
fn milestones_alias_predictably_under_each_type() {
    // FF+0 and FS+0 milestones both land on the predecessor finish instant,
    // while SS+0 and SF+0 land on the predecessor start instant.
    let result = calculate_schedule(&progress_input(
        vec![
            task("A", 480),
            task("MF", 0),
            task("MS", 0),
            task("MG", 0),
            task("MH", 0),
        ],
        vec![
            fs("A", "MF", 0),
            ss("A", "MS", 0),
            ff("A", "MG", 0),
            sf("A", "MH", 0),
        ],
    ))
    .expect("valid milestone schedule");
    assert_eq!(leaf(&result, "MF").early_finish, at("2026-01-05", "16:00"));
    assert_eq!(leaf(&result, "MG").early_finish, at("2026-01-05", "16:00"));
    assert_eq!(leaf(&result, "MS").early_finish, at("2026-01-05", "08:00"));
    assert_eq!(leaf(&result, "MH").early_finish, at("2026-01-05", "08:00"));
}

#[test]
fn mixed_type_chain_schedules_deterministically() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480), task("C", 480)],
        vec![ss("A", "B", 0), fs("B", "C", 0)],
    ))
    .expect("valid mixed chain");
    assert_eq!(leaf(&result, "A").early_start, at("2026-01-05", "08:00"));
    assert_eq!(leaf(&result, "B").early_start, at("2026-01-05", "08:00"));
    assert_eq!(leaf(&result, "C").early_start, at("2026-01-06", "08:00"));
}

#[test]
fn start_start_drives_the_critical_path() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![ss("A", "B", 0)],
    ))
    .expect("valid SS driving schedule");
    assert_eq!(result.critical_path, vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn finish_finish_drives_the_critical_path() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 960), task("B", 480)],
        vec![ff("A", "B", 0)],
    ))
    .expect("valid FF driving schedule");
    assert_eq!(result.critical_path, vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn start_start_drives_a_negative_float_path_to_a_violation() {
    let result = calculate_schedule_with_constraints(
        &progress_input(
            vec![task("A", 480), task("B", 480)],
            vec![ss("A", "B", 480)],
        ),
        &[constraint("B", None, Some("2026-01-05"))],
    )
    .expect("valid constrained SS schedule");
    assert_eq!(leaf(&result, "B").total_float_minutes, -480);
    assert!(result
        .directly_violated_leaf_task_ids
        .contains(&"B".to_string()));
    assert_eq!(result.critical_path, vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn complete_predecessor_anchors_by_type_against_the_data_date() {
    // A completes Monday. An FS successor resumes after the actual finish
    // (Tuesday); an SS successor resumes at the actual start (Monday).
    let data_date = Some("2026-01-05");
    let entries = || vec![progress("A", 100, Some("2026-01-05"), Some("2026-01-05"))];

    let fs_result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(data_date, entries()),
    )
    .expect("valid complete-FS schedule");
    assert_eq!(leaf(&fs_result, "B").early_start, at("2026-01-06", "08:00"));

    let ss_result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![ss("A", "B", 0)]),
        &[],
        &status(data_date, entries()),
    )
    .expect("valid complete-SS schedule");
    assert_eq!(leaf(&ss_result, "B").early_start, at("2026-01-05", "08:00"));
}

#[test]
fn in_progress_start_start_predecessor_still_floors_at_the_data_date() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 960), task("B", 480)], vec![ss("A", "B", 0)]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![progress("A", 50, Some("2026-01-05"), None)],
        ),
    )
    .expect("valid in-progress SS schedule");
    // B's remaining work cannot start before the Wednesday data date even though
    // A actually started Monday.
    assert_eq!(leaf(&result, "B").early_start, at("2026-01-07", "08:00"));
}

#[test]
fn different_type_links_between_the_same_pair_are_accepted() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 960), task("B", 480)],
        vec![ss("A", "B", 0), ff("A", "B", 0)],
    ))
    .expect("valid dual-type schedule");
    // SS allows Monday start, but FF pulls the finish to A's Tuesday finish.
    assert_eq!(leaf(&result, "B").early_start, at("2026-01-06", "08:00"));
    assert_eq!(leaf(&result, "B").early_finish, at("2026-01-06", "16:00"));
}

#[test]
fn duplicate_link_of_the_same_type_is_rejected() {
    let error = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![ss("A", "B", 0), ss("A", "B", 60)],
    ))
    .expect_err("duplicate typed link must be rejected");
    assert_eq!(error.code(), "dependency_duplicate");
}

#[test]
fn mixed_type_cycle_is_rejected_regardless_of_link_type() {
    let error = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![ss("A", "B", 0), ff("B", "A", 0)],
    ))
    .expect_err("mixed-type cycle must be rejected");
    assert_eq!(error.code(), "dependency_cycle");
}

#[test]
fn default_dependency_type_is_byte_identical_to_finish_start() {
    let typed = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![fs("A", "B", 120)],
    ))
    .expect("explicit FS schedule");
    let defaulted = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![FinishStartDependency {
            predecessor_task_id: "A".into(),
            successor_task_id: "B".into(),
            dependency_type: DependencyType::default(),
            lag_minutes: 120,
        }],
    ))
    .expect("defaulted schedule");
    assert_eq!(typed, defaulted);
}

#[test]
fn json_without_dependency_type_deserializes_as_finish_start() {
    let dependency: FinishStartDependency =
        serde_json::from_str(r#"{"predecessorTaskId":"A","successorTaskId":"B","lagMinutes":0}"#)
            .expect("dependency without a type");
    assert_eq!(dependency.dependency_type, DependencyType::FinishStart);
}

#[test]
fn driving_leaf_with_incomplete_successors_stays_on_the_critical_path() {
    // Each start/finish-anchored link lets A finish the project while its
    // successor B is still open; A must remain the terminal driving leaf.
    for link in [ss("A", "B", 0), ff("A", "B", -480), sf("A", "B", 0)] {
        let result = calculate_schedule(&progress_input(
            vec![task("A", 960), task("B", 480)],
            vec![link.clone()],
        ))
        .expect("valid driving schedule");
        assert_eq!(
            result.critical_path,
            vec!["A".to_string()],
            "unexpected path for {:?}",
            link.dependency_type
        );
    }
}

#[test]
fn driving_path_walks_through_a_start_anchored_successor() {
    // Z -> A finishes the project; A drives B by SS. The path must include both
    // Z and A even though A has an incomplete SS successor.
    let result = calculate_schedule(&progress_input(
        vec![task("Z", 480), task("A", 960), task("B", 480)],
        vec![fs("Z", "A", 0), ss("A", "B", 0)],
    ))
    .expect("valid chained driving schedule");
    assert_eq!(result.critical_path, vec!["Z".to_string(), "A".to_string()]);
}

#[test]
fn negative_lag_on_finish_finish_and_start_finish_clamps_at_start() {
    let ff_result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![ff("A", "B", -960)],
    ))
    .expect("valid negative FF schedule");
    assert_eq!(leaf(&ff_result, "B").early_start, at("2026-01-05", "08:00"));

    let sf_result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![sf("A", "B", -480)],
    ))
    .expect("valid negative SF schedule");
    assert_eq!(leaf(&sf_result, "B").early_start, at("2026-01-05", "08:00"));
}

#[test]
fn started_start_anchored_predecessor_imposes_no_late_bound() {
    // A is in progress; a tight FNLT on its SS successor B would pull A's late
    // dates back through the edge, but a started leaf's start is immovable, so A
    // keeps the same float as the no-link control and is not critical.
    let tasks = || vec![task("A", 960), task("B", 480), task("Z", 4800)];
    let progress_status = || {
        status(
            Some("2026-01-07"),
            vec![progress("A", 50, Some("2026-01-05"), None)],
        )
    };

    let linked = calculate_schedule_with_progress(
        &progress_input(tasks(), vec![ss("A", "B", 0)]),
        &[constraint("B", None, Some("2026-01-07"))],
        &progress_status(),
    )
    .expect("valid linked schedule");
    let control = calculate_schedule_with_progress(
        &progress_input(tasks(), vec![]),
        &[constraint("B", None, Some("2026-01-07"))],
        &progress_status(),
    )
    .expect("valid control schedule");

    assert_eq!(
        leaf(&linked, "A").total_float_minutes,
        leaf(&control, "A").total_float_minutes
    );
    assert!(!leaf(&linked, "A").critical);
}

#[test]
fn data_date_before_schedule_start_is_clamped_to_the_first_working_day() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480)], vec![]),
        &[],
        &status(Some("2026-01-01"), vec![]),
    )
    .expect("valid empty status update");
    assert_eq!(result.data_date, Some(at("2026-01-05", "08:00")));
    assert_eq!(result.tasks[0].early_start, at("2026-01-05", "08:00"));
    assert_eq!(result.tasks[0].early_finish, at("2026-01-05", "16:00"));
}

// --- Schedule explanations ---------------------------------------------------

fn ymd(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date")
}

fn calendar_with_exceptions(exceptions: &[&str]) -> WorkingCalendar {
    WorkingCalendar {
        exceptions: exceptions.iter().map(|value| ymd(value)).collect(),
        ..standard_calendar()
    }
}

fn explanation_task_id(explanation: &TaskExplanation) -> &str {
    match explanation {
        TaskExplanation::Summary { task_id }
        | TaskExplanation::Complete { task_id, .. }
        | TaskExplanation::Scheduled { task_id, .. } => task_id,
    }
}

fn explanation<'a>(result: &'a ScheduleResult, id: &str) -> &'a TaskExplanation {
    result
        .explanations
        .iter()
        .find(|explanation| explanation_task_id(explanation) == id)
        .expect("explanation present")
}

fn predecessor(id: &str, dependency_type: DependencyType, lag_minutes: i64) -> ScheduleDriver {
    ScheduleDriver::Predecessor {
        task_id: id.into(),
        dependency_type,
        lag_minutes,
    }
}

#[test]
fn explains_a_lone_leaf_from_the_schedule_start_floor() {
    let result =
        calculate_schedule(&progress_input(vec![task("A", 480)], vec![])).expect("valid schedule");
    let expected = TaskExplanation::Scheduled {
        task_id: "A".into(),
        primary_driver: ScheduleDriver::ScheduleStart {},
        other_binding_drivers: vec![],
        started_actual_start: None,
        calendar_gap: None,
        total_float_minutes: 0,
        critical: true,
        late_finish_limit: LateFinishLimit::ProjectFinish {},
    };
    assert_eq!(explanation(&result, "A"), &expected);
    // Lock the exact camelCase serialization including the kind tags.
    assert_eq!(
        serde_json::to_string(explanation(&result, "A")).expect("serialize"),
        r#"{"kind":"scheduled","taskId":"A","primaryDriver":{"kind":"scheduleStart"},"otherBindingDrivers":[],"startedActualStart":null,"calendarGap":null,"totalFloatMinutes":0,"critical":true,"lateFinishLimit":{"kind":"projectFinish"}}"#
    );
}

#[test]
fn explains_a_finish_start_chain_from_both_ends() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 960), task("B", 480)],
        vec![fs("A", "B", 0)],
    ))
    .expect("valid schedule");
    let b = explanation(&result, "B");
    assert_eq!(
        b,
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 0),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
    // Exact serialization exercising the predecessor driver tag from the spec.
    assert_eq!(
        serde_json::to_string(b).expect("serialize"),
        r#"{"kind":"scheduled","taskId":"B","primaryDriver":{"kind":"predecessor","taskId":"A","dependencyType":"FS","lagMinutes":0},"otherBindingDrivers":[],"startedActualStart":null,"calendarGap":null,"totalFloatMinutes":0,"critical":true,"lateFinishLimit":{"kind":"projectFinish"}}"#
    );
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Scheduled {
            task_id: "A".into(),
            primary_driver: ScheduleDriver::ScheduleStart {},
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::Successor {
                task_id: "B".into(),
                dependency_type: DependencyType::FinishStart,
                lag_minutes: 0,
            },
        }
    );
}

#[test]
fn explains_a_floating_branch_late_finish_limit() {
    // A -> B/C -> D with a shorter B branch: B carries 240 float bounded by D.
    let result = calculate_schedule(&progress_input(
        vec![
            task("A", 480),
            task("B", 240),
            task("C", 480),
            task("D", 480),
        ],
        vec![
            fs("A", "B", 0),
            fs("A", "C", 0),
            fs("B", "D", 0),
            fs("C", "D", 0),
        ],
    ))
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 0),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 240,
            critical: false,
            late_finish_limit: LateFinishLimit::Successor {
                task_id: "D".into(),
                dependency_type: DependencyType::FinishStart,
                lag_minutes: 0,
            },
        }
    );
}

#[test]
fn explains_a_weekend_calendar_gap_across_a_finish_start_link() {
    // A pinned to Friday via SNET; the weekend before B's Monday start is a gap.
    let result = calculate_schedule_with_constraints(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[constraint("A", Some("2026-01-09"), None)],
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 0),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: Some(CalendarGap {
                from_date: ymd("2026-01-09"),
                to_date: ymd("2026-01-12"),
                non_working_day_count: 2,
            }),
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn explains_a_start_constraint_over_a_closed_day() {
    // SNET Wednesday with Wednesday closed: primary startConstraint, gap counts Wed.
    let result = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: ymd("2026-01-05"),
            calendar: calendar_with_exceptions(&["2026-01-07"]),
            tasks: vec![task("A", 480)],
            dependencies: vec![],
        },
        &[constraint("A", Some("2026-01-07"), None)],
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Scheduled {
            task_id: "A".into(),
            primary_driver: ScheduleDriver::StartConstraint {
                date: ymd("2026-01-07"),
                normalized_date: ymd("2026-01-08"),
            },
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: Some(CalendarGap {
                from_date: ymd("2026-01-07"),
                to_date: ymd("2026-01-08"),
                non_working_day_count: 1,
            }),
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn reports_a_co_binding_start_constraint_as_a_secondary_driver() {
    // A finishes Monday 16:00 and B's SNET is Tuesday: both bind Tuesday 08:00.
    let result = calculate_schedule_with_constraints(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[constraint("B", Some("2026-01-06"), None)],
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 0),
            other_binding_drivers: vec![ScheduleDriver::StartConstraint {
                date: ymd("2026-01-06"),
                normalized_date: ymd("2026-01-06"),
            }],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn explains_a_data_date_push_and_its_successor() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(Some("2026-01-07"), vec![]),
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Scheduled {
            task_id: "A".into(),
            primary_driver: ScheduleDriver::DataDate {
                date: at("2026-01-07", "08:00"),
            },
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::Successor {
                task_id: "B".into(),
                dependency_type: DependencyType::FinishStart,
                lag_minutes: 0,
            },
        }
    );
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 0),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn explains_a_complete_leaf_and_an_in_progress_leaf() {
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 960)], vec![]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![
                progress("A", 100, Some("2026-01-05"), Some("2026-01-05")),
                progress("B", 50, Some("2026-01-06"), None),
            ],
        ),
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Complete {
            task_id: "A".into(),
            actual_start: at("2026-01-05", "08:00"),
            actual_finish: at("2026-01-05", "16:00"),
        }
    );
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: ScheduleDriver::DataDate {
                date: at("2026-01-07", "08:00"),
            },
            other_binding_drivers: vec![],
            started_actual_start: Some(at("2026-01-06", "08:00")),
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn omits_a_non_binding_negative_lag_link() {
    // SS-480 clamps C at Monday; the non-binding SS link is not reported.
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("C", 480)],
        vec![ss("A", "C", -480)],
    ))
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "C"),
        &TaskExplanation::Scheduled {
            task_id: "C".into(),
            primary_driver: ScheduleDriver::ScheduleStart {},
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn explains_a_finish_finish_primary_measuring_to_the_early_finish() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 960), task("B", 480)],
        vec![ff("A", "B", 0)],
    ))
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishFinish, 0),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn explains_a_positive_lag_gap_over_a_closed_day() {
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: ymd("2026-01-05"),
        calendar: calendar_with_exceptions(&["2026-01-06"]),
        tasks: vec![task("A", 480), task("B", 480)],
        dependencies: vec![fs("A", "B", 480)],
    })
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 480),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: Some(CalendarGap {
                from_date: ymd("2026-01-05"),
                to_date: ymd("2026-01-08"),
                non_working_day_count: 1,
            }),
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn explains_a_deadline_bounded_late_finish() {
    let result = calculate_schedule_with_constraints(
        &progress_input(vec![task("A", 960)], vec![]),
        &[constraint("A", None, Some("2026-01-05"))],
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Scheduled {
            task_id: "A".into(),
            primary_driver: ScheduleDriver::ScheduleStart {},
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: -480,
            critical: true,
            late_finish_limit: LateFinishLimit::Deadline {
                date: ymd("2026-01-05"),
                normalized_date: ymd("2026-01-05"),
            },
        }
    );
}

#[test]
fn explains_a_summary_row_without_drivers() {
    let result = calculate_schedule(&progress_input(
        vec![
            summary("S1"),
            child_task("A", "S1", 480),
            child_task("B", "S1", 480),
        ],
        vec![],
    ))
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "S1"),
        &TaskExplanation::Summary {
            task_id: "S1".into(),
        }
    );
}

#[test]
fn explains_a_lagged_milestone_predecessor() {
    let result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("M", 0)],
        vec![fs("A", "M", 960)],
    ))
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "M"),
        &TaskExplanation::Scheduled {
            task_id: "M".into(),
            primary_driver: predecessor("A", DependencyType::FinishStart, 960),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn finish_finish_arrival_overrides_in_progress_and_reports_no_spurious_gap() {
    // A(2880) finishes Monday 2026-01-12; B(1920) is 50% and started Monday. FF+0
    // ties B's finish to A's finish, so the arrival is B's early-finish civil date
    // (Monday) even though B is in progress. Its remaining work begins Friday, so
    // the buggy remaining-start conversion would have invented a weekend gap.
    let result = calculate_schedule_with_progress(
        &progress_input(
            vec![task("A", 2880), task("B", 1920)],
            vec![ff("A", "B", 0)],
        ),
        &[],
        &status(
            Some("2026-01-05"),
            vec![progress("B", 50, Some("2026-01-05"), None)],
        ),
    )
    .expect("valid schedule");
    match explanation(&result, "B") {
        TaskExplanation::Scheduled {
            primary_driver,
            calendar_gap,
            started_actual_start,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishFinish, 0)
            );
            // Reference (A finish Monday) and arrival (B finish Monday) coincide.
            assert_eq!(calendar_gap, &None);
            assert_eq!(started_actual_start, &Some(at("2026-01-05", "08:00")));
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
    // The reported early finish confirms the FF hand-off lands Monday 2026-01-12.
    assert_eq!(leaf(&result, "B").early_finish, at("2026-01-12", "16:00"));
}

#[test]
fn undisplaced_milestone_reports_no_phantom_gap_under_a_data_date() {
    // A lone milestone floored at a later-Monday data date has a remaining-work
    // start of Monday 2026-01-12, but its event instant is the prior Friday 16:00.
    // The start-anchored dataDate reference measures to the working-day start, so
    // there is no phantom Friday->Monday gap.
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("M", 0)], vec![]),
        &[],
        &status(Some("2026-01-12"), vec![]),
    )
    .expect("valid schedule");
    match explanation(&result, "M") {
        TaskExplanation::Scheduled {
            primary_driver,
            calendar_gap,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &ScheduleDriver::DataDate {
                    date: at("2026-01-12", "08:00"),
                }
            );
            assert_eq!(calendar_gap, &None);
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn undisplaced_milestone_reports_no_phantom_gap_under_start_anchored_predecessors() {
    // A pinned to Monday 2026-01-12 drives a milestone whose event instant is the
    // prior Friday 16:00. Both SS+0 and SF+0 are start-anchored, so the arrival is
    // the milestone's Monday working-day start and no phantom weekend gap appears.
    for link_type in [DependencyType::StartStart, DependencyType::StartFinish] {
        let result = calculate_schedule_with_constraints(
            &progress_input(
                vec![task("A", 480), task("M", 0)],
                vec![dep("A", "M", link_type, 0)],
            ),
            &[constraint("A", Some("2026-01-12"), None)],
        )
        .expect("valid schedule");
        match explanation(&result, "M") {
            TaskExplanation::Scheduled {
                primary_driver,
                calendar_gap,
                ..
            } => {
                assert_eq!(primary_driver, &predecessor("A", link_type, 0));
                assert_eq!(calendar_gap, &None, "unexpected gap for {link_type:?}");
            }
            other => panic!("expected scheduled explanation for {link_type:?}, got {other:?}"),
        }
    }
}

#[test]
fn lagged_finish_finish_hand_off_measures_a_started_successors_weekend_gap() {
    // A is pinned to Friday 2026-01-09; FF+480 pushes B's finish to Monday
    // 2026-01-12. B is 50% and started Monday 2026-01-05, so the finish-anchored FF
    // arrival (B's Monday early finish) against A's Friday finish measures the real
    // weekend gap rather than suppressing it.
    let result = calculate_schedule_with_progress(
        &progress_input(
            vec![task("A", 480), task("B", 960)],
            vec![ff("A", "B", 480)],
        ),
        &[constraint("A", Some("2026-01-09"), None)],
        &status(
            Some("2026-01-05"),
            vec![progress("B", 50, Some("2026-01-05"), None)],
        ),
    )
    .expect("valid schedule");
    match explanation(&result, "B") {
        TaskExplanation::Scheduled {
            primary_driver,
            calendar_gap,
            started_actual_start,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishFinish, 480)
            );
            assert_eq!(
                calendar_gap,
                &Some(CalendarGap {
                    from_date: ymd("2026-01-09"),
                    to_date: ymd("2026-01-12"),
                    non_working_day_count: 2,
                })
            );
            assert_eq!(started_actual_start, &Some(at("2026-01-05", "08:00")));
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
    assert_eq!(leaf(&result, "B").early_finish, at("2026-01-12", "16:00"));
}

#[test]
fn schedule_start_gap_reference_is_the_normalized_first_working_date() {
    // Saturday schedule start: the normalized reference is Monday 2026-01-12, so a
    // lone leaf reports no weekend gap (the entered Saturday would have invented one).
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: ymd("2026-01-10"),
        calendar: standard_calendar(),
        tasks: vec![task("A", 480)],
        dependencies: vec![],
    })
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Scheduled {
            task_id: "A".into(),
            primary_driver: ScheduleDriver::ScheduleStart {},
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
    assert_eq!(result.schedule_start, at("2026-01-12", "08:00"));
}

#[test]
fn data_date_gap_reference_is_the_normalized_data_date() {
    // Saturday entered data date normalizes to Monday 2026-01-12; the reference is
    // the normalized civil date, so there is no weekend gap.
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480)], vec![]),
        &[],
        &status(Some("2026-01-10"), vec![]),
    )
    .expect("valid schedule");
    match explanation(&result, "A") {
        TaskExplanation::Scheduled {
            primary_driver,
            calendar_gap,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &ScheduleDriver::DataDate {
                    date: at("2026-01-12", "08:00"),
                }
            );
            assert_eq!(calendar_gap, &None);
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn milestone_landing_on_its_predecessor_finish_reports_no_gap() {
    // A pinned to Friday drives both a milestone M and a positive-duration B by
    // FS+0. M lands on A's Friday finish (no displacement, no gap); B is displaced
    // to Monday and reports the weekend gap.
    let result = calculate_schedule_with_constraints(
        &progress_input(
            vec![task("A", 480), task("M", 0), task("B", 480)],
            vec![fs("A", "M", 0), fs("A", "B", 0)],
        ),
        &[constraint("A", Some("2026-01-09"), None)],
    )
    .expect("valid schedule");
    match explanation(&result, "M") {
        TaskExplanation::Scheduled {
            primary_driver,
            calendar_gap,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishStart, 0)
            );
            assert_eq!(calendar_gap, &None);
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
    match explanation(&result, "B") {
        TaskExplanation::Scheduled { calendar_gap, .. } => assert_eq!(
            calendar_gap,
            &Some(CalendarGap {
                from_date: ymd("2026-01-09"),
                to_date: ymd("2026-01-12"),
                non_working_day_count: 2,
            })
        ),
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn complete_predecessor_data_date_lift_reports_a_contributing_data_date() {
    // A completes Monday; the Wednesday data date lifts B's FS bound above the raw
    // actual-finish anchor. The lift wins, so dataDate is reported alongside the
    // binding predecessor.
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(
            Some("2026-01-07"),
            vec![progress("A", 100, Some("2026-01-05"), Some("2026-01-05"))],
        ),
    )
    .expect("valid schedule");
    match explanation(&result, "B") {
        TaskExplanation::Scheduled {
            primary_driver,
            other_binding_drivers,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishStart, 0)
            );
            assert_eq!(
                other_binding_drivers,
                &vec![ScheduleDriver::DataDate {
                    date: at("2026-01-07", "08:00"),
                }]
            );
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn late_finish_limit_skips_a_complete_successor() {
    // B is complete, so it can impose no late bound on A; A falls back to the
    // project-finish anchor.
    let result = calculate_schedule_with_progress(
        &progress_input(vec![task("A", 480), task("B", 480)], vec![fs("A", "B", 0)]),
        &[],
        &status(
            Some("2026-01-05"),
            vec![progress("B", 100, Some("2026-01-05"), Some("2026-01-05"))],
        ),
    )
    .expect("valid schedule");
    match explanation(&result, "A") {
        TaskExplanation::Scheduled {
            late_finish_limit, ..
        } => assert_eq!(late_finish_limit, &LateFinishLimit::ProjectFinish {}),
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn late_finish_limit_skips_a_start_anchored_successor_through_a_started_task() {
    // A is in progress with a tight-deadline SS successor B. The SS edge imposes no
    // late bound back through a started task, so A's limit is the project finish;
    // switching the same edge to FS names B instead.
    let tasks = || vec![task("A", 960), task("B", 480)];
    let progress_status = || {
        status(
            Some("2026-01-07"),
            vec![progress("A", 50, Some("2026-01-05"), None)],
        )
    };
    let ss_result = calculate_schedule_with_progress(
        &progress_input(tasks(), vec![ss("A", "B", 0)]),
        &[constraint("B", None, Some("2026-01-06"))],
        &progress_status(),
    )
    .expect("valid SS schedule");
    match explanation(&ss_result, "A") {
        TaskExplanation::Scheduled {
            late_finish_limit, ..
        } => assert_eq!(late_finish_limit, &LateFinishLimit::ProjectFinish {}),
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
    let fs_result = calculate_schedule_with_progress(
        &progress_input(tasks(), vec![fs("A", "B", 0)]),
        &[constraint("B", None, Some("2026-01-06"))],
        &progress_status(),
    )
    .expect("valid FS schedule");
    match explanation(&fs_result, "A") {
        TaskExplanation::Scheduled {
            late_finish_limit, ..
        } => assert_eq!(
            late_finish_limit,
            &LateFinishLimit::Successor {
                task_id: "B".into(),
                dependency_type: DependencyType::FinishStart,
                lag_minutes: 0,
            }
        ),
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn orders_binding_predecessors_lexically_then_by_type() {
    // Two distinct predecessors both finish Monday and bind B; the lexical id
    // order puts A first and C second.
    let distinct = calculate_schedule(&progress_input(
        vec![task("A", 480), task("C", 480), task("B", 480)],
        vec![fs("A", "B", 0), fs("C", "B", 0)],
    ))
    .expect("valid distinct-predecessor schedule");
    match explanation(&distinct, "B") {
        TaskExplanation::Scheduled {
            primary_driver,
            other_binding_drivers,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishStart, 0)
            );
            assert_eq!(
                other_binding_drivers,
                &vec![predecessor("C", DependencyType::FinishStart, 0)]
            );
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }

    // The same predecessor A binds B through both FS+0 and SS+480 (each reaching
    // Tuesday 08:00); declaration order FS then SS breaks the tie.
    let same_pair = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![fs("A", "B", 0), ss("A", "B", 480)],
    ))
    .expect("valid same-pair schedule");
    match explanation(&same_pair, "B") {
        TaskExplanation::Scheduled {
            primary_driver,
            other_binding_drivers,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishStart, 0)
            );
            assert_eq!(
                other_binding_drivers,
                &vec![predecessor("A", DependencyType::StartStart, 480)]
            );
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn reports_only_the_binding_link_of_a_same_pair() {
    // SS+0 lets B start Monday but does not bind its Tuesday start; only the FF+0
    // link binds, so the SS link is not reported.
    let result = calculate_schedule(&progress_input(
        vec![task("A", 960), task("B", 480)],
        vec![ss("A", "B", 0), ff("A", "B", 0)],
    ))
    .expect("valid schedule");
    match explanation(&result, "B") {
        TaskExplanation::Scheduled {
            primary_driver,
            other_binding_drivers,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("A", DependencyType::FinishFinish, 0)
            );
            assert!(other_binding_drivers.is_empty());
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

#[test]
fn late_finish_limit_names_each_successor_link_type() {
    // SS: B(960) driven start-to-start by A(480) bounds A's late finish.
    let ss_result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 960)],
        vec![ss("A", "B", 0)],
    ))
    .expect("valid SS schedule");
    assert_eq!(
        scheduled_limit(&ss_result, "A"),
        LateFinishLimit::Successor {
            task_id: "B".into(),
            dependency_type: DependencyType::StartStart,
            lag_minutes: 0,
        }
    );

    // FF: B(480) finish-to-finish with A(960) bounds A's late finish.
    let ff_result = calculate_schedule(&progress_input(
        vec![task("A", 960), task("B", 480)],
        vec![ff("A", "B", 0)],
    ))
    .expect("valid FF schedule");
    assert_eq!(
        scheduled_limit(&ff_result, "A"),
        LateFinishLimit::Successor {
            task_id: "B".into(),
            dependency_type: DependencyType::FinishFinish,
            lag_minutes: 0,
        }
    );

    // SF: B(480) start-to-finish with A(480) and +960 lag bounds A's late finish.
    let sf_result = calculate_schedule(&progress_input(
        vec![task("A", 480), task("B", 480)],
        vec![sf("A", "B", 960)],
    ))
    .expect("valid SF schedule");
    assert_eq!(
        scheduled_limit(&sf_result, "A"),
        LateFinishLimit::Successor {
            task_id: "B".into(),
            dependency_type: DependencyType::StartFinish,
            lag_minutes: 960,
        }
    );
}

#[test]
fn reversed_negative_lag_gap_reports_ascending_endpoints() {
    // P is pinned to Monday 2026-01-12; Q's FS-960 link pulls its start back to the
    // prior Friday, so the arrival precedes the reference and the gap ascends
    // Friday -> Monday across the weekend.
    let result = calculate_schedule_with_constraints(
        &progress_input(
            vec![task("P", 480), task("Q", 480)],
            vec![fs("P", "Q", -960)],
        ),
        &[constraint("P", Some("2026-01-12"), None)],
    )
    .expect("valid schedule");
    match explanation(&result, "Q") {
        TaskExplanation::Scheduled {
            primary_driver,
            calendar_gap,
            ..
        } => {
            assert_eq!(
                primary_driver,
                &predecessor("P", DependencyType::FinishStart, -960)
            );
            assert_eq!(
                calendar_gap,
                &Some(CalendarGap {
                    from_date: ymd("2026-01-09"),
                    to_date: ymd("2026-01-12"),
                    non_working_day_count: 2,
                })
            );
        }
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

fn scheduled_limit(result: &ScheduleResult, id: &str) -> LateFinishLimit {
    match explanation(result, id) {
        TaskExplanation::Scheduled {
            late_finish_limit, ..
        } => late_finish_limit.clone(),
        other => panic!("expected scheduled explanation, got {other:?}"),
    }
}

// A rich fixture that exercises a summary, a complete leaf, an in-progress leaf,
// SNET/FNLT constraints, a data date, and typed links.
fn rich_explanation_case() -> (Vec<ScheduleTask>, Vec<FinishStartDependency>) {
    (
        vec![
            summary("S"),
            child_task("A", "S", 480),
            child_task("B", "S", 960),
            task("C", 480),
            task("M", 0),
            task("Z", 480),
        ],
        vec![fs("A", "C", 0), fs("B", "C", 120), fs("C", "M", 0)],
    )
}

fn rich_explanation_result(
    tasks: Vec<ScheduleTask>,
    dependencies: Vec<FinishStartDependency>,
) -> ScheduleResult {
    calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: ymd("2026-01-05"),
            calendar: standard_calendar(),
            tasks,
            dependencies,
        },
        &[
            constraint("A", Some("2026-01-05"), None),
            constraint("M", None, Some("2026-01-16")),
        ],
        &status(
            Some("2026-01-06"),
            vec![
                progress("B", 50, Some("2026-01-05"), None),
                progress("Z", 100, Some("2026-01-05"), Some("2026-01-05")),
            ],
        ),
    )
    .expect("valid rich schedule")
}

#[test]
fn explanations_are_byte_stable_and_order_independent() {
    let (tasks, dependencies) = rich_explanation_case();
    let first = rich_explanation_result(tasks.clone(), dependencies.clone());
    let second = rich_explanation_result(tasks.clone(), dependencies.clone());
    let serialize = |result: &ScheduleResult| {
        serde_json::to_string(&result.explanations).expect("serialize explanations")
    };
    // Identical inputs serialize byte-identical explanations.
    assert_eq!(serialize(&first), serialize(&second));

    // Shuffled declaration order of dependencies keeps the task order, so the
    // whole explanation vector stays byte-identical.
    let shuffled_deps = vec![fs("C", "M", 0), fs("B", "C", 120), fs("A", "C", 0)];
    let reordered = rich_explanation_result(tasks.clone(), shuffled_deps);
    assert_eq!(serialize(&first), serialize(&reordered));

    // Shuffled task declaration order reorders the vector but every task's own
    // explanation content is identical.
    let shuffled_tasks = vec![
        task("Z", 480),
        child_task("B", "S", 960),
        summary("S"),
        task("M", 0),
        child_task("A", "S", 480),
        task("C", 480),
    ];
    let (_, dependencies) = rich_explanation_case();
    let task_shuffled = rich_explanation_result(shuffled_tasks, dependencies);
    for original in &first.explanations {
        let id = explanation_task_id(original);
        assert_eq!(original, explanation(&task_shuffled, id));
    }
}

#[test]
fn every_task_has_a_consistent_explanation() {
    let (tasks, dependencies) = rich_explanation_case();
    let result = rich_explanation_result(tasks, dependencies);
    assert_eq!(result.explanations.len(), result.tasks.len());
    for (index, scheduled) in result.tasks.iter().enumerate() {
        let explanation = &result.explanations[index];
        // Order matches `tasks` exactly.
        assert_eq!(explanation_task_id(explanation), scheduled.id);
        // Kind matches the task's status.
        if scheduled.summary {
            assert!(matches!(explanation, TaskExplanation::Summary { .. }));
            continue;
        }
        let complete = scheduled.percent_complete == 100 && scheduled.actual_finish.is_some();
        if complete {
            assert!(matches!(explanation, TaskExplanation::Complete { .. }));
            continue;
        }
        let TaskExplanation::Scheduled {
            primary_driver,
            started_actual_start,
            total_float_minutes,
            critical,
            ..
        } = explanation
        else {
            panic!("expected a scheduled explanation for {}", scheduled.id);
        };
        // Float rationale matches the task flags.
        assert_eq!(*total_float_minutes, scheduled.total_float_minutes);
        assert_eq!(*critical, scheduled.total_float_minutes <= 0);
        // For a not-started leaf the primary driver's bound is the reported early
        // start; predecessor bounds are verified exactly by the fixture tests.
        if started_actual_start.is_none() {
            match primary_driver {
                ScheduleDriver::ScheduleStart {} => {
                    assert_eq!(scheduled.early_start, result.schedule_start)
                }
                ScheduleDriver::StartConstraint {
                    normalized_date, ..
                } => assert_eq!(
                    scheduled.early_start,
                    at(&normalized_date.to_string(), "08:00")
                ),
                ScheduleDriver::DataDate { date } => assert_eq!(scheduled.early_start, *date),
                ScheduleDriver::Predecessor { .. } => {}
            }
        }
    }
}

// A synthetic 1,000-leaf FS chain used to time the explanation post-pass. Run
// with `cargo test --release -- --ignored explanation_chain_timing`.
#[test]
#[ignore]
fn explanation_chain_timing() {
    use std::time::Instant;

    let leaf_count = 1_000;
    let mut tasks = Vec::with_capacity(leaf_count);
    let mut dependencies = Vec::with_capacity(leaf_count - 1);
    for index in 0..leaf_count {
        tasks.push(task(&format!("T{index:04}"), 480));
        if index > 0 {
            dependencies.push(fs(
                &format!("T{:04}", index - 1),
                &format!("T{index:04}"),
                0,
            ));
        }
    }
    let input = ScheduleInput {
        schedule_start: ymd("2026-01-05"),
        calendar: standard_calendar(),
        tasks,
        dependencies,
    };
    let constraints = [
        constraint("T0100", Some("2026-02-02"), None),
        constraint("T0500", None, Some("2030-01-01")),
    ];
    let progress = status(Some("2026-01-05"), vec![]);

    let mut samples = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let result = calculate_schedule_with_progress(&input, &constraints, &progress)
            .expect("valid chain schedule");
        let elapsed = started.elapsed();
        assert_eq!(result.explanations.len(), leaf_count);
        samples.push(elapsed);
    }
    samples.sort();
    println!("median calculate_schedule_with_progress: {:?}", samples[2]);
}

#[test]
fn positive_duration_start_finish_gap_measures_the_hand_off_not_the_duration() {
    // A is pinned to Monday 2026-01-19; SF+0 bounds B's finish at A's start, so
    // six-day B runs 2026-01-09 through Friday 2026-01-16. The SF arrival is the
    // working-day-start alias of B's remaining-work finish (Monday 2026-01-19),
    // which coincides with the reference, so the undisplaced hand-off reports no
    // gap — B's own internal weekend never counts as displacement.
    let result = calculate_schedule_with_constraints(
        &progress_input(
            vec![task("A", 480), task("B", 2880)],
            vec![dep("A", "B", DependencyType::StartFinish, 0)],
        ),
        &[constraint("A", Some("2026-01-19"), None)],
    )
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::StartFinish, 0),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 480,
            critical: false,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
    assert_eq!(
        explanation(&result, "A"),
        &TaskExplanation::Scheduled {
            task_id: "A".into(),
            primary_driver: ScheduleDriver::StartConstraint {
                date: ymd("2026-01-19"),
                normalized_date: ymd("2026-01-19"),
            },
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: None,
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
}

#[test]
fn displaced_start_finish_gap_counts_the_closed_day_in_the_lag() {
    // SF+960 from A's Monday start lands B's finish two working days later, and
    // the closed Tuesday inside the lag displaces the hand-off: B runs Wednesday
    // and the gap reads A's Monday start to the Thursday alias of B's finish,
    // counting exactly the closed Tuesday.
    let result = calculate_schedule(&ScheduleInput {
        schedule_start: ymd("2026-01-05"),
        calendar: calendar_with_exceptions(&["2026-01-06"]),
        tasks: vec![task("A", 480), task("B", 480)],
        dependencies: vec![dep("A", "B", DependencyType::StartFinish, 960)],
    })
    .expect("valid schedule");
    assert_eq!(
        explanation(&result, "B"),
        &TaskExplanation::Scheduled {
            task_id: "B".into(),
            primary_driver: predecessor("A", DependencyType::StartFinish, 960),
            other_binding_drivers: vec![],
            started_actual_start: None,
            calendar_gap: Some(CalendarGap {
                from_date: ymd("2026-01-05"),
                to_date: ymd("2026-01-08"),
                non_working_day_count: 1,
            }),
            total_float_minutes: 0,
            critical: true,
            late_finish_limit: LateFinishLimit::ProjectFinish {},
        }
    );
    match explanation(&result, "A") {
        TaskExplanation::Scheduled {
            late_finish_limit, ..
        } => assert_eq!(
            late_finish_limit,
            &LateFinishLimit::Successor {
                task_id: "B".into(),
                dependency_type: DependencyType::StartFinish,
                lag_minutes: 960,
            }
        ),
        other => panic!("expected scheduled explanation for A, got {other:?}"),
    }
}
