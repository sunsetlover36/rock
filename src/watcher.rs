use std::{path::Path, time::Duration};

use notify_debouncer_mini::{DebounceEventResult, new_debouncer, notify::RecursiveMode};

use crate::runtime::RuntimeCommand;

pub(super) fn spawn_reload_watcher(
    cmd_tx: flume::Sender<RuntimeCommand>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let tx = cmd_tx.clone();

        let mut debouncer = match new_debouncer(
            Duration::from_millis(300),
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    let should_reload = events.iter().any(|e| {
                        let path = &e.path;

                        path.ends_with("config.cfg")
                            || path
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| ext.eq_ignore_ascii_case("lua"))
                                .unwrap_or(false)
                    });

                    if should_reload {
                        let _ = tx.send(RuntimeCommand::Reload);
                    }
                }
                Err(errs) => {
                    eprintln!("[HRM] Hot reload watch error: {errs}");
                }
            },
        ) {
            Ok(d) => d,
            Err(err) => {
                eprintln!("[HRM] Failed to create a debouncer: {err}");
                return;
            }
        };

        if let Err(err) = debouncer
            .watcher()
            .watch(Path::new("config.cfg"), RecursiveMode::NonRecursive)
        {
            eprintln!("[HRM] Failed to watch `config.cfg`: {err}");
            return;
        }

        if let Err(err) = debouncer
            .watcher()
            .watch(Path::new("gamemodes"), RecursiveMode::Recursive)
        {
            eprintln!("[HRM] Failed to watch `gamemodes/`: {err}");
            return;
        }

        loop {
            std::thread::park();
        }
    })
}
