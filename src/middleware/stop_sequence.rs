use axum::response::{IntoResponse, Response, Sse};
use eventsource_stream::{Event as EventSourceEvent, Eventsource};
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{debug, info};

use crate::{
    context::RequestContext,
    types::message::{MessageDeltaContent, StopReason, StreamEvent, StreamUsage},
    utils::stop_sequence::StopSequenceMatcher,
};

use super::FormatInfo;

/// Middleware for handling stop sequences in streaming responses
///
/// This middleware intercepts streaming responses and checks for stop sequences.
/// When a stop sequence is detected, it:
/// 1. Sends any remaining text before the stop sequence
/// 2. Sends appropriate stop events based on the API format
/// 3. Terminates the stream
///
/// # Arguments
/// * `resp` - The original response to be processed
///
/// # Returns
/// The original or transformed response with stop sequence handling
pub async fn handle_stop_sequences(resp: Response) -> impl IntoResponse {
    // Get format info and request context from response extensions
    let stop_sequences = {
        // Get format info
        let Some(format_info) = resp.extensions().get::<FormatInfo>() else {
            return resp;
        };

        // Only process streaming responses with 200 status
        if !format_info.stream || resp.status() != 200 {
            return resp;
        }

        // Get request context
        let Some(ctx) = resp.extensions().get::<RequestContext>() else {
            return resp;
        };

        // Check if stop_sequences are present in the request
        let Some(stop_sequences) = ctx
            .current_request
            .as_ref()
            .and_then(|req| req.stop_sequences.clone())
        else {
            return resp;
        };

        // If no stop sequences are defined, return the original response
        if stop_sequences.is_empty() {
            return resp;
        }

        stop_sequences
    };

    debug!(
        "Stop sequences middleware active with: {:?}",
        stop_sequences
    );

    // Create a stop sequence matcher
    let matcher = StopSequenceMatcher::new(&stop_sequences);

    // Get the response body as a stream
    let body = resp.into_body();
    let stream = body.into_data_stream();

    // Create a stream transformer that checks for stop sequences
    let stream = StopSequenceStream {
        inner: stream.eventsource(),
        matcher,
        stopped: false,
        stop_event_state: 0,
        stop_events: None,
    };

    // Return the transformed stream as a response
    Sse::new(stream)
        .keep_alive(Default::default())
        .into_response()
}

/// Stream transformer that checks for stop sequences
struct StopSequenceStream<S> {
    inner: S,
    matcher: StopSequenceMatcher,
    stopped: bool,
    stop_event_state: u8,
    stop_events: Option<(
        axum::response::sse::Event,
        axum::response::sse::Event,
        axum::response::sse::Event,
    )>,
}

impl<S, E> Stream for StopSequenceStream<S>
where
    S: Stream<Item = Result<EventSourceEvent, E>> + Unpin,
    E: std::error::Error + 'static,
{
    type Item = Result<axum::response::sse::Event, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If we have stop events to send, send them in sequence
        if self.stopped {
            // We've already matched a stop sequence and need to send the remaining events
            // Clone the event we need to send based on the current state
            let event_to_send = match self.stop_event_state {
                0 => {
                    // Get content_block_stop event
                    if let Some((content_block_stop, _, _)) = &self.stop_events {
                        let event = content_block_stop.clone();
                        Some(event)
                    } else {
                        None
                    }
                }
                1 => {
                    // Get message_delta event
                    if let Some((_, message_delta, _)) = &self.stop_events {
                        let event = message_delta.clone();
                        Some(event)
                    } else {
                        None
                    }
                }
                2 => {
                    // Get message_stop event
                    if let Some((_, _, message_stop)) = &self.stop_events {
                        let event = message_stop.clone();
                        Some(event)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            // If we have an event to send, return it and increment the state
            if let Some(event) = event_to_send {
                self.stop_event_state += 1;
                return Poll::Ready(Some(Ok(event)));
            } else {
                return Poll::Ready(None);
            }
        }

        // Poll the inner stream
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                // Process the event data
                let event_data = event.data.clone();

                // Try to parse the event as a StreamEvent
                if let Ok(stream_event) = serde_json::from_str::<StreamEvent>(&event_data) {
                    // Handle different event types
                    match stream_event {
                        StreamEvent::ContentBlockDelta { index, delta } => {
                            // Extract text from the delta
                            let text = match delta {
                                crate::types::message::ContentBlockDelta::TextDelta { text } => {
                                    text
                                }
                                _ => return Poll::Ready(Some(Ok(convert_event(event)))),
                            };

                            // Check if the text contains a stop sequence
                            let (output, matched) = self.matcher.process(&text);

                            if let Some(stop_sequence) = matched {
                                info!("Stop sequence matched: {}", stop_sequence);

                                // Set stopped flag to true
                                self.stopped = true;

                                // Create content_block_stop event
                                let content_block_stop = StreamEvent::ContentBlockStop { index };
                                let content_block_stop_event =
                                    axum::response::sse::Event::default()
                                        .event("content_block_stop")
                                        .json_data(&content_block_stop)
                                        .unwrap();

                                // Create message_delta event with stop_reason and stop_sequence
                                let message_delta = StreamEvent::MessageDelta {
                                    delta: MessageDeltaContent {
                                        stop_reason: Some(StopReason::StopSequence),
                                        stop_sequence: Some(stop_sequence),
                                    },
                                    usage: Some(StreamUsage {
                                        input_tokens: 0,
                                        output_tokens: 0,
                                    }),
                                };
                                let delta_event = axum::response::sse::Event::default()
                                    .event("message_delta")
                                    .json_data(&message_delta)
                                    .unwrap();

                                // Create message_stop event
                                let message_stop = StreamEvent::MessageStop;
                                let stop_message_event = axum::response::sse::Event::default()
                                    .event("message_stop")
                                    .json_data(&message_stop)
                                    .unwrap();

                                // Store all stop events for sequential sending
                                self.stop_events = Some((
                                    content_block_stop_event,
                                    delta_event,
                                    stop_message_event,
                                ));

                                // Send the remaining text before the stop sequence if any
                                if !output.is_empty() {
                                    // Create a content block delta event with the remaining text
                                    let content_delta = StreamEvent::ContentBlockDelta {
                                        index,
                                        delta:
                                            crate::types::message::ContentBlockDelta::TextDelta {
                                                text: output,
                                            },
                                    };

                                    let output_event = axum::response::sse::Event::default()
                                        .event("content_block_delta")
                                        .json_data(&content_delta)
                                        .unwrap();

                                    // Return the output event with remaining text
                                    return Poll::Ready(Some(Ok(output_event)));
                                } else {
                                    // No remaining text, start sending stop events immediately
                                    let event_to_send = if let Some((content_block_stop, _, _)) =
                                        &self.stop_events
                                    {
                                        let event = content_block_stop.clone();
                                        self.stop_event_state = 1;
                                        Some(event)
                                    } else {
                                        None
                                    };

                                    if let Some(event) = event_to_send {
                                        return Poll::Ready(Some(Ok(event)));
                                    } else {
                                        return Poll::Ready(None);
                                    }
                                }
                            }

                            // No stop sequence matched, convert the event
                            let event = axum::response::sse::Event::default()
                                .event(event.event)
                                .json_data(&StreamEvent::ContentBlockDelta {
                                    index,
                                    delta: crate::types::message::ContentBlockDelta::TextDelta {
                                        text: output,
                                    },
                                })
                                .unwrap();

                            Poll::Ready(Some(Ok(event)))
                        }
                        // Pass through other event types
                        _ => Poll::Ready(Some(Ok(convert_event(event)))),
                    }
                } else {
                    // Not a StreamEvent, pass through
                    Poll::Ready(Some(Ok(convert_event(event))))
                }
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

// Helper function to convert from eventsource_stream::Event to axum::response::sse::Event
fn convert_event(event: EventSourceEvent) -> axum::response::sse::Event {
    let mut sse_event = axum::response::sse::Event::default();

    if !event.event.is_empty() {
        sse_event = sse_event.event(event.event);
    }

    sse_event.data(event.data)
}
