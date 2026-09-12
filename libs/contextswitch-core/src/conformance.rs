//! Reusable conformance checks for [`StorageProvider`] implementations.
//!
//! Every provider — the local filesystem provider now, remote object storage
//! later — must pass [`check_provider_conformance`] before it can be trusted
//! with canonical data. The suite exercises the contract's safety properties:
//! versioned reads, conditional commits, invariant enforcement, and
//! serialization of concurrent writers.

use chrono::{DateTime, Utc};

use crate::domain::Span;
use crate::storage::{StorageError, StorageProvider};

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

/// Run the full conformance suite against a provider factory.
///
/// Each call to `make_provider` must return a provider bound to a fresh,
/// empty storage location.
pub fn check_provider_conformance<F>(mut make_provider: F)
where
    F: FnMut() -> Box<dyn StorageProvider>,
{
    initial_read_is_empty(&mut make_provider);
    commit_returns_incremented_version(&mut make_provider);
    stale_commit_is_a_conflict(&mut make_provider);
    commit_rejects_invalid_documents(&mut make_provider);
    concurrent_commits_have_a_single_winner(&mut make_provider);
}

fn initial_read_is_empty<F>(make_provider: &mut F)
where
    F: FnMut() -> Box<dyn StorageProvider>,
{
    let provider = make_provider();
    let snapshot = provider.read().expect("initial read failed");
    assert_eq!(snapshot.version, "0", "fresh storage must read version 0");
    assert!(snapshot.document.spans.is_empty());
    assert!(snapshot.document.projects.is_empty());
    assert_eq!(snapshot.document.active_span_id, None);
}

fn commit_returns_incremented_version<F>(make_provider: &mut F)
where
    F: FnMut() -> Box<dyn StorageProvider>,
{
    let provider = make_provider();
    let snapshot = provider.read().unwrap();
    let mut document = snapshot.document.clone();
    let project = document.add_project("work", at(0)).unwrap();
    document.start_timer(at(10), Some(project), vec![]).unwrap();

    let version = provider.commit(document, &snapshot.version).unwrap();
    assert_eq!(version, "1", "first commit must produce version 1");

    let reread = provider.read().unwrap();
    assert_eq!(reread.version, "1");
    assert_eq!(reread.document.spans.len(), 1);
    assert!(reread.document.active_span_id.is_some());
}

fn stale_commit_is_a_conflict<F>(make_provider: &mut F)
where
    F: FnMut() -> Box<dyn StorageProvider>,
{
    let provider = make_provider();
    let snapshot = provider.read().unwrap();

    let mut first = snapshot.document.clone();
    first.add_tag("one", at(0)).unwrap();
    provider.commit(first, &snapshot.version).unwrap();

    // A commit written against the now-stale version must never overwrite.
    let mut stale = snapshot.document.clone();
    stale.add_tag("two", at(0)).unwrap();
    match provider.commit(stale, &snapshot.version) {
        Err(StorageError::Conflict { expected, actual }) => {
            assert_eq!(expected, "0");
            assert_eq!(actual, "1");
        }
        other => panic!("stale commit must fail with Conflict, got {other:?}"),
    }
}

fn commit_rejects_invalid_documents<F>(make_provider: &mut F)
where
    F: FnMut() -> Box<dyn StorageProvider>,
{
    let provider = make_provider();
    let snapshot = provider.read().unwrap();

    // Craft a document with two unstopped spans: the one-active-timer
    // invariant is enforced by the commit, not by client convention.
    let mut document = snapshot.document.clone();
    let mut span_a = Span::new(at(0), None, vec![]);
    let mut span_b = Span::new(at(5), None, vec![]);
    span_a.stopped_at = None;
    span_b.stopped_at = None;
    document.spans.insert(span_a.id, span_a.clone());
    document.spans.insert(span_b.id, span_b);
    document.active_span_id = Some(span_a.id);

    match provider.commit(document, &snapshot.version) {
        Err(StorageError::InvalidData(_)) => {}
        other => panic!("invalid document must fail with InvalidData, got {other:?}"),
    }

    // The rejected commit must not have modified canonical data.
    assert_eq!(provider.read().unwrap().version, snapshot.version);

    // Overlapping spans are likewise rejected: time must not be
    // double-recorded, so this is enforced by the commit.
    let mut document = snapshot.document.clone();
    let mut span_a = Span::new(at(0), None, vec![]);
    span_a.stopped_at = Some(at(100));
    let mut span_b = Span::new(at(50), None, vec![]);
    span_b.stopped_at = Some(at(150));
    document.spans.insert(span_a.id, span_a);
    document.spans.insert(span_b.id, span_b);

    match provider.commit(document, &snapshot.version) {
        Err(StorageError::InvalidData(_)) => {}
        other => panic!("overlapping spans must fail with InvalidData, got {other:?}"),
    }
    assert_eq!(provider.read().unwrap().version, snapshot.version);
}

fn concurrent_commits_have_a_single_winner<F>(make_provider: &mut F)
where
    F: FnMut() -> Box<dyn StorageProvider>,
{
    let provider = make_provider();
    let provider = provider.as_ref();
    let snapshot = provider.read().unwrap();

    let results: Vec<Result<String, StorageError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let expected = snapshot.version.clone();
                let mut document = snapshot.document.clone();
                scope.spawn(move || {
                    document.add_tag(format!("tag-{i}"), at(0)).unwrap();
                    provider.commit(document, &expected)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        winners, 1,
        "exactly one concurrent commit may win: {results:?}"
    );
    for result in &results {
        if let Err(e) = result {
            assert!(
                matches!(e, StorageError::Conflict { .. }),
                "losing commits must fail with Conflict, got {e:?}"
            );
        }
    }
}
