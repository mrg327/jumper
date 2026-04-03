mod app;
mod cli;
mod events;
mod keyhints;
mod modals;
mod plugins;
mod screens;
mod text_utils;
mod theme;
mod widgets;

use std::path::PathBuf;
use std::process;

use chrono::Local;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use regex::Regex;

use jm_core::config::{ensure_dirs, expand_tilde, Config};
use jm_core::export::{dump_to_stdout_with_issues, export_to_file};
use jm_core::models::{Blocker, JournalEntry, LogEntry};
use jm_core::storage::{ActiveProjectStore, InboxStore, IssueStore, JournalStore, PeopleStore, ProjectStore};
use jm_core::time as jm_time;

use cli::{Cli, Commands, JiraCommands};

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    // Handle --dump
    if cli.dump {
        let config = Config::load();
        let data_dir = ensure_dirs(&config);
        let (ps, js, pps, as_) = jm_core::storage::store::create_stores(&data_dir);
        let is = IssueStore::new(&data_dir);

        if let Some(output) = cli.output {
            match export_to_file(&ps, &js, &pps, &as_, Some(&PathBuf::from(&output))) {
                Ok(path) => println!("Exported to {}", path.display()),
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        } else {
            dump_to_stdout_with_issues(&ps, &js, &pps, &as_, &is);
        }
        return;
    }

    // Handle subcommands
    match cli.command {
        Some(Commands::Note { text }) => cmd_note(&text.join(" ")),
        Some(Commands::Block { text }) => cmd_block(&text.join(" ")),
        Some(Commands::Switch { project_name, no_capture }) => cmd_switch(&project_name, no_capture),
        Some(Commands::Status) => cmd_status(),
        Some(Commands::Work { project_name }) => cmd_work(project_name.as_deref()),
        Some(Commands::Break { r#type }) => cmd_break(&r#type),
        Some(Commands::Done { reflect, tomorrow }) => {
            cmd_done(reflect.as_deref(), tomorrow.as_deref());
        }
        Some(Commands::Add {
            name,
            status,
            priority,
            tags,
        }) => {
            let tag_list: Vec<String> = tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            cmd_add(&name, &status, &priority, tag_list);
        }
        Some(Commands::List { status }) => cmd_list(status.as_deref()),
        Some(Commands::Time { date }) => cmd_time(date.as_deref()),
        Some(Commands::Standup { date }) => cmd_standup(date.as_deref()),
        Some(Commands::Inbox { text }) => cmd_inbox(&text.join(" ")),
        Some(Commands::Refs { slug }) => cmd_refs(&slug),
        Some(Commands::SetStatus {
            project_slug,
            status,
        }) => cmd_set_status(&project_slug, &status),
        Some(Commands::SetPriority {
            project_slug,
            priority,
        }) => cmd_set_priority(&project_slug, &priority),
        Some(Commands::Issue {
            title,
            project,
            parent,
        }) => cmd_issue(&title.join(" "), project.as_deref(), parent),
        Some(Commands::Issues {
            project,
            status,
            all,
        }) => cmd_issues(project.as_deref(), status.as_deref(), all),
        Some(Commands::IssueStatus {
            project_slug,
            issue_id,
            status,
        }) => cmd_issue_status(&project_slug, issue_id, &status),
        Some(Commands::Jira { command }) => match command {
            JiraCommands::Config { test } => cmd_jira_config(test),
        },
        None => {
            // Launch TUI
            let config = Config::load();
            let data_dir = ensure_dirs(&config);

            // Terminal setup with panic hook
            let mut terminal = match setup_terminal() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to set up terminal: {e}");
                    process::exit(1);
                }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut app = app::App::new(config, data_dir);
                app.run(&mut terminal)
            }));
            if let Err(e) = restore_terminal(&mut terminal) {
                eprintln!("Failed to restore terminal: {e}");
            }

            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
                Err(_) => {
                    eprintln!("Panic occurred, terminal restored.");
                    process::exit(1);
                }
            }
        }
    }
}

fn stores() -> (ProjectStore, JournalStore, PeopleStore, ActiveProjectStore) {
    let config = Config::load();
    let data_dir = ensure_dirs(&config);
    jm_core::storage::store::create_stores(&data_dir)
}

fn now_time() -> String {
    Local::now().format("%H:%M").to_string()
}

/// Track all @mentions in text, associating them with a project in the people store.
fn track_mentions(pps: &PeopleStore, text: &str, project_name: &str) {
    let re = Regex::new(r"@([\w-]+)").unwrap();
    for caps in re.captures_iter(text) {
        let handle = format!("@{}", &caps[1]);
        let person = jm_core::models::Person {
            handle,
            role: String::new(),
            projects: vec![project_name.to_string()],
            pending: Vec::new(),
        };
        let _ = pps.add_or_update_person(person);
    }
}

fn cmd_note(text: &str) {
    let (ps, js, pps, as_) = stores();

    let slug = match as_.get_active() {
        Some(s) => s,
        None => {
            eprintln!("No active project. Run: jm work <project-slug>");
            process::exit(1);
        }
    };

    let mut project = match ps.get_project(&slug) {
        Some(p) => p,
        None => {
            eprintln!("Project '{slug}' not found.");
            process::exit(1);
        }
    };

    // Add to project log
    let today = Local::now().date_naive();
    let today_log = project.log.iter_mut().find(|e| e.date == today);
    match today_log {
        Some(entry) => entry.lines.push(text.to_string()),
        None => {
            project.log.insert(
                0,
                LogEntry {
                    date: today,
                    lines: vec![text.to_string()],
                },
            );
        }
    }
    ps.save_project(&mut project).unwrap();

    // Track @mentions
    track_mentions(&pps, text, &project.name);

    // Add to journal
    let mut entry = JournalEntry::new(&now_time(), "Note", &project.name);
    entry
        .details
        .insert("note".to_string(), text.to_string());
    js.append(entry).unwrap();

    println!("Note added to {}: {text}", project.name);
}

fn cmd_block(text: &str) {
    let (ps, js, pps, as_) = stores();

    let slug = match as_.get_active() {
        Some(s) => s,
        None => {
            eprintln!("No active project. Run: jm work <project-slug>");
            process::exit(1);
        }
    };

    let mut project = match ps.get_project(&slug) {
        Some(p) => p,
        None => {
            eprintln!("Project '{slug}' not found.");
            process::exit(1);
        }
    };

    // Extract @mention
    let re = Regex::new(r"@([\w-]+)").unwrap();
    let person = re
        .captures(text)
        .map(|caps| format!("@{}", &caps[1]));

    project.blockers.push(Blocker {
        description: text.to_string(),
        person,
        since: Some(Local::now().date_naive()),
        ..Default::default()
    });
    ps.save_project(&mut project).unwrap();

    // Track @mentions
    track_mentions(&pps, text, &project.name);

    // Journal
    let mut entry = JournalEntry::new(&now_time(), "Note", &project.name);
    entry
        .details
        .insert("blocker".to_string(), text.to_string());
    js.append(entry).unwrap();

    println!("Blocker logged on {}: {text}", project.name);
}

fn cmd_switch(slug: &str, no_capture: bool) {
    use std::io::{self, BufRead, Write};

    let (ps, js, pps, as_) = stores();

    let project = match ps.get_project(slug) {
        Some(p) => p,
        None => {
            eprintln!("Project '{slug}' not found.");
            process::exit(1);
        }
    };

    let old_slug = as_.get_active();
    let old_project = old_slug
        .as_deref()
        .and_then(|s| ps.get_project(s));

    // Show current context before prompting.
    if let Some(ref old) = old_project {
        println!("Current: {}", old.name);
        if !old.current_focus.is_empty() {
            println!("  Focus: {}", old.current_focus);
        }
        println!();
    }

    // Context-capture prompts (skipped when --no-capture or no active project).
    let mut left_off = String::new();
    let blocker = String::new();
    let mut next_step = String::new();

    if !no_capture && old_project.is_some() {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        print!("Capture notes before switching? [y/N]: ");
        stdout.flush().unwrap();
        let mut answer = String::new();
        stdin.lock().read_line(&mut answer).unwrap();

        if answer.trim().eq_ignore_ascii_case("y") {
            print!("Where did you leave off? (Enter to skip): ");
            stdout.flush().unwrap();
            let mut buf = String::new();
            stdin.lock().read_line(&mut buf).unwrap();
            left_off = buf.trim().to_string();

            print!("Next step? (Enter to skip): ");
            stdout.flush().unwrap();
            let mut buf = String::new();
            stdin.lock().read_line(&mut buf).unwrap();
            next_step = buf.trim().to_string();
        }
    }

    let time = now_time();

    // Save captured context to the old project.
    if let Some(mut old) = old_project {
        let switch_label = format!("{} \u{2192} {}", old.name, project.name);
        let mut entry = JournalEntry::new(&time, "Switched", &switch_label);
        if !left_off.is_empty() {
            entry.details.insert("left_off".to_string(), left_off.clone());
        }
        if !blocker.is_empty() {
            entry.details.insert("blocker".to_string(), blocker.clone());
        }
        if !next_step.is_empty() {
            entry.details.insert("next_step".to_string(), next_step.clone());
        }
        js.append(entry).unwrap();

        // Persist next_step as current focus.
        if !next_step.is_empty() {
            old.current_focus = next_step.clone();
            ps.save_project(&mut old).unwrap();
        }

        // Track @mentions from captured context.
        track_mentions(&pps, &left_off, &old.name);
        track_mentions(&pps, &blocker, &old.name);
        track_mentions(&pps, &next_step, &old.name);
    } else if old_slug.is_some() {
        // Active slug set but project file missing — still log the switch.
        let entry = JournalEntry::new(&time, "Switched", &format!("? \u{2192} {}", project.name));
        js.append(entry).unwrap();
    }

    // Perform the switch.
    as_.set_active(slug).unwrap();

    let mut started = JournalEntry::new(&time, "Started", &project.name);
    if !project.current_focus.is_empty() {
        started.details.insert("focus".to_string(), project.current_focus.clone());
        track_mentions(&pps, &project.current_focus, &project.name);
    }
    js.append(started).unwrap();

    println!("Switched to: {}", project.name);
    if !project.current_focus.is_empty() {
        println!("  Resume: {}", project.current_focus);
    }
}

fn cmd_status() {
    let (ps, js, _, as_) = stores();

    let active_slug = match as_.get_active() {
        Some(s) => s,
        None => {
            println!("No active project");
            return;
        }
    };

    let project = match ps.get_project(&active_slug) {
        Some(p) => p,
        None => {
            println!("Active: {active_slug} (project file missing)");
            return;
        }
    };

    let open_blockers = project.blockers.iter().filter(|b| !b.resolved).count();

    let journal = js.today();
    let mut started = String::new();
    for entry in journal.entries.iter().rev() {
        if (entry.entry_type == "Started" || entry.entry_type == "Switched")
            && entry.project.contains(&project.name)
        {
            started = format!(" (since {})", entry.time);
            break;
        }
    }

    let blocker_str = if open_blockers > 0 {
        format!(", {open_blockers} blockers")
    } else {
        String::new()
    };
    let focus_str = if !project.current_focus.is_empty() {
        format!(" -- {}", project.current_focus)
    } else {
        String::new()
    };

    println!("Active: {}{started}{blocker_str}{focus_str}", project.name);
}

fn cmd_work(slug: Option<&str>) {
    let (ps, js, pps, as_) = stores();

    let slug = match slug {
        Some(s) => s,
        None => {
            eprintln!("Usage: jm work <project-slug>");
            process::exit(1);
        }
    };

    let project = match ps.get_project(slug) {
        Some(p) => p,
        None => {
            eprintln!("Project '{slug}' not found.");
            process::exit(1);
        }
    };

    as_.set_active(slug).unwrap();
    let mut entry = JournalEntry::new(&now_time(), "Started", &project.name);
    if !project.current_focus.is_empty() {
        entry
            .details
            .insert("focus".to_string(), project.current_focus.clone());
        track_mentions(&pps, &project.current_focus, &project.name);
    }
    js.append(entry).unwrap();

    println!("Now working on: {}", project.name);
}

fn cmd_break(break_type: &str) {
    let (ps, js, _, as_) = stores();
    let time = now_time();

    let entry_type = match break_type {
        "15min" => "Break",
        "lunch" => "Lunch",
        "eod" => "Done",
        _ => "Done",
    };
    let label = match break_type {
        "15min" => "15 min break",
        "lunch" => "Out to lunch",
        _ => "Done for day",
    };

    let project_name = as_
        .get_active()
        .and_then(|slug| ps.get_project(&slug).map(|p| p.name.clone()).or(Some(slug)))
        .unwrap_or_default();

    let mut entry = JournalEntry::new(&time, entry_type, &project_name);
    entry
        .details
        .insert("break".to_string(), label.to_string());
    js.append(entry).unwrap();

    if break_type == "eod" {
        as_.clear_active();
    }

    println!("{label}");
}

fn cmd_done(reflect: Option<&str>, tomorrow: Option<&str>) {
    let (ps, js, _, as_) = stores();
    let time = now_time();

    // Print active project summary first (informational, scriptable).
    let active_info = as_.get_active().and_then(|slug| {
        ps.get_project(&slug).map(|p| {
            let open_blockers = p.blockers.iter().filter(|b| !b.resolved).count();
            let focus_str = if p.current_focus.is_empty() {
                String::new()
            } else {
                format!(" | focus: {}", p.current_focus)
            };
            let blocker_str = if open_blockers > 0 {
                format!(" | {} open blockers", open_blockers)
            } else {
                String::new()
            };
            (p.name.clone(), format!("{}{blocker_str}{focus_str}", p.name))
        })
    });

    if let Some((ref _name, ref summary)) = active_info {
        println!("Wrapping up: {summary}");
    }

    // Log EOD reflection if provided.
    let project_name = as_
        .get_active()
        .and_then(|slug| ps.get_project(&slug).map(|p| p.name.clone()).or(Some(slug)))
        .unwrap_or_default();

    if reflect.is_some() || tomorrow.is_some() {
        let mut refl_entry = JournalEntry::new(&time, "Reflection", &project_name);
        if let Some(r) = reflect {
            if !r.is_empty() {
                refl_entry.details.insert("shipped".to_string(), r.to_string());
            }
        }
        if let Some(t) = tomorrow {
            if !t.is_empty() {
                refl_entry.details.insert("tomorrow".to_string(), t.to_string());
            }
        }
        js.append(refl_entry).unwrap();
        println!("Reflection logged.");
    }

    // Log Done entry and clear active.
    let mut done_entry = JournalEntry::new(&time, "Done", &project_name);
    done_entry
        .details
        .insert("break".to_string(), "Done for day".to_string());
    js.append(done_entry).unwrap();
    as_.clear_active();

    println!("Done for day.");
}

fn cmd_add(name: &str, status: &str, priority: &str, tags: Vec<String>) {
    let (ps, _, _, _) = stores();

    let project = ps
        .create_project_with(name, status, priority, tags)
        .unwrap();
    println!("Created project '{}' (slug: {})", project.name, project.slug);
}

fn cmd_list(status: Option<&str>) {
    let (ps, _, _, _) = stores();

    let projects = ps.list_projects(status);
    if projects.is_empty() {
        println!("No projects found.");
        return;
    }

    for p in &projects {
        println!("{}\t{}\t{}\t{}", p.slug, p.status, p.priority, p.name);
    }
}

fn cmd_set_status(slug: &str, new_status: &str) {
    let (ps, _, _, _) = stores();

    let mut project = match ps.get_project(slug) {
        Some(p) => p,
        None => {
            eprintln!("Project '{slug}' not found.");
            process::exit(1);
        }
    };

    let old_status = project.status.clone();
    project.status = new_status.parse().unwrap_or_else(|_| {
        eprintln!("Invalid status '{new_status}'. Valid: active, blocked, pending, parked, done.");
        std::process::exit(1);
    });
    ps.save_project(&mut project).unwrap();
    println!(
        "{}: status {} \u{2192} {new_status}",
        project.name, old_status
    );
}

fn cmd_set_priority(slug: &str, new_priority: &str) {
    let (ps, _, _, _) = stores();

    let mut project = match ps.get_project(slug) {
        Some(p) => p,
        None => {
            eprintln!("Project '{slug}' not found.");
            process::exit(1);
        }
    };

    let old_priority = project.priority.clone();
    project.priority = new_priority.parse().unwrap_or_else(|_| {
        eprintln!("Invalid priority '{new_priority}'. Valid: high, medium, low.");
        std::process::exit(1);
    });
    ps.save_project(&mut project).unwrap();
    println!(
        "{}: priority {} \u{2192} {new_priority}",
        project.name, old_priority
    );
}

fn cmd_time(date_str: Option<&str>) {
    let (_, js, _, _) = stores();

    let journal = if let Some(ds) = date_str {
        match chrono::NaiveDate::parse_from_str(ds, "%Y-%m-%d") {
            Ok(date) => js.get_day(date).unwrap_or_else(|| {
                eprintln!("No journal for {ds}");
                process::exit(1);
            }),
            Err(_) => {
                eprintln!("Invalid date format. Use YYYY-MM-DD.");
                process::exit(1);
            }
        }
    } else {
        js.today()
    };

    let sessions = jm_time::compute_sessions(&journal);
    let now = Local::now().time();
    let aggregated = jm_time::aggregate_sessions(&sessions, now);

    let date_label = journal.date.format("%a %b %d").to_string();
    println!("Time tracked — {date_label}:");
    println!();

    if aggregated.is_empty() {
        println!("  No sessions recorded.");
        return;
    }

    let max_name_len = aggregated.iter().map(|(n, _)| n.len()).max().unwrap_or(20);
    let max_dur = aggregated.iter().map(|(_, d)| d.num_minutes()).max().unwrap_or(1).max(1);

    let mut total = chrono::Duration::zero();
    for (name, dur) in &aggregated {
        total = total + *dur;
        let formatted = jm_time::format_duration(*dur);
        let bar_len = (dur.num_minutes() * 20 / max_dur) as usize;
        let bar: String = "\u{2588}".repeat(bar_len);
        println!(
            "  {:<width$}  {:>7}  {}",
            name,
            formatted,
            bar,
            width = max_name_len
        );
    }

    println!();
    println!(
        "  {:<width$}  {:>7}",
        "Total",
        jm_time::format_duration(total),
        width = max_name_len
    );

    // Show active session if any
    if let Some(elapsed) = jm_time::active_session_elapsed(&sessions, now) {
        if let Some(session) = sessions.last() {
            println!();
            println!(
                "  Active: {} ({})",
                session.project,
                jm_time::format_duration(elapsed)
            );
        }
    }
}

fn cmd_standup(date_str: Option<&str>) {
    let config = Config::load();
    let (ps, js, _, _) = stores();

    let target_date = if let Some(ds) = date_str {
        match chrono::NaiveDate::parse_from_str(ds, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Invalid date format. Use YYYY-MM-DD.");
                process::exit(1);
            }
        }
    } else {
        Local::now().date_naive()
    };

    let yesterday = js.get_previous_workday(Some(target_date));
    let today_journal = js.get_day(target_date);

    println!("Standup — {}", target_date.format("%a %b %d, %Y"));
    println!();

    // Collect all project names mentioned in yesterday + today journals
    let mut project_names: Vec<String> = Vec::new();
    if let Some(ref yj) = yesterday {
        for entry in &yj.entries {
            if !entry.project.is_empty() && !project_names.contains(&entry.project) {
                project_names.push(entry.project.clone());
            }
        }
    }
    if let Some(ref tj) = today_journal {
        for entry in &tj.entries {
            if !entry.project.is_empty() && !project_names.contains(&entry.project) {
                project_names.push(entry.project.clone());
            }
        }
    }

    if project_names.is_empty() {
        println!("  No journal entries found.");
        return;
    }

    for project_name in &project_names {
        println!("## {project_name}");
        println!();

        // Yesterday entries
        if let Some(ref yj) = yesterday {
            let entries: Vec<_> = yj
                .entries
                .iter()
                .filter(|e| e.project.contains(project_name.as_str()))
                .collect();
            if !entries.is_empty() {
                println!("  Yesterday:");
                for entry in entries {
                    let details: Vec<String> = entry
                        .details
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect();
                    let detail_str = if details.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", details.join(", "))
                    };
                    println!("    - {} {}{}", entry.time, entry.entry_type, detail_str);
                }
            }
        }

        // Today entries
        if let Some(ref tj) = today_journal {
            let entries: Vec<_> = tj
                .entries
                .iter()
                .filter(|e| e.project.contains(project_name.as_str()))
                .collect();
            if !entries.is_empty() {
                println!("  Today:");
                for entry in entries {
                    let details: Vec<String> = entry
                        .details
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect();
                    let detail_str = if details.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", details.join(", "))
                    };
                    println!("    - {} {}{}", entry.time, entry.entry_type, detail_str);
                }
            }
        }

        // Git log if configured
        // Find slug for this project name
        let all_projects = ps.list_projects(None);
        if let Some(proj) = all_projects.iter().find(|p| p.name == *project_name) {
            if let Some(git_path) = config.git_paths.get(&proj.slug) {
                let expanded = expand_tilde(git_path);
                if expanded.exists() {
                    // Get git user name
                    let author = std::process::Command::new("git")
                        .args(["config", "user.name"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();

                    let git_output = std::process::Command::new("git")
                        .args([
                            "-C",
                            &expanded.to_string_lossy(),
                            "log",
                            "--oneline",
                            "--since=yesterday",
                            &format!("--author={author}"),
                        ])
                        .output();

                    if let Ok(output) = git_output {
                        let log = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = log.lines().collect();
                        if !lines.is_empty() {
                            println!("  Commits:");
                            for line in lines.iter().take(10) {
                                println!("    {line}");
                            }
                            if lines.len() > 10 {
                                println!("    ... {} more", lines.len() - 10);
                            }
                        }
                    }
                }
            }
        }

        println!();
    }

    // Time summary
    if let Some(ref yj) = yesterday {
        let sessions = jm_time::compute_sessions(yj);
        let now = chrono::NaiveTime::from_hms_opt(23, 59, 0).unwrap();
        let agg = jm_time::aggregate_sessions(&sessions, now);
        if !agg.is_empty() {
            let total: chrono::Duration = agg.iter().map(|(_, d)| *d).sum();
            println!(
                "Yesterday total: {}",
                jm_time::format_duration(total)
            );
        }
    }
}

fn cmd_inbox(text: &str) {
    let config = Config::load();
    let data_dir = ensure_dirs(&config);
    let inbox_store = InboxStore::new(&data_dir);

    inbox_store.append(text).unwrap();
    println!("Captured: {text}");
}

fn cmd_refs(slug: &str) {
    let (ps, _, _, _) = stores();

    let all_projects = ps.list_projects(None);

    // Verify the target project exists
    if !all_projects.iter().any(|p| p.slug == slug) {
        eprintln!("Project '{slug}' not found.");
        process::exit(1);
    }

    let refs = jm_core::crosslinks::find_references(slug, &all_projects);

    if refs.is_empty() {
        println!("No cross-references to [[{slug}]] found.");
        return;
    }

    println!("References to [[{slug}]]:");
    for (ref_slug, context) in &refs {
        println!("  {ref_slug}: {context}");
    }
}

fn issue_store() -> IssueStore {
    let config = Config::load();
    let data_dir = ensure_dirs(&config);
    IssueStore::new(&data_dir)
}

fn cmd_issue(title: &str, project: Option<&str>, parent: Option<u32>) {
    let (ps, _, _, as_) = stores();
    let is = issue_store();

    let slug = match project {
        Some(s) => s.to_string(),
        None => match as_.get_active() {
            Some(s) => s,
            None => {
                eprintln!("No active project. Use --project or run: jm work <slug>");
                process::exit(1);
            }
        },
    };

    // Verify project exists
    if ps.get_project(&slug).is_none() {
        eprintln!("Project '{slug}' not found.");
        process::exit(1);
    }

    match is.create_issue(&slug, title, parent) {
        Ok(issue) => {
            let parent_label = if let Some(pid) = parent {
                format!(" (sub-issue of #{pid})")
            } else {
                String::new()
            };
            println!("#{} created on {slug}: {title}{parent_label}", issue.id);
        }
        Err(e) => {
            eprintln!("Error creating issue: {e}");
            process::exit(1);
        }
    }
}

fn cmd_issues(project: Option<&str>, status: Option<&str>, all: bool) {
    let (ps, _, _, as_) = stores();
    let is = issue_store();

    let slugs: Vec<String> = if all {
        ps.list_projects(None).iter().map(|p| p.slug.clone()).collect()
    } else {
        let slug = match project {
            Some(s) => s.to_string(),
            None => match as_.get_active() {
                Some(s) => s,
                None => {
                    eprintln!("No active project. Use --project or --all.");
                    process::exit(1);
                }
            },
        };
        vec![slug]
    };

    let status_filter: Option<jm_core::models::IssueStatus> =
        status.and_then(|s| s.parse().ok());

    let mut any_output = false;
    for slug in &slugs {
        let issue_file = is.load(slug);
        let cm = issue_file.children_map();
        let top_issues = cm.get(&None).cloned().unwrap_or_default();

        // Filter and check if there's anything to show
        let has_matching = issue_file.issues.iter().any(|i| {
            status_filter.map_or(true, |sf| i.status == sf)
        });
        if !has_matching {
            continue;
        }

        if all && !top_issues.is_empty() {
            println!("{slug}:");
        }

        for issue in &top_issues {
            if status_filter.map_or(true, |sf| issue.status == sf) {
                let prefix = if all { "  " } else { "" };
                println!(
                    "{prefix}#{:<3} {:<8} {}",
                    issue.id, issue.status.to_string(), issue.title
                );
            }
            // Show children
            if let Some(children) = cm.get(&Some(issue.id)) {
                for child in children {
                    if status_filter.map_or(true, |sf| child.status == sf) {
                        let prefix = if all { "    " } else { "  " };
                        println!(
                            "{prefix}#{:<3} {:<8} {}",
                            child.id, child.status.to_string(), child.title
                        );
                    }
                }
            }
        }
        any_output = true;
    }

    if !any_output {
        println!("No issues found.");
    }
}

fn cmd_issue_status(slug: &str, issue_id: u32, new_status: &str) {
    let (ps, _, _, _) = stores();
    let is = issue_store();

    if ps.get_project(slug).is_none() {
        eprintln!("Project '{slug}' not found.");
        process::exit(1);
    }

    let status: jm_core::models::IssueStatus = match new_status.parse() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Invalid status '{new_status}'. Valid: todo, active, blocked, done.");
            process::exit(1);
        }
    };

    match is.set_status(slug, issue_id, status) {
        Ok(true) => println!("#{issue_id}: status -> {new_status}"),
        Ok(false) => {
            eprintln!("Issue #{issue_id} not found in project '{slug}'.");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

// ── jm jira config ──────────────────────────────────────────────────────────

fn cmd_jira_config(test: bool) {
    use std::io::{self, BufRead, Write};

    let mut config = Config::load();
    let _ = ensure_dirs(&config);

    println!();
    println!("JIRA Cloud Setup");
    println!("{}", "─".repeat(40));

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Prompt for URL
    let existing_url = plugins::jira::JiraConfig::from_plugin_config(&config)
        .map(|c| c.url)
        .unwrap_or_default();
    if existing_url.is_empty() {
        print!("Instance URL (e.g., https://myorg.atlassian.net): ");
    } else {
        print!("Instance URL [{}]: ", existing_url);
    }
    stdout.flush().ok();
    let mut url = String::new();
    stdin.lock().read_line(&mut url).ok();
    let url = url.trim();
    let url = if url.is_empty() { &existing_url } else { url }.to_string();
    if url.is_empty() {
        eprintln!("Error: JIRA URL is required.");
        process::exit(1);
    }

    // Prompt for target user email
    let existing_email = plugins::jira::JiraConfig::from_plugin_config(&config)
        .map(|c| c.email)
        .unwrap_or_default();
    if existing_email.is_empty() {
        print!("Your email (whose issues to show): ");
    } else {
        print!("Your email [{}]: ", existing_email);
    }
    stdout.flush().ok();
    let mut email = String::new();
    stdin.lock().read_line(&mut email).ok();
    let email = email.trim();
    let email = if email.is_empty() { &existing_email } else { email }.to_string();
    if email.is_empty() {
        eprintln!("Error: Email is required.");
        process::exit(1);
    }

    // Prompt for auth_email (optional)
    let existing_auth = plugins::jira::JiraConfig::from_plugin_config(&config)
        .and_then(|c| c.auth_email)
        .unwrap_or_default();
    if existing_auth.is_empty() {
        print!("Service account email (optional, Enter to skip): ");
    } else {
        print!("Service account email [{}]: ", existing_auth);
    }
    stdout.flush().ok();
    let mut auth_email = String::new();
    stdin.lock().read_line(&mut auth_email).ok();
    let auth_email = auth_email.trim();
    let auth_email = if auth_email.is_empty() && !existing_auth.is_empty() {
        existing_auth.clone()
    } else {
        auth_email.to_string()
    };

    // Check JIRA_API_TOKEN
    let token = std::env::var("JIRA_API_TOKEN").unwrap_or_default();
    if token.is_empty() {
        println!();
        eprintln!("Warning: JIRA_API_TOKEN is not set.");
        eprintln!("Generate one at: https://id.atlassian.com/manage-profile/security/api-tokens");
        eprintln!("Then: export JIRA_API_TOKEN=\"your-token\"");
        print!("Continue without token? [y/N]: ");
        stdout.flush().ok();
        let mut ans = String::new();
        stdin.lock().read_line(&mut ans).ok();
        if !ans.trim().eq_ignore_ascii_case("y") {
            eprintln!("Setup cancelled.");
            process::exit(1);
        }
    } else {
        println!("JIRA_API_TOKEN ... set");
    }

    // Build JIRA config JSON
    let mut jira_json = serde_json::json!({
        "url": url,
        "email": email,
        "refresh_interval_secs": 60,
    });
    if !auth_email.is_empty() {
        jira_json["auth_email"] = serde_json::json!(auth_email);
    }

    // Insert into config.plugins.extra
    let jira_yml = serde_json::from_value::<serde_yml::Value>(
        serde_json::to_value(&jira_json).unwrap_or_default(),
    )
    .unwrap_or_default();
    config.plugins.extra.insert("jira".to_string(), jira_yml);

    // Ensure "jira" is in the enabled list
    if !config.plugins.enabled.iter().any(|n| n == "jira") {
        config.plugins.enabled.push("jira".to_string());
    }

    // Write config back
    let config_path = expand_tilde("~/.jm/config.yaml");
    match serde_yml::to_string(&config) {
        Ok(yaml) => match std::fs::write(&config_path, &yaml) {
            Ok(_) => {
                println!();
                println!("Configuration saved to {}", config_path.display());
                println!("  URL:        {}", url);
                println!("  Email:      {}", email);
                if !auth_email.is_empty() {
                    println!("  Auth email: {}", auth_email);
                }
            }
            Err(e) => {
                eprintln!("Error writing config: {e}");
                process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error serializing config: {e}");
            process::exit(1);
        }
    }

    // Optional: test connection
    if test && !token.is_empty() {
        println!();
        let effective_auth = if auth_email.is_empty() { &email } else { &auth_email };

        print!("Testing connection... ");
        stdout.flush().ok();
        match plugins::jira::api::validate_credentials(&url, effective_auth, &token) {
            Ok(myself) => {
                println!("authenticated ({})", myself.display_name);
            }
            Err(err) => {
                println!("FAILED");
                eprintln!("  {}", err.display());
                process::exit(1);
            }
        }

        if !auth_email.is_empty() {
            print!("Looking up user {}... ", email);
            stdout.flush().ok();
            match plugins::jira::api::search_user_by_email(&url, effective_auth, &token, &email) {
                Ok(user) => {
                    println!("found ({})", user.display_name);
                }
                Err(err) => {
                    println!("FAILED");
                    eprintln!("  {}", err.display());
                    process::exit(1);
                }
            }
        }

        println!();
        println!("JIRA integration ready. Press J in the dashboard to open.");
    } else if test {
        println!();
        println!("Skipping connection test (JIRA_API_TOKEN not set).");
    }
}
