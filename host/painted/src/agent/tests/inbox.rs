//! What the reader's side hands exact2 off its thread (`crate::inbox`) is
//! applied by the turn a wait runs, not by a task queued behind the op
//! that holds the reader's thread (an agent's `clock` waited out its bound
//! on a module answer so queued).
use super::*;

#[test]
fn a_pump_applies_what_the_reader_sent_off_thread() {
    let plan = crate::test_plan("component App\n  view\n    text \"x\"\n");
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (80., 40.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    crate::inbox::push(crate::inbox::In::Journal("the reader answered".to_owned()));
    // An answer to a request never asked changes nothing and keeps no one.
    crate::inbox::push(crate::inbox::In::Answer {
        id: 77,
        answer: Some("{}".to_owned()),
    });
    // Nothing asks the host for it until a turn: the journal does not move.
    assert!(!p
        .host()
        .runner()
        .journal()
        .any(|l| l.contains("the reader answered")));
    let _ = p.pump(p.host().now());
    assert!(
        p.host()
            .runner()
            .journal()
            .any(|l| l.contains("the reader answered")),
        "the pushed line landed in the turn"
    );
}
