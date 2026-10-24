//! A fixed number of background mod checks; queued work from abandoned pages is skipped.
use super::mods_page::{Checking, Event};
use super::*;
use horizon_launcher::browser::{self, java, Project};
use std::sync::{atomic::AtomicU64, Arc};

const CHECK_WORKERS: usize = 2;

pub(super) struct Request {
    project: Project,
    everything: bool,
    generation: u64,
}

impl Launcher {
    pub(super) fn mods_check(&mut self, found: Vec<Project>) {
        let mods = &mut self.mods;
        let todo = found
            .into_iter()
            .filter(|project| project.source != browser::SourceId::Horizon)
            .filter(|project| project.kind == browser::Kind::Mod && project.support().installable())
            .filter(|project| {
                !mods
                    .checks
                    .contains_key(&(project.source, project.id.clone()))
            })
            .collect::<Vec<_>>();
        if todo.is_empty() {
            return;
        }
        let generation = mods.search;
        let active = mods.check_generation.clone();
        let sender = mods
            .check_jobs
            .get_or_insert_with(|| start_workers(mods.tx.clone(), active));
        for project in todo {
            let key = (project.source, project.id.clone());
            mods.checks.insert(key.clone(), Checking::Running);
            let request = Request {
                project,
                generation,
                everything: mods.everything || mods.browse.filters.other_version(),
            };
            if sender.send(request).is_err() {
                mods.checks.insert(
                    key,
                    Checking::Failed(
                        "The background checker stopped. Reopen the launcher to retry.".into(),
                    ),
                );
            }
        }
    }
}

fn start_workers(events: Sender<Event>, active: Arc<AtomicU64>) -> Sender<Request> {
    let (send, receive) = channel::<Request>();
    let receive = Arc::new(Mutex::new(receive));
    for _ in 0..CHECK_WORKERS {
        let (receive, events, active) = (receive.clone(), events.clone(), active.clone());
        let home = backend::game_home();
        std::thread::spawn(move || loop {
            let Ok(request) = receive.lock().map(|queue| queue.recv()) else {
                break;
            };
            let Ok(request) = request else {
                break;
            };
            if request.generation != active.load(Ordering::Relaxed) {
                continue;
            }
            let project = request.project;
            let result = browser::source(project.source)
                .ok_or_else(|| format!("{} isn't set up.", project.source.label()))
                .and_then(|source| {
                    java::check_project(&home, &*source, &project, request.everything, &|_| {})
                })
                .map(|(version, check)| {
                    let _ = events.send(Event::Loaded(mods_project::Loaded::Checked(
                        (project.source, project.id.clone()),
                        version.id,
                        check.verdict,
                    )));
                    Box::new(check)
                });
            if events
                .send(Event::Checked(
                    request.generation,
                    (project.source, project.id),
                    result,
                ))
                .is_err()
            {
                break;
            }
        });
    }
    send
}
