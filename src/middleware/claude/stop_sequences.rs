use async_stream::try_stream;
use axum::response::{IntoResponse, Response, Sse, sse::Event};
use eventsource_stream::{Event as SourceEvent, Eventsource};
use futures::Stream;

use crate::{
    middleware::claude::ClaudeContext,
    types::claude::{ContentBlockDelta, MessageDeltaContent, StopReason, StreamEvent},
};

type EventResult<T> = Result<T, eventsource_stream::EventStreamError<axum::Error>>;

fn stop_stream(
    sequences: Vec<String>,
    stream: impl Stream<Item = EventResult<SourceEvent>>,
) -> impl Stream<Item = EventResult<Event>> {
    let trie = trie_rs::map::Trie::from_iter(sequences.into_iter().map(|s| (s.clone(), s)));
    try_stream!({
        let mut searches = vec![trie.inc_search()];
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
                let mut next_searches = vec![trie.inc_search()];
                for mut s in searches {
                    match s.query(&input[i]) {
                        // match found, return
                        Some(a) if a.is_match() => {
                            let seq = s.value().unwrap();
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
                                    stop_sequence: Some(seq.clone()),
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
                        // prefix found, add it to the next searches
                        Some(a) if a.is_prefix() => next_searches.push(s),
                        _ => (),
                    }
                }
                searches = next_searches;
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
        .keep_alive(Default::default())
        .into_response();

    resp.extensions_mut().insert(f);
    resp
}
