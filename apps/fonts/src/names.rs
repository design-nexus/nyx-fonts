//! Each family's name drawn in itself, for the list: the files come from a few
//! worker threads in the order the rows asked; a row that scrolls away or shows
//! another font takes its request back, so the rows on screen fill in first.

use crate::catalog::{FontEntry, PreviewFile, Source};
use crate::preview;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

pub type Key = (Source, String, String);
type Done = Box<dyn FnOnce(Option<preview::Loaded>)>;

const WORKERS: usize = 4;

struct Shared {
    jobs: Mutex<Vec<(Key, FontEntry)>>,
    ready: Condvar,
}

struct Pool {
    shared: Arc<Shared>,
    waiting: HashMap<Key, Vec<Done>>,
    /// Files already fetched this run, so rebinding a row is immediate.
    files: HashMap<Key, Option<PathBuf>>,
}

thread_local! {
    static POOL: RefCell<Option<Pool>> = const { RefCell::new(None) };
}

pub fn key(e: &FontEntry) -> Key {
    (e.source, e.id.clone(), e.version.clone())
}

fn start() -> Arc<Shared> {
    let shared = Arc::new(Shared { jobs: Mutex::new(Vec::new()), ready: Condvar::new() });
    let (tx, rx) = async_channel::unbounded::<(Key, Option<PathBuf>)>();
    for _ in 0..WORKERS {
        let (shared, tx) = (shared.clone(), tx.clone());
        std::thread::spawn(move || {
            loop {
                let (k, entry) = {
                    let mut jobs = shared.jobs.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if !jobs.is_empty() {
                            break jobs.remove(0);
                        }
                        jobs = shared.ready.wait(jobs).unwrap_or_else(|e| e.into_inner());
                    }
                };
                let file = preview::fetch_name(&entry).ok();
                if tx.send_blocking((k, file)).is_err() {
                    return;
                }
            }
        });
    }
    gtk::glib::spawn_future_local(async move {
        while let Ok((k, file)) = rx.recv().await {
            let done = POOL.with(|p| {
                let mut p = p.borrow_mut();
                let p = p.as_mut()?;
                p.files.insert(k.clone(), file.clone());
                p.waiting.remove(&k)
            });
            let loaded = file.and_then(|f| preview::load(&f, preview::Fallback::Text));
            for d in done.into_iter().flatten() {
                d(loaded.clone());
            }
        }
    });
    shared
}

/// Call `done` with the font map that draws `entry`'s name, once it's here. Local
/// files are read straight away; a request taken back with [`forget`] never answers.
pub fn load(entry: &FontEntry, done: impl FnOnce(Option<preview::Loaded>) + 'static) {
    if let PreviewFile::Local(p) = &entry.preview {
        done(preview::load(p, preview::Fallback::Text));
        return;
    }
    let k = key(entry);
    let known = POOL.with(|p| p.borrow().as_ref().and_then(|p| p.files.get(&k).cloned()));
    if let Some(file) = known {
        done(file.and_then(|f| preview::load(&f, preview::Fallback::Text)));
        return;
    }
    POOL.with(|p| {
        let mut p = p.borrow_mut();
        let pool = p.get_or_insert_with(|| Pool { shared: start(), waiting: HashMap::new(), files: HashMap::new() });
        let first = !pool.waiting.contains_key(&k);
        pool.waiting.entry(k.clone()).or_default().push(Box::new(done));
        let mut jobs = pool.shared.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if first {
            jobs.push((k, entry.clone()));
        }
        drop(jobs);
        pool.shared.ready.notify_one();
    });
}

/// Take back the requests for a font no row shows any more.
pub fn forget(k: &Key) {
    POOL.with(|p| {
        if let Some(pool) = p.borrow_mut().as_mut()
            && pool.waiting.remove(k).is_some()
        {
            pool.shared.jobs.lock().unwrap_or_else(|e| e.into_inner()).retain(|(j, _)| j != k);
        }
    });
}
