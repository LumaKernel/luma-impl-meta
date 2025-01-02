use crate::fix::fix;
use notify::{self, recommended_watcher, Event, RecursiveMode, Watcher};
use std::path::{self, Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

pub fn watch_fix(root_dir: impl AsRef<Path>) -> notify::Result<()> {
    let root_dir = root_dir.as_ref();

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = recommended_watcher(tx)?;
    watcher.watch(&root_dir.join("crates"), RecursiveMode::Recursive)?;
    let accepting = Arc::new(Mutex::new(false));
    let run_wait = Arc::new(Mutex::new(0_usize));
    for res in rx {
        let run = {
            let run_wait = run_wait.clone();
            let root_dir = root_dir.to_owned();
            move || {
                fn run(run_wait: Arc<Mutex<usize>>, root_dir: PathBuf) {
                    match fix(&root_dir) {
                        Ok(_) => log::info!("done"),
                        Err(err) => {
                            log::error!("error: {}", err.join("\n"));
                        }
                    }
                    loop {
                        if let Ok(mut v) = run_wait.lock() {
                            *v -= 1;
                            if *v > 0 {
                                let run_wait = run_wait.clone();
                                thread::spawn(move || run(run_wait, root_dir));
                            }
                            break;
                        }
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                run(run_wait, root_dir);
            }
        };
        match res {
            Ok(event) => {
                if accepting.lock().map(|a| *a).unwrap_or_else(|_| true) {
                    continue;
                }
                if event.paths.iter().any(|p| {
                    p.components()
                        .last()
                        .and_then(|last| match last {
                            path::Component::Normal(s) => s.to_str(),
                            _ => None,
                        })
                        .map(|s| s == "Cargo.toml")
                        .unwrap_or_default()
                }) {
                    log::debug!("event: {:?}", event);
                    loop {
                        if let Ok(mut a) = accepting.lock() {
                            *a = true;
                            break;
                        }
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                    loop {
                        if let Ok(mut r) = run_wait.lock() {
                            match *r {
                                0 => {
                                    *r += 1;
                                    thread::spawn(run);
                                }
                                1 => {
                                    *r += 1;
                                }
                                _ => {}
                            }
                            break;
                        }
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                    let accepting = accepting.clone();
                    thread::spawn(move || {
                        thread::sleep(std::time::Duration::from_millis(300));
                        if let Ok(mut a) = accepting.lock() {
                            *a = false;
                        }
                    });
                }
            }
            Err(e) => log::error!("watch error: {:?}", e),
        }
    }

    Ok(())
}
