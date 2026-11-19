//! A child's structure is published before its one-shot CR. Only the original reply waits for
//! the input; a target that changes after publication never receives the late CR.
use super::*;

type Receiver = std::sync::mpsc::Receiver<JsonRpcResponse>;

/// A command and the output it prints. The output is not in the command's typed echo, because
/// only the shell's arithmetic expansion turns `$((6*7))` into 42. It is counted wherever it lands:
/// a login shell that starts after the body and CR were typed ahead prints its prompt after their
/// echo, and dash then writes the output right after `$ `. A nonce for each run keeps a prompt,
/// path or profile output from holding the same text.
struct Marker {
    command: String,
    output: String,
}

fn marker() -> Marker {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    let nonce = format!(
        "{}n{}n{nanos}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );
    Marker {
        command: format!("echo child-{nonce}-$((6*7))-done"),
        output: format!("child-{nonce}-42-done"),
    }
}

fn turn(journal: &mut JournalApplication, session: &mut EngineSession) {
    journal.poll_bootstrap(&mut [&mut *session], None).unwrap();
    for (ticket, request) in journal.requests_needing_resolution() {
        match request.method.as_str() {
            "terminal.spawn" => journal.resolve_public_creation(ticket, session),
            "workspace.create" => journal.resolve_workspace_creation(ticket, session, None),
            _ => journal.resolve_ipc_for_engine(ticket, session),
        }
    }
}

/// The quiet test shell replaces itself with `sleep`; a child that runs commands needs a shell.
fn boot_with_shell() -> (EngineSession, JournalApplication) {
    let (mut session, journal) = boot();
    session.runtime.settings.general.startup_command = String::new();
    (session, journal)
}

fn spawn(
    journal: &mut JournalApplication,
    session: &EngineSession,
    id: u64,
    marker: &Marker,
) -> Receiver {
    let workspace = &session.core_state.local_workspaces()[0];
    send(
        journal,
        request(
            "terminal.spawn",
            serde_json::json!({
                "parent": workspace.all_surface_ids()[0],
                "workspace": workspace.id.to_string(),
                "command": marker.command,
            }),
            Some("child-input-key"),
            id,
        ),
    )
}

fn child_surface(session: &EngineSession) -> Option<u32> {
    let workspace = &session.core_state.local_workspaces()[0];
    let parent = workspace.all_surface_ids()[0];
    workspace
        .all_surface_ids()
        .into_iter()
        .find(|surface| *surface != parent)
}

fn waiting_input(journal: &JournalApplication) -> bool {
    journal
        .creations
        .values()
        .any(|creation| creation.waits_child_input())
}

fn outputs(session: &EngineSession, surface: u32, marker: &Marker) -> usize {
    session
        .runtime
        .terminals
        .get(surface)
        .map(|terminal| {
            terminal.with_content(|view| {
                count_outputs(&unwrapped_text(&view.screen_lines(), view.cols()), marker)
            })
        })
        .unwrap_or_default()
}

/// Screen rows joined back into the lines the shell wrote. The terminal keeps no wrap flag for
/// screen rows, so a row whose last column holds a non-blank cell counts as soft-wrapped, the same
/// rule the terminal uses when it moves rows into scrollback. A row that ends exactly at the last
/// column with a newline is joined too; that can only add a match across the join, and the output
/// has no blank, so an output split by a wrap always leaves its row's last column non-blank.
fn unwrapped_text(rows: &[termwiz::surface::line::Line], cols: usize) -> String {
    let mut text = String::new();
    for row in rows {
        let mut wrapped = false;
        for cell in row.visible_cells() {
            text.push_str(cell.str());
            wrapped =
                cell.cell_index() + cell.width().max(1) == cols && !cell.str().trim().is_empty();
        }
        if !wrapped {
            text.push('\n');
        }
    }
    text
}

/// Counts the command's output regardless of the line or prompt it shares.
fn count_outputs(screen: &str, marker: &Marker) -> usize {
    screen.matches(marker.output.as_str()).count()
}

/// The screen a terminal of `cols` columns shows after `output` is written to it.
fn screen_of(cols: usize, output: &str) -> String {
    let mut surface = termwiz::surface::Surface::new(cols, 6);
    surface.add_change(output);
    unwrapped_text(&surface.screen_lines(), cols)
}

#[test]
fn the_output_is_counted_after_a_prompt_or_on_its_own_line_but_not_in_the_echo() {
    let marker = marker();
    let Marker { command, output } = &marker;
    let joined = screen_of(80, &format!("{command}\r\n$ {output}\r\n$ "));
    let own_line = screen_of(80, &format!("{command}$ \r\n{output}\r\n$ "));
    assert_eq!(count_outputs(&joined, &marker), 1, "{joined:?}");
    assert_eq!(count_outputs(&own_line, &marker), 1, "{own_line:?}");
    assert_eq!(
        count_outputs(&screen_of(80, &format!("{command}\r\n$ ")), &marker),
        0
    );
}

#[test]
fn an_output_that_a_long_prompt_wraps_across_two_rows_is_counted_once() {
    let marker = marker();
    let Marker { command, output } = &marker;
    let prompt = format!("{}$ ", "p".repeat(67));
    assert_eq!(prompt.len(), 69);
    let typed = format!("{command}\r\n{prompt}{output}\r\n$ ");
    let mut surface = termwiz::surface::Surface::new(80, 6);
    surface.add_change(typed.as_str());
    let rows = surface.screen_chars_to_string();
    assert!(
        !rows.contains(output.as_str()),
        "the output crosses a row boundary: {rows:?}"
    );
    let screen = screen_of(80, &typed);
    assert_eq!(count_outputs(&screen, &marker), 1, "{screen:?}");
}

/// Pumps until the child is published while its CR is still pending.
fn published_before_submit(journal: &mut JournalApplication, session: &mut EngineSession) -> u32 {
    published_counting(journal, session, &std::sync::atomic::AtomicUsize::new(0))
}

/// [`published_before_submit`] that counts each turn that returned, for a thread watching progress.
fn published_counting(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    turns: &std::sync::atomic::AtomicUsize,
) -> u32 {
    let mut stall = StallBudget::new(journal);
    loop {
        turn(journal, session);
        turns.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(child) = child_surface(session)
            && waiting_input(journal)
        {
            assert!(
                !journal.pauses_observation(),
                "a pending child CR does not pause observation"
            );
            return child;
        }
        stall.nap("child published before its CR");
    }
}

fn reply(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    rx: &Receiver,
) -> JsonRpcResponse {
    let mut stall = StallBudget::new(journal);
    loop {
        turn(journal, session);
        if let Ok(response) = rx.try_recv() {
            return response;
        }
        stall.nap("child spawn reply");
    }
}

#[test]
fn a_child_is_published_before_its_command_runs_and_the_reply_waits_for_the_submit() {
    let marker = marker();
    let (mut session, mut journal) = boot_with_shell();
    let parent = session.core_state.local_workspaces()[0].all_surface_ids()[0];
    // Consume an actual reservation so the child's real name cannot be mistaken for index zero.
    assert_eq!(
        session
            .runtime
            .child_terminals
            .reserve_index(&parent.into()),
        Ok(0)
    );
    let rx = spawn(&mut journal, &session, 900, &marker);
    let child = published_before_submit(&mut journal, &mut session);
    let pane = session.core_state.find_pane_for_surface(child).unwrap();
    let tab = session.core_state.find_tab_for_surface(child).unwrap();
    let tab = session
        .core_state
        .find_pane_by_id(pane)
        .unwrap()
        .tabs
        .iter()
        .find(|candidate| candidate.id == tab)
        .unwrap();
    assert_eq!(tab.name, "child1");
    assert_eq!(tab.explicit_name.as_deref(), Some("child1"));
    assert!(
        matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)),
        "the reply waits for the submit"
    );
    let response = reply(&mut journal, &mut session, &rx);
    assert!(response.error.is_none(), "{response:?}");
    assert_eq!(response.result.as_ref().unwrap()["child_index"], 1);
    assert!(!waiting_input(&journal));
    let terminal = session
        .runtime
        .terminals
        .pty(child)
        .and_then(|pty| pty.process_id());
    let mut stall = StallBudget::for_process(terminal);
    while outputs(&session, child, &marker) == 0 {
        stall.nap_with(|| {
            format!(
                "child command output: {:?}",
                session
                    .runtime
                    .terminals
                    .get(child)
                    .map(|terminal| terminal.with_content(|view| view.screen_text(false)))
            )
        });
    }
    // The same key replays the stored result without writing the body again.
    let again = spawn(&mut journal, &session, 901, &marker);
    let replay = reply(&mut journal, &mut session, &again);
    assert_eq!(replay.result, response.result);
    for _ in 0..50 {
        turn(&mut journal, &mut session);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        outputs(&session, child, &marker),
        1,
        "body and CR were sent once"
    );
}

#[test]
fn a_child_held_after_publication_never_receives_the_late_submit() {
    let marker = marker();
    let (mut session, mut journal) = boot_with_shell();
    let rx = spawn(&mut journal, &session, 910, &marker);
    let child = published_before_submit(&mut journal, &mut session);
    session
        .borrow_mut()
        .live
        .occupancy
        .acquire(child, 99)
        .unwrap();
    let response = reply(&mut journal, &mut session, &rx);
    let error = response
        .error
        .expect("an undelivered submit is not a success");
    assert_eq!(
        error.code,
        crate::ipc::protocol::ERR_OPERATION_RECOVERY_REQUIRED
    );
    assert!(
        error.message.contains("published its structure"),
        "{error:?}"
    );
    assert!(error.message.contains("one-shot input"), "{error:?}");
    assert_eq!(child_surface(&session), Some(child), "the structure stays");
    for _ in 0..50 {
        turn(&mut journal, &mut session);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        outputs(&session, child, &marker),
        0,
        "the late CR was not sent"
    );
    assert!(!journal.has_creation(session.id));
}

/// Longest the test thread may go without finishing a turn while the lock holder waits. A turn does
/// no fsync of its own, so it took at most 20 ms under load 30 and under 2.5 s fsyncs. This is the
/// idle limit of the other waits in this test, far above that and below the time a slow journal
/// keeps the lock. A synchronous wait shorter than this is not detected.
const TURN_LIMIT: Duration = StallBudget::IDLE_LIMIT;

#[test]
fn a_held_relationship_receipt_keeps_command_and_original_reply_pending_after_publication() {
    let marker = marker();
    let (mut session, mut journal) = boot_with_shell();
    let rx = spawn(&mut journal, &session, 902, &marker);
    let mut stall = StallBudget::new(&journal);
    loop {
        turn(&mut journal, &mut session);
        if journal
            .commands
            .pending
            .values()
            .any(|pending| pending.child_spawn.is_some())
        {
            break;
        }
        stall.nap("index reservation admitted");
    }
    assert!(
        journal
            .requests_needing_resolution()
            .iter()
            .any(|(_, request)| request.id == Some(serde_json::json!(902))),
        "the original request keeps its continuation while the index receipt is pending"
    );
    // Pause the host continuation until the reservation has replaced the file; registration
    // cannot have been submitted because the retained spawn has not consumed its index receipt.
    let path = tasty_utils::path::tasty_home()
        .unwrap()
        .join("child-terminals.json");
    let parent = session.core_state.local_workspaces()[0].all_surface_ids()[0].to_string();
    loop {
        let reserved = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value["next_index"][&parent].as_u64())
            .is_some_and(|next| next > 0);
        if reserved {
            break;
        }
        stall.nap("index reservation replacement");
    }
    let (entered, ready) = std::sync::mpsc::sync_channel(1);
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let expired = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let timed_out = expired.clone();
    let turns = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let watched = turns.clone();
    let holder = std::thread::spawn(move || {
        let _participant = tasty_utils::file_replace::lock(&path).unwrap();
        entered.send(()).unwrap();
        // Time spent waiting for the journal's fsync does not count: the test thread keeps turning.
        // Only a turn that does not return, which is what a synchronous wait on this lock looks
        // like, reaches the limit. It then lets go so that the failure is reported, not a hang.
        let mut seen = watched.load(std::sync::atomic::Ordering::SeqCst);
        let mut since = std::time::Instant::now();
        loop {
            match wait.recv_timeout(Duration::from_millis(10)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            let now = watched.load(std::sync::atomic::Ordering::SeqCst);
            if now != seen {
                seen = now;
                since = std::time::Instant::now();
            } else if since.elapsed() >= TURN_LIMIT {
                timed_out.store(true, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        }
    });
    // Nothing is pumped here: the holder only waits for the writer's lock. It has no result to
    // lose except by ending, so read its end first and bound only a wait that never ends.
    let started = std::time::Instant::now();
    loop {
        let ended = holder.is_finished();
        match ready.recv_timeout(Duration::from_millis(10)) {
            Ok(()) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("the lock holder ended before it entered")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        assert!(!ended, "the lock holder ended before it entered");
        assert!(
            started.elapsed() < StallBudget::CEILING,
            "the lock holder did not enter within {:?}",
            StallBudget::CEILING
        );
    }
    let child = published_counting(&mut journal, &mut session, &turns);
    let waited = || {
        format!(
            "a turn did not return for {TURN_LIMIT:?} with the file participant locked: \
             publication must not synchronously wait on it"
        )
    };
    assert!(
        !expired.load(std::sync::atomic::Ordering::SeqCst),
        "{}",
        waited()
    );
    // Publication's own turn may not have polled the input yet. The relationship receipt cannot
    // finish under this lock, so the body stays held however many turns poll it.
    for _ in 0..3 {
        turn(&mut journal, &mut session);
        turns.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    assert!(
        journal
            .creations
            .values()
            .any(|creation| creation.holds_child_body()),
        "the one-shot body left before its relationship receipt finished"
    );
    assert_eq!(outputs(&session, child, &marker), 0);
    assert!(matches!(
        rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(!journal.pauses_observation());
    // A holder that gave up after these checks has already dropped its receiver. Judged after the
    // join, so the checks above are known to have run with the lock held.
    let released = release.send(());
    holder.join().unwrap();
    assert!(
        released.is_ok() && !expired.load(std::sync::atomic::Ordering::SeqCst),
        "{}",
        waited()
    );
    let response = reply(&mut journal, &mut session, &rx);
    assert!(response.error.is_none(), "{response:?}");
    assert_eq!(response.id, serde_json::json!(902));
    assert!(
        rx.try_recv().is_err(),
        "the original caller receives one response"
    );
}
