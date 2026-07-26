use async_stream::try_stream;
use axum::response::{IntoResponse, Response, Sse, sse::Event};
use eventsource_stream::{Event as SourceEvent, Eventsource};
use futures::Stream;

use crate::{
    middleware::claude::ClaudeContext,
    types::claude::{ContentBlockDelta, MessageDeltaContent, StopReason, StreamEvent},
};

type EventResult<T> = Result<T, eventsource_stream::EventStreamError<axum::Error>>;

/// Watches a byte stream for any of a set of stop sequences.
///
/// A sequence may straddle two SSE events, so the matcher owns the trailing
/// bytes rather than looking at one event in isolation. It keeps only as many
/// as the longest sequence needs: nothing older than that can begin a match.
struct StopMatcher {
    /// Shortest first, so that when several sequences end on the same byte the
    /// reported one is the shortest.
    sequences: Vec<String>,
    /// The most recent bytes, capped at the longest sequence's length.
    tail: Vec<u8>,
    window: usize,
}

impl StopMatcher {
    fn new(mut sequences: Vec<String>) -> Self {
        // An empty needle is a suffix of everything, so it would stop the
        // response on its first byte. Drop them: they never matched before.
        sequences.retain(|s| !s.is_empty());
        sequences.sort_unstable_by_key(String::len);
        let window = sequences.iter().map(String::len).max().unwrap_or(0);
        Self {
            sequences,
            tail: Vec::with_capacity(window),
            window,
        }
    }

    /// Feeds one byte, returning the sequence that just completed, if any.
    fn push(&mut self, byte: u8) -> Option<&str> {
        self.tail.push(byte);
        if self.tail.len() > self.window {
            self.tail.drain(..self.tail.len() - self.window);
        }
        let tail = &self.tail;
        self.sequences
            .iter()
            .find(|s| tail.ends_with(s.as_bytes()))
            .map(String::as_str)
    }
}

// `try_stream!` is this function's return value, but the lint reads the macro's
// expansion and asks for a `;` that would make the body return `()`.
#[expect(
    clippy::semicolon_if_nothing_returned,
    reason = "false positive inside the try_stream! expansion"
)]
fn stop_stream(
    sequences: Vec<String>,
    stream: impl Stream<Item = EventResult<SourceEvent>>,
) -> impl Stream<Item = EventResult<Event>> {
    try_stream!({
        let mut matcher = StopMatcher::new(sequences);
        for await event in stream {
            let eventsource_stream::Event {
                data,
                id,
                event,
                retry,
            } = event?;
            let event = Event::default().event(event).id(id).data(&data);
            let event = if let Some(retry) = retry {
                event.retry(retry)
            } else {
                event
            };
            let Ok(parsed) = serde_json::from_str::<StreamEvent>(&data) else {
                yield event;
                continue;
            };
            let StreamEvent::ContentBlockDelta { delta, index } = parsed else {
                yield event;
                continue;
            };
            let ContentBlockDelta::TextDelta { text } = delta else {
                yield event;
                continue;
            };
            let input = text.into_bytes();
            for i in 0..input.len() {
                let Some(seq) = matcher.push(input[i]) else {
                    continue;
                };
                let seq = seq.to_owned();
                // stop sequence found
                let result = String::from_utf8_lossy(&input[..=i]).to_string();
                let event = StreamEvent::ContentBlockDelta {
                    delta: ContentBlockDelta::TextDelta { text: result },
                    index,
                };
                let content_block_stop = StreamEvent::ContentBlockStop { index };
                let message_delta = StreamEvent::MessageDelta {
                    delta: MessageDeltaContent {
                        stop_reason: Some(StopReason::StopSequence),
                        stop_sequence: Some(seq),
                    },
                    usage: None,
                };
                let message_stop = StreamEvent::MessageStop;

                for e in [event, content_block_stop, message_delta, message_stop] {
                    let event = Event::default();
                    let event = event.json_data(e).unwrap();
                    yield event;
                }
                return;
            }
            yield event;
        }
    })
}

pub async fn apply_stop_sequences(resp: Response) -> Response {
    let Some(f) = resp.extensions().get::<ClaudeContext>().cloned() else {
        return resp;
    };
    if !f.is_stream() || f.stop_sequences().is_empty() {
        return resp;
    }

    let stream = resp.into_body().into_data_stream().eventsource();
    let stream = stop_stream(f.stop_sequences().to_owned(), stream);
    let mut resp = Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response();

    resp.extensions_mut().insert(f);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed a whole string and report the first sequence to complete, with the
    /// byte offset it completed at.
    fn first_match(sequences: &[&str], text: &str) -> Option<(String, usize)> {
        let mut matcher = StopMatcher::new(sequences.iter().map(|s| (*s).to_string()).collect());
        for (i, &byte) in text.as_bytes().iter().enumerate() {
            if let Some(seq) = matcher.push(byte) {
                return Some((seq.to_owned(), i));
            }
        }
        None
    }

    #[test]
    fn a_sequence_is_found_where_it_ends() {
        assert_eq!(
            first_match(&["STOP"], "hello STOP world"),
            Some(("STOP".to_string(), 9))
        );
    }

    #[test]
    fn text_without_the_sequence_never_matches() {
        assert_eq!(first_match(&["STOP"], "hello world"), None);
    }

    /// The whole point of holding bytes across events: upstream chunks the text
    /// wherever it likes, including in the middle of a stop sequence.
    #[test]
    fn a_sequence_split_across_chunks_still_matches() {
        let mut matcher = StopMatcher::new(vec!["STOP".to_string()]);
        for byte in b"hello ST" {
            assert_eq!(matcher.push(*byte), None);
        }
        // second chunk
        assert_eq!(matcher.push(b'O'), None);
        assert_eq!(matcher.push(b'P'), Some("STOP"));
    }

    /// A partial match that turns out not to be one must not poison the bytes
    /// after it.
    #[test]
    fn a_false_start_does_not_prevent_a_later_match() {
        assert_eq!(
            first_match(&["STOP"], "STO-STOP"),
            Some(("STOP".to_string(), 7))
        );
    }

    /// Two sequences completing on the very same byte: the shortest is
    /// reported, matching the incremental trie search this replaced, which
    /// kept its candidates newest-start-first and so reached that one first.
    #[test]
    fn the_shortest_of_two_matches_ending_together_is_reported() {
        // Both "END" and "THE END" are suffixes at the final byte.
        assert_eq!(
            first_match(&["THE END", "END"], "THE END"),
            Some(("END".to_string(), 6))
        );
        // Same tie, with the sequences supplied in the other order.
        assert_eq!(
            first_match(&["END", "THE END"], "THE END"),
            Some(("END".to_string(), 6))
        );
    }

    /// A short sequence that completes earlier wins on position, before any
    /// longer one has had the chance to finish.
    #[test]
    fn the_earliest_match_wins_over_a_longer_later_one() {
        assert_eq!(
            first_match(&["aaa", "a"], "aaa"),
            Some(("a".to_string(), 0))
        );
    }

    /// A longer sequence still wins if it is the only one that completes.
    #[test]
    fn a_longer_sequence_matches_when_no_shorter_one_does() {
        assert_eq!(
            first_match(&["xyz", "END"], "the END"),
            Some(("END".to_string(), 6))
        );
    }

    /// An empty sequence is a suffix of every string, so honouring it would
    /// truncate the response at its first byte.
    #[test]
    fn an_empty_sequence_is_ignored() {
        assert_eq!(first_match(&[""], "anything"), None);
        assert_eq!(
            first_match(&["", "END"], "the END"),
            Some(("END".to_string(), 6))
        );
    }

    #[test]
    fn no_sequences_means_nothing_ever_matches() {
        assert_eq!(first_match(&[], "whatever text"), None);
    }

    /// The retained tail is bounded by the longest sequence, so a long response
    /// with a short needle must not accumulate the whole body.
    #[test]
    fn the_retained_tail_stays_bounded() {
        let mut matcher = StopMatcher::new(vec!["END".to_string()]);
        for byte in "x".repeat(10_000).as_bytes() {
            assert_eq!(matcher.push(*byte), None);
        }
        assert_eq!(matcher.tail.len(), 3);
    }

    /// Sequences are matched as bytes, so a multi-byte needle must match on its
    /// final byte and not on a partial code point.
    #[test]
    fn a_multibyte_sequence_matches_whole() {
        assert_eq!(
            first_match(&["。"], "文章。続き"),
            Some(("。".to_string(), 8))
        );
    }
}
