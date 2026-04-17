use std::{path::Path, time::Duration};

use notify_debouncer_full::{
    DebounceEventResult, new_debouncer,
    notify::{
        RecursiveMode,
        event::{CreateKind, EventKind, ModifyKind},
    },
};

use crate::runtime::RuntimeCommand;

pub(super) fn spawn_reload_watcher(
    cmd_tx: flume::Sender<RuntimeCommand>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let tx = cmd_tx.clone();

        let mut debouncer = match new_debouncer(
            Duration::from_millis(500),
            None,
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    let should_reload = events.iter().any(|event| {
                        let path_matches = event.paths.iter().any(|path| {
                            path.ends_with("config.cfg")
                                || path
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|ext| ext.eq_ignore_ascii_case("lua"))
                                    .unwrap_or(false)
                        });

                        let kind_matches = matches!(
                            event.event.kind,
                            EventKind::Modify(ModifyKind::Data(_))
                                | EventKind::Modify(ModifyKind::Name(_))
                                | EventKind::Create(CreateKind::File)
                        );

                        path_matches && kind_matches
                    });

                    if should_reload {
                        let _ = tx.send(RuntimeCommand::Reload);
                    }
                }
                Err(errs) => {
                    eprintln!("[HRM] Hot reload watch error: {errs:?}");
                }
            },
        ) {
            Ok(d) => d,
            Err(err) => {
                eprintln!("[HRM] Failed to create a debouncer: {err}");
                return;
            }
        };

        if let Err(err) = debouncer.watch(Path::new("config.cfg"), RecursiveMode::NonRecursive) {
            eprintln!("[HRM] Failed to watch `config.cfg`: {err}");
            return;
        }

        if let Err(err) = debouncer.watch(Path::new("gamemodes"), RecursiveMode::Recursive) {
            eprintln!("[HRM] Failed to watch `gamemodes/`: {err}");
            return;
        }

        loop {
            std::thread::park();
        }
    })
}
