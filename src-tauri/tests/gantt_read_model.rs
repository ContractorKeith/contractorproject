use chrono::{NaiveDate, NaiveDateTime};
use contractorproject_lib::gantt::{
    build_gantt_read_model, GanttBaselineSource, GanttBaselineTaskSource, GanttPredecessorSource,
    GanttReadModelError, GanttReadModelSource, GanttTaskKind, GanttTaskSource,
    GANTT_READ_MODEL_VERSION,
};
use contractorproject_lib::scheduling::{
    calculate_schedule, calculate_schedule_with_constraints, CalendarWeekday,
    FinishStartDependency, ScheduleInput, ScheduleTask, TaskConstraint, WorkingCalendar,
};
use serde_json::json;

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

fn date_time(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").expect("valid date time")
}

#[test]
fn empty_schedule_serializes_as_a_versioned_job_projection() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![],
        dependencies: vec![],
    })
    .expect("calculate empty schedule");

    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 7,
        tasks: vec![],
        schedule,
        baseline: None,
        predecessors: vec![],
    })
    .expect("build read model");

    assert_eq!(read_model.contract_version, GANTT_READ_MODEL_VERSION);
    assert_eq!(
        serde_json::to_value(read_model).expect("serialize read model"),
        json!({
            "contractVersion": 2,
            "jobId": "job-1",
            "jobVersion": 7,
            "scheduleStart": "2026-01-05T08:00:00",
            "scheduleFinish": "2026-01-05T08:00:00",
            "baselineId": null,
            "rowCount": 0,
            "criticalTaskIds": [],
            "criticalPath": [],
            "rows": []
        })
    );
}

#[test]
fn constrained_violating_leaf_serializes_the_exact_v2_contract() {
    let schedule = calculate_schedule_with_constraints(
        &ScheduleInput {
            schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
            calendar: standard_calendar(),
            tasks: vec![ScheduleTask {
                id: "leaf".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            }],
            dependencies: vec![],
        },
        &[TaskConstraint {
            task_id: "leaf".into(),
            start_no_earlier_than: Some(NaiveDate::from_ymd_opt(2026, 1, 6).expect("date")),
            finish_no_later_than: Some(NaiveDate::from_ymd_opt(2026, 1, 5).expect("date")),
        }],
    )
    .expect("calculate constrained schedule");
    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![GanttTaskSource {
            id: "leaf".into(),
            parent_task_id: None,
            sort_key: 0,
            name: "Constrained leaf".into(),
            start_no_earlier_than: Some(NaiveDate::from_ymd_opt(2026, 1, 6).expect("date")),
            finish_no_later_than: Some(NaiveDate::from_ymd_opt(2026, 1, 5).expect("date")),
        }],
        schedule,
        baseline: None,
        predecessors: vec![],
    })
    .expect("build read model");

    assert_eq!(
        serde_json::to_value(read_model).expect("serialize read model"),
        json!({
            "contractVersion": 2,
            "jobId": "job-1",
            "jobVersion": 1,
            "scheduleStart": "2026-01-05T08:00:00",
            "scheduleFinish": "2026-01-06T16:00:00",
            "baselineId": null,
            "rowCount": 1,
            "criticalTaskIds": ["leaf"],
            "criticalPath": ["leaf"],
            "rows": [{
                "taskId": "leaf",
                "parentTaskId": null,
                "logicalIndex": 0,
                "depth": 0,
                "positionInSet": 1,
                "setSize": 1,
                "sortKey": 0,
                "wbs": "1",
                "name": "Constrained leaf",
                "kind": "task",
                "hasChildren": false,
                "durationMinutes": 480,
                "start": "2026-01-06T08:00:00",
                "finish": "2026-01-06T16:00:00",
                "totalFloatMinutes": -480,
                "startNoEarlierThan": "2026-01-06",
                "finishNoLaterThan": "2026-01-05",
                "constraintViolated": true,
                "critical": true,
                "milestone": false,
                "summary": false,
                "predecessorIds": [],
                "baseline": null
            }]
        })
    );
}

#[test]
fn nested_schedule_exposes_stable_hierarchy_schedule_baseline_and_predecessors() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![
            ScheduleTask {
                id: "summary".into(),
                parent_task_id: None,
                duration_minutes: None,
            },
            ScheduleTask {
                id: "layout".into(),
                parent_task_id: Some("summary".into()),
                duration_minutes: Some(480),
            },
            ScheduleTask {
                id: "excavate".into(),
                parent_task_id: Some("summary".into()),
                duration_minutes: Some(480),
            },
        ],
        dependencies: vec![FinishStartDependency {
            predecessor_task_id: "layout".into(),
            successor_task_id: "excavate".into(),
            lag_minutes: 0,
        }],
    })
    .expect("calculate nested schedule");

    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 12,
        tasks: vec![
            GanttTaskSource {
                id: "summary".into(),
                parent_task_id: None,
                sort_key: 0,
                name: "Site work".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "layout".into(),
                parent_task_id: Some("summary".into()),
                sort_key: 0,
                name: "Layout".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "excavate".into(),
                parent_task_id: Some("summary".into()),
                sort_key: 1,
                name: "Excavate".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
        ],
        schedule,
        baseline: Some(GanttBaselineSource {
            id: "baseline-1".into(),
            tasks: vec![GanttBaselineTaskSource {
                task_id: "excavate".into(),
                start: date_time("2026-01-05T08:00:00"),
                finish: date_time("2026-01-05T16:00:00"),
                duration_minutes: 480,
            }],
        }),
        predecessors: vec![GanttPredecessorSource {
            task_id: "excavate".into(),
            predecessor_ids: vec!["layout".into()],
        }],
    })
    .expect("build read model");

    assert_eq!(read_model.row_count, 3);
    assert_eq!(read_model.baseline_id.as_deref(), Some("baseline-1"));
    assert_eq!(
        read_model.critical_task_ids,
        vec!["excavate", "layout", "summary"]
    );
    assert_eq!(read_model.critical_path, vec!["layout", "excavate"]);

    let summary = &read_model.rows[0];
    assert_eq!(summary.task_id, "summary");
    assert_eq!(summary.logical_index, 0);
    assert_eq!(summary.depth, 0);
    assert_eq!(summary.position_in_set, 1);
    assert_eq!(summary.set_size, 1);
    assert_eq!(summary.wbs, "1");
    assert_eq!(summary.kind, GanttTaskKind::Summary);
    assert!(summary.has_children);
    assert_eq!(summary.start, date_time("2026-01-05T08:00:00"));
    assert_eq!(summary.finish, date_time("2026-01-06T16:00:00"));
    assert_eq!(summary.duration_minutes, 960);
    assert!(summary.critical);
    assert!(summary.baseline.is_none());

    let excavate = &read_model.rows[2];
    assert_eq!(excavate.parent_task_id.as_deref(), Some("summary"));
    assert_eq!(excavate.logical_index, 2);
    assert_eq!(excavate.depth, 1);
    assert_eq!(excavate.position_in_set, 2);
    assert_eq!(excavate.set_size, 2);
    assert_eq!(excavate.wbs, "1.2");
    assert_eq!(excavate.kind, GanttTaskKind::Task);
    assert!(!excavate.has_children);
    assert_eq!(excavate.predecessor_ids, vec!["layout"]);
    let baseline = excavate.baseline.as_ref().expect("baseline comparison");
    assert_eq!(baseline.start_variance_minutes, 1_440);
    assert_eq!(baseline.finish_variance_minutes, 1_440);
}

#[test]
fn rejects_metadata_that_cannot_join_the_authoritative_schedule() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![ScheduleTask {
            id: "scheduled-task".into(),
            parent_task_id: None,
            duration_minutes: Some(480),
        }],
        dependencies: vec![],
    })
    .expect("calculate schedule");

    let error = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![GanttTaskSource {
            id: "different-task".into(),
            parent_task_id: None,
            sort_key: 0,
            name: "Different task".into(),
            start_no_earlier_than: None,
            finish_no_later_than: None,
        }],
        schedule,
        baseline: None,
        predecessors: vec![],
    })
    .expect_err("reject a mismatched join");

    assert_eq!(error.code(), "gantt_schedule_join_mismatch");
    assert_eq!(
        error,
        GanttReadModelError::ScheduleJoinMismatch {
            task_id: "different-task".into()
        }
    );
}

#[test]
fn rejects_constraints_on_a_scheduled_summary_with_a_stable_code() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![
            ScheduleTask {
                id: "summary".into(),
                parent_task_id: None,
                duration_minutes: None,
            },
            ScheduleTask {
                id: "leaf".into(),
                parent_task_id: Some("summary".into()),
                duration_minutes: Some(480),
            },
        ],
        dependencies: vec![],
    })
    .expect("calculate schedule");
    let error = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![
            GanttTaskSource {
                id: "summary".into(),
                parent_task_id: None,
                sort_key: 0,
                name: "Summary".into(),
                start_no_earlier_than: Some(NaiveDate::from_ymd_opt(2026, 1, 5).expect("date")),
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "leaf".into(),
                parent_task_id: Some("summary".into()),
                sort_key: 0,
                name: "Leaf".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
        ],
        schedule,
        baseline: None,
        predecessors: vec![],
    })
    .expect_err("reject constrained summary");
    assert_eq!(error.code(), "gantt_summary_constraint_invalid");
}

#[test]
fn one_thousand_rows_keep_stable_logical_and_sibling_metadata() {
    let mut tasks = Vec::with_capacity(1_000);
    let mut schedule_tasks = Vec::with_capacity(1_000);
    for phase in 1..=10 {
        let phase_id = format!("phase-{phase}");
        tasks.push(GanttTaskSource {
            id: phase_id.clone(),
            parent_task_id: None,
            sort_key: phase - 1,
            name: format!("Phase {phase}"),
            start_no_earlier_than: None,
            finish_no_later_than: None,
        });
        schedule_tasks.push(ScheduleTask {
            id: phase_id.clone(),
            parent_task_id: None,
            duration_minutes: None,
        });
        for package in 1..=9 {
            let package_id = format!("{phase_id}-package-{package}");
            tasks.push(GanttTaskSource {
                id: package_id.clone(),
                parent_task_id: Some(phase_id.clone()),
                sort_key: package - 1,
                name: format!("Package {package}"),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            });
            schedule_tasks.push(ScheduleTask {
                id: package_id.clone(),
                parent_task_id: Some(phase_id.clone()),
                duration_minutes: None,
            });
            for leaf in 1..=10 {
                let task_id = format!("{package_id}-task-{leaf}");
                tasks.push(GanttTaskSource {
                    id: task_id.clone(),
                    parent_task_id: Some(package_id.clone()),
                    sort_key: leaf - 1,
                    name: format!("Task {leaf}"),
                    start_no_earlier_than: None,
                    finish_no_later_than: None,
                });
                schedule_tasks.push(ScheduleTask {
                    id: task_id,
                    parent_task_id: Some(package_id.clone()),
                    duration_minutes: Some(60),
                });
            }
        }
    }
    assert_eq!(tasks.len(), 1_000);

    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: schedule_tasks,
        dependencies: vec![],
    })
    .expect("calculate large schedule");
    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-large".into(),
        job_version: 1_001,
        tasks,
        schedule,
        baseline: None,
        predecessors: vec![],
    })
    .expect("build large read model");

    assert_eq!(read_model.row_count, 1_000);
    assert_eq!(read_model.rows[0].wbs, "1");
    assert_eq!(read_model.rows[0].set_size, 10);
    assert_eq!(read_model.rows[1].wbs, "1.1");
    assert_eq!(read_model.rows[1].set_size, 9);
    assert_eq!(read_model.rows[2].wbs, "1.1.1");
    assert_eq!(read_model.rows[2].set_size, 10);
    assert_eq!(read_model.rows[999].logical_index, 999);
    assert_eq!(read_model.rows[999].wbs, "10.9.10");
    assert_eq!(read_model.rows[999].position_in_set, 10);
    assert_eq!(
        serde_json::to_value(&read_model).expect("serialize large projection")["rows"]
            .as_array()
            .expect("serialized rows")
            .len(),
        1_000
    );
}
