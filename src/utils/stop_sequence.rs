use std::collections::HashMap;
use std::cmp::max;
use std::sync::{Arc, Mutex};

/// A matcher for stop sequences in text streams.
/// Characters that might be part of a stop sequence are buffered until they can be confirmed
/// not to be part of any match, at which point they are immediately returned.
pub struct StopSequenceMatcher {
    root: Arc<TrieNode>,                                // Root of the trie data structure (shared)
    state: Arc<Mutex<StopSequenceState>>,               // Thread-safe mutable state
}

/// Internal mutable state of the matcher
struct StopSequenceState {
    buffered_chars: String,           // Characters being processed
    active_matches: Vec<ActiveMatch>, // Active match attempts
}

/// Represents a node in the Trie data structure for efficient string matching.
#[derive(Clone)]
struct TrieNode {
    children: HashMap<char, Arc<TrieNode>>,  // Child nodes mapped by character
    is_end: bool,                            // Indicates if this node is the end of a sequence
    sequence: Option<String>,                // The full sequence if this is an end node
}

/// Represents an active match attempt in progress.
#[derive(Clone)]
struct ActiveMatch {
    node: Arc<TrieNode>,  // Current node in the trie (using Arc instead of raw pointer)
    buffer: String,       // Characters matched so far
}

// StopSequenceMatcher is safely Send and Sync because all internal mutable state is protected by Mutex
// and shared data is wrapped in Arc
unsafe impl Send for StopSequenceMatcher {}
unsafe impl Sync for StopSequenceMatcher {}

impl Clone for StopSequenceMatcher {
    fn clone(&self) -> Self {
        StopSequenceMatcher {
            root: self.root.clone(),
            state: self.state.clone(),
        }
    }
}

impl StopSequenceMatcher {
    /// Creates a new StopSequenceMatcher with the given stop sequences.
    ///
    /// # Arguments
    /// * `stop_sequences` - List of sequences that should stop processing when matched
    pub fn new(stop_sequences: &[String]) -> Self {
        // Create a root node for the trie
        let mut root = TrieNode::new();

        // Build the trie from the stop sequences
        for sequence in stop_sequences {
            let mut node = &mut root;
            for c in sequence.chars() {
                node = Arc::get_mut(node.children
                    .entry(c)
                    .or_insert_with(|| Arc::new(TrieNode::new())))
                    .expect("Failed to get mutable reference to TrieNode");
            }
            node.is_end = true;
            node.sequence = Some(sequence.clone());
        }

        // Create the matcher with thread-safe state
        StopSequenceMatcher {
            root: Arc::new(root),
            state: Arc::new(Mutex::new(StopSequenceState {
                buffered_chars: String::new(),
                active_matches: Vec::new(),
            })),
        }
    }

    /// Process a chunk of text and check if any stop sequence is matched
    ///
    /// # Arguments
    /// * `text` - The text to process
    ///
    /// # Returns
    /// A tuple containing:
    /// - The text that should be sent to the client (may be empty if all text is part of a stop sequence)
    /// - The matched stop sequence if any
    pub fn process(&self, text: &str) -> (String, Option<String>) {
        let mut output = String::new();
        let mut matched_sequence = None;

        for c in text.chars() {
            // Process the character
            let (processed_chars, match_result) = self.process_char(c);

            // Add any processed characters to the output
            if let Some(chars) = processed_chars {
                output.push_str(&chars);
            }

            // If we found a match, return it
            if let Some(seq) = match_result {
                matched_sequence = Some(seq);
                break;
            }
        }

        (output, matched_sequence)
    }

    /// Processes a single character, updating the matching state.
    ///
    /// # Arguments
    /// * `c` - Character to process
    ///
    /// # Returns
    /// A tuple containing:
    /// - Characters that can be safely output (if any)
    /// - Matched stop sequence (if any)
    fn process_char(&self, c: char) -> (Option<String>, Option<String>) {
        // Lock the state for the duration of this method
        let mut state = self.state.lock().expect("Failed to lock state mutex");

        // Start a new match from the root for the current character
        state.active_matches.push(ActiveMatch {
            node: self.root.clone(),
            buffer: String::new(),
        });

        // Add current character to the buffer for potential matches
        state.buffered_chars.push(c);

        let mut new_active_matches = Vec::new();
        let mut max_match_length = 0;
        let mut matched_sequence = None;

        // Process all active matches with the current character
        for match_state in &state.active_matches {
            let node = &match_state.node;

            if let Some(next_node) = node.children.get(&c) {
                // Continue the match by adding the character and updating the node
                let mut new_buffer = match_state.buffer.clone();
                new_buffer.push(c);

                let new_match = ActiveMatch {
                    node: next_node.clone(),
                    buffer: new_buffer.clone(),
                };

                if next_node.is_end {
                    // We found a complete match
                    matched_sequence = next_node.sequence.clone();
                    // No need to continue processing
                    break;
                }

                new_active_matches.push(new_match);
                max_match_length = max(max_match_length, new_buffer.len());
            }
        }

        // If we found a match, clear state and return the match
        if let Some(seq) = &matched_sequence {
            state.buffered_chars.clear();
            state.active_matches.clear();
            return (None, Some(seq.clone()));
        }

        // Output characters that can no longer be part of any match
        if state.buffered_chars.len() > max_match_length {
            let chars_to_output = state.buffered_chars.len() - max_match_length;
            let output = state.buffered_chars[..chars_to_output].to_string();
            state.buffered_chars = state.buffered_chars[chars_to_output..].to_string();

            // Update active matches for the next iteration
            state.active_matches = new_active_matches;

            return (Some(output), None);
        }

        // Update active matches for the next iteration
        state.active_matches = new_active_matches;

        (None, None)
    }
}

impl TrieNode {
    /// Creates a new TrieNode.
    fn new() -> Self {
        TrieNode {
            children: HashMap::new(),
            is_end: false,
            sequence: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_matcher() {
        let stop_sequences = vec!["stop".to_string(), "halt".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Verify the matcher was created with empty buffer and no active matches
        let state = matcher.state.lock().unwrap();
        assert_eq!(state.buffered_chars, "");
        assert!(state.active_matches.is_empty());
        drop(state); // Explicitly release the lock

        // Verify the trie structure was built correctly
        // Check for 's' in the root's children
        assert!(matcher.root.children.contains_key(&'s'));
        // Check for 'h' in the root's children
        assert!(matcher.root.children.contains_key(&'h'));
    }

    #[test]
    fn test_simple_match() {
        let stop_sequences = vec!["stop".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process a text that contains the stop sequence
        let (output, matched) = matcher.process("This should stop here");

        // Verify the output contains text before the match
        assert_eq!(output, "This should ");
        // Verify the matched sequence
        assert_eq!(matched, Some("stop".to_string()));
    }

    #[test]
    fn test_no_match() {
        let stop_sequences = vec!["stop".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process a text that doesn't contain the stop sequence
        let (output, matched) = matcher.process("This text will be output directly");

        // Verify all text is output
        assert_eq!(output, "This text will be output directly");
        // Verify no match was found
        assert_eq!(matched, None);
    }

    #[test]
    fn test_partial_match() {
        let stop_sequences = vec!["stop".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process a text that contains part of the stop sequence
        let (output, matched) = matcher.process("This is st");

        // Verify text before potential match is output
        assert_eq!(output, "This is ");
        // Verify no match was found yet
        assert_eq!(matched, None);

        // Process the rest of the stop sequence
        let (output2, matched2) = matcher.process("op now");

        // Verify the matched sequence
        assert_eq!(matched2, Some("stop".to_string()));
        // No output before the match in the second chunk
        assert_eq!(output2, "");
    }

    #[test]
    fn test_multiple_stop_sequences() {
        let stop_sequences = vec!["stop".to_string(), "halt".to_string(), "end".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Test with the second stop sequence
        let (output, matched) = matcher.process("Please halt processing");

        // Verify the output contains text before the match
        assert_eq!(output, "Please ");
        // Verify the matched sequence
        assert_eq!(matched, Some("halt".to_string()));

        // Reset and test with the third stop sequence
        let matcher = StopSequenceMatcher::new(&stop_sequences);
        let (output, matched) = matcher.process("This is the end of the text");

        // Verify the output contains text before the match
        assert_eq!(output, "This is the ");
        // Verify the matched sequence
        assert_eq!(matched, Some("end".to_string()));
    }

    #[test]
    fn test_overlapping_sequences() {
        let stop_sequences = vec!["stop".to_string(), "stopping".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process text with the longer sequence
        let (output, matched) = matcher.process("We are stopping now");

        // Verify the output contains text before the match
        assert_eq!(output, "We are ");
        // Verify the matched sequence is the shorter one that appears first
        assert_eq!(matched, Some("stop".to_string()));
    }

    #[test]
    fn test_empty_input() {
        let stop_sequences = vec!["stop".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process empty text
        let (output, matched) = matcher.process("");

        // Verify empty output
        assert_eq!(output, "");
        // Verify no match
        assert_eq!(matched, None);
    }

    #[test]
    fn test_empty_stop_sequences() {
        let stop_sequences: Vec<String> = vec![];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process some text
        let (output, matched) = matcher.process("This text should pass through");

        // Verify all text is output
        assert_eq!(output, "This text should pass through");
        // Verify no match
        assert_eq!(matched, None);
    }

    #[test]
    fn test_incremental_processing() {
        let stop_sequences = vec!["stop".to_string()];
        let matcher = StopSequenceMatcher::new(&stop_sequences);

        // Process text character by character
        let (output1, matched1) = matcher.process("T");
        assert_eq!(output1, "T");
        assert_eq!(matched1, None);

        let (output2, matched2) = matcher.process("h");
        assert_eq!(output2, "h");
        assert_eq!(matched2, None);

        let (output3, matched3) = matcher.process("is is s");
        assert_eq!(output3, "is is ");
        assert_eq!(matched3, None);

        let (output4, matched4) = matcher.process("t");
        assert_eq!(output4, "");
        assert_eq!(matched4, None);

        let (output5, matched5) = matcher.process("op");
        assert_eq!(output5, "");
        assert_eq!(matched5, Some("stop".to_string()));
    }
}