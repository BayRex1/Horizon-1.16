//! A bounded artwork worker pool. Drawing a slot performs no filesystem or network reads.
use super::*;
use horizon_launcher::content_icons::{Icon, Resolver};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{mpsc, Arc},
};

const ART_WORKERS: usize = 4;

enum State {
    Pending(u64),
    Ready(Option<PathBuf>),
}
struct Request {
    icon: Icon,
    id: u64,
}
struct Answer {
    icon: Icon,
    id: u64,
    path: Option<PathBuf>,
}

#[derive(Default)]
pub(super) struct ContentArt {
    states: HashMap<Icon, State>,
    queue: Option<mpsc::Sender<Request>>,
    answers: Option<mpsc::Receiver<Answer>>,
    next: u64,
}

impl ContentArt {
    pub(super) fn get(&mut self, icon: Icon) -> Option<PathBuf> {
        if let Some(state) = self.states.get(&icon) {
            return match state {
                State::Ready(path) => path.clone(),
                State::Pending(_) => None,
            };
        }
        self.start_workers();
        self.next += 1;
        self.states.insert(icon.clone(), State::Pending(self.next));
        if self.queue.as_ref().is_none_or(|queue| {
            queue
                .send(Request {
                    icon: icon.clone(),
                    id: self.next,
                })
                .is_err()
        }) {
            self.states.insert(icon, State::Ready(None));
        }
        None
    }

    fn start_workers(&mut self) {
        if self.queue.is_some() {
            return;
        }
        let (send, receive) = mpsc::channel::<Request>();
        let (answers, completed) = mpsc::channel();
        let receive = Arc::new(Mutex::new(receive));
        for _ in 0..ART_WORKERS {
            let receive = receive.clone();
            let answers = answers.clone();
            let home = backend::game_home();
            std::thread::spawn(move || {
                let mut resolver = Resolver::new(home);
                loop {
                    let Ok(request) = receive.lock().map(|queue| queue.recv()) else {
                        break;
                    };
                    let Ok(request) = request else {
                        break;
                    };
                    let path = match resolver.resolve(&request.icon) {
                        Ok(path) => path,
                        Err(error) => {
                            eprintln!("Horizon artwork: {error}");
                            None
                        }
                    };
                    if answers
                        .send(Answer {
                            icon: request.icon,
                            id: request.id,
                            path,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        self.queue = Some(send);
        self.answers = Some(completed);
    }

    pub(super) fn drain(&mut self) -> bool {
        let mut changed = false;
        let Some(answers) = &self.answers else {
            return false;
        };
        while let Ok(answer) = answers.try_recv() {
            if matches!(self.states.get(&answer.icon), Some(State::Pending(id)) if *id == answer.id)
            {
                self.states.insert(answer.icon, State::Ready(answer.path));
                changed = true;
            }
        }
        changed
    }

    pub(super) fn retry_missing(&mut self) {
        self.states
            .retain(|_, state| matches!(state, State::Ready(Some(_))));
    }
}

impl Launcher {
    pub(super) fn instance_art(&mut self, id: &InstanceId, side: f32) -> Div {
        let Some(target) = self.instance_target(id) else {
            return blocks::slot(None, "?", side, &self.theme);
        };
        let icon = match id {
            InstanceId::Registry(id) => {
                Icon::Project(horizon_launcher::browser::SourceId::Horizon, id.clone())
            }
            _ => Icon::Instance {
                home: target.home,
                work: self.instance_work(id),
                build: self.selected().cloned(),
            },
        };
        let path = self.content_art.get(icon);
        blocks::slot(
            path.map(gpui::ImageSource::from),
            &target.name,
            side,
            &self.theme,
        )
    }

    pub(super) fn content_art_slot(&mut self, icon: Icon, title: &str, side: f32) -> Div {
        let path = self.content_art.get(icon);
        blocks::slot(path.map(gpui::ImageSource::from), title, side, &self.theme)
    }

    pub(super) fn remote_art(&mut self, url: &str, size: u32) -> Option<PathBuf> {
        self.content_art.get(Icon::Url(url.to_owned(), size))
    }
}
