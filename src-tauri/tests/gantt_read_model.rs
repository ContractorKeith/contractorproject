use chrono::{NaiveDate, NaiveDateTime};
use contractorproject_lib::gantt::{
    build_gantt_read_model, GanttBaselineSource, GanttBaselineTaskSource, GanttPredecessorLink,
    GanttPredecessorSource, GanttProgressStatus, GanttReadModelError, GanttReadModelSource,
    GanttTaskKind, GanttTaskSource, GANTT_READ_MODEL_VERSION,
};
use contractorproject_lib::scheduling::{
    calculate_schedule, calculate_schedule_with_constraints, calculate_schedule_with_progress,
    CalendarWeekday, DependencyType, FinishStartDependency, ScheduleInput, ScheduleProgress,
    ScheduleTask, TaskConstraint, TaskProgress, WorkingCalendar,
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
        exceptions: Vec::new(),
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
        calendar: standard_calendar(),
        baseline: None,
        predecessors: vec![],
    })
    .expect("build read model");

    assert_eq!(read_model.contract_version, GANTT_READ_MODEL_VERSION);
    assert_eq!(
        serde_json::to_value(read_model).expect("serialize read model"),
        json!({
            "contractVersion": 6,
            "jobId": "job-1",
            "jobVersion": 7,
            "scheduleStart": "2026-01-05T08:00:00",
            "scheduleFinish": "2026-01-05T08:00:00",
            "dataDate": null,
            "baselineId": null,
            "rowCount": 0,
            "criticalTaskIds": [],
            "criticalPath": [],
            "calendar": {
                "workingWeekdays": ["monday", "tuesday", "wednesday", "thursday", "friday"],
                "exceptionDates": []
            },
            "rows": []
        })
    );
}

#[test]
fn constrained_violating_leaf_serializes_the_exact_v4_contract() {
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
        calendar: standard_calendar(),
        baseline: None,
        predecessors: vec![],
    })
    .expect("build read model");

    assert_eq!(
        serde_json::to_value(read_model).expect("serialize read model"),
        json!({
            "contractVersion": 6,
            "jobId": "job-1",
            "jobVersion": 1,
            "scheduleStart": "2026-01-05T08:00:00",
            "scheduleFinish": "2026-01-06T16:00:00",
            "dataDate": null,
            "baselineId": null,
            "rowCount": 1,
            "criticalTaskIds": ["leaf"],
            "criticalPath": ["leaf"],
            "calendar": {
                "workingWeekdays": ["monday", "tuesday", "wednesday", "thursday", "friday"],
                "exceptionDates": []
            },
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
                "percentComplete": 0,
                "actualStart": null,
                "actualFinish": null,
                "progressStatus": "notStarted",
                "predecessors": [],
                "baseline": null
            }]
        })
    );
}

#[test]
fn statused_schedule_projects_the_v4_progress_facts_and_data_date() {
    let date = |year, month, day| NaiveDate::from_ymd_opt(year, month, day).expect("valid date");
    let schedule = calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: date(2026, 1, 5),
            calendar: standard_calendar(),
            tasks: vec![
                ScheduleTask {
                    id: "phase".into(),
                    parent_task_id: None,
                    duration_minutes: None,
                },
                ScheduleTask {
                    id: "done".into(),
                    parent_task_id: Some("phase".into()),
                    duration_minutes: Some(480),
                },
                ScheduleTask {
                    id: "running".into(),
                    parent_task_id: Some("phase".into()),
                    duration_minutes: Some(960),
                },
                ScheduleTask {
                    id: "waiting".into(),
                    parent_task_id: Some("phase".into()),
                    duration_minutes: Some(480),
                },
            ],
            dependencies: vec![FinishStartDependency {
                predecessor_task_id: "running".into(),
                successor_task_id: "waiting".into(),
                dependency_type: DependencyType::default(),
                lag_minutes: 0,
            }],
        },
        &[],
        &ScheduleProgress {
            data_date: Some(date(2026, 1, 7)),
            entries: vec![
                TaskProgress {
                    task_id: "done".into(),
                    percent_complete: 100,
                    actual_start: Some(date(2026, 1, 5)),
                    actual_finish: Some(date(2026, 1, 5)),
                },
                TaskProgress {
                    task_id: "running".into(),
                    percent_complete: 50,
                    actual_start: Some(date(2026, 1, 6)),
                    actual_finish: None,
                },
            ],
        },
    )
    .expect("calculate statused schedule");

    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 3,
        tasks: vec![
            GanttTaskSource {
                id: "phase".into(),
                parent_task_id: None,
                sort_key: 0,
                name: "Phase".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "done".into(),
                parent_task_id: Some("phase".into()),
                sort_key: 0,
                name: "Done".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "running".into(),
                parent_task_id: Some("phase".into()),
                sort_key: 1,
                name: "Running".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "waiting".into(),
                parent_task_id: Some("phase".into()),
                sort_key: 2,
                name: "Waiting".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
        ],
        schedule,
        calendar: standard_calendar(),
        baseline: None,
        predecessors: vec![GanttPredecessorSource {
            task_id: "waiting".into(),
            predecessors: vec![GanttPredecessorLink {
                task_id: "running".into(),
                dependency_type: DependencyType::FinishStart,
                lag_minutes: 0,
            }],
        }],
    })
    .expect("build statused read model");

    assert_eq!(read_model.contract_version, 6);
    assert_eq!(read_model.data_date, Some(date_time("2026-01-07T08:00:00")));

    let summary = &read_model.rows[0];
    assert_eq!(summary.task_id, "phase");
    // Duration-weighted rollup over 480@100, 960@50, 480@0 minutes.
    assert_eq!(summary.percent_complete, 50);
    assert_eq!(summary.progress_status, GanttProgressStatus::InProgress);
    assert!(summary.actual_start.is_none());
    assert!(summary.actual_finish.is_none());

    let done = &read_model.rows[1];
    assert_eq!(done.percent_complete, 100);
    assert_eq!(done.progress_status, GanttProgressStatus::Completed);
    assert_eq!(done.actual_start, Some(date_time("2026-01-05T08:00:00")));
    assert_eq!(done.actual_finish, Some(date_time("2026-01-05T16:00:00")));

    let running = &read_model.rows[2];
    assert_eq!(running.percent_complete, 50);
    assert_eq!(running.progress_status, GanttProgressStatus::InProgress);
    assert_eq!(running.actual_start, Some(date_time("2026-01-06T08:00:00")));
    assert!(running.actual_finish.is_none());

    let waiting = &read_model.rows[3];
    assert_eq!(waiting.percent_complete, 0);
    assert_eq!(waiting.progress_status, GanttProgressStatus::NotStarted);
    assert!(waiting.actual_start.is_none());
    assert!(waiting.actual_finish.is_none());

    // The completed leaf serializes the exact camel-case v4 progress facts.
    let value = serde_json::to_value(&read_model).expect("serialize statused projection");
    assert_eq!(value["dataDate"], json!("2026-01-07T08:00:00"));
    assert_eq!(value["rows"][1]["percentComplete"], json!(100));
    assert_eq!(
        value["rows"][1]["actualStart"],
        json!("2026-01-05T08:00:00")
    );
    assert_eq!(
        value["rows"][1]["actualFinish"],
        json!("2026-01-05T16:00:00")
    );
    assert_eq!(value["rows"][1]["progressStatus"], json!("completed"));
    assert_eq!(value["rows"][2]["progressStatus"], json!("inProgress"));
    assert_eq!(value["rows"][3]["progressStatus"], json!("notStarted"));
    assert_eq!(value["rows"][2]["actualFinish"], json!(null));
}

#[test]
fn summary_status_reflects_started_descendants_when_percent_floors_to_zero() {
    let date = |year, month, day| NaiveDate::from_ymd_opt(year, month, day).expect("valid date");
    // A summary over two milestones: one complete, one not started. The
    // all-zero-duration rollup floors the summary percent to 0, but a descendant
    // has started, so the derived status must be in progress, not not started.
    let schedule = calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start: date(2026, 1, 5),
            calendar: standard_calendar(),
            tasks: vec![
                ScheduleTask {
                    id: "phase".into(),
                    parent_task_id: None,
                    duration_minutes: None,
                },
                ScheduleTask {
                    id: "done".into(),
                    parent_task_id: Some("phase".into()),
                    duration_minutes: Some(0),
                },
                ScheduleTask {
                    id: "todo".into(),
                    parent_task_id: Some("phase".into()),
                    duration_minutes: Some(0),
                },
            ],
            dependencies: vec![],
        },
        &[],
        &ScheduleProgress {
            data_date: Some(date(2026, 1, 6)),
            entries: vec![TaskProgress {
                task_id: "done".into(),
                percent_complete: 100,
                actual_start: Some(date(2026, 1, 5)),
                actual_finish: Some(date(2026, 1, 5)),
            }],
        },
    )
    .expect("calculate milestone summary schedule");

    let source_task = |id: &str, parent: Option<&str>, sort_key: i64| GanttTaskSource {
        id: id.into(),
        parent_task_id: parent.map(Into::into),
        sort_key,
        name: id.into(),
        start_no_earlier_than: None,
        finish_no_later_than: None,
    };
    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![
            source_task("phase", None, 0),
            source_task("done", Some("phase"), 0),
            source_task("todo", Some("phase"), 1),
        ],
        schedule,
        calendar: standard_calendar(),
        baseline: None,
        predecessors: vec![],
    })
    .expect("build milestone summary read model");

    let summary = &read_model.rows[0];
    assert_eq!(summary.percent_complete, 0);
    assert_eq!(summary.progress_status, GanttProgressStatus::InProgress);
    assert_eq!(
        read_model.rows[1].progress_status,
        GanttProgressStatus::Completed
    );
    assert_eq!(
        read_model.rows[2].progress_status,
        GanttProgressStatus::NotStarted
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
            dependency_type: DependencyType::default(),
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
        calendar: standard_calendar(),
        baseline: Some(GanttBaselineSource {
            id: "baseline-1".into(),
            tasks: vec![GanttBaselineTaskSource {
                task_id: "excavate".into(),
                start: date_time("2026-01-05T08:00:00"),
                finish: date_time("2026-01-05T16:00:00"),
                duration_minutes: 300,
            }],
        }),
        predecessors: vec![GanttPredecessorSource {
            task_id: "excavate".into(),
            predecessors: vec![GanttPredecessorLink {
                task_id: "layout".into(),
                dependency_type: DependencyType::FinishStart,
                lag_minutes: 0,
            }],
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
    assert_eq!(excavate.predecessors.len(), 1);
    assert_eq!(excavate.predecessors[0].task_id, "layout");
    assert_eq!(
        excavate.predecessors[0].dependency_type,
        DependencyType::FinishStart
    );
    assert_eq!(excavate.predecessors[0].lag_minutes, 0);
    // The predecessor link serializes as a typed camel-case object in v5.
    let excavate_value = serde_json::to_value(read_model.rows[2].predecessors.clone())
        .expect("serialize predecessor links");
    assert_eq!(
        excavate_value,
        json!([{ "taskId": "layout", "dependencyType": "FS", "lagMinutes": 0 }])
    );
    let baseline = excavate.baseline.as_ref().expect("baseline comparison");
    assert_eq!(baseline.start_variance_minutes, 1_440);
    assert_eq!(baseline.finish_variance_minutes, 1_440);
    // Current duration (480) minus baseline duration (300) = +180.
    assert_eq!(baseline.duration_variance_minutes, 180);

    // The baseline comparison serializes the exact camel-case v4 facts.
    let value = serde_json::to_value(&read_model).expect("serialize baseline projection");
    assert_eq!(
        value["rows"][2]["baseline"],
        json!({
            "start": "2026-01-05T08:00:00",
            "finish": "2026-01-05T16:00:00",
            "durationMinutes": 300,
            "startVarianceMinutes": 1_440,
            "finishVarianceMinutes": 1_440,
            "durationVarianceMinutes": 180
        })
    );
}

#[test]
fn baseline_omitting_a_later_added_task_reports_a_null_comparison() {
    // A baseline captured before "excavate" existed omits that row. The task
    // still schedules and projects, but its baseline comparison stays null while
    // the covered "layout" row carries its comparison.
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![
            ScheduleTask {
                id: "layout".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
            ScheduleTask {
                id: "excavate".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
        ],
        dependencies: vec![],
    })
    .expect("calculate schedule");
    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![
            GanttTaskSource {
                id: "layout".into(),
                parent_task_id: None,
                sort_key: 0,
                name: "Layout".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
            GanttTaskSource {
                id: "excavate".into(),
                parent_task_id: None,
                sort_key: 1,
                name: "Excavate".into(),
                start_no_earlier_than: None,
                finish_no_later_than: None,
            },
        ],
        schedule,
        calendar: standard_calendar(),
        baseline: Some(GanttBaselineSource {
            id: "baseline-1".into(),
            tasks: vec![GanttBaselineTaskSource {
                task_id: "layout".into(),
                start: date_time("2026-01-05T08:00:00"),
                finish: date_time("2026-01-05T16:00:00"),
                duration_minutes: 480,
            }],
        }),
        predecessors: vec![],
    })
    .expect("build partial-baseline read model");

    assert_eq!(read_model.baseline_id.as_deref(), Some("baseline-1"));
    assert!(read_model.rows[0].baseline.is_some());
    assert!(read_model.rows[1].baseline.is_none());
}

#[test]
fn rejects_a_baseline_row_for_an_unknown_task() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![ScheduleTask {
            id: "leaf".into(),
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
            id: "leaf".into(),
            parent_task_id: None,
            sort_key: 0,
            name: "Leaf".into(),
            start_no_earlier_than: None,
            finish_no_later_than: None,
        }],
        schedule,
        calendar: standard_calendar(),
        baseline: Some(GanttBaselineSource {
            id: "baseline-1".into(),
            tasks: vec![GanttBaselineTaskSource {
                task_id: "ghost".into(),
                start: date_time("2026-01-05T08:00:00"),
                finish: date_time("2026-01-05T16:00:00"),
                duration_minutes: 480,
            }],
        }),
        predecessors: vec![],
    })
    .expect_err("reject unknown baseline task");
    assert_eq!(error.code(), "gantt_baseline_task_unknown");
    assert_eq!(
        error,
        GanttReadModelError::UnknownBaselineTask {
            task_id: "ghost".into()
        }
    );
}

#[test]
fn rejects_a_duplicate_baseline_row() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![ScheduleTask {
            id: "leaf".into(),
            parent_task_id: None,
            duration_minutes: Some(480),
        }],
        dependencies: vec![],
    })
    .expect("calculate schedule");
    let baseline_row = |duration: i64| GanttBaselineTaskSource {
        task_id: "leaf".into(),
        start: date_time("2026-01-05T08:00:00"),
        finish: date_time("2026-01-05T16:00:00"),
        duration_minutes: duration,
    };
    let error = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![GanttTaskSource {
            id: "leaf".into(),
            parent_task_id: None,
            sort_key: 0,
            name: "Leaf".into(),
            start_no_earlier_than: None,
            finish_no_later_than: None,
        }],
        schedule,
        calendar: standard_calendar(),
        baseline: Some(GanttBaselineSource {
            id: "baseline-1".into(),
            tasks: vec![baseline_row(480), baseline_row(300)],
        }),
        predecessors: vec![],
    })
    .expect_err("reject duplicate baseline task");
    assert_eq!(error.code(), "gantt_baseline_task_duplicate");
    assert_eq!(
        error,
        GanttReadModelError::DuplicateBaselineTask {
            task_id: "leaf".into()
        }
    );
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
        calendar: standard_calendar(),
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
        calendar: standard_calendar(),
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
        calendar: standard_calendar(),
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

#[test]
fn typed_predecessor_links_serialize_sorted_by_id_then_type() {
    // Four independent leaves so mixed link types and negative lag do not perturb
    // the schedule; the read model only carries the links through, sorted.
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![
            ScheduleTask {
                id: "a".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
            ScheduleTask {
                id: "b".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
            ScheduleTask {
                id: "c".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
            ScheduleTask {
                id: "target".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
        ],
        dependencies: vec![],
    })
    .expect("calculate schedule");

    let source_task = |id: &str| GanttTaskSource {
        id: id.into(),
        parent_task_id: None,
        sort_key: match id {
            "a" => 0,
            "b" => 1,
            "c" => 2,
            _ => 3,
        },
        name: id.into(),
        start_no_earlier_than: None,
        finish_no_later_than: None,
    };

    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![
            source_task("a"),
            source_task("b"),
            source_task("c"),
            source_task("target"),
        ],
        schedule,
        calendar: standard_calendar(),
        baseline: None,
        // Supplied out of order and with a duplicate pair carrying two types.
        predecessors: vec![GanttPredecessorSource {
            task_id: "target".into(),
            predecessors: vec![
                GanttPredecessorLink {
                    task_id: "c".into(),
                    dependency_type: DependencyType::StartFinish,
                    lag_minutes: -60,
                },
                GanttPredecessorLink {
                    task_id: "a".into(),
                    dependency_type: DependencyType::FinishFinish,
                    lag_minutes: 0,
                },
                GanttPredecessorLink {
                    task_id: "a".into(),
                    dependency_type: DependencyType::StartStart,
                    lag_minutes: 120,
                },
            ],
        }],
    })
    .expect("build read model");

    let target = read_model
        .rows
        .iter()
        .find(|row| row.task_id == "target")
        .expect("target row");
    // Sorted by predecessor id, then dependency type declaration order (FS,SS,FF,SF).
    let value = serde_json::to_value(target.predecessors.clone()).expect("serialize links");
    assert_eq!(
        value,
        json!([
            { "taskId": "a", "dependencyType": "SS", "lagMinutes": 120 },
            { "taskId": "a", "dependencyType": "FF", "lagMinutes": 0 },
            { "taskId": "c", "dependencyType": "SF", "lagMinutes": -60 }
        ])
    );
}

#[test]
fn duplicate_typed_predecessor_link_is_rejected() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![
            ScheduleTask {
                id: "a".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
            ScheduleTask {
                id: "b".into(),
                parent_task_id: None,
                duration_minutes: Some(480),
            },
        ],
        dependencies: vec![],
    })
    .expect("calculate schedule");

    let task = |id: &str, sort_key: i64| GanttTaskSource {
        id: id.into(),
        parent_task_id: None,
        sort_key,
        name: id.into(),
        start_no_earlier_than: None,
        finish_no_later_than: None,
    };

    let error = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![task("a", 0), task("b", 1)],
        schedule,
        calendar: standard_calendar(),
        baseline: None,
        predecessors: vec![GanttPredecessorSource {
            task_id: "b".into(),
            predecessors: vec![
                GanttPredecessorLink {
                    task_id: "a".into(),
                    dependency_type: DependencyType::StartStart,
                    lag_minutes: 0,
                },
                GanttPredecessorLink {
                    task_id: "a".into(),
                    dependency_type: DependencyType::StartStart,
                    lag_minutes: 30,
                },
            ],
        }],
    })
    .expect_err("duplicate typed link rejected");
    assert_eq!(error.code(), "gantt_predecessor_duplicate");
    // A different type on the same pair remains legal.
    matches!(error, GanttReadModelError::DuplicatePredecessor { .. });
}

#[test]
fn calendar_facts_serialize_sorted_and_deduplicated() {
    let schedule = calculate_schedule(&ScheduleInput {
        schedule_start: NaiveDate::from_ymd_opt(2026, 11, 23).expect("valid date"),
        calendar: standard_calendar(),
        tasks: vec![],
        dependencies: vec![],
    })
    .expect("calculate empty schedule");

    // Working weekdays supplied out of canonical order and with a duplicate; the
    // read model canonicalizes them Monday-to-Sunday. Exceptions span a
    // Thursday-Friday-Monday closure around the intervening weekend and arrive
    // unsorted with a duplicate to prove the facts sort and de-duplicate.
    let calendar = WorkingCalendar {
        working_weekdays: vec![
            CalendarWeekday::Friday,
            CalendarWeekday::Monday,
            CalendarWeekday::Monday,
            CalendarWeekday::Wednesday,
            CalendarWeekday::Tuesday,
            CalendarWeekday::Thursday,
        ],
        workday_start_minute: 8 * 60,
        workday_duration_minutes: 8 * 60,
        exceptions: vec![
            NaiveDate::from_ymd_opt(2026, 11, 30).expect("valid date"),
            NaiveDate::from_ymd_opt(2026, 11, 26).expect("valid date"),
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("valid date"),
            NaiveDate::from_ymd_opt(2026, 11, 26).expect("valid date"),
        ],
    };

    let read_model = build_gantt_read_model(GanttReadModelSource {
        job_id: "job-1".into(),
        job_version: 1,
        tasks: vec![],
        schedule,
        calendar,
        baseline: None,
        predecessors: vec![],
    })
    .expect("build read model");

    assert_eq!(read_model.contract_version, 6);
    let value = serde_json::to_value(&read_model).expect("serialize read model");
    assert_eq!(
        value["calendar"],
        json!({
            "workingWeekdays": ["monday", "tuesday", "wednesday", "thursday", "friday"],
            "exceptionDates": ["2026-11-26", "2026-11-27", "2026-11-30"]
        })
    );
}
