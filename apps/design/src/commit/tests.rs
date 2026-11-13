#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::disk::Edit;
use super::recover::Kept;
use super::recover::{Problem, Recovery};
use super::state::Record;
use super::*;

type Snapshot = BTreeMap<PathBuf, Option<Vec<u8>>>;

/// Every folder and file under `dir`, hidden ones included, with each file's
/// bytes.
fn snapshot(dir: &Path) -> Snapshot {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap().flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(dir).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert(rel, None);
                stack.push(path);
            } else {
                // A file made unreadable on purpose is told apart by its mode.
                let bytes = std::fs::read(&path).unwrap_or_else(|_| {
                    format!("unreadable {:?}", path.metadata().unwrap().permissions()).into_bytes()
                });
                out.insert(rel, Some(bytes));
            }
        }
    }
    out
}

fn names(snapshot: &Snapshot) -> Vec<String> {
    snapshot.keys().map(|p| p.display().to_string()).collect()
}

/// A save to run against a fresh project.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Two files rewritten in place, the config last.
    Edits,
    /// Two indexes' schema files swapped, and the config.
    Swap,
    /// A file written into a new nested folder, one rewritten, one removed.
    MoveAndRemove,
}

const SHAPES: [Shape; 3] = [Shape::Edits, Shape::Swap, Shape::MoveAndRemove];

/// A unique, empty folder for one test case, and the user's state folder
/// it saves with (beside it, so snapshots of the folder never see it); both
/// removed by [`Project::drop`].
struct Project {
    dir: PathBuf,
    state: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
        std::fs::remove_dir_all(&self.state).ok();
    }
}

impl Project {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("flusso-design-commit-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("flusso.toml"), "config\n").unwrap();
        std::fs::write(dir.join("users.schema.yml"), "users\n").unwrap();
        std::fs::write(dir.join("orders.schema.yml"), "orders\n").unwrap();
        let state = dir.with_extension("state");
        std::fs::remove_dir_all(&state).ok();
        Self { dir, state }
    }

    /// Another folder, saved by the same user (the same state folder).
    fn beside(&self, suffix: &str) -> Self {
        let dir = self.dir.with_file_name(format!(
            "{}-{suffix}",
            self.dir.file_name().unwrap().to_string_lossy()
        ));
        std::fs::remove_dir_all(&dir).ok();
        Self {
            dir,
            state: self.state.clone(),
        }
    }

    fn save(&self, shape: Shape) -> (Vec<Write>, Vec<Remove>) {
        let w = |name: &str, text: &str| Write {
            dest: self.dir.join(name),
            text: text.into(),
            expect: Expect::AsFound,
        };
        match shape {
            Shape::Edits => (
                vec![
                    w("users.schema.yml", "users 2\n"),
                    w("flusso.toml", "config 2\n"),
                ],
                vec![],
            ),
            Shape::Swap => (
                vec![
                    w("users.schema.yml", "orders\n"),
                    w("orders.schema.yml", "users\n"),
                    w("flusso.toml", "config swapped\n"),
                ],
                vec![],
            ),
            Shape::MoveAndRemove => (
                vec![
                    w("users.schema.yml", "users 2\n"),
                    w("new/deep/orders.schema.yml", "orders moved\n"),
                    w("flusso.toml", "config 2\n"),
                ],
                vec![Remove {
                    path: self.dir.join("orders.schema.yml"),
                    expect: Expect::AsFound,
                }],
            ),
        }
    }

    fn lock(&self) -> ProjectLock {
        ProjectLock::acquire(&self.dir, Duration::ZERO).unwrap()
    }

    /// The user's state folder.
    fn state(&self) -> State {
        State::at(self.state.clone()).unwrap()
    }

    fn commit(&self, shape: Shape, disk: &mut Disk) -> Result<()> {
        let (writes, removals) = self.save(shape);
        commit(&self.lock(), &writes, &removals, Some(&self.state()), disk).map(|_| ())
    }

    /// Recover as a designer opening the project would.
    fn recover(&self, disk: &mut Disk) -> Recovery {
        self.recover_with(Ok(&self.state()), disk)
    }

    fn recover_with(&self, state: Result<&State, &str>, disk: &mut Disk) -> Recovery {
        let lock = self.lock();
        let config = lock.base().join("flusso.toml");
        recover(&lock, &config, state, disk)
    }

    /// The records of saves in flight, for any folder (none when the state
    /// folder is broken).
    fn records(&self) -> Vec<Record> {
        State::at(self.state.clone())
            .map(|s| s.records())
            .unwrap_or_default()
    }

    /// Record `bytes` as a journal this user's designer wrote for `dir`.
    fn record(&self, bytes: &[u8], dir: &Path, committed: bool) {
        let token = serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .and_then(|j| j["token"].as_str().map(str::to_owned))
            .filter(|t| journal::token_ok(t))
            .unwrap_or_else(|| "1-2".to_owned());
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let record = Record::new(&token, bytes, &dir);
        let state = self.state();
        state.remove(&token, &mut Disk::counting()).unwrap();
        state.create(&record, &mut Disk::counting()).unwrap();
        if committed {
            state.commit(&record, &mut Disk::counting()).unwrap();
        }
    }
}

/// The step that marks the save's record committed: the commit point.
fn commit_point(shape: Shape) -> usize {
    (1..)
        .find(|&n| {
            let project = Project::new(&format!("point-{shape:?}-{n}"));
            project.commit(shape, &mut Disk::stopping_after(n)).ok();
            project.records().iter().any(|r| r.committed)
        })
        .unwrap()
}

/// The project before and after an uninterrupted save of `shape`, and the
/// steps the save takes.
fn reference(shape: Shape) -> (Snapshot, Snapshot, usize) {
    let project = Project::new(&format!("ref-{shape:?}"));
    let before = snapshot(&project.dir);
    let mut disk = Disk::counting();
    project.commit(shape, &mut disk).unwrap();
    assert!(project.records().is_empty());
    (before, snapshot(&project.dir), disk.steps())
}

#[test]
fn a_commit_writes_removes_and_leaves_nothing_behind() {
    let (_, after, _) = reference(Shape::MoveAndRemove);
    assert_eq!(
        names(&after),
        [
            "flusso.toml",
            "new",
            "new/deep",
            "new/deep/orders.schema.yml",
            "users.schema.yml"
        ]
    );
    assert_eq!(
        after[Path::new("flusso.toml")],
        Some(b"config 2\n".to_vec())
    );
}

#[cfg(unix)]
#[test]
fn a_replaced_file_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new("mode");
    let config = project.dir.join("flusso.toml");
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    project.commit(Shape::Edits, &mut Disk::default()).unwrap();
    let mode = std::fs::metadata(&config).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

/// A failure at any step up to the commit point leaves the project byte for
/// byte as it was; a failure while cleaning up leaves the save in place, and
/// the next recovery deletes what is left.
#[test]
fn a_failure_at_any_step_restores_the_project_or_keeps_the_save() {
    for shape in SHAPES {
        let (before, after, steps) = reference(shape);
        let mut failed_before_commit = true;
        for n in 1..=steps {
            let project = Project::new(&format!("fail-{shape:?}-{n}"));
            match project.commit(shape, &mut Disk::failing(vec![n])) {
                Err(_) => {
                    assert!(
                        failed_before_commit,
                        "{shape:?}: step {n} failed after a cleanup step"
                    );
                    assert!(project.records().is_empty(), "{shape:?}: step {n}");
                    assert_eq!(
                        snapshot(&project.dir),
                        before,
                        "{shape:?}: failure at step {n}"
                    );
                }
                // The record failing to be written leaves an unrecorded
                // save, which goes on.
                Ok(()) if n == 1 => {
                    assert_eq!(snapshot(&project.dir), after, "{shape:?}: step 1");
                    assert!(project.records().is_empty());
                }
                Ok(()) => {
                    failed_before_commit = false;
                    let recovery = project.recover(&mut Disk::counting());
                    assert!(
                        recovery.blocked.is_none(),
                        "{shape:?} step {n}: {recovery:?}"
                    );
                    assert_eq!(
                        snapshot(&project.dir),
                        after,
                        "{shape:?}: failure at step {n}"
                    );
                    assert!(project.records().is_empty(), "{shape:?}: step {n}");
                }
            }
        }
        assert!(!failed_before_commit, "{shape:?}: no cleanup step");
    }
}

/// When putting a file back fails too, the error names the backups that
/// still hold the previous text, and the next recovery finishes the job.
#[test]
fn a_failed_restore_names_the_backups_left_and_recovery_finishes_it() {
    let (before, _, _) = reference(Shape::MoveAndRemove);
    let commit_point = commit_point(Shape::MoveAndRemove);
    let project = Project::new("restore");
    let err = project
        .commit(
            Shape::MoveAndRemove,
            &mut Disk::failing(vec![commit_point, commit_point + 1]),
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("restoring the project failed"), "{err}");
    assert!(err.contains(".flusso.toml.flusso-save."), "{err}");
    assert!(err.contains(".bak"), "{err}");

    let recovery = project.recover(&mut Disk::counting());
    assert_eq!(recovery.settled.len(), 1, "{recovery:?}");
    assert_eq!(recovery.settled[0].outcome, Outcome::RolledBack);
    assert_eq!(snapshot(&project.dir), before);
}

/// The issue's case, for every step: a process killed right after step `n`
/// of a save leaves a project that recovery turns into exactly the project
/// before the save, or exactly the one after it — before up to the commit
/// point, after from then on.
#[test]
fn a_save_killed_after_any_step_recovers_to_before_or_after() {
    for shape in SHAPES {
        let (before, after, steps) = reference(shape);
        let mut reached_after = false;
        for n in 0..=steps {
            let project = Project::new(&format!("kill-{shape:?}-{n}"));
            project.commit(shape, &mut Disk::stopping_after(n)).ok();
            let recovery = project.recover(&mut Disk::counting());
            assert!(
                recovery.blocked.is_none(),
                "{shape:?} killed after {n}: {recovery:?}"
            );
            assert!(
                project.records().is_empty(),
                "{shape:?} killed after {n}: a record is left"
            );
            let now = snapshot(&project.dir);
            if now == after {
                reached_after = true;
            } else {
                assert!(
                    !reached_after,
                    "{shape:?}: killed after {n}, back to before after a later step reached after"
                );
                assert_eq!(
                    now,
                    before,
                    "{shape:?}: killed after {n}: {:?}",
                    names(&now)
                );
            }
            if let Some(settled) = recovery.settled.first() {
                let expected = if now == after {
                    Outcome::Completed
                } else {
                    Outcome::RolledBack
                };
                assert_eq!(settled.outcome, expected, "{shape:?} killed after {n}");
            }
        }
        assert!(reached_after, "{shape:?}");
    }
}

/// Recovery itself killed after any of its steps, then run again, still ends
/// at before or after, and the same one an uninterrupted recovery reaches.
#[test]
fn a_recovery_killed_after_any_step_recovers_when_run_again() {
    for shape in SHAPES {
        let (before, after, steps) = reference(shape);
        for n in 0..steps {
            let expected = {
                let project = Project::new(&format!("re-ref-{shape:?}-{n}"));
                project.commit(shape, &mut Disk::stopping_after(n)).ok();
                project.recover(&mut Disk::counting());
                snapshot(&project.dir)
            };
            assert!(expected == before || expected == after);
            for m in 0.. {
                let project = Project::new(&format!("re-{shape:?}-{n}-{m}"));
                project.commit(shape, &mut Disk::stopping_after(n)).ok();
                let mut cut = Disk::stopping_after(m);
                project.recover(&mut cut);
                let finished = !cut.stopped();
                let again = project.recover(&mut Disk::counting());
                assert!(again.blocked.is_none(), "{shape:?} {n}/{m}: {again:?}");
                assert!(project.records().is_empty(), "{shape:?} {n}/{m}");
                assert_eq!(
                    snapshot(&project.dir),
                    expected,
                    "{shape:?}: save killed after {n}, recovery killed after {m}"
                );
                if finished {
                    break;
                }
            }
        }
    }
}

/// Stop a swap right after its first file was replaced, the issue's state.
fn half_swapped(tag: &str) -> Project {
    // record: 1, journal: 2, temps: 3, backups: 3, then the first rename.
    let project = Project::new(tag);
    project
        .commit(Shape::Swap, &mut Disk::stopping_after(10))
        .unwrap_err();
    assert_eq!(
        std::fs::read_to_string(project.dir.join("users.schema.yml")).unwrap(),
        "orders\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.dir.join("flusso.toml")).unwrap(),
        "config\n"
    );
    project
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    strays(&[dir.to_path_buf()])
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "bak"))
        .collect()
}

#[test]
fn a_missing_backup_stops_recovery_and_changes_nothing() {
    let project = half_swapped("missing");
    let users_backup = backups(&project.dir)
        .into_iter()
        .find(|p| p.to_string_lossy().contains(".users.schema.yml."))
        .unwrap();
    std::fs::remove_file(&users_backup).unwrap();
    let left = snapshot(&project.dir);

    let recovery = project.recover(&mut Disk::counting());
    let blocked = recovery.blocked.expect("blocked");
    assert!(recovery.settled.is_empty());
    assert_eq!(blocked.problems.len(), 1, "{blocked:?}");
    assert_eq!(blocked.problems[0].kind, ProblemKind::BackupMissing);
    assert_eq!(
        blocked.problems[0].path,
        project.dir.join("users.schema.yml")
    );
    assert_eq!(blocked.problems[0].backup.as_ref(), Some(&users_backup));
    assert!(
        blocked
            .leftovers
            .iter()
            .any(|p| p.ends_with(journal::JOURNAL))
    );
    assert_eq!(snapshot(&project.dir), left, "recovery changed a file");
}

#[test]
fn leftovers_with_no_journal_are_reported_and_left_alone() {
    let project = Project::new("old-style");
    let old = [
        project
            .dir
            .join(".users.schema.yml.flusso-save.123-456.0.bak"),
        project.dir.join(".flusso.toml.flusso-save.123-456.1.tmp"),
    ];
    for file in &old {
        std::fs::write(file, "old\n").unwrap();
    }
    let left = snapshot(&project.dir);

    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.settled.is_empty() && recovery.blocked.is_none());
    let mut sorted = old.to_vec();
    sorted.sort();
    assert_eq!(strays(&[project.dir.clone()]), sorted);
    assert_eq!(snapshot(&project.dir), left);
}

/// A second writer waits for the lock and gives up while the first holds it:
/// it never takes a live save's journal for a dead one.
#[test]
fn a_second_writer_waits_for_the_first_and_leaves_its_journal_alone() {
    let project = Project::new("two");
    let held = project.lock();
    let (writes, removals) = project.save(Shape::Edits);
    // The first writer is mid-save: its journal is on disk, its lock held.
    commit(
        &held,
        &writes,
        &removals,
        Some(&project.state()),
        &mut Disk::stopping_after(5),
    )
    .unwrap_err();
    let mid = snapshot(&project.dir);

    let second = ProjectLock::acquire(&project.dir, Duration::from_millis(60));
    assert!(matches!(second, Err(LockError::Busy)), "{second:?}");
    assert_eq!(snapshot(&project.dir), mid);

    drop(held);
    let recovery = project.recover(&mut Disk::counting());
    assert_eq!(recovery.settled.len(), 1);
    assert_eq!(
        names(&snapshot(&project.dir)),
        ["flusso.toml", "orders.schema.yml", "users.schema.yml"]
    );
}

#[test]
fn a_released_lock_leaves_no_file() {
    let project = Project::new("lockfile");
    drop(project.lock());
    if cfg!(unix) {
        assert!(!project.dir.join(lock::LOCK).exists());
    }
}

/// The child half of [`a_process_aborted_mid_save_is_recovered`]: run only
/// when that test starts this binary with `FLUSSO_DESIGN_TEST_ABORT` set.
#[test]
#[ignore = "run by a_process_aborted_mid_save_is_recovered"]
fn child_aborts_mid_save() {
    let Ok(spec) = std::env::var("FLUSSO_DESIGN_TEST_ABORT") else {
        return;
    };
    let (step, dir) = spec.split_once(':').unwrap();
    let project = Project {
        dir: PathBuf::from(dir),
        state: PathBuf::from(dir).with_extension("state"),
    };
    let (writes, removals) = project.save(Shape::Swap);
    let lock = ProjectLock::acquire(&project.dir, Duration::ZERO).unwrap();
    commit(
        &lock,
        &writes,
        &removals,
        Some(&project.state()),
        &mut Disk::aborting_after(step.parse().unwrap()),
    )
    .ok();
    std::mem::forget(project);
}

/// A real process aborted in the middle of a swap (holding the lock, its
/// files flushed), recovered by this one.
#[test]
fn a_process_aborted_mid_save_is_recovered() {
    let (before, after, steps) = reference(Shape::Swap);
    for step in [3, 10, steps - 4] {
        let project = Project::new(&format!("abort-{step}"));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commit::tests::child_aborts_mid_save",
                "--ignored",
                "--nocapture",
            ])
            .env(
                "FLUSSO_DESIGN_TEST_ABORT",
                format!("{step}:{}", project.dir.display()),
            )
            .output()
            .unwrap()
            .status;
        assert!(!status.success(), "the child did not abort at step {step}");
        assert!(
            project.dir.join(journal::JOURNAL).exists() || project.dir.join(journal::DONE).exists(),
            "no journal after an abort at step {step}"
        );
        let recovery = project.recover(&mut Disk::counting());
        assert_eq!(recovery.settled.len(), 1, "{recovery:?}");
        assert!(project.records().is_empty());
        let now = snapshot(&project.dir);
        assert!(
            now == before || now == after,
            "abort at {step}: {:?}",
            names(&now)
        );
    }
}

/// Copy `from` into a fresh `to`, as `cp -r` does (hard links become copies).
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A folder beside `project` that no journal of it may touch; removed on drop.
fn victim(project: &Project) -> Project {
    let victim = project.beside("victim");
    std::fs::create_dir_all(victim.dir.join("empty")).unwrap();
    std::fs::write(victim.dir.join("keep.yml"), "mine\n").unwrap();
    victim
}

/// A journal written by hand, in format 2 unless `body` says otherwise.
fn journal_body(body: &serde_json::Value) -> Vec<u8> {
    let mut journal = serde_json::json!({
        "format": 2, "started_at_ms": 1, "pid": 1, "token": "1-2",
        "dirs": [], "writes": [], "removals": [],
    });
    for (key, value) in body.as_object().unwrap() {
        journal[key] = value.clone();
    }
    serde_json::to_vec_pretty(&journal).unwrap()
}

/// Whose record a planted journal has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recorded {
    /// None: another user's or machine's, an old one, or written by hand.
    No,
    /// This user's, for another folder.
    Elsewhere,
    /// This user's, for this folder.
    Here,
}

fn plant(project: &Project, name: &str, body: &serde_json::Value, recorded: Recorded) {
    let bytes = journal_body(body);
    match recorded {
        Recorded::No => {}
        Recorded::Elsewhere => {
            project.record(&bytes, &project.dir.with_file_name("elsewhere"), false);
        }
        Recorded::Here => project.record(&bytes, &project.dir, false),
    }
    std::fs::write(project.dir.join(name), bytes).unwrap();
}

fn digest(text: &str) -> String {
    serde_json::to_value(journal::Digest::of(text.as_bytes()))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// Recover `project` and check it refused the journal `name` for `cause`:
/// nothing in `watch` changed, the recovery blocked, and it says what the
/// journal names.
fn refused(
    project: &Project,
    name: &str,
    cause: Unusable,
    watch: &[&Path],
    label: &str,
) -> recover::Report {
    refused_with(project, Ok(&project.state()), name, cause, watch, label)
}

fn refused_with(
    project: &Project,
    state: Result<&State, &str>,
    name: &str,
    cause: Unusable,
    watch: &[&Path],
    label: &str,
) -> recover::Report {
    let was: Vec<Snapshot> = watch.iter().map(|d| snapshot(d)).collect();
    let records = project.records();
    let recovery = project.recover_with(state, &mut Disk::counting());
    let blocked = recovery
        .blocked
        .unwrap_or_else(|| panic!("{label} was acted on: {:?}", recovery.settled));
    assert!(recovery.settled.is_empty(), "{label}");
    assert_eq!(blocked.problems.len(), 1, "{label}");
    assert_eq!(
        blocked.problems[0].kind,
        ProblemKind::JournalUnusable,
        "{label}: {blocked:?}"
    );
    for (dir, was) in watch.iter().zip(&was) {
        assert_eq!(&snapshot(dir), was, "{label} changed {}", dir.display());
    }
    assert_eq!(project.records(), records, "{label} changed the records");
    let base = project.lock().base().to_path_buf();
    assert!(
        blocked.leftovers.contains(&base.join(name)),
        "{label}: {:?}",
        blocked.leftovers
    );
    assert!(
        blocked.leftovers.iter().all(|p| p.starts_with(&base)),
        "{label}: a leftover outside the project: {:?}",
        blocked.leftovers
    );
    let report = blocked.report.expect("a report");
    assert_eq!(
        report.cause, cause,
        "{label}: {:?}",
        blocked.problems[0].detail
    );
    *report
}

/// A project copied after a killed swap is only reported (the record names
/// the original's folder), and the copy is left byte for byte as it is. The
/// original, never touched, still recovers; then the copy's journal has no
/// record at all.
#[test]
fn a_copied_project_is_reported_and_the_original_still_recovers() {
    let (before, _, _) = reference(Shape::Swap);
    let original = half_swapped("copied-from");
    let left = snapshot(&original.dir);
    let copy = original.beside("copy");
    copy_tree(&original.dir, &copy.dir);
    let copied = snapshot(&copy.dir);

    let report = refused(
        &copy,
        journal::JOURNAL,
        Unusable::OtherFolder,
        &[&copy.dir, &original.dir],
        "the copy",
    );
    assert_eq!(
        report.recorded_for.as_deref(),
        Some(std::fs::canonicalize(&original.dir).unwrap().as_path())
    );
    // Following the report's way back by hand gives the project before.
    follow(&report.route(true).expect("a way back"));
    assert_eq!(snapshot(&copy.dir), before);

    assert_eq!(snapshot(&original.dir), left, "the original was touched");
    let recovery = original.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    assert_eq!(snapshot(&original.dir), before);
    assert!(original.records().is_empty());

    // The original settled: the copy's journal (put back) has no record now.
    let again = original.beside("copy-again");
    std::fs::create_dir_all(&again.dir).unwrap();
    for (rel, bytes) in &copied {
        match bytes {
            Some(bytes) => std::fs::write(again.dir.join(rel), bytes).unwrap(),
            None => std::fs::create_dir_all(again.dir.join(rel)).unwrap(),
        }
    }
    refused(
        &again,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&again.dir],
        "after",
    );
}

/// A project moved after a killed swap is reported, not recovered; moved
/// back, it recovers.
#[test]
fn a_moved_project_is_reported_and_recovers_where_it_was() {
    let (before, _, _) = reference(Shape::Swap);
    let project = half_swapped("moved-from");
    let moved = project.beside("to");
    std::fs::rename(&project.dir, &moved.dir).unwrap();
    refused(
        &moved,
        journal::JOURNAL,
        Unusable::OtherFolder,
        &[&moved.dir],
        "moved",
    );
    std::fs::rename(&moved.dir, &project.dir).unwrap();
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    assert_eq!(snapshot(&project.dir), before);
}

/// The reviewer's case: a journal committed to a repository names
/// `.git/HEAD` as a file the save created, with the digest of its known
/// text, so rolling it back would delete it. That journal and its variants
/// (a user's own file, a user's empty folder, a planted backup and temp)
/// change nothing with no record, or a record of another folder, in
/// `.journal` and `.done`: recovery blocks with a report, and once the
/// journal is deleted the project opens.
#[test]
fn a_journal_without_a_record_of_this_folder_changes_nothing_and_is_reported() {
    let head = "ref: refs/heads/main\n";
    let notes = "my notes\n";
    for name in [journal::JOURNAL, journal::DONE] {
        for recorded in [Recorded::No, Recorded::Elsewhere] {
            let project = Project::new(&format!("foreign-{}-{recorded:?}", name.len()));
            std::fs::create_dir_all(project.dir.join(".git")).unwrap();
            std::fs::write(project.dir.join(".git/HEAD"), head).unwrap();
            std::fs::write(project.dir.join("notes.md"), notes).unwrap();
            std::fs::create_dir(project.dir.join("empty")).unwrap();
            std::fs::write(
                project.dir.join(".users.schema.yml.flusso-save.1-2.0.bak"),
                "evil\n",
            )
            .unwrap();
            std::fs::write(
                project.dir.join(".users.schema.yml.flusso-save.1-2.0.tmp"),
                "",
            )
            .unwrap();
            let base = project.lock().base().to_path_buf();
            let cases = [
                serde_json::json!({ "writes": [{ "dest": ".git/HEAD", "before": null, "after": digest(head) }] }),
                serde_json::json!({ "writes": [{ "dest": "notes.md", "before": null, "after": digest(notes) }] }),
                serde_json::json!({ "dirs": ["empty"],
                                    "writes": [{ "dest": "empty/x.yml", "before": null, "after": digest("x\n") }] }),
                serde_json::json!({ "writes": [{ "dest": "users.schema.yml", "before": digest("evil\n"),
                                                 "after": digest("users\n") }] }),
            ];
            let cause = match recorded {
                Recorded::No => Unusable::NoRecord,
                _ => Unusable::OtherFolder,
            };
            for (n, case) in cases.iter().enumerate() {
                let label = format!("{name} {recorded:?} case {n}");
                plant(&project, name, case, recorded);
                let report = refused(&project, name, cause, &[&project.dir], &label);
                let file = &report.files[0];
                match n {
                    // A dotfile and a file that is no schema get no steps:
                    // neither is looked at, and no route names it.
                    0 | 1 => {
                        assert!(file.path.is_none(), "{label}");
                        assert_eq!(file.holds, Holds::Unchecked, "{label}");
                        assert!(
                            report.route(true).is_none() && report.route(false).is_none(),
                            "{label}"
                        );
                    }
                    3 => {
                        assert_eq!(file.holds, Holds::After, "{label}");
                        assert_eq!(
                            file.backup.as_ref(),
                            Some(&base.join(".users.schema.yml.flusso-save.1-2.0.bak"))
                        );
                        // The empty temp holds no text the journal names.
                        assert!(file.temp.is_none(), "{label}");
                    }
                    _ => {}
                }
                std::fs::remove_file(project.dir.join(name)).unwrap();
                let recovery = project.recover(&mut Disk::counting());
                assert!(
                    recovery.blocked.is_none() && recovery.settled.is_empty(),
                    "{label}: {recovery:?}"
                );
            }
            assert_eq!(
                std::fs::read_to_string(project.dir.join(".git/HEAD")).unwrap(),
                head
            );
            assert!(project.dir.join("empty").is_dir());
        }
    }
}

/// A journal recorded for this folder, but naming what no save of the
/// project writes, changes nothing, inside the project or beside it.
#[test]
fn a_recorded_journal_naming_what_no_save_writes_changes_nothing() {
    let mine = digest("mine\n");
    let users = digest("users\n");
    for name in [journal::JOURNAL, journal::DONE] {
        let project = Project::new(&format!("not-a-save-{}", name.len()));
        let outside = victim(&project);
        std::fs::write(project.dir.join("notes.md"), "mine\n").unwrap();
        std::fs::create_dir(project.dir.join("notes-dir")).unwrap();
        let folder = outside
            .dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let own = project
            .dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside.dir, project.dir.join("link")).unwrap();
        let w = |dest: &str| serde_json::json!({ "writes": [{ "dest": dest, "before": null, "after": mine }] });
        let users_write =
            serde_json::json!({ "dest": "users.schema.yml", "before": users, "after": mine });
        let mut cases = vec![
            w(&format!("{}/keep.yml", outside.dir.display())),
            w("./users.schema.yml"),
            w("sub/../users.schema.yml"),
            w(".flusso-save.lock"),
            w(".users.schema.yml.flusso-save.1-2.0.bak"),
            w(&format!("../{own}/notes.md")),
            w("a//b.yml"),
            w("x/"),
            w(""),
            w("a\u{0}b.yml"),
            w(&("a/".repeat(3000) + "x.yml")),
            serde_json::json!({ "removals": [{ "path": format!("../{folder}/keep.yml"), "before": mine }] }),
            serde_json::json!({ "writes": [{ "dest": "flusso.toml", "before": null, "after": mine }, users_write] }),
            serde_json::json!({ "removals": [{ "path": "flusso.toml", "before": digest("config\n") }] }),
            serde_json::json!({ "dirs": ["notes-dir"], "writes": [users_write] }),
            serde_json::json!({ "dirs": [".."],
                                "writes": [{ "dest": format!("../{folder}/new.yml"), "before": null, "after": mine }] }),
            serde_json::json!({ "writes": [{ "dest": "users.schema.yml", "before": "00", "after": mine }] }),
            serde_json::json!({ "token": "../x", "writes": [users_write] }),
            serde_json::json!({ "writes": [users_write, users_write] }),
            serde_json::json!({ "writes": (0..=journal::MAX_ENTRIES)
                .map(|i| serde_json::json!({ "dest": format!("f{i}.yml"), "before": null, "after": mine }))
                .collect::<Vec<_>>() }),
        ];
        if cfg!(unix) {
            cases.push(w("link/keep.yml"));
            cases.push(serde_json::json!({ "dirs": ["link"],
                "writes": [{ "dest": "link/new.yml", "before": null, "after": mine }] }));
        }
        for (n, case) in cases.iter().enumerate() {
            plant(&project, name, case, Recorded::Here);
            refused(
                &project,
                name,
                Unusable::NotASave,
                &[&project.dir, &outside.dir],
                &format!("{name} case {n}"),
            );
            std::fs::remove_file(project.dir.join(name)).unwrap();
        }
        for body in [&b"{ not json"[..], br#"{"format": 1}"#, br#"{"format": 3}"#] {
            project.record(body, &project.dir, false);
            std::fs::write(project.dir.join(name), body).unwrap();
            refused(
                &project,
                name,
                Unusable::Malformed,
                &[&project.dir],
                &format!("{name} {}", String::from_utf8_lossy(body)),
            );
            std::fs::remove_file(project.dir.join(name)).unwrap();
        }
    }
}

/// One byte changed anywhere in a recorded journal (a path, a digest, the
/// token) and it is no longer the one recorded: nothing changes. Put back,
/// it recovers.
#[test]
fn a_tampered_journal_changes_nothing() {
    let (before, _, _) = reference(Shape::Swap);
    let project = half_swapped("tampered");
    let path = project.dir.join(journal::JOURNAL);
    let original = std::fs::read(&path).unwrap();
    let text = String::from_utf8(original.clone()).unwrap();
    let at = |needle: &str, offset: usize| text.find(needle).unwrap() + offset;
    for (what, i) in [
        ("a path", at("users.schema.yml", 0)),
        ("a digest", at("\"after\": \"", 10)),
        ("the token", at("\"token\": \"", 10)),
        ("the end", original.len() - 2),
    ] {
        let mut tampered = original.clone();
        tampered[i] = if tampered[i] == b'1' { b'2' } else { b'1' };
        std::fs::write(&path, &tampered).unwrap();
        refused(
            &project,
            journal::JOURNAL,
            Unusable::NoRecord,
            &[&project.dir],
            what,
        );
    }
    std::fs::write(&path, &original).unwrap();
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    assert_eq!(snapshot(&project.dir), before);
}

/// The reviewer's replay. A save in `a` writing `../shared/new.schema.yml`
/// is killed once that file is in place; its journal is captured. `a` is
/// recovered, then saved again whole, so `a` now uses the shared file. The
/// captured journal planted in a sibling `c`, with a `flusso.toml` and a
/// backup made to match its digests, changes nothing: `c` is reported and
/// the shared file survives. So does the same journal replayed in `a`.
/// While `a`'s save is still in flight, the copy in `c` is reported too
/// (the record names `a`), and `a` then recovers.
#[test]
fn a_captured_journal_replayed_in_a_sibling_or_its_own_project_changes_nothing() {
    let root = std::env::temp_dir().join(format!("flusso-design-replay-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("shared")).unwrap();
    let root = std::fs::canonicalize(&root).unwrap();
    let project = |name: &str| {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("flusso.toml"), "config\n").unwrap();
        std::fs::write(dir.join("users.schema.yml"), "users\n").unwrap();
        Project {
            dir,
            state: root.join("state"),
        }
    };
    let a = project("a");
    let shared = root.join("shared/new.schema.yml");
    let writes = || {
        vec![
            Write {
                dest: shared.clone(),
                text: "shared\n".into(),
                expect: Expect::AsFound,
            },
            Write {
                dest: a.dir.join("flusso.toml"),
                text: "config 2\n".into(),
                expect: Expect::AsFound,
            },
        ]
    };
    let mut n = 1;
    let captured = loop {
        a.commit_writes(&writes(), &mut Disk::stopping_after(n))
            .ok();
        let journal = a.dir.join(journal::JOURNAL);
        if journal.exists() && shared.exists() {
            break std::fs::read(journal).unwrap();
        }
        a.recover(&mut Disk::counting());
        n += 1;
    };
    let token = serde_json::from_slice::<serde_json::Value>(&captured).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let plant_in = |c: &Project| {
        std::fs::write(c.dir.join("flusso.toml"), "config 2\n").unwrap();
        std::fs::write(
            c.dir
                .join(format!(".flusso.toml.flusso-save.{token}.1.bak")),
            "config\n",
        )
        .unwrap();
        std::fs::write(c.dir.join(journal::JOURNAL), &captured).unwrap();
    };

    // In flight: the record names `a`, so `c` is reported.
    let c = project("c");
    plant_in(&c);
    refused(
        &c,
        journal::JOURNAL,
        Unusable::OtherFolder,
        &[&root.join("shared"), &a.dir, &c.dir],
        "in flight",
    );
    std::fs::remove_dir_all(&c.dir).unwrap();

    let recovery = a.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    assert!(!shared.exists());
    a.commit_writes(&writes(), &mut Disk::counting()).unwrap();
    assert!(shared.exists());
    assert!(a.records().is_empty());

    let c = project("c");
    plant_in(&c);
    refused(
        &c,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&root.join("shared"), &a.dir, &c.dir],
        "replayed in a sibling",
    );
    assert_eq!(std::fs::read_to_string(&shared).unwrap(), "shared\n");

    std::fs::write(a.dir.join(journal::JOURNAL), &captured).unwrap();
    refused(
        &a,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&root.join("shared"), &a.dir],
        "replayed in its own project",
    );
    assert_eq!(std::fs::read_to_string(&shared).unwrap(), "shared\n");
    drop((a, c));
    std::fs::remove_dir_all(&root).ok();
}

impl Project {
    fn commit_writes(&self, writes: &[Write], disk: &mut Disk) -> Result<()> {
        commit(&self.lock(), writes, &[], Some(&self.state()), disk).map(|_| ())
    }
}

/// A save killed before its commit point, its `.journal` renamed to `.done`:
/// the record says it never committed, so it is rolled back, never
/// "completed", its backups kept until the rollback needs them. After the
/// commit point, `.done` renamed back to `.journal` is finished: the record
/// decides, not the name.
#[test]
fn the_record_not_the_journal_name_decides_roll_back_or_finish() {
    for shape in SHAPES {
        let (before, after, steps) = reference(shape);
        let point = commit_point(shape);
        for n in 0..=steps {
            let project = Project::new(&format!("renamed-{shape:?}-{n}"));
            project.commit(shape, &mut Disk::stopping_after(n)).ok();
            let (from, to) = if n < point {
                (journal::JOURNAL, journal::DONE)
            } else {
                (journal::DONE, journal::JOURNAL)
            };
            if project.dir.join(from).exists() {
                std::fs::rename(project.dir.join(from), project.dir.join(to)).unwrap();
            }
            let recovery = project.recover(&mut Disk::counting());
            assert!(recovery.blocked.is_none(), "{shape:?} {n}: {recovery:?}");
            let expected = if n < point { &before } else { &after };
            assert_eq!(
                &snapshot(&project.dir),
                expected,
                "{shape:?} killed after {n}"
            );
            if let Some(settled) = recovery.settled.first() {
                let outcome = if n < point {
                    Outcome::RolledBack
                } else {
                    Outcome::Completed
                };
                assert_eq!(settled.outcome, outcome, "{shape:?} {n}");
            }
            assert!(project.records().is_empty(), "{shape:?} {n}");
        }
    }
}

/// `.journal` and `.done` both there, as no save leaves them: reported,
/// both named, nothing changed, recorded or not.
#[test]
fn a_journal_and_a_done_both_there_are_reported() {
    let project = half_swapped("both");
    let journal = std::fs::read(project.dir.join(journal::JOURNAL)).unwrap();
    std::fs::write(project.dir.join(journal::DONE), &journal).unwrap();
    let report = refused(
        &project,
        journal::JOURNAL,
        Unusable::Conflict,
        &[&project.dir],
        "both, recorded",
    );
    let base = project.lock().base().to_path_buf();
    assert_eq!(
        report.journals,
        [base.join(journal::JOURNAL), base.join(journal::DONE)]
    );

    let planted = Project::new("both-planted");
    let body = serde_json::json!({ "writes": [{ "dest": "users.schema.yml",
        "before": digest("users\n"), "after": digest("x\n") }] });
    plant(&planted, journal::JOURNAL, &body, Recorded::No);
    plant(&planted, journal::DONE, &body, Recorded::No);
    let report = refused(
        &planted,
        journal::DONE,
        Unusable::Conflict,
        &[&planted.dir],
        "both, planted",
    );
    assert_eq!(report.journals.len(), 2);
}

/// A report never looks outside the config's folder: a planted journal
/// naming `../victim/secret.yml` with digests the attacker chose shows the
/// path as text, marked outside, and no filesystem call touches it; the
/// backups and temps it would have are never listed.
#[test]
fn a_report_never_looks_outside_the_project() {
    let project = Project::new("outside-report");
    let outside = victim(&project);
    std::fs::write(outside.dir.join("secret.yml"), "secret\n").unwrap();
    std::fs::write(
        outside.dir.join(".secret.yml.flusso-save.1-2.0.bak"),
        "guess\n",
    )
    .unwrap();
    let folder = outside
        .dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let body = serde_json::json!({ "writes": [
        { "dest": format!("../{folder}/secret.yml"), "before": digest("guess\n"), "after": digest("secret\n") },
        { "dest": "users.schema.yml", "before": digest("users\n"), "after": digest("x\n") },
    ] });
    plant(&project, journal::JOURNAL, &body, Recorded::No);
    let base = project.lock().base().to_path_buf();
    recover::LOOKED_AT.with(|l| l.borrow_mut().clear());
    let report = refused(
        &project,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&project.dir, &outside.dir],
        "outside",
    );
    let looked: Vec<PathBuf> = recover::LOOKED_AT.with(|l| l.borrow().clone());
    assert!(!looked.is_empty());
    assert!(
        looked.iter().all(|p| p.starts_with(&base)),
        "looked outside: {looked:?}"
    );
    let secret = &report.files[0];
    assert_eq!(secret.holds, Holds::Outside);
    assert!(secret.path.is_none() && secret.backup.is_none() && secret.temp.is_none());
    assert_eq!(secret.text, format!("../{folder}/secret.yml"));
    assert_eq!(report.files[1].holds, Holds::Before);
}

/// A state folder for one test, at a path the test breaks; removed on drop.
struct Broken(PathBuf);

impl Drop for Broken {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for entry in walk(&self.0) {
                std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o700)).ok();
            }
        }
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir.to_path_buf()];
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().symlink_metadata().is_ok_and(|m| m.is_dir()) {
            out.extend(walk(&entry.path()));
        } else {
            out.push(entry.path());
        }
    }
    out
}

/// The state folder is made private: 0700, a record 0600 while its save is
/// in flight, and gone once the save settles.
#[cfg(unix)]
#[test]
fn records_are_private_and_gone_once_the_save_settles() {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new("private");
    project
        .commit(Shape::Edits, &mut Disk::stopping_after(1))
        .unwrap_err();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&project.state), 0o700);
    let records: Vec<PathBuf> = std::fs::read_dir(&project.state)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(mode(&records[0]), 0o600);
    project.recover(&mut Disk::counting());
    assert!(project.records().is_empty());
    assert_eq!(std::fs::read_dir(&project.state).unwrap().count(), 0);
}

/// With no usable state folder (none, a link, a file, open to others, a
/// record open to others or that is a FIFO) a save still lands whole and an
/// in-process failure is still undone; a journal is then only reported.
#[cfg(unix)]
#[test]
fn without_a_usable_state_folder_saves_work_and_recovery_only_reports() {
    use std::os::unix::fs::PermissionsExt;
    let (before, after, _) = reference(Shape::Edits);
    for broken in ["link", "file", "open", "record-open", "record-fifo"] {
        let project = half_swapped(&format!("stateless-{broken}"));
        let left = snapshot(&project.dir);
        let place = Broken(project.state.with_extension("broken"));
        std::fs::create_dir_all(&place.0).unwrap();
        match broken {
            "link" => {
                std::fs::rename(&project.state, place.0.join("real")).unwrap();
                std::os::unix::fs::symlink(place.0.join("real"), &project.state).unwrap();
            }
            "file" => {
                std::fs::rename(&project.state, place.0.join("real")).unwrap();
                std::fs::write(&project.state, "").unwrap();
            }
            "open" => {
                std::fs::set_permissions(&project.state, std::fs::Permissions::from_mode(0o755))
                    .unwrap();
            }
            "record-open" => {
                for record in walk(&project.state).into_iter().skip(1) {
                    std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o644))
                        .unwrap();
                }
            }
            _ => {
                for record in walk(&project.state).into_iter().skip(1) {
                    std::fs::remove_file(&record).unwrap();
                    assert!(
                        std::process::Command::new("mkfifo")
                            .arg(&record)
                            .status()
                            .unwrap()
                            .success()
                    );
                }
            }
        }
        let opened = State::at(project.state.clone());
        let why = opened
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
        let record_broken = broken.starts_with("record");
        assert_eq!(opened.is_ok(), record_broken, "{broken}: {why}");
        let started = std::time::Instant::now();
        refused_with(
            &project,
            opened.as_ref().map_err(|_| why.as_str()),
            journal::JOURNAL,
            if record_broken {
                Unusable::NoRecord
            } else {
                Unusable::NoState
            },
            &[&project.dir],
            broken,
        );
        assert!(started.elapsed() < Duration::from_secs(5), "{broken}");
        assert_eq!(snapshot(&project.dir), left);

        // A save without a state folder lands whole; a failure is undone.
        let fresh = Project::new(&format!("stateless-save-{broken}"));
        let (writes, removals) = fresh.save(Shape::Edits);
        commit(
            &fresh.lock(),
            &writes,
            &removals,
            None,
            &mut Disk::counting(),
        )
        .unwrap();
        assert_eq!(snapshot(&fresh.dir), after, "{broken}");
        let failing = Project::new(&format!("stateless-fail-{broken}"));
        let (writes, removals) = failing.save(Shape::Edits);
        commit(
            &failing.lock(),
            &writes,
            &removals,
            None,
            &mut Disk::failing(vec![5]),
        )
        .unwrap_err();
        assert_eq!(snapshot(&failing.dir), before, "{broken}");
        std::fs::remove_file(&project.state).ok();
    }

    // A killed save that wrote no record leaves a journal: only reported.
    let project = Project::new("stateless-killed");
    let (writes, removals) = project.save(Shape::Edits);
    commit(
        &project.lock(),
        &writes,
        &removals,
        None,
        &mut Disk::stopping_after(5),
    )
    .unwrap_err();
    refused(
        &project,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&project.dir],
        "unrecorded",
    );
}

/// A record is created with `create_new` semantics: a file already at its
/// path is never replaced, and the save goes on unrecorded.
#[test]
fn a_record_never_replaces_a_file_at_its_path() {
    let project = Project::new("no-replace");
    let state = project.state();
    let record = Record::new("1-2", b"journal", &project.dir);
    let path = state.path().join("1-2.json");
    std::fs::write(&path, "someone's\n").unwrap();
    assert!(state.create(&record, &mut Disk::counting()).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "someone's\n");
    std::fs::remove_file(&path).unwrap();
}

/// Records whose folder is gone, or too old, and stray temp files are
/// pruned; a current record of an existing folder stays.
/// Pruning: a record older than `MAX_AGE` goes; one whose folder is missing
/// goes only once older than `GONE_AGE` too (the folder may come back). A
/// temp file a killed write left goes once its process is gone, and after a
/// day when it looks alive.
#[test]
fn stale_records_are_pruned() {
    let project = Project::new("prune");
    let state = project.state();
    let aged = |token: &str, dir: &Path, age: Duration| {
        let mut record = serde_json::to_value(Record::new(token, b"x", dir)).unwrap();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .saturating_sub(age)
            .as_millis();
        record["created_at_ms"] = u64::try_from(ms).unwrap().into();
        let path = state.path().join(format!("{token}.json"));
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        serde_json::from_value::<Record>(record).unwrap()
    };
    let day = Duration::from_secs(24 * 3600);
    let gone = project.dir.join("gone");
    let current = aged("1-1", &project.dir, Duration::ZERO);
    let away_a_while = aged("1-2", &gone, state::GONE_AGE - day);
    aged("1-3", &gone, state::GONE_AGE + day);
    aged("1-4", &project.dir, state::MAX_AGE + day);
    let kept_here = aged("1-5", &project.dir, state::GONE_AGE + day);

    let me = std::process::id();
    let dead = state.path().join(".1-6.json.999999999-1.tmp");
    std::fs::write(&dead, "").unwrap();
    let alive = state.path().join(format!(".1-7.json.{me}-1.tmp"));
    std::fs::write(&alive, "").unwrap();
    let alive_old = state.path().join(format!(".1-8.json.{me}-2.tmp"));
    std::fs::File::create(&alive_old)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - 2 * day)
        .unwrap();
    assert_eq!(state.records().len(), 5);

    state.prune();
    let mut left = state.records();
    left.sort_by(|a, b| a.token.cmp(&b.token));
    assert_eq!(left, [current, away_a_while, kept_here]);
    assert!(!dead.exists(), "a temp of a process that is gone is kept");
    assert!(alive.exists(), "a temp of a running process is pruned");
    assert!(!alive_old.exists());
}

/// A journal larger than any save writes is refused unread: from its size
/// when the filesystem reports it, and after at most one byte past the cap
/// when it doesn't.
#[test]
fn an_oversized_journal_is_refused_without_being_read_whole() {
    let mut endless = Endless(0);
    assert!(
        journal::read_at_most(&mut endless, journal::MAX_JOURNAL)
            .unwrap()
            .is_none()
    );
    assert!(endless.0 <= journal::MAX_JOURNAL + 1, "{}", endless.0);

    let project = Project::new("oversized");
    let path = project.dir.join(journal::JOURNAL);
    std::fs::File::create(&path)
        .unwrap()
        .set_len(1 << 30)
        .unwrap();
    let started = std::time::Instant::now();
    let recovery = project.recover(&mut Disk::counting());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    let blocked = recovery.blocked.unwrap();
    assert_eq!(blocked.report.unwrap().cause, Unusable::TooLarge);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 1 << 30);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        names(&snapshot(&project.dir)),
        ["flusso.toml", "orders.schema.yml", "users.schema.yml"]
    );
}

/// A reader of endless bytes that counts what it hands out.
struct Endless(u64);

impl std::io::Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(b' ');
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
}

/// The reviewer's case: a link at the lock file's path made every page load
/// create the file it points at. The lock, the journals and the staging file
/// are never followed: the lock is refused naming the file, a journal or
/// staging link is reported, and the file it points at is never made,
/// read into a decision, or written.
#[cfg(unix)]
#[test]
fn a_link_at_the_lock_or_a_journal_path_is_never_followed() {
    let project = Project::new("links");
    let outside = victim(&project);
    let created = outside.dir.join("created-by-lock");
    std::os::unix::fs::symlink(&created, project.dir.join(lock::LOCK)).unwrap();
    let refused_lock = ProjectLock::acquire(&project.dir, Duration::ZERO).unwrap_err();
    assert!(
        matches!(&refused_lock, LockError::NotAFile(p) if p.ends_with(lock::LOCK)),
        "{refused_lock:?}"
    );
    assert!(refused_lock.to_string().contains(lock::LOCK));
    assert!(!created.exists());
    std::fs::remove_file(project.dir.join(lock::LOCK)).unwrap();

    let out = snapshot(&outside.dir);
    for name in [journal::JOURNAL, journal::DONE, journal::STAGING] {
        // The victim holds a journal recorded for this very folder.
        let bytes = journal_body(&serde_json::json!({
            "writes": [{ "dest": "users.schema.yml", "before": digest("users\n"), "after": digest("x") }]
        }));
        project.record(&bytes, &project.dir, false);
        std::fs::write(outside.dir.join("journal"), &bytes).unwrap();
        std::os::unix::fs::symlink(outside.dir.join("journal"), project.dir.join(name)).unwrap();
        let report = refused(&project, name, Unusable::Unreadable, &[&project.dir], name);
        assert!(report.files.is_empty());
        std::fs::remove_file(project.dir.join(name)).unwrap();
        std::fs::remove_file(outside.dir.join("journal")).unwrap();
    }
    assert_eq!(snapshot(&outside.dir), out);

    // A FIFO at the journal's path is not waited on.
    let fifo = project.dir.join(journal::JOURNAL);
    if std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .is_ok_and(|s| s.success())
    {
        let blocked = project.recover(&mut Disk::counting()).blocked.unwrap();
        assert_eq!(blocked.report.unwrap().cause, Unusable::Unreadable);
        std::fs::remove_file(&fifo).unwrap();
    }

    // A save never writes its journal through a link either.
    std::os::unix::fs::symlink(
        outside.dir.join("keep.yml"),
        project.dir.join(journal::STAGING),
    )
    .unwrap();
    project
        .commit(Shape::Edits, &mut Disk::counting())
        .unwrap_err();
    assert_eq!(snapshot(&outside.dir), out);
}

/// A unique folder holding a project (`proj`) and a folder beside it
/// (`shared`) a save writes into; removed on drop.
struct Beside(PathBuf);

impl Drop for Beside {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

impl Beside {
    fn new(tag: &str) -> (Self, Project) {
        let root =
            std::env::temp_dir().join(format!("flusso-design-beside-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("proj")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("proj/flusso.toml"), "config\n").unwrap();
        std::fs::write(root.join("proj/users.schema.yml"), "users\n").unwrap();
        std::fs::write(root.join("shared/existing.schema.yml"), "shared\n").unwrap();
        let project = Project {
            dir: root.join("proj"),
            state: root.with_extension("state"),
        };
        std::fs::remove_dir_all(&project.state).ok();
        (Self(root), project)
    }

    /// A save that replaces a file in `../shared`, creates one two new
    /// folders deep there, and rewrites the project's own files.
    fn commit(&self, project: &Project, disk: &mut Disk) -> Result<()> {
        let w = |dest: PathBuf, text: &str| Write {
            dest,
            text: text.into(),
            expect: Expect::AsFound,
        };
        let writes = [
            w(self.0.join("shared/existing.schema.yml"), "shared 2\n"),
            w(self.0.join("shared/new/deep/x.schema.yml"), "x\n"),
            w(project.dir.join("users.schema.yml"), "users 2\n"),
            w(project.dir.join("flusso.toml"), "config 2\n"),
        ];
        commit(&project.lock(), &writes, &[], Some(&project.state()), disk).map(|_| ())
    }
}

/// A save that writes beside the project (`../shared/…`, which a save
/// allows), killed after any step, and its recovery killed after any step,
/// ends with the project and the shared folder both before or after, and no
/// record left. Moved, the project is reported instead.
#[test]
fn a_save_beside_the_project_recovers_to_before_or_after() {
    let (before, after, steps) = {
        let (root, project) = Beside::new("ref");
        let before = snapshot(&root.0);
        let mut disk = Disk::counting();
        root.commit(&project, &mut disk).unwrap();
        (before, snapshot(&root.0), disk.steps())
    };
    assert!(after.contains_key(Path::new("shared/new/deep/x.schema.yml")));
    for n in 0..=steps {
        let expected = {
            let (root, project) = Beside::new(&format!("{n}"));
            root.commit(&project, &mut Disk::stopping_after(n)).ok();
            let recovery = project.recover(&mut Disk::counting());
            assert!(recovery.blocked.is_none(), "killed after {n}: {recovery:?}");
            assert!(project.records().is_empty(), "killed after {n}");
            snapshot(&root.0)
        };
        assert!(
            expected == before || expected == after,
            "killed after {n}: {:?}",
            names(&expected)
        );
        for m in 0.. {
            let (root, project) = Beside::new(&format!("{n}-{m}"));
            root.commit(&project, &mut Disk::stopping_after(n)).ok();
            let mut cut = Disk::stopping_after(m);
            project.recover(&mut cut);
            let finished = !cut.stopped();
            let again = project.recover(&mut Disk::counting());
            assert!(again.blocked.is_none(), "{n}/{m}: {again:?}");
            assert!(project.records().is_empty(), "{n}/{m}");
            assert_eq!(
                snapshot(&root.0),
                expected,
                "save killed after {n}, recovery after {m}"
            );
            if finished {
                break;
            }
        }
    }

    // Killed once the shared file is replaced, then moved where `../shared`
    // is another folder: the record names the old folder, so nothing is
    // touched, inside the project or beside it.
    let (root, project) = Beside::new("moved");
    let mut n = 1;
    loop {
        let (probe, probed) = Beside::new("probe");
        probe.commit(&probed, &mut Disk::stopping_after(n)).ok();
        if std::fs::read_to_string(probe.0.join("shared/existing.schema.yml")).unwrap()
            == "shared 2\n"
        {
            break;
        }
        n += 1;
    }
    root.commit(&project, &mut Disk::stopping_after(n))
        .unwrap_err();
    std::fs::create_dir_all(root.0.join("elsewhere/shared")).unwrap();
    std::fs::write(
        root.0.join("elsewhere/shared/existing.schema.yml"),
        "other\n",
    )
    .unwrap();
    let moved = Project {
        dir: root.0.join("elsewhere/proj"),
        state: project.state.clone(),
    };
    std::fs::rename(&project.dir, &moved.dir).unwrap();
    let left = snapshot(&root.0);
    let recovery = moved.recover(&mut Disk::counting());
    let blocked = recovery.blocked.expect("blocked");
    assert_eq!(blocked.report.map(|r| r.cause), Some(Unusable::OtherFolder));
    assert_eq!(snapshot(&root.0), left);
    std::mem::forget(moved);
}

/// A backup that holds other text than the file's from before the save is
/// never deleted or put back: when the rollback needs it, recovery blocks;
/// when it doesn't, the backup stays and is reported as a leftover.
#[test]
fn a_backup_not_holding_the_text_before_is_left_alone() {
    let (before, _, _) = reference(Shape::Swap);
    let find = |project: &Project, file: &str| {
        backups(&project.dir)
            .into_iter()
            .find(|p| p.to_string_lossy().contains(&format!(".{file}.")))
            .unwrap()
    };

    // users.schema.yml was replaced: its backup is needed, and damaged.
    let project = half_swapped("bad-backup");
    let users_backup = find(&project, "users.schema.yml");
    std::fs::write(&users_backup, "not users\n").unwrap();
    let left = snapshot(&project.dir);
    let blocked = project.recover(&mut Disk::counting()).blocked.unwrap();
    assert_eq!(blocked.problems[0].kind, ProblemKind::BackupMissing);
    assert_eq!(snapshot(&project.dir), left);

    // orders.schema.yml was not replaced: its backup is not needed.
    let project = half_swapped("stray-backup");
    let orders_backup = find(&project, "orders.schema.yml");
    std::fs::remove_file(&orders_backup).unwrap();
    std::fs::write(&orders_backup, "not orders\n").unwrap();
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    assert_eq!(
        std::fs::read_to_string(&orders_backup).unwrap(),
        "not orders\n"
    );
    assert_eq!(strays(&[project.dir.clone()]), [orders_backup.clone()]);
    std::fs::remove_file(&orders_backup).unwrap();
    assert_eq!(snapshot(&project.dir), before);
}

/// A save file of another save (another token) beside a journal's files is
/// not the journal's: it stays, and is reported.
#[test]
fn a_save_file_another_save_named_is_left_alone() {
    let (before, _, _) = reference(Shape::Swap);
    let project = half_swapped("other-token");
    let other = project.dir.join(".users.schema.yml.flusso-save.9-9.0.bak");
    std::fs::write(&other, "users\n").unwrap();
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    assert!(other.exists());
    std::fs::remove_file(&other).unwrap();
    assert_eq!(snapshot(&project.dir), before);
}

/// Finding the commit mark unflushed, the save keeps its backups and leaves
/// the rest to recovery: which completes it, or, when a power cut took the
/// mark (the record back to not committed), rolls it back from those
/// backups.
#[test]
fn an_unflushed_commit_mark_keeps_the_backups_for_recovery() {
    let (before, after, _) = reference(Shape::MoveAndRemove);
    let point = commit_point(Shape::MoveAndRemove);
    for lost in [false, true] {
        let project = Project::new(&format!("unflushed-{lost}"));
        project
            .commit(Shape::MoveAndRemove, &mut Disk::failing_sync_from(point))
            .unwrap();
        let journal = project.dir.join(journal::JOURNAL);
        assert!(journal.exists());
        assert!(project.records()[0].committed);
        assert_eq!(
            backups(&project.dir).len(),
            3,
            "{:?}",
            names(&snapshot(&project.dir))
        );
        if lost {
            project.record(&std::fs::read(&journal).unwrap(), &project.dir, false);
        }
        let recovery = project.recover(&mut Disk::counting());
        assert!(recovery.blocked.is_none(), "{recovery:?}");
        assert!(project.records().is_empty());
        assert_eq!(
            snapshot(&project.dir),
            if lost { before.clone() } else { after.clone() }
        );
    }
}

/// Do every step by hand exactly as the blocked page lists it.
fn follow(steps: &[HandStep]) {
    for step in steps {
        match step {
            HandStep::Copy { from, to } => {
                std::fs::copy(from, to).unwrap_or_else(|e| panic!("{step:?}: {e}"));
            }
            HandStep::Delete(path) => {
                std::fs::remove_file(path).unwrap_or_else(|e| panic!("{step:?}: {e}"));
            }
            HandStep::RemoveDir(dir) => match std::fs::remove_dir(dir) {
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                other => other.unwrap_or_else(|e| panic!("{step:?}: {e}")),
            },
        }
    }
}

/// `snapshot` with `file` (relative) holding `text` instead.
fn with(snapshot: &Snapshot, file: &str, text: Option<&[u8]>) -> Snapshot {
    let mut out = snapshot.clone();
    match text {
        Some(text) => out.insert(PathBuf::from(file), Some(text.to_vec())),
        None => out.remove(Path::new(file)),
    };
    out
}

/// Reload after following a way out: recovery goes through, nothing of the
/// save is left (no journal, temp file, backup or record), and the project
/// is exactly `expected`.
fn reloads_to(project: &Project, expected: &Snapshot, label: &str) {
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{label}: {recovery:?}");
    assert!(project.records().is_empty(), "{label}: a record is left");
    let now = snapshot(&project.dir);
    assert_eq!(names(&now), names(expected), "{label}");
    assert_eq!(&now, expected, "{label}");
}

/// The recovery of `project`, blocked going `direction`, with its one
/// problem, on `file`, of `kind`.
fn blocked_on(
    project: &Project,
    direction: Direction,
    file: &str,
    kind: ProblemKind,
) -> (Blocked, Problem) {
    let left = snapshot(&project.dir);
    let mut blocked = project
        .recover(&mut Disk::counting())
        .blocked
        .expect("blocked");
    assert_eq!(blocked.direction, Some(direction), "{blocked:?}");
    assert_eq!(blocked.problems.len(), 1, "{blocked:?}");
    let problem = blocked.problems.remove(0);
    assert_eq!(
        problem.path,
        project.lock().base().join(file),
        "{problem:?}"
    );
    assert_eq!(problem.kind, kind, "{problem:?}");
    assert_eq!(snapshot(&project.dir), left, "recovery changed a file");
    (blocked, problem)
}

fn edit_in_place(path: &Path, text: &str) {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

/// Rolling back, a backup gone: no copy of the text before is left, so the
/// page says to put it back by hand; kept, the file holds what the save wrote
/// and every other file goes back.
#[test]
fn rolling_back_without_a_backup_either_way_ends_where_the_page_says() {
    let (before, _, _) = reference(Shape::Swap);
    let setup = |tag: &str| {
        let project = half_swapped(tag);
        for backup in backups(&project.dir) {
            if backup.to_string_lossy().contains(".users.schema.yml.") {
                std::fs::remove_file(backup).unwrap();
            }
        }
        project
    };

    let project = setup("back-missing-fix");
    let (_, problem) = blocked_on(
        &project,
        Direction::RollBack,
        "users.schema.yml",
        ProblemKind::BackupMissing,
    );
    assert_eq!((&problem.fix, &problem.before_copy), (&None, &None));
    std::fs::write(project.dir.join("users.schema.yml"), "users\n").unwrap();
    reloads_to(&project, &before, "put back by hand");

    let project = setup("back-missing-keep");
    let (blocked, _) = blocked_on(
        &project,
        Direction::RollBack,
        "users.schema.yml",
        ProblemKind::BackupMissing,
    );
    follow(&blocked.keep.expect("a way to keep it"));
    reloads_to(
        &project,
        &with(&before, "users.schema.yml", Some(b"orders\n")),
        "kept",
    );
}

/// A file that can't be read, either way: no way to keep it is offered;
/// made readable, the designer carries on.
#[test]
fn an_unreadable_file_either_way_carries_on_once_readable() {
    use std::os::unix::fs::PermissionsExt;
    let (before, _, _) = reference(Shape::Swap);
    let (_, after, _) = reference(Shape::Edits);
    let point = commit_point(Shape::Edits);
    let cases = [
        (
            half_swapped("unreadable-back"),
            Direction::RollBack,
            &before,
        ),
        (
            {
                let project = Project::new("unreadable-forward");
                project
                    .commit(Shape::Edits, &mut Disk::stopping_after(point))
                    .ok();
                project
            },
            Direction::Finish,
            &after,
        ),
    ];
    for (project, direction, expected) in cases {
        let file = project.dir.join("users.schema.yml");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&file).is_ok() {
            return; // Run as root: nothing is unreadable.
        }
        let (blocked, problem) = blocked_on(
            &project,
            direction,
            "users.schema.yml",
            ProblemKind::Unreadable,
        );
        assert!(
            blocked.keep.is_none() && problem.fix.is_none(),
            "{blocked:?}"
        );
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        reloads_to(&project, expected, &format!("{direction:?}"));
    }
}

/// A recovery step that fails, either way: no way to keep is offered; the
/// next recovery carries on.
#[test]
fn a_failed_step_either_way_carries_on_at_the_next_recovery() {
    let (before, _, _) = reference(Shape::Swap);
    let (_, after, _) = reference(Shape::Edits);
    let point = commit_point(Shape::Edits);
    let forward = Project::new("failed-forward");
    forward
        .commit(Shape::Edits, &mut Disk::stopping_after(point))
        .ok();
    for (project, direction, expected) in [
        (half_swapped("failed-back"), Direction::RollBack, &before),
        (forward, Direction::Finish, &after),
    ] {
        let blocked = project
            .recover(&mut Disk::failing(vec![1]))
            .blocked
            .expect("blocked");
        assert_eq!(blocked.direction, Some(direction));
        assert_eq!(blocked.problems[0].kind, ProblemKind::Failed);
        assert!(blocked.keep.is_none(), "{blocked:?}");
        reloads_to(&project, expected, &format!("{direction:?}"));
    }
}

/// A move into a new nested folder, killed after each step, in a copied
/// project (only reported): following the way back literally gives exactly
/// the project before, with no folder or temp file left; where there is no
/// way back, the way forward gives exactly the one after.
#[test]
fn following_a_reported_route_literally_gives_exactly_before_or_after() {
    let (before, after, steps) = reference(Shape::MoveAndRemove);
    for n in 0..=steps {
        let original = Project::new(&format!("route-{n}"));
        original
            .commit(Shape::MoveAndRemove, &mut Disk::stopping_after(n))
            .ok();
        let copy = original.beside("copy");
        copy_tree(&original.dir, &copy.dir);
        let recovery = copy.recover(&mut Disk::counting());
        let Some(blocked) = recovery.blocked else {
            let now = snapshot(&copy.dir);
            assert!(
                now == before || now == after,
                "killed after {n}: {:?}",
                names(&now)
            );
            continue;
        };
        let report = blocked.report.expect("a report");
        let (route, expected) = match report.route(true) {
            Some(route) => (route, &before),
            None => (report.route(false).expect("a way forward"), &after),
        };
        follow(&route);
        let now = snapshot(&copy.dir);
        assert_eq!(names(&now), names(expected), "killed after {n}");
        assert_eq!(&now, expected, "killed after {n}");
        assert!(copy.recover(&mut Disk::counting()).blocked.is_none());
    }
}

/// A temp file a rollback keeps (it holds other text) inside a folder the
/// save created is in the folders the leftover scan covers.
#[test]
fn a_leftover_in_a_created_folder_is_found_by_the_scan() {
    let project = Project::new("kept-in-new");
    let point = commit_point(Shape::MoveAndRemove);
    project
        .commit(Shape::MoveAndRemove, &mut Disk::stopping_after(point - 4))
        .unwrap_err();
    let temp = strays(&[project.dir.join("new/deep")])
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "tmp"))
        .expect("a temp file in the new folder");
    std::fs::write(&temp, "something else\n").unwrap();
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{recovery:?}");
    let folders: Vec<PathBuf> = recovery
        .settled
        .iter()
        .flat_map(|s| s.folders.clone())
        .collect();
    let base = project.lock().base().to_path_buf();
    assert_eq!(
        strays(&folders),
        [base.join(temp.strip_prefix(&project.dir).unwrap())]
    );
}

/// A save the designer could not record, or whose record was pruned, is
/// reported with the cause that says this designer has no record of it.
#[test]
fn an_unrecorded_or_pruned_save_has_no_record() {
    let unrecorded = Project::new("unrecorded");
    let (writes, removals) = unrecorded.save(Shape::Swap);
    commit(
        &unrecorded.lock(),
        &writes,
        &removals,
        None,
        &mut Disk::stopping_after(9),
    )
    .unwrap_err();
    refused(
        &unrecorded,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&unrecorded.dir],
        "unrecorded",
    );

    let pruned = half_swapped("pruned");
    for record in pruned.records() {
        pruned
            .state()
            .remove(&record.token, &mut Disk::counting())
            .unwrap();
    }
    refused(
        &pruned,
        journal::JOURNAL,
        Unusable::NoRecord,
        &[&pruned.dir],
        "pruned",
    );
}

/// Writing a record, or marking one committed, that fails on a full disk
/// leaves no temp file in the state folder.
#[test]
fn a_failing_private_write_leaves_no_temp() {
    let project = Project::new("full");
    let state = project.state();
    let record = Record::new("1-2", b"a", &project.dir);
    state
        .create(&record, &mut Disk::failing_private_writes())
        .unwrap_err();
    assert_eq!(std::fs::read_dir(state.path()).unwrap().count(), 0);
    state.create(&record, &mut Disk::counting()).unwrap();
    state
        .commit(&record, &mut Disk::failing_private_writes())
        .unwrap_err();
    let left: Vec<_> = std::fs::read_dir(state.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left, ["1-2.json"]);
}

/// `snapshot` with `file` (relative) holding `bytes`, and every folder on its
/// way there.
fn with_edit(snapshot: &Snapshot, file: &str, bytes: &[u8]) -> Snapshot {
    let mut out = with(snapshot, file, Some(bytes));
    for folder in Path::new(file).ancestors().skip(1) {
        if !folder.as_os_str().is_empty() {
            out.entry(folder.to_path_buf()).or_insert(None);
        }
    }
    out
}

/// Recover `project`: it settles (never blocks), nothing of the save is
/// left but the backups its notice names, each holding its file's text from
/// `before`, and the project is then `expected`. The files kept as another
/// program wrote them.
fn settles_keeping(
    project: &Project,
    expected: &Snapshot,
    before: &Snapshot,
    label: &str,
) -> Vec<Kept> {
    let recovery = project.recover(&mut Disk::counting());
    assert!(recovery.blocked.is_none(), "{label}: {recovery:?}");
    assert!(project.records().is_empty(), "{label}: a record is left");
    let kept: Vec<Kept> = recovery
        .settled
        .iter()
        .flat_map(|s| s.kept.clone())
        .collect();
    let base = project.lock().base().to_path_buf();
    let mut now = snapshot(&project.dir);
    for k in &kept {
        if let Some(backup) = &k.backup {
            let held = now
                .remove(backup.strip_prefix(&base).unwrap())
                .unwrap_or_else(|| panic!("{label}: the named backup {backup:?} is gone"));
            let file = k.path.strip_prefix(&base).unwrap();
            assert_eq!(held, before.get(file).cloned().flatten(), "{label}: {k:?}");
        }
    }
    assert_eq!(names(&now), names(expected), "{label}");
    assert_eq!(&now, expected, "{label}");
    kept
}

/// Rolling back, a file written since the kill is kept as it is, its backup
/// of the text before named; every other file goes back.
#[test]
fn a_file_edited_since_the_kill_is_kept_and_the_rest_rolled_back() {
    let (before, _, _) = reference(Shape::Swap);
    let project = half_swapped("edited");
    std::fs::write(project.dir.join("users.schema.yml"), "typed by hand\n").unwrap();
    let kept = settles_keeping(
        &project,
        &with(&before, "users.schema.yml", Some(b"typed by hand\n")),
        &before,
        "kept",
    );
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].path.ends_with("users.schema.yml"));
    assert!(
        kept[0].backup.is_some(),
        "the text before is kept and named"
    );
}

/// Rolling back, a file the save created and one it removed, each written
/// since: both kept, every other file back as before.
#[test]
fn rolling_back_keeps_a_created_or_removed_file_written_since() {
    let (before, _, _) = reference(Shape::MoveAndRemove);
    let point = commit_point(Shape::MoveAndRemove);
    for (file, text) in [
        ("new/deep/orders.schema.yml", "typed by hand\n"),
        ("orders.schema.yml", "made again\n"),
    ] {
        let project = Project::new(&format!("back-made-{}", file.len()));
        project
            .commit(Shape::MoveAndRemove, &mut Disk::stopping_after(point - 1))
            .unwrap_err();
        std::fs::write(project.dir.join(file), text).unwrap();
        let kept = settles_keeping(
            &project,
            &with_edit(&before, file, text.as_bytes()),
            &before,
            file,
        );
        assert!(
            kept.iter().any(|k| k.path.ends_with(file)),
            "{file}: {kept:?}"
        );
    }
}

/// Finishing a committed save, a file edited or deleted since, or one it
/// removed made again: kept as it is; everything else is as the save wrote
/// it, and the backups are gone.
#[test]
fn finishing_keeps_a_file_written_since_and_cleans_up() {
    let (_, after, _) = reference(Shape::Edits);
    let point = commit_point(Shape::Edits);
    for (n, edit) in [(point, true), (point + 1, true), (point, false)] {
        let label = format!("killed after {n}, edited: {edit}");
        let project = Project::new(&format!("forward-{n}-{edit}"));
        project
            .commit(Shape::Edits, &mut Disk::stopping_after(n))
            .ok();
        let file = project.dir.join("users.schema.yml");
        if edit {
            edit_in_place(&file, "# typed by hand\n");
        } else {
            std::fs::remove_file(&file).unwrap();
        }
        let now = std::fs::read(&file).ok();
        let kept = settles_keeping(
            &project,
            &with(&after, "users.schema.yml", now.as_deref()),
            &after,
            &label,
        );
        assert_eq!(kept.len(), 1, "{label}: {kept:?}");
        assert_eq!(kept[0].backup, None, "{label}");
    }

    let (_, after, _) = reference(Shape::MoveAndRemove);
    let point = commit_point(Shape::MoveAndRemove);
    let project = Project::new("forward-made");
    project
        .commit(Shape::MoveAndRemove, &mut Disk::stopping_after(point))
        .ok();
    std::fs::write(project.dir.join("orders.schema.yml"), "made again\n").unwrap();
    settles_keeping(
        &project,
        &with(&after, "orders.schema.yml", Some(b"made again\n")),
        &after,
        "made again",
    );
}

/// Killed anywhere before the commit point, then a file edited in place
/// (`>>`, or an editor that keeps hard links): the file is kept; a backup
/// hard-linked to it holds its bytes, not the text before, and goes, while a
/// backup that still holds the text before is kept and named.
#[test]
fn a_backup_edited_through_its_hard_link_is_never_named_as_the_text_before() {
    let (before, _, _) = reference(Shape::Edits);
    let point = commit_point(Shape::Edits);
    let mut linked = 0;
    for n in 1..point {
        let project = Project::new(&format!("linked-{n}"));
        project
            .commit(Shape::Edits, &mut Disk::stopping_after(n))
            .unwrap_err();
        if !project.dir.join(journal::JOURNAL).exists() {
            continue;
        }
        let file = project.dir.join("users.schema.yml");
        #[cfg(unix)]
        let shared = backups(&project.dir).iter().any(|b| {
            use std::os::unix::fs::MetadataExt as _;
            b.metadata().unwrap().ino() == file.metadata().unwrap().ino()
        });
        edit_in_place(&file, "# typed by hand\n");
        let edited = std::fs::read(&file).unwrap();
        let kept = settles_keeping(
            &project,
            &with(&before, "users.schema.yml", Some(&edited)),
            &before,
            &format!("killed after {n}"),
        );
        #[cfg(unix)]
        if shared {
            linked += 1;
            assert!(
                kept.iter().all(|k| k.backup.is_none()),
                "killed after {n}: {kept:?}"
            );
        }
    }
    #[cfg(unix)]
    assert!(linked > 0, "no kill point left a hard-linked backup");
}

/// Every file a save of `shape` writes or removes, relative.
fn touched(project: &Project, shape: Shape) -> Vec<String> {
    let (writes, removals) = project.save(shape);
    let rel = |p: &Path| p.strip_prefix(&project.dir).unwrap().display().to_string();
    writes
        .iter()
        .map(|w| rel(&w.dest))
        .chain(removals.iter().map(|r| rel(&r.path)))
        .collect()
}

const EDITOR: &[u8] = b"# the editor's\n";

/// Another program writes a file a save touches right after any step of the
/// save (in place, or by rename): the save ends as before it or after it,
/// plus that write, never losing it, and leaves nothing behind. In process:
/// refused as [`Changed`] when it undid itself. Killed right after the write
/// or one step later: recovery settles the same way, never blocking, keeping
/// only the backups its notice names.
#[test]
fn an_editor_write_after_any_step_is_kept_and_nothing_is_left() {
    for shape in SHAPES {
        let (before, after, steps) = reference(shape);
        let files = touched(&Project::new("files"), shape);
        for file in &files {
            for by_rename in [false, true] {
                for n in 1..=steps {
                    for kill in [None, Some(n), Some(n + 1)] {
                        let label =
                            format!("{shape:?} {file} rename:{by_rename} after {n} kill:{kill:?}");
                        let project = Project::new(&format!(
                            "editor-{shape:?}-{}-{by_rename}-{n}-{kill:?}",
                            file.len()
                        ));
                        let edit = Edit {
                            after_step: n,
                            path: project.dir.join(file),
                            bytes: EDITOR.to_vec(),
                            by_rename,
                            done: false,
                        };
                        let mut disk = Disk::counting().with_edit(edit);
                        if let Some(k) = kill {
                            disk = disk.and_stopping_after(k);
                        }
                        let saved = project.commit(shape, &mut disk);
                        let edited = disk.edits().iter().all(|e| e.done);
                        let (b, a) = if edited {
                            (
                                with_edit(&before, file, EDITOR),
                                with_edit(&after, file, EDITOR),
                            )
                        } else {
                            (before.clone(), after.clone())
                        };
                        if kill.is_none() || !disk.stopped() {
                            assert!(project.records().is_empty(), "{label}: a record is left");
                            let now = snapshot(&project.dir);
                            match saved {
                                Ok(()) => assert_eq!(now, a, "{label}"),
                                Err(e) => {
                                    assert!(e.is::<Changed>(), "{label}: {e:#}");
                                    assert_eq!(now, b, "{label}");
                                }
                            }
                            continue;
                        }
                        let recovery = project.recover(&mut Disk::counting());
                        assert!(recovery.blocked.is_none(), "{label}: {recovery:?}");
                        assert!(project.records().is_empty(), "{label}: a record is left");
                        let base = project.lock().base().to_path_buf();
                        let mut now = snapshot(&project.dir);
                        for k in recovery.settled.iter().flat_map(|s| &s.kept) {
                            if let Some(backup) = &k.backup {
                                now.remove(backup.strip_prefix(&base).unwrap())
                                    .unwrap_or_else(|| {
                                        panic!("{label}: {backup:?} named, not there")
                                    });
                            }
                        }
                        assert!(now == b || now == a, "{label}: {:?}", names(&now));
                    }
                }
            }
        }
    }
}

/// The review's case: after the first file is renamed into place, another
/// program rewrites it and the next one. The save refuses, naming both;
/// both keep the other program's bytes, the rest is as before, nothing is
/// left. Killed at any step of that undo, recovery settles the same way.
#[test]
fn an_undo_meeting_files_written_meanwhile_keeps_them_all() {
    let (before, _, steps) = reference(Shape::Edits);
    // record, journal (2), temps (2), backups (2), then users' rename.
    let renamed = 8;
    let expected = with(
        &with(&before, "users.schema.yml", Some(EDITOR)),
        "flusso.toml",
        Some(EDITOR),
    );
    let edits = |project: &Project| {
        ["users.schema.yml", "flusso.toml"].map(|f| Edit {
            after_step: renamed,
            path: project.dir.join(f),
            bytes: EDITOR.to_vec(),
            by_rename: false,
            done: false,
        })
    };
    let project = Project::new("undo-meets");
    let mut disk = Disk::counting();
    for e in edits(&project) {
        disk = disk.with_edit(e);
    }
    let err = project.commit(Shape::Edits, &mut disk).unwrap_err();
    assert_eq!(
        std::fs::read(project.dir.join("users.schema.yml")).unwrap(),
        EDITOR,
        "the first rename is the one edited"
    );
    let Changed(files) = err.downcast::<Changed>().expect("refused as changed");
    let mut named: Vec<String> = files
        .iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    named.sort();
    assert_eq!(named, ["flusso.toml", "users.schema.yml"]);
    assert_eq!(snapshot(&project.dir), expected);
    assert!(project.records().is_empty());

    for k in renamed..=steps + 4 {
        let project = Project::new(&format!("undo-meets-{k}"));
        let mut disk = Disk::counting().and_stopping_after(k);
        for e in edits(&project) {
            disk = disk.with_edit(e);
        }
        if project.commit(Shape::Edits, &mut disk).is_ok() || !disk.stopped() {
            continue;
        }
        settles_keeping(
            &project,
            &expected,
            &before,
            &format!("undo killed after {k}"),
        );
    }
}
