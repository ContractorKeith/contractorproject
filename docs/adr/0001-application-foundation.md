# ADR 0001: Application foundation

Status: proposed pending macOS and Windows CI
Date: 2026-08-14

## Context

ContractorProject needs a desktop-first, offline application that can support a schedule-heavy interface, deterministic business rules, local SQLite data, and a documented local agent interface. The first implementation spike had to prove the smallest path from the desktop UI through the application core to durable storage.

## Decision

Use Tauri 2 as the desktop shell, React and TypeScript with Vite for the UI, Rust for the application interface and domain logic, and SQLite through `rusqlite` for persistence.

Both the Tauri command adapter and the future MCP adapter will call the same Rust application interface. Neither the React UI nor agent tools may access SQLite directly.

The stack decision becomes accepted when the shell compiles and its tests pass in macOS and Windows CI.

## Evidence

- The initial React job workspace compiles and its create-job workflow is covered at the UI seam.
- The Rust application interface creates a job, persists it in SQLite, and reads it after the database is reopened.
- The Tauri shell compiles locally on macOS.
- The repository quality workflow exercises the native shell on macOS and Windows.

## Consequences

- Schedule and validation rules remain testable without a webview or a running desktop shell.
- SQLite schema changes are owned by the Rust storage adapter and applied as forward migrations.
- UI work stays flexible enough for the Gantt spike without making a scheduler component the domain owner.
- Tauri and Rust introduce a second toolchain, justified by native packaging and one shared local application core.
- The Gantt rendering and dependency-license decisions remain separate spikes.
